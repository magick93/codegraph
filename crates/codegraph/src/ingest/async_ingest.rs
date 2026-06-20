use std::collections::{HashMap, HashSet};
use std::path::Path;

use heck::ToUpperCamelCase;
use codegraph_classifier::classify::{classify_plain_type, classify_ref};
use codegraph_classifier::config::ClassifierConfig;
use codegraph_type_contracts::{DddFieldProjection, RefClassificationKind};
use codegraph_config::UiOverrideConfig;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{
    CodeList, CompositeColumn, CompositeRange, EdgeProperties, EdgeType, EnumValue, PropertyNode,
    SchemaNode,
};
use codegraph_naming::{escape_rust_keyword, strip_suffix, to_kebab_case, to_snake_case};

use crate::error::{Error, Result};
use crate::generate::ddd::dto::strip_code_suffix_safe;
use crate::ingest::schema_loader::SchemaLoader;

/// Sanitize a schema/property description for use in generated code doc comments.
/// Truncates to the first line (newlines break /// doc comments), trims whitespace,
/// and caps length to 1000 characters.
fn sanitize_description(s: &str) -> String {
    s.lines()
        .next()
        .unwrap_or("")
        .trim()
        .chars()
        .take(1000)
        .collect()
}

/// Sanitize a string into a valid PascalCase Rust type identifier.
/// Removes characters that aren't alphanumeric (except underscores),
/// converts to PascalCase, and truncates to 200 chars.
fn sanitize_rust_type_name(s: &str) -> String {
    // Keep only valid identifier characters and spaces (for PascalCase conversion)
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == ' ')
        .collect();
    // Convert to PascalCase
    let pascal = cleaned.to_upper_camel_case();
    // Remove leading digits
    let trimmed: String = pascal.chars().skip_while(|c| c.is_ascii_digit()).collect();
    let trimmed = if trimmed.is_empty() { "_".to_string() } else { trimmed };
    // Cap length
    trimmed.chars().take(200).collect()
}

/// Ingest all schemas from `schema_dir` into the graph via `GraphIngestor`.
///
/// Uses `entity_names` to determine which schemas are entities vs value objects.
/// This is the async replacement for the legacy `Ingester` which used `GraphClient`.
pub async fn ingest_schemas(
    db: &dyn GraphIngestor,
    schema_dir: &Path,
    classifier: &ClassifierConfig,
    entity_names: &HashSet<String>,
    ui_overrides: &UiOverrideConfig,
    suffix: &str,
) -> Result<IngestResult> {
    let loader = SchemaLoader::load(schema_dir)?;
    let is_entity =
        |name: &str| entity_names.contains(name) || entity_names.contains(&format!("{}Type", name));

    let mut result = IngestResult::default();

    // Pass 1: Ingest all schema nodes
    let uris: Vec<String> = loader
        .iter_top_level()
        .map(|(uri, _)| uri.to_string())
        .collect();

    for uri in &uris {
        ingest_schema_node(db, &loader, uri, classifier, &is_entity, None, suffix, &mut result).await?;
    }

    // Pass 1b: Ingest codelist entries and enum values for codelist schemas
    for uri in &uris {
        ingest_codelist_values(db, &loader, uri, suffix).await?;
    }

    // Build stem → schema_id map for $ref resolution.
    // This maps each stem (filename without .json) to all schema rel_paths
    // that share it. A Vec is used because stems can collide across domains.
    let mut stem_to_schema_ids: HashMap<String, Vec<String>> = HashMap::new();
    for (_uri, entry) in loader.iter_top_level() {
        stem_to_schema_ids
            .entry(entry.stem.clone())
            .or_default()
            .push(entry.rel_path.clone());
    }

    // Pass 2: Ingest inline definitions
    let mut inline_uris: Vec<String> = Vec::new();
    for uri in &uris {
        let new_uris =
            ingest_inline_defs(db, &loader, uri, classifier, &is_entity, suffix, &mut result).await?;
        inline_uris.extend(new_uris);
    }

    // Pass 3: Ingest properties for all schemas (top-level + inline defs)
    let mut all_uris = uris.clone();
    all_uris.extend(inline_uris);
    for uri in &all_uris {
        ingest_properties(
            db,
            &loader,
            uri,
            classifier,
            &is_entity,
            ui_overrides,
            suffix,
            &mut result,
            &stem_to_schema_ids,
        )
        .await?;
    }

    // Pass 4: Ingest composition edges (allOf) for all schemas (top-level + inline defs)
    for uri in &all_uris {
        ingest_allof_edges(db, &loader, uri, &mut result).await?;
    }

    // Pass 5: Ingest composite ranges from classifier config
    ingest_composite_ranges(db, classifier).await?;

    // Pass 6: Ingest required extensions and link to schemas that use them
    ingest_required_extensions(db, classifier, &loader, &all_uris).await?;

    Ok(result)
}

#[derive(Debug, Default)]
pub struct IngestResult {
    pub schemas_created: usize,
    pub properties_created: usize,
    pub edges_created: usize,
}

