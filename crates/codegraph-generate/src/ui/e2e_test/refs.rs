use std::collections::HashSet;

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{PropertyNode, SchemaNode};

use crate::api::api_model::resolve_path_segment_with_config;
// The name-convention FK shape is shared with the IFML fixture plane
// (`ifml::e2e_test::body`) so the two e2e surfaces cannot seed different
// parents for the same column.
use crate::fk_convention::{is_convention_shape, resolve_convention_ref};

use super::page::UiField;

/// Resolve the referenced schema for an entity-ref property via the graph,
/// falling back to parsing the raw `$ref` string when the graph has no edge.
/// Mirrors `collect_ui_fields` resolution: handles `.schema.json` stems,
/// filename/title divergence, and cross-domain refs.
pub(super) async fn resolve_ref_schema(
    db: &dyn GraphQuerier,
    prop: &PropertyNode,
    schema_title: &str,
    current_domain: Option<&str>,
) -> Option<SchemaNode> {
    let mut resolved = if prop.is_array {
        db.get_array_item_schema(&prop.name, schema_title)
            .await
            .ok()
            .flatten()
    } else {
        db.get_property_ref_target(&prop.name, schema_title)
            .await
            .ok()
            .flatten()
    };
    if resolved.is_none()
        && let Some(ref target) = prop.ref_target
    {
        let last_segment = target.rsplit('/').next().unwrap_or(target);
        let ref_schema_title = last_segment
            .strip_suffix(".schema.json")
            .or_else(|| last_segment.strip_suffix(".json#"))
            .or_else(|| last_segment.strip_suffix(".json"))
            .unwrap_or(last_segment);
        if let Ok(Some(ref_schema)) = db
            .get_schema_in_domain(ref_schema_title, current_domain.unwrap_or(""))
            .await
        {
            resolved = Some(ref_schema);
        }
        if resolved.is_none()
            && let Ok(Some(ref_schema)) = db.get_schema(ref_schema_title).await
        {
            resolved = Some(ref_schema);
        }
    }
    // Final fallback: a required plain `format: uuid` scalar ending in `_id`
    // with no `$ref`/graph edge (e.g. `party.case_id`) resolves to an entity by
    // naming convention. The subject entity itself is never a convention
    // target (#479).
    if resolved.is_none() {
        resolved = resolve_convention_ref(db, prop, current_domain, schema_title).await;
    }
    // Prefer a same-domain schema when the resolved one lives elsewhere.
    if let (Some(cur_domain), Some(found)) = (current_domain, &resolved)
        && found.domain.as_deref() != Some(cur_domain)
        && let Ok(schemas) = db.list_schemas(Some(cur_domain)).await
        && let Some(same_domain) = schemas.iter().find(|s| s.title == found.title)
    {
        resolved = Some(same_domain.clone());
    }
    resolved
}

/// One convention-resolved entity ref: the marked field and its target.
pub(super) struct ConventionRefResolution {
    pub field_name: String,
    pub target: SchemaNode,
}

/// Post-process collected UI fields: mark required plain-uuid convention FK
/// columns (e.g. `case_id`) as entity refs so they participate in dependency
/// setup and `testData()` emits the dep branch instead of a literal.
///
/// Returns the resolutions so the caller can gate on target creatability
/// (the create-API check lives with the main-entity context, not here: the
/// transitive dep path wants every resolved target marked so the tolerant
/// optional-dep machinery applies).
pub(super) async fn apply_convention_refs(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    subject_title: &str,
    props: &[PropertyNode],
    fields: &mut [UiField],
) -> Vec<ConventionRefResolution> {
    let mut resolved = Vec::new();
    for field in fields.iter_mut() {
        if field.is_entity_ref || !field.is_required {
            continue;
        }
        let Some(prop) = find_ref_property(props, &field.name) else {
            continue;
        };
        if let Some(target) = resolve_convention_ref(db, prop, Some(domain), subject_title).await {
            field.is_entity_ref = true;
            field.ref_api_path = Some(api_path_for_schema(&target, config));
            resolved.push(ConventionRefResolution {
                field_name: field.name.clone(),
                target,
            });
        }
    }
    resolved
}

/// True when the field has the name-convention FK shape but has NO creatable
/// entity target — `testData` keeps the text sentinel and the templates emit
/// the `// fixture: convention ref '<field>' has no creatable target` gap
/// comment instead of silently shipping a mistyped literal.
///
/// Callers unmark resolved-but-not-creatable fields first, so a `true` here
/// means exactly: the name claims an entity, but there is nothing we can
/// dep-create for it.
pub(super) fn is_convention_ref_gap(field: &UiField, props: &[PropertyNode]) -> bool {
    if field.is_entity_ref
        || !field.is_required
        || field.is_codelist
        || field.is_array
        || !field.name.ends_with("_id")
    {
        return false;
    }
    find_ref_property(props, &field.name)
        .map(is_convention_shape)
        .unwrap_or(false)
}

/// Match a UI entity-ref field to its graph property. Entity-ref UI fields use
/// the `_id`-suffixed DTO name for scalars; arrays keep the raw field name.
pub(super) fn find_ref_property<'a>(
    props: &'a [PropertyNode],
    field_name: &str,
) -> Option<&'a PropertyNode> {
    let raw = field_name.strip_suffix("_id").unwrap_or(field_name);
    props.iter().find(|p| {
        p.rust_field_name == field_name
            || p.rust_field_name == raw
            || p.name == raw
            || p.pg_column_name == field_name
    })
}

pub(super) fn api_path_for_schema(schema: &SchemaNode, config: &DomainConfig) -> String {
    let domain = schema.domain.clone().unwrap_or_default();
    format!(
        "/{}/{}",
        domain,
        resolve_path_segment_with_config(None, schema, config)
    )
}

/// Allocate a unique `depIds` key, suffixing `_2`, `_3`, ... on collision.
pub(super) fn unique_dep_id(base: &str, used: &mut HashSet<String>) -> String {
    let mut candidate = base.to_string();
    let mut n = 2usize;
    while used.contains(&candidate) {
        candidate = format!("{}_{}", base, n);
        n += 1;
    }
    used.insert(candidate.clone());
    candidate
}
