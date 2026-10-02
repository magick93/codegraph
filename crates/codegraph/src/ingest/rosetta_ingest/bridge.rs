//! Element → Schema/Property/CodeList bridging for the rosetta bridge
//! (split out of the `rosetta_ingest` module, #371).

use std::collections::{HashMap, HashSet};

use sigil_model::CardinalityMax;

use codegraph_core::types::{PropertyNode, SchemaNode};
use codegraph_naming::{escape_rust_keyword, strip_suffix, to_kebab_case, to_snake_case};
use codegraph_type_contracts::{RefClassificationKind, TypeExpr};

use super::mapping::{alias_builtin_mapping, builtin_mapping};
use super::naming::referenced_title;
use super::{ROSETTA_ORIGIN, RosettaIngestStats};
use crate::ingest::async_ingest::{sanitize_description, sanitize_rust_type_name};
use crate::ingest::mox_ingest::{build_projection, strip_code_suffix};

/// Attributes in the JSON path's allOf-canonical order: inherited
/// attributes before own, each group name-sorted, first occurrence wins.
/// Rosetta `override` attributes REPLACE the inherited entry in place
/// (override semantics beat first-wins), keeping the inherited slot's
/// position so field order stays canonical.
pub(super) fn ordered_bridge_attributes<'a>(
    data: &'a sigil_model::Data,
    data_by_name: &HashMap<&str, &'a sigil_model::Data>,
) -> Vec<&'a sigil_model::Attribute> {
    let mut ordered = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    collect_ancestor_attributes(data, data_by_name, &mut visited, &mut ordered);
    let mut own: Vec<&sigil_model::Attribute> = data.attributes.iter().collect();
    own.sort_by(|a, b| a.name.cmp(&b.name));
    ordered.extend(own);

    let mut positions: HashMap<String, usize> = HashMap::new();
    let mut result: Vec<&'a sigil_model::Attribute> = Vec::with_capacity(ordered.len());
    for attribute in ordered {
        match positions.get(&attribute.name) {
            Some(&index) => {
                if attribute.is_override {
                    result[index] = attribute;
                }
            }
            None => {
                positions.insert(attribute.name.clone(), result.len());
                result.push(attribute);
            }
        }
    }
    result
}

fn collect_ancestor_attributes<'a>(
    data: &'a sigil_model::Data,
    data_by_name: &HashMap<&str, &'a sigil_model::Data>,
    visited: &mut HashSet<String>,
    ordered: &mut Vec<&'a sigil_model::Attribute>,
) {
    if !visited.insert(data.name.clone()) {
        return;
    }
    if let Some(parent) = &data.super_type {
        let parent_name = referenced_title(parent);
        if let Some(parent_data) = data_by_name.get(parent_name.as_str()) {
            collect_ancestor_attributes(parent_data, data_by_name, visited, ordered);
        }
    }
    let mut group: Vec<&sigil_model::Attribute> = data.attributes.iter().collect();
    group.sort_by(|a, b| a.name.cmp(&b.name));
    ordered.extend(group);
}

fn rosetta_annotations() -> HashMap<String, serde_json::Value> {
    HashMap::from([(
        "origin".to_string(),
        serde_json::Value::String(ROSETTA_ORIGIN.to_string()),
    )])
}

/// JSON payload for one attribute's annotation surface (labels, rule
/// references, doc references, `[metadata]`-style annotation refs).
fn attribute_annotations_json(attribute: &sigil_model::Attribute) -> Option<serde_json::Value> {
    let mut payload = serde_json::Map::new();
    if !attribute.annotations.is_empty() {
        payload.insert(
            "annotations".to_string(),
            serde_json::to_value(&attribute.annotations).ok()?,
        );
    }
    if !attribute.labels.is_empty() {
        payload.insert(
            "labels".to_string(),
            serde_json::to_value(&attribute.labels).ok()?,
        );
    }
    if !attribute.rule_references.is_empty() {
        payload.insert(
            "rule_references".to_string(),
            serde_json::to_value(&attribute.rule_references).ok()?,
        );
    }
    if !attribute.doc_references.is_empty() {
        payload.insert(
            "doc_references".to_string(),
            serde_json::to_value(&attribute.doc_references).ok()?,
        );
    }
    (!payload.is_empty()).then_some(serde_json::Value::Object(payload))
}