async fn ingest_schema_node(
    db: &dyn GraphIngestor,
    loader: &SchemaLoader,
    uri: &str,
    classifier: &ClassifierConfig,
    is_entity: &dyn Fn(&str) -> bool,
    parent_schema: Option<&str>,
    suffix: &str,
    result: &mut IngestResult,
) -> Result<()> {
    let entry = loader
        .get(uri)
        .ok_or_else(|| Error::SchemaNotFound(uri.into()))?;

    let classification = classify_ref(&entry.stem, uri, classifier, is_entity);
    let classification_str = classification_kind_str(&classification.kind);
    let title = entry
        .schema
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(&entry.stem);
    let stripped = strip_suffix(title, suffix);

    let node = SchemaNode {
        schema_id: uri.to_string(),
        title: title.to_string(),
        description: entry
            .schema
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| sanitize_description(s)),
        schema_type: entry
            .schema
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("object")
            .to_string(),
        classification: classification_str.to_string(),
        domain: Some(entry.domain.clone()),
        rel_path: entry.rel_path.clone(),
        pg_type: "UUID".to_string(),
        rust_type: stripped.to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: sanitize_rust_type_name(&stripped),
        pg_table_name: to_snake_case(&stripped),
        api_path_segment: to_kebab_case(&stripped),
        parent_schema: parent_schema.map(|s| s.to_string()),
        is_entity: is_entity(&entry.stem),
        is_codelist: classification_str == "codelist",
        is_primitive_wrapper: classification_str == "primitive_wrapper",
        has_all_of: entry.schema.get("allOf").is_some(),
        has_one_of: entry.schema.get("oneOf").is_some(),
        has_any_of: entry.schema.get("anyOf").is_some(),
        has_definitions: entry.schema.get("definitions").is_some()
            || entry.schema.get("$defs").is_some(),
    };

    db.ingest_schema(&node).await.map_err(Error::Graph)?;
    result.schemas_created += 1;
    Ok(())
}

async fn ingest_inline_defs(
    db: &dyn GraphIngestor,
    loader: &SchemaLoader,
    uri: &str,
    classifier: &ClassifierConfig,
    is_entity: &dyn Fn(&str) -> bool,
    suffix: &str,
    result: &mut IngestResult,
) -> Result<Vec<String>> {
    let entry = loader
        .get(uri)
        .ok_or_else(|| Error::SchemaNotFound(uri.into()))?;

    let parent_title = entry
        .schema
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(&entry.stem);

    let mut created_uris = Vec::new();
    for key in &["definitions", "$defs"] {
        if let Some(serde_json::Value::Object(defs)) = entry.schema.get(*key) {
            for def_name in defs.keys() {
                let def_uri = format!("{}#/{}/{}", uri, key, def_name);
                if loader.get(&def_uri).is_some() {
                    ingest_schema_node(
                        db,
                        loader,
                        &def_uri,
                        classifier,
                        is_entity,
                        Some(parent_title),
                        suffix,
                        result,
                    )
                    .await?;
                    created_uris.push(def_uri);
                }
            }
        }
    }
    Ok(created_uris)
}

async fn ingest_properties(
    db: &dyn GraphIngestor,
    loader: &SchemaLoader,
    uri: &str,
    classifier: &ClassifierConfig,
    is_entity: &dyn Fn(&str) -> bool,
    ui_overrides: &UiOverrideConfig,
    suffix: &str,
    result: &mut IngestResult,
    stem_to_schema_ids: &HashMap<String, Vec<String>>,
) -> Result<()> {
    let entry = loader
        .get(uri)
        .ok_or_else(|| Error::SchemaNotFound(uri.into()))?;
    let schema = &entry.schema;

    let schema_title = schema
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(&entry.stem);

    // If this schema is "type": "array", follow the items ref and ingest the
    // item type's properties under this schema's title.
    let schema_type = schema
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("object");
    if schema_type == "array" {
        // Resolve the item schema — handles both direct $ref and allOf with $refs
        let items_ref = schema.get("items").and_then(|v| {
            // Direct $ref: { "items": { "$ref": "EmailType.json#" } }
            v.get("$ref")
                .and_then(|r| r.as_str())
                .map(|s| s.to_string())
                // allOf: { "items": { "allOf": [{ "$ref": "EmailType.json#" }, ...] } }
                .or_else(|| {
                    v.get("allOf")
                        .and_then(|arr| arr.as_array())
                        .and_then(|entries| {
                            entries
                                .iter()
                                .find_map(|e| e.get("$ref").and_then(|r| r.as_str()))
                                .map(|s| s.to_string())
                        })
                })
        });

        if let Some(ref items_ref_str) = items_ref {
            if let Ok((_resolved_uri, resolved_entry)) = loader.resolve_ref(items_ref_str, uri) {
                let item_schema = &resolved_entry.schema;
                // Ingest item type's properties under the array schema's title
                return ingest_properties_from_schema(
                    db,
                    loader,
                    uri,
                    item_schema,
                    schema_title,
                    classifier,
                    is_entity,
                    ui_overrides,
                    suffix,
                    result,
                    stem_to_schema_ids,
                )
                .await;
            }
        }
        // Array schema with no resolvable items ref — nothing to ingest
        return Ok(());
    }

    ingest_properties_from_schema(
        db,
        loader,
        uri,
        schema,
        schema_title,
        classifier,
        is_entity,
        ui_overrides,
        suffix,
        result,
        stem_to_schema_ids,
    )
    .await
}