pub(super) fn data_schema_node(
    data: &sigil_model::Data,
    domain: &str,
    schema_id: &str,
    namespace: &str,
    type_suffix: &str,
) -> SchemaNode {
    // Choices KEEP their full name for code identifiers: the Type-suffix
    // strip assumes types carry the suffix, but in Rosetta it is the
    // choices that do (`choice ProductType` optioning `type Product`) —
    // stripping would collide both at pg_table_name `product`
    // (gap-analysis defect #9).
    let stripped = if data.is_choice {
        data.name.clone()
    } else {
        strip_suffix(&data.name, type_suffix)
    };
    let mut custom_annotations = rosetta_annotations();
    custom_annotations.insert(
        "rosetta_namespace".to_string(),
        serde_json::Value::String(namespace.to_string()),
    );
    if data.is_choice {
        custom_annotations.insert("rosetta_choice".to_string(), serde_json::Value::Bool(true));
    }
    if !data.annotations.is_empty()
        && let Ok(value) = serde_json::to_value(&data.annotations)
    {
        custom_annotations.insert("rosetta_annotations".to_string(), value);
    }
    if !data.doc_references.is_empty()
        && let Ok(value) = serde_json::to_value(&data.doc_references)
    {
        custom_annotations.insert("rosetta_doc_references".to_string(), value);
    }
    let attribute_annotations: serde_json::Map<String, serde_json::Value> = data
        .attributes
        .iter()
        .filter_map(|attribute| {
            attribute_annotations_json(attribute).map(|value| (attribute.name.clone(), value))
        })
        .collect();
    if !attribute_annotations.is_empty() {
        custom_annotations.insert(
            "rosetta_attribute_annotations".to_string(),
            serde_json::Value::Object(attribute_annotations),
        );
    }
    if !data.conditions.is_empty() {
        let conditions: Vec<serde_json::Value> = data
            .conditions
            .iter()
            .map(|condition| {
                serde_json::json!({
                    "name": condition.name,
                    "expression": condition.expression.to_json(),
                })
            })
            .collect();
        custom_annotations.insert(
            "rosetta_conditions".to_string(),
            serde_json::Value::Array(conditions),
        );
    }

    SchemaNode {
        namespace: (!namespace.trim().is_empty()).then(|| namespace.to_string()),
        schema_id: schema_id.to_string(),
        title: data.name.clone(),
        description: data.definition.as_deref().map(sanitize_description),
        schema_type: "object".to_string(),
        // Rosetta types are auto-scored by the classifier (#258) — the
        // bridge records a VO-shaped default and never declares entities.
        classification: "value_object".to_string(),
        domain: Some(domain.to_string()),
        rel_path: schema_id.to_string(),
        pg_type: "UUID".to_string(),
        rust_type: stripped.clone(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: sanitize_rust_type_name(&stripped),
        pg_table_name: to_snake_case(&stripped),
        api_path_segment: to_kebab_case(&stripped),
        parent_schema: None,
        is_entity: false,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: data.super_type.is_some(),
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations,
        access: None,
        annotations: None,
    }
}

/// Module-level counter hop removed: condition counts are added to stats
/// directly in the bridge pass that builds each schema node.
pub(super) fn enum_schema_node(
    enumeration: &sigil_model::Enumeration,
    domain: &str,
    schema_id: &str,
    namespace: &str,
    type_suffix: &str,
) -> SchemaNode {
    let stripped = strip_suffix(&enumeration.name, type_suffix);
    let mut custom_annotations = rosetta_annotations();
    custom_annotations.insert(
        "rosetta_namespace".to_string(),
        serde_json::Value::String(namespace.to_string()),
    );
    SchemaNode {
        namespace: (!namespace.trim().is_empty()).then(|| namespace.to_string()),
        schema_id: schema_id.to_string(),
        title: enumeration.name.clone(),
        description: enumeration.definition.as_deref().map(sanitize_description),
        schema_type: "string".to_string(),
        classification: "codelist".to_string(),
        domain: Some(domain.to_string()),
        rel_path: schema_id.to_string(),
        pg_type: "UUID".to_string(),
        rust_type: stripped.clone(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: sanitize_rust_type_name(&stripped),
        pg_table_name: to_snake_case(&stripped),
        api_path_segment: to_kebab_case(&stripped),
        parent_schema: None,
        is_entity: false,
        is_codelist: true,
        is_primitive_wrapper: false,
        has_all_of: enumeration.super_type.is_some(),
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations,
        access: None,
        annotations: None,
    }
}

/// Rosetta cardinality → the graph's two multiplicity bits. A `(min..max)`
/// cardinality is required iff `min >= 1`, and an array iff max is
/// unbounded or greater than one.
///
/// Returns `(is_required, is_array)`.
fn cardinality_flags(cardinality: &sigil_model::Cardinality) -> (bool, bool) {
    let is_required = cardinality.min >= 1;
    let is_array = match cardinality.max {
        CardinalityMax::Unbounded => true,
        CardinalityMax::Finite(max) => max > 1,
    };
    (is_required, is_array)
}

/// Rosetta cardinality → JSON-Schema-equivalent array bounds (issue #261,
/// closing gap-analysis finding 2): `(2..10)` maps to
/// `min_items = 2 / max_items = 10`. Only arrays carry item counts — a
/// scalar attribute's `min` is requiredness, not an item bound.
fn cardinality_items(
    cardinality: &sigil_model::Cardinality,
    is_array: bool,
) -> (Option<u32>, Option<u32>) {
    if !is_array {
        return (None, None);
    }
    let min_items = (cardinality.min > 1).then_some(cardinality.min);
    let max_items = match cardinality.max {
        CardinalityMax::Finite(max) if max > 1 => Some(max),
        _ => None,
    };
    (min_items, max_items)
}

fn classification_str(kind: &RefClassificationKind) -> &'static str {
    match kind {
        RefClassificationKind::PrimitiveWrapper => "primitive_wrapper",
        RefClassificationKind::ArrayWrapper => "array_wrapper",
        RefClassificationKind::RangeWrapper => "range_wrapper",
        RefClassificationKind::CodelistReference => "codelist",
        RefClassificationKind::CodelistCheck => "codelist_check",
        RefClassificationKind::InlineEnum => "inline_enum",
        RefClassificationKind::EntityReference => "entity_reference",
        RefClassificationKind::ValueObject => "value_object",
        RefClassificationKind::CompositeWrapper => "composite_wrapper",
        RefClassificationKind::MediaWrapper => "media_wrapper",
        RefClassificationKind::StructuredWrapper => "structured_wrapper",
    }
}

pub(super) fn attribute_property(
    attribute: &sigil_model::Attribute,
    schema_title: &str,
    enum_titles: &HashSet<String>,
    type_titles: &HashSet<String>,
    alias_types: &HashMap<String, sigil_model::TypeRef>,
    stats: &mut RosettaIngestStats,
) -> Option<PropertyNode> {
    let target_title = referenced_title(&attribute.type_ref);
    let (is_required, is_array) = cardinality_flags(&attribute.cardinality);
    let (min_items, max_items) = cardinality_items(&attribute.cardinality, is_array);

    let (kind, pg_base, rust_base, sea_base, ref_target, format_hint, prop_type) =
        if let Some((pg, format, json_type)) = builtin_mapping(&target_title)
            .or_else(|| alias_builtin_mapping(&target_title, alias_types, 0))
        {
            (
                RefClassificationKind::PrimitiveWrapper,
                pg.pg_ddl().to_string(),
                pg.canonical_rust_type().as_rust_str().to_string(),
                pg.sea_orm_type().to_string(),
                None,
                format.map(str::to_string),
                json_type.to_string(),
            )
        } else if enum_titles.contains(&target_title) {
            (
                RefClassificationKind::CodelistReference,
                "TEXT".to_string(),
                "String".to_string(),
                "Text".to_string(),
                Some(target_title.clone()),
                None,
                "string".to_string(),
            )
        } else if type_titles.contains(&target_title) {
            (
                RefClassificationKind::EntityReference,
                String::new(),
                target_title.clone(),
                String::new(),
                Some(target_title.clone()),
                None,
                "object".to_string(),
            )
        } else {
            eprintln!(
                "Warning: rosetta attribute '{schema_title}.{}' references unknown type \
                 '{target_title}' — mapping to TEXT",
                attribute.name
            );
            stats.skipped += 1;
            (
                RefClassificationKind::PrimitiveWrapper,
                "TEXT".to_string(),
                "String".to_string(),
                "Text".to_string(),
                None,
                None,
                "string".to_string(),
            )
        };

    // Arrays of primitives wrap in Vec + pg [] suffix; entity/VO/codelist
    // arrays keep child-table semantics (mirrors the JSON path's array
    // branch and the mox bridge).
    let (render_strategy, pg_type, rust_type) = if is_array {
        match kind {
            RefClassificationKind::EntityReference
            | RefClassificationKind::ValueObject
            | RefClassificationKind::CodelistReference
            | RefClassificationKind::CodelistCheck => {
                ("child_table".to_string(), pg_base, rust_base)
            }
            _ => (
                classification_str(&kind).to_string(),
                format!("{pg_base}[]"),
                format!("Vec<{rust_base}>"),
            ),
        }
    } else {
        (classification_str(&kind).to_string(), pg_base, rust_base)
    };

    let sanitized_name = attribute.name.replace(['@', '-'], "");
    let snake = to_snake_case(&sanitized_name);
    let projection = build_projection(&kind, &snake, &pg_type, &rust_type, &sea_base);
    let mut rust_field_name = escape_rust_keyword(&snake);
    if matches!(
        kind,
        RefClassificationKind::CodelistReference | RefClassificationKind::CodelistCheck
    ) {
        rust_field_name = strip_code_suffix(&rust_field_name);
    }

    // Express the frozen classification structurally (issue #277); the
    // legacy strings on the node stay populated either way.
    let type_expr = TypeExpr::from_frozen(&kind, ref_target.as_deref(), &rust_type);

    Some(PropertyNode {
        name: attribute.name.clone(),
        prop_type,
        description: attribute.definition.as_deref().map(sanitize_description),
        format: format_hint,
        is_required,
        is_nullable: !is_required,
        is_array,
        min_items,
        max_items,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: snake.clone(),
        pg_column_type: pg_type,
        rust_field_name,
        rust_field_type: rust_type,
        sea_orm_type: sea_base,
        render_strategy,
        ref_target,
        classification: None,
        projection: Some(projection),
        classification_kind: Some(kind),
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr,
    })
}