/// Core property ingestion logic. Extracted so array-type schemas can delegate
/// to the item type's schema while keeping the parent schema's title.
///
/// `loader` and `base_uri` are used to resolve `$ref` entries in `allOf`,
/// merging referenced schema properties into the current schema's property list.
#[allow(clippy::too_many_arguments)]
async fn ingest_properties_from_schema(
    db: &dyn GraphIngestor,
    loader: &SchemaLoader,
    base_uri: &str,
    schema: &serde_json::Value,
    schema_title: &str,
    classifier: &ClassifierConfig,
    is_entity: &dyn Fn(&str) -> bool,
    ui_overrides: &UiOverrideConfig,
    suffix: &str,
    result: &mut IngestResult,
    stem_to_schema_ids: &HashMap<String, Vec<String>>,
) -> Result<()> {
    // Collect property blocks: top-level properties + allOf inline/ref properties
    let mut prop_blocks: Vec<(serde_json::Map<String, serde_json::Value>, HashSet<String>)> =
        Vec::new();

    // Top-level properties
    if let Some(serde_json::Value::Object(props)) = schema.get("properties") {
        let required: HashSet<String> = schema
            .get("required")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        prop_blocks.push((props.clone(), required));
    }

    // allOf entries: inline properties AND $ref properties
    if let Some(all_of) = schema.get("allOf").and_then(|v| v.as_array()) {
        for item in all_of {
            if let Some(ref_path) = item.get("$ref").and_then(|v| v.as_str()) {
                // Follow $ref to merge the referenced schema's properties
                if let Ok((_resolved_uri, ref_entry)) = loader.resolve_ref(ref_path, base_uri) {
                    collect_allof_property_blocks(
                        &ref_entry.schema,
                        loader,
                        base_uri,
                        &mut prop_blocks,
                    );
                }
            } else if let Some(serde_json::Value::Object(inline_props)) = item.get("properties") {
                let inline_required: HashSet<String> = item
                    .get("required")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                prop_blocks.push((inline_props.clone(), inline_required));
            }
        }
    }

    // Ingest all collected properties
    for (properties, required) in &prop_blocks {
        for (name, prop_schema) in properties {
            let is_array = prop_schema.get("type").and_then(|v| v.as_str()) == Some("array");
            let is_required = required.contains(name.as_str());
            // Strip characters invalid in Rust/SQL identifiers before converting.
            let sanitized_name = name.replace(['@', '-'], "");
            let snake = to_snake_case(&sanitized_name);

            let domain = loader
                .get(base_uri)
                .map(|e| e.domain.as_str())
                .unwrap_or("common");
            let clf = classify_single_property(
                prop_schema,
                is_array,
                classifier,
                is_entity,
                schema_title,
                name,
                domain,
                suffix,
            );
            // When a property uses $ref to an array schema (e.g. StringTypeArray),
            // the property itself lacks `type: "array"`. Detect this from the
            // classified Rust type so downstream templates know it is an array.
            let is_array = is_array || clf.rust_type.starts_with("Vec<");
            let is_composite_wrapper = clf.render_strategy == "composite_wrapper"
                || clf.render_strategy == "media_wrapper";

            // If the property has inline enum values, create a synthetic codelist
            // so the existing codelist pipeline generates a Rust enum + CHECK constraint.
            if !clf.inline_enum_values.is_empty() {
                if let Some(ref synthetic_name) = clf.ref_target {
                    let codelist = CodeList {
                        name: synthetic_name.clone(),
                        description: Some(format!(
                            "Inline enum values for {}.{}",
                            schema_title, name
                        )),
                        pg_table_name: to_snake_case(synthetic_name),
                        render_as: "enum".to_string(),
                        check_expression: None,
                    };
                    db.ingest_codelist(&codelist).await.map_err(Error::Graph)?;
                    for (i, val) in clf.inline_enum_values.iter().enumerate() {
                        let ev = EnumValue {
                            value: val.clone(),
                            display_name: None,
                            sort_order: i as i32,
                        };
                        db.ingest_enum_value(synthetic_name, &ev)
                            .await
                            .map_err(Error::Graph)?;
                    }
                }
            }

            let mut prop = PropertyNode {
                name: name.clone(),
                prop_type: prop_schema
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("object")
                    .to_string(),
                description: prop_schema
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| sanitize_description(s)),
                format: prop_schema
                    .get("format")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                is_required,
                is_nullable: !is_required,
                is_array,
                pattern: prop_schema
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                min_length: prop_schema.get("minLength").and_then(|v| v.as_u64()),
                max_length: prop_schema.get("maxLength").and_then(|v| v.as_u64()),
                minimum: prop_schema
                    .get("minimum")
                    .and_then(|v| v.as_f64())
                    .map(|f| rust_decimal::Decimal::from_f64_retain(f).unwrap_or_default()),
                maximum: prop_schema
                    .get("maximum")
                    .and_then(|v| v.as_f64())
                    .map(|f| rust_decimal::Decimal::from_f64_retain(f).unwrap_or_default()),
                pg_column_name: snake.clone(),
                pg_column_type: clf.pg_type,
                rust_field_name: escape_rust_keyword(&snake),
                rust_field_type: clf.rust_type,
                sea_orm_type: clf.sea_type,
                render_strategy: clf.render_strategy,
                ref_target: clf.ref_target,
                classification: None,
                projection: clf.projection,
                classification_kind: Some(clf.kind),
                ui_override_detail: None,
                ui_override_list_cell: None,
                ui_override_form: None,
                ui_override_inline: None,
            };

            // Sanitize rust_field_name for codelist properties: strip the _code
            // suffix so that entity model, DTO, and repository generators all see
            // the same field name (e.g., "worker_type" instead of "worker_type_code").
            // The pg_column_name retains the _code suffix for the actual DB column.
            if matches!(prop.effective_kind(), Some(RefClassificationKind::CodelistReference | RefClassificationKind::CodelistCheck)) {
                prop.rust_field_name = strip_code_suffix_safe(&prop.rust_field_name);
            }

            // Apply UI overrides based on ref_target
            if let Some(ref_target) = prop.ref_target.clone() {
                let keys_to_try = [
                    format!("{}/{}", domain, ref_target),
                    format!("common/{}", ref_target),
                    ref_target.clone(),
                ];
                for key in &keys_to_try {
                    if let Some(entry) = ui_overrides.overrides.get(key) {
                        prop.ui_override_detail = entry.detail.clone();
                        prop.ui_override_list_cell = entry.list_cell.clone();
                        prop.ui_override_form = entry.form.clone();
                        prop.ui_override_inline = entry.inline.clone();
                        break;
                    }
                }
            }

            db.ingest_property(schema_title, &prop)
                .await
                .map_err(Error::Graph)?;
            result.properties_created += 1;

            // Create graph edges for $ref targets:
            // - ItemsOf for array properties with items.$ref
            // - ReferencesSchema for scalar $ref properties
            if let Some(ref ref_path) = prop.ref_target {
                let target_stem = extract_ref_stem(ref_path);
                // Resolve stem to schema_id using the map built during Pass 1b.
                // If the stem maps to exactly one schema_id, use it. Otherwise fall back to the stem.
                let target_id = stem_to_schema_ids
                    .get(target_stem)
                    .and_then(|ids| {
                        if ids.len() == 1 {
                            ids.first().cloned()
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| target_stem.to_string());
                let edge_type = if prop.is_array {
                    EdgeType::ItemsOf
                } else {
                    EdgeType::ReferencesSchema
                };
                let edge_props = EdgeProperties {
                    ref_path: Some(ref_path.clone()),
                    ..Default::default()
                };
                db.ingest_edge(
                    &format!("{}::{}", name, schema_title),
                    &target_id,
                    edge_type,
                    Some(&edge_props),
                )
                .await
                .map_err(Error::Graph)?;
                result.edges_created += 1;
            }

            // Ingest composite columns and create ExpandsTo edges for CompositeWrapper/MediaWrapper
            if is_composite_wrapper {
                if let Some(ref ref_path) = prop.ref_target {
                    let ref_stem = extract_ref_stem(ref_path);
                    // Collect column definitions from composite_wrappers or media_wrappers
                    let columns: Option<&[codegraph_classifier::config::CompositeWrapperColumn]> =
                        classifier
                            .composite_wrappers
                            .iter()
                            .find(|cw| cw.schema == ref_stem)
                            .map(|cw| cw.columns.as_slice())
                            .or_else(|| {
                                classifier
                                    .media_wrappers
                                    .get(ref_stem)
                                    .map(|mw| mw.columns.as_slice())
                            });
                    if let Some(col_defs) = columns {
                        for col_def in col_defs {
                            let comp_col = CompositeColumn {
                                suffix: col_def.suffix.clone(),
                                pg_type: col_def.postgres.clone(),
                                rust_type: col_def.rust.clone(),
                                sea_orm_type: col_def.sea_orm.clone(),
                                fk_target: if col_def.fk_table.is_empty() {
                                    None
                                } else {
                                    Some(col_def.fk_table.clone())
                                },
                                dto_rust_type: col_def.dto_rust_type.clone(),
                                wrapper_schema: ref_stem.to_string(),
                            };
                            db.ingest_composite_column(&comp_col)
                                .await
                                .map_err(Error::Graph)?;

                            db.ingest_edge(
                                &format!("{}::{}", name, schema_title),
                                &format!("{}::{}", col_def.suffix, ref_stem),
                                EdgeType::ExpandsTo,
                                None,
                            )
                            .await
                            .map_err(Error::Graph)?;
                            result.edges_created += 1;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn ingest_allof_edges(
    db: &dyn GraphIngestor,
    loader: &SchemaLoader,
    uri: &str,
    result: &mut IngestResult,
) -> Result<()> {
    let entry = loader
        .get(uri)
        .ok_or_else(|| Error::SchemaNotFound(uri.into()))?;

    let all_of = match entry.schema.get("allOf").and_then(|v| v.as_array()) {
        Some(arr) => arr.clone(),
        None => return Ok(()),
    };

    let from_title = entry
        .schema
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(&entry.stem);

    for item in &all_of {
        if let Some(ref_path) = item.get("$ref").and_then(|v| v.as_str()) {
            if let Ok((_resolved, target_entry)) = loader.resolve_ref(ref_path, uri) {
                let to_title = target_entry
                    .schema
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&target_entry.stem);
                let props = EdgeProperties {
                    composition_type: Some("allOf".to_string()),
                    ..Default::default()
                };
                db.ingest_edge(from_title, to_title, EdgeType::ExtendsSchema, Some(&props))
                    .await
                    .map_err(Error::Graph)?;
                result.edges_created += 1;
            }
        }
    }

    Ok(())
}

/// Recursively collect property blocks from a referenced schema, following its own
/// allOf `$ref` chains. This flattens deeply composed schemas (e.g., DistributeToType →
/// DistributionType) into a single property list.
fn collect_allof_property_blocks(
    schema: &serde_json::Value,
    loader: &SchemaLoader,
    base_uri: &str,
    prop_blocks: &mut Vec<(serde_json::Map<String, serde_json::Value>, HashSet<String>)>,
) {
    // Collect the schema's own properties
    if let Some(serde_json::Value::Object(props)) = schema.get("properties") {
        let required: HashSet<String> = schema
            .get("required")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        prop_blocks.push((props.clone(), required));
    }

    // Recursively follow allOf $refs
    if let Some(all_of) = schema.get("allOf").and_then(|v| v.as_array()) {
        for item in all_of {
            if let Some(ref_path) = item.get("$ref").and_then(|v| v.as_str()) {
                if let Ok((_resolved_uri, ref_entry)) = loader.resolve_ref(ref_path, base_uri) {
                    collect_allof_property_blocks(&ref_entry.schema, loader, base_uri, prop_blocks);
                }
            } else if let Some(serde_json::Value::Object(inline_props)) = item.get("properties") {
                let inline_required: HashSet<String> = item
                    .get("required")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                prop_blocks.push((inline_props.clone(), inline_required));
            }
        }
    }
}

/// Ingest CodeList and EnumValue entries for codelist schemas.
///
/// When a schema has an `enum` array and is classified as a codelist, we create
/// a `CodeList` node and individual `EnumValue` nodes in the graph.
async fn ingest_codelist_values(
    db: &dyn GraphIngestor,
    loader: &SchemaLoader,
    uri: &str,
    suffix: &str,
) -> Result<()> {
    let entry = loader
        .get(uri)
        .ok_or_else(|| Error::SchemaNotFound(uri.into()))?;
    let schema = &entry.schema;

    // Only process schemas that have an enum array (codelist schemas)
    let enum_values = match schema.get("enum").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return Ok(()),
    };

    let title = schema
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(&entry.stem);

    let description = schema
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| sanitize_description(s));

    let pg_table_name = to_snake_case(&strip_suffix(title, suffix));

    let codelist = CodeList {
        name: title.to_string(),
        description,
        pg_table_name,
        render_as: "codelist".to_string(),
        check_expression: None,
    };

    db.ingest_codelist(&codelist).await.map_err(Error::Graph)?;

    // Extract enumNames for display names if available
    let enum_names: Vec<Option<&str>> = schema
        .get("enumNames")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().map(|v| v.as_str()).collect())
        .unwrap_or_default();

    for (i, val) in enum_values.iter().enumerate() {
        if let Some(code) = val.as_str() {
            let display_name = enum_names.get(i).copied().flatten().map(|s| s.to_string());

            let ev = EnumValue {
                value: code.to_string(),
                display_name,
                sort_order: i as i32,
            };
            db.ingest_enum_value(title, &ev)
                .await
                .map_err(Error::Graph)?;
        }
    }

    Ok(())
}

/// Result of classifying a single property.
struct PropertyClassification {
    pg_type: String,
    rust_type: String,
    sea_type: String,
    render_strategy: String,
    ref_target: Option<String>,
    kind: codegraph_type_contracts::RefClassificationKind,
    /// Inline enum values to be ingested as a synthetic codelist.
    inline_enum_values: Vec<String>,
    projection: Option<DddFieldProjection>,
}

/// Classify a single property and return its type mapping and classification.
fn classify_single_property(
    prop_schema: &serde_json::Value,
    is_array: bool,
    classifier: &ClassifierConfig,
    is_entity: &dyn Fn(&str) -> bool,
    schema_title: &str,
    prop_name: &str,
    domain: &str,
    suffix: &str,
) -> PropertyClassification {
    if let Some(ref_path) = prop_schema.get("$ref").and_then(|v| v.as_str()) {
        let ref_stem = extract_ref_stem(ref_path);
        let clf = classify_ref(ref_stem, ref_path, classifier, is_entity);
        let render_strategy = classification_kind_str(&clf.kind).to_string();
        let kind = clf.kind.clone();
        let (pg, rust, sea) = derive_type_strings(&clf, ref_stem);
        PropertyClassification {
            pg_type: pg,
            rust_type: rust,
            sea_type: sea,
            render_strategy,
            ref_target: Some(ref_path.to_string()),
            kind,
            inline_enum_values: vec![],
            projection: Some(clf.projection),
        }
    } else if let Some(enum_vals) = prop_schema.get("enum").and_then(|v| v.as_array()) {
        let vals: Vec<String> = enum_vals
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        // Build a synthetic codelist name: {Domain}{EntityName}{PropName}
        let synthetic_name = format!(
            "{}{}{}",
            domain.to_upper_camel_case(),
            strip_suffix(schema_title, suffix),
            prop_name.to_upper_camel_case(),
        );
        let kind = codegraph_type_contracts::RefClassificationKind::CodelistCheck;
        let render_strategy = classification_kind_str(&kind).to_string();
        PropertyClassification {
            pg_type: "TEXT".to_string(),
            rust_type: "String".to_string(),
            sea_type: "Text".to_string(),
            render_strategy,
            ref_target: Some(synthetic_name),
            kind,
            inline_enum_values: vals,
            projection: None,
        }
    } else if is_array {
        if let Some(items_ref) = prop_schema
            .get("items")
            .and_then(|v| v.get("$ref"))
            .and_then(|v| v.as_str())
        {
            let ref_stem = extract_ref_stem(items_ref);
            let clf = classify_ref(ref_stem, items_ref, classifier, is_entity);
            let kind_str = classification_kind_str(&clf.kind);
            let kind = clf.kind.clone();
            let (pg, rust, sea) = derive_type_strings(&clf, ref_stem);

            // Entity/VO/Codelist arrays get child_table treatment (handled by
            // DTO generator via ChildDtoContext and DDL generator via
            // CompositionNode). Other arrays get Vec<T> wrapping since the
            // template uses rust_field_type directly.
            let (render_strategy, rust, pg) = match clf.kind {
                codegraph_type_contracts::RefClassificationKind::EntityReference
                | codegraph_type_contracts::RefClassificationKind::ValueObject
                | codegraph_type_contracts::RefClassificationKind::CodelistReference
                | codegraph_type_contracts::RefClassificationKind::CodelistCheck => {
                    ("child_table".to_string(), rust, pg)
                }
                // StructuredWrapper arrays are stored as a single JSONB column
                // containing a JSON array (not JSONB[]). The DTO uses Vec<T>
                // but the DB column is plain JSONB serialized via serde_json.
                codegraph_type_contracts::RefClassificationKind::StructuredWrapper => (
                    kind_str.to_string(),
                    format!("Vec<{rust}>"),
                    pg, // keep JSONB, not JSONB[]
                ),
                _ => (
                    kind_str.to_string(),
                    format!("Vec<{rust}>"),
                    format!("{pg}[]"),
                ),
            };

            PropertyClassification {
                pg_type: pg,
                rust_type: rust,
                sea_type: sea,
                render_strategy,
                ref_target: Some(items_ref.to_string()),
                kind,
                inline_enum_values: vec![],
                projection: Some(clf.projection),
            }
        } else {
            let clf = classify_plain_type(prop_schema.get("items").unwrap_or(prop_schema));
            let render_strategy = classification_kind_str(&clf.kind).to_string();
            let kind = clf.kind.clone();
            let (pg, rust, sea) = derive_type_strings(&clf, "");
            // Inline arrays (e.g., type: array, items: {type: string}) need Vec<> wrapping
            // and pg array suffix, just like $ref arrays.
            let rust = format!("Vec<{rust}>");
            let pg = format!("{pg}[]");
            PropertyClassification {
                pg_type: pg,
                rust_type: rust,
                sea_type: sea,
                render_strategy,
                ref_target: None,
                kind,
                inline_enum_values: vec![],
                projection: Some(clf.projection),
            }
        }
    } else {
        let clf = classify_plain_type(prop_schema);
        let render_strategy = classification_kind_str(&clf.kind).to_string();
        let kind = clf.kind.clone();
        let (pg, rust, sea) = derive_type_strings(&clf, "");
        PropertyClassification {
            pg_type: pg,
            rust_type: rust,
            sea_type: sea,
            render_strategy,
            ref_target: None,
            kind,
            inline_enum_values: vec![],
            projection: Some(clf.projection),
        }
    }
}

fn classification_kind_str(kind: &codegraph_type_contracts::RefClassificationKind) -> &'static str {
    match kind {
        codegraph_type_contracts::RefClassificationKind::PrimitiveWrapper => "primitive_wrapper",
        codegraph_type_contracts::RefClassificationKind::ArrayWrapper => "array_wrapper",
        codegraph_type_contracts::RefClassificationKind::RangeWrapper => "range_wrapper",
        codegraph_type_contracts::RefClassificationKind::CodelistReference => "codelist",
        codegraph_type_contracts::RefClassificationKind::CodelistCheck => "codelist_check",
        codegraph_type_contracts::RefClassificationKind::InlineEnum => "inline_enum",
        codegraph_type_contracts::RefClassificationKind::EntityReference => "entity_reference",
        codegraph_type_contracts::RefClassificationKind::ValueObject => "value_object",
        codegraph_type_contracts::RefClassificationKind::CompositeWrapper => "composite_wrapper",
        codegraph_type_contracts::RefClassificationKind::MediaWrapper => "media_wrapper",
        codegraph_type_contracts::RefClassificationKind::StructuredWrapper => "structured_wrapper",
    }
}

fn derive_type_strings(
    clf: &codegraph_classifier::ClassificationResult,
    ref_stem: &str,
) -> (String, String, String) {
    let pg = clf
        .column_type
        .as_ref()
        .map(|ct| ct.pg_ddl())
        .unwrap_or_default();
    // StructuredWrapper: use the configured Rust type name from the projection
    // (e.g. "IdentifierType"), not the canonical JSONB type ("serde_json::Value").
    let rust = if clf.kind == codegraph_type_contracts::RefClassificationKind::StructuredWrapper {
        clf.projection.domain.rust_type.as_rust_str()
    } else if clf.column_type.is_none() && !ref_stem.is_empty() {
        ref_stem.to_string()
    } else {
        clf.column_type
            .as_ref()
            .map(|ct| ct.rust_type_str())
            .unwrap_or_default()
    };
    let sea = clf
        .column_type
        .as_ref()
        .map(|ct| ct.sea_orm_type().to_string())
        .unwrap_or_default();
    (pg, rust, sea)
}

fn extract_ref_stem(ref_path: &str) -> &str {
    let path = ref_path.strip_suffix('#').unwrap_or(ref_path);
    let filename = path.rsplit('/').next().unwrap_or(path);
    if ref_path.starts_with("#/") {
        return filename;
    }
    filename.strip_suffix(".json").unwrap_or(filename)
}

/// Pass 2: Update entity flags and re-classify property $ref targets.
/// Called after the AutoClassifier has determined which schemas are entities.
pub async fn reclassify_with_entities(
    db: &dyn GraphIngestor,
    querier: &dyn codegraph_core::traits::GraphQuerier,
    entity_names: &HashSet<String>,
) -> Result<()> {
    let is_entity =
        |name: &str| entity_names.contains(name) || entity_names.contains(&format!("{}Type", name));

    // Update schema nodes
    let schemas = querier.list_schemas(None).await.map_err(Error::Graph)?;
    for schema in &schemas {
        let should_be_entity = is_entity(&schema.title);
        if schema.is_entity != should_be_entity {
            db.update_entity_flag(&schema.title, should_be_entity)
                .await
                .map_err(Error::Graph)?;
        }
    }

    // Re-classify properties whose ref_target classification may have changed
    for schema in &schemas {
        let properties = querier
            .get_properties(&schema.title)
            .await
            .map_err(Error::Graph)?;
        for prop in &properties {
            if let Some(ref ref_target) = prop.ref_target {
                let ref_stem = extract_ref_stem(ref_target);
                let target_is_entity = is_entity(ref_stem);
                let current_is_entity_ref = prop.classification_kind
                    == Some(codegraph_type_contracts::RefClassificationKind::EntityReference);

                if target_is_entity && !current_is_entity_ref {
                    db.update_property_classification(
                        &schema.title,
                        &prop.name,
                        "entity_reference",
                    )
                    .await
                    .map_err(Error::Graph)?;
                } else if !target_is_entity && current_is_entity_ref {
                    db.update_property_classification(&schema.title, &prop.name, "value_object")
                        .await
                        .map_err(Error::Graph)?;
                }
            }
        }
    }

    Ok(())
}

/// Ingest composite range definitions from classifier config into the graph.
///
/// For each `[[composite_ranges]]` entry in classifier.toml, this:
/// 1. Creates a CompositeRange node in the graph
/// 2. Creates a CollapsesTo edge from the schema to the range
/// 3. Creates ConsumesField edges from the range to the start/end properties
async fn ingest_composite_ranges(
    db: &dyn GraphIngestor,
    classifier: &ClassifierConfig,
) -> Result<()> {
    for cr in &classifier.composite_ranges {
        let range = CompositeRange {
            pg_column_name: to_snake_case(&cr.column),
            pg_type: cr.postgres.clone(),
            rust_type: cr.rust.clone(),
            start_field: cr.start.clone(),
            end_field: cr.end.clone(),
            open_end: false,
        };
        db.ingest_composite_range(&range)
            .await
            .map_err(Error::Graph)?;

        // CollapsesTo edge: Schema → CompositeRange
        db.ingest_edge(
            &cr.schema,
            &to_snake_case(&cr.column),
            EdgeType::CollapsesTo,
            None,
        )
        .await
        .map_err(Error::Graph)?;

        // ConsumesField edges: CompositeRange → Property (start + end)
        let range_node_name = to_snake_case(&cr.column);
        db.ingest_edge(
            &range_node_name,
            &format!("{}::{}", cr.start, cr.schema),
            EdgeType::ConsumesField,
            Some(&EdgeProperties {
                role: Some("start".to_string()),
                ..Default::default()
            }),
        )
        .await
        .map_err(Error::Graph)?;
        db.ingest_edge(
            &range_node_name,
            &format!("{}::{}", cr.end, cr.schema),
            EdgeType::ConsumesField,
            Some(&EdgeProperties {
                role: Some("end".to_string()),
                ..Default::default()
            }),
        )
        .await
        .map_err(Error::Graph)?;
    }
    Ok(())
}

/// Pass 6: Ingest Extension nodes and create RequiresExtension edges.
///
/// For each extension in `classifier.required_extensions`, creates an Extension node.
/// Then determines which composite_wrapper schemas need which extensions (by checking
/// if a column's pg_type requires one), and creates RequiresExtension edges from
/// every entity schema that references such a wrapper.
async fn ingest_required_extensions(
    db: &dyn GraphIngestor,
    classifier: &ClassifierConfig,
    loader: &SchemaLoader,
    all_uris: &[String],
) -> Result<()> {
    if classifier.required_extensions.is_empty() {
        return Ok(());
    }

    // Step 1: Create Extension nodes
    for ext_name in &classifier.required_extensions {
        db.ingest_extension(ext_name).await.map_err(Error::Graph)?;
    }

    // Step 2: Build map of composite_wrapper schema → required extension names.
    // Currently: GEOMETRY → postgis. This could be extended for other types.
    let mut wrapper_extensions: HashMap<&str, Vec<&str>> = HashMap::new();
    for cw in &classifier.composite_wrappers {
        for col in &cw.columns {
            let pg_upper = col.postgres.to_uppercase();
            for ext in &classifier.required_extensions {
                let needs_ext = match ext.as_str() {
                    "postgis" => pg_upper.contains("GEOMETRY") || pg_upper.contains("GEOGRAPHY"),
                    "vector" => pg_upper.contains("VECTOR"),
                    _ => false,
                };
                if needs_ext {
                    wrapper_extensions.entry(&cw.schema).or_default().push(ext);
                }
            }
        }
    }

    if wrapper_extensions.is_empty() {
        return Ok(());
    }

    // Step 3: For each schema, check if any property references a wrapper that needs an extension
    for uri in all_uris {
        let entry = match loader.get(uri) {
            Some(e) => e,
            None => continue,
        };
        let schema_title = entry
            .schema
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or(&entry.stem);

        // Collect all $ref paths from properties (including from allOf)
        let refs = collect_all_refs(&entry.schema, loader, uri);
        for ref_path in &refs {
            let ref_stem = extract_ref_stem(ref_path);
            if let Some(ext_names) = wrapper_extensions.get(ref_stem) {
                for ext_name in ext_names {
                    db.ingest_edge(schema_title, ext_name, EdgeType::RequiresExtension, None)
                        .await
                        .map_err(Error::Graph)?;
                }
            }
        }
    }

    Ok(())
}

/// Collect all $ref paths from a schema's properties (including allOf-inherited properties).
///
/// TODO: This only resolves one level of $ref. Transitive references
/// (e.g. PersonType → AddressType → GeoType) are missed, which means
/// `ingest_required_extensions` won't create RequiresExtension edges for
/// entities that reference extension-requiring types through intermediaries.
/// The DDL generator has a safety net (`detect_extensions_from_columns`)
/// but a recursive walk here would be the proper fix.
fn collect_all_refs(
    schema: &serde_json::Value,
    loader: &SchemaLoader,
    base_uri: &str,
) -> Vec<String> {
    let mut refs = Vec::new();

    let mut prop_blocks: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();

    if let Some(props) = schema.get("properties").and_then(|v| v.as_object()) {
        prop_blocks.push(props.clone());
    }

    if let Some(all_of) = schema.get("allOf").and_then(|v| v.as_array()) {
        for item in all_of {
            if let Some(ref_path) = item.get("$ref").and_then(|v| v.as_str()) {
                if let Ok((_resolved_uri, ref_entry)) = loader.resolve_ref(ref_path, base_uri) {
                    if let Some(props) = ref_entry
                        .schema
                        .get("properties")
                        .and_then(|v| v.as_object())
                    {
                        prop_blocks.push(props.clone());
                    }
                }
            } else if let Some(props) = item.get("properties").and_then(|v| v.as_object()) {
                prop_blocks.push(props.clone());
            }
        }
    }

    for props in &prop_blocks {
        for (_name, prop_schema) in props {
            if let Some(ref_path) = prop_schema.get("$ref").and_then(|v| v.as_str()) {
                refs.push(ref_path.to_string());
            }
            // Also check items.$ref for array properties
            if let Some(items) = prop_schema.get("items") {
                if let Some(ref_path) = items.get("$ref").and_then(|v| v.as_str()) {
                    refs.push(ref_path.to_string());
                }
            }
        }
    }

    refs
}
