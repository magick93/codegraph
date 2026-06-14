use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;

/// A fully resolved include path ready for DTO/repository code generation.
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedIncludePath {
    /// Full dot-notation alias, e.g. "person" or "deployment.position"
    pub alias: String,
    /// Dot-delimited segments, resolved against the graph
    pub segments: Vec<IncludeSegment>,
    /// Generated DTO type name for this path's response, e.g. "DeploymentWithPositionResponse".
    /// For single-segment paths this matches the target entity's Response type.
    /// For multi-segment paths this is a generated enriched type name.
    pub response_rust_type: String,
    /// The method name generated in the repository, e.g. "fetch_person_for_worker"
    pub fetch_method: String,
    /// The method name generated for list batch-fetch, e.g. "fetch_person_batch_for_worker"
    pub batch_fetch_method: String,
}

/// A single segment in an include path chain.
#[derive(Debug, Clone, Serialize)]
pub struct IncludeSegment {
    /// The target entity name (with Type suffix), e.g. "PersonType"
    pub entity_name: String,
    /// Rust-safe module name, e.g. "person"
    pub module_name: String,
    /// Domain name, e.g. "common"
    pub domain: String,
    /// Schema-qualified table name, e.g. "\"common\".\"person\""
    pub table: String,
    /// FK column on the source entity's table that references this target,
    /// e.g. "person_id" on the worker table
    pub fk_column: String,
    /// FK column on this target's table that references back to the source,
    /// e.g. "worker_id" on the person table
    pub reverse_fk_column: String,
    /// Whether this is a one-to-many relationship (Vec) vs one-to-one (Option)
    pub is_array: bool,
}

/// Resolve include paths from configuration against graph data.
/// Returns resolved paths with FK columns, table names, and generated type names.
pub async fn resolve_include_paths(
    db: &dyn GraphQuerier,
    config: &codegraph_config::DomainConfig,
    domain: &str,
    schema_title: &str,
    allow_include: Option<&Vec<String>>,
) -> Result<Vec<ResolvedIncludePath>> {
    let source_schema = db
        .get_schema(schema_title)
        .await?
        .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

    let source_entity_name = &source_schema.rust_type_name;
    let source_module = &source_schema.pg_table_name;

    // Bulk-fetch all properties for FK lookups.
    let all_props = db.list_all_properties().await?;

    match allow_include {
        Some(paths) if paths.is_empty() => Ok(Vec::new()),
        Some(paths) => {
            resolve_explicit_paths(
                db, config, domain, schema_title, source_entity_name, source_module, &all_props,
                paths,
            )
            .await
        }
        None => {
            resolve_auto_paths(
                db, config, domain, schema_title, source_entity_name, source_module, &all_props,
            )
            .await
        }
    }
}

// ── Explicit path resolution ──────────────────────────────────────────

async fn resolve_explicit_paths(
    db: &dyn GraphQuerier,
    config: &codegraph_config::DomainConfig,
    domain: &str,
    schema_title: &str,
    _source_entity_name: &str,
    source_module: &str,
    all_props: &std::collections::HashMap<String, Vec<codegraph_core::types::PropertyNode>>,
    paths: &[String],
) -> Result<Vec<ResolvedIncludePath>> {
    let mut resolved = Vec::new();

    for path in paths {
        let segment_strs: Vec<&str> = path.split('.').collect();
        if segment_strs.len() > 3 {
            tracing::warn!(
                "include path '{path}' exceeds max depth of 3 — skipping"
            );
            continue;
        }

        let mut segments = Vec::new();
        let mut current_source_title: &str = schema_title;

        for &seg in &segment_strs {
            // Resolve the target schema title from the segment string.
            let target_title = resolve_target_title(db, current_source_title, seg).await?;
            let target_schema = db
                .get_schema(&target_title)
                .await?
                .ok_or_else(|| crate::error::Error::SchemaNotFound(target_title.clone()))?;

            let target_entity_name = target_schema.rust_type_name.clone();
            let target_module = target_schema.pg_table_name.clone();
            let target_domain = target_schema
                .domain
                .clone()
                .unwrap_or_else(|| domain.to_string());
            let target_table = format!("\"{}\".\"{}\"", target_domain, target_module);

            // Find the FK column and is_array from the source entity's properties.
            let source_props = all_props
                .get(current_source_title)
                .map(|v| v.as_slice())
                .unwrap_or_default();
            let (fk_column, is_array) =
                resolve_fk_for_target(source_props, current_source_title, &target_title, seg);

            let reverse_fk_column = format!("{}_id", codegraph_naming::to_snake_case(
                super::router::strip_suffix(current_source_title, &config.defaults.type_suffix),
            ));

            segments.push(IncludeSegment {
                entity_name: target_entity_name,
                module_name: target_module,
                domain: target_domain,
                table: target_table,
                fk_column,
                reverse_fk_column,
                is_array,
            });

            current_source_title = segments.last().unwrap().entity_name.as_str();
        }

        let alias_snake = path.replace('.', "_");
        let response_rust_type = derive_response_type(&segments);
        let fetch_method = format!("fetch_{alias_snake}_for_{source_module}");
        let batch_fetch_method = format!("fetch_{alias_snake}_batch_for_{source_module}");

        resolved.push(ResolvedIncludePath {
            alias: path.clone(),
            segments,
            response_rust_type,
            fetch_method,
            batch_fetch_method,
        });
    }

    Ok(resolved)
}

// ── Auto-discover path resolution ─────────────────────────────────────

async fn resolve_auto_paths(
    db: &dyn GraphQuerier,
    config: &codegraph_config::DomainConfig,
    domain: &str,
    schema_title: &str,
    source_entity_name: &str,
    source_module: &str,
    all_props: &std::collections::HashMap<String, Vec<codegraph_core::types::PropertyNode>>,
) -> Result<Vec<ResolvedIncludePath>> {
    let mut paths: Vec<ResolvedIncludePath> = Vec::new();

    // Source 1: Children from parent_candidates.
    let parent_candidates = db.get_parent_candidates().await?;
    for pc in &parent_candidates {
        if pc.parent_title != schema_title {
            continue;
        }
        let target_title = &pc.child_title;
        let Some(target_schema) = db.get_schema(target_title).await? else {
            continue;
        };
        let target_module = target_schema.pg_table_name.clone();
        let target_domain = target_schema
            .domain
            .clone()
            .unwrap_or_else(|| domain.to_string());
        let target_table = format!("\"{}\".\"{}\"", target_domain, target_module);

        // Children: no FK on parent; default is array=true.
        let fk_column = format!("{}_id", codegraph_naming::to_snake_case(
            super::router::strip_suffix(target_title, &config.defaults.type_suffix),
        ));
        let reverse_fk_column = format!(
            "{}_id",
            codegraph_naming::to_snake_case(super::router::strip_suffix(
                schema_title,
                &config.defaults.type_suffix,
            ))
        );

        let alias_seg = codegraph_naming::to_snake_case(super::router::strip_suffix(
            target_title,
            &config.defaults.type_suffix,
        ));

        // Resolve is_array from the array-item direction (parent has array prop → ItemsOf → child).
        let prop_is_array = all_props
            .get(schema_title)
            .map(|props| {
                props.iter().any(|p| {
                    p.is_array
                        && p
                            .ref_target
                            .as_deref()
                            .map(|rt| rt == target_title.as_str())
                            .unwrap_or(false)
                })
            })
            .unwrap_or(false);

        paths.push(ResolvedIncludePath {
            alias: alias_seg.clone(),
            segments: vec![IncludeSegment {
                entity_name: target_title.clone(),
                module_name: target_module,
                domain: target_domain,
                table: target_table,
                fk_column,
                reverse_fk_column,
                is_array: prop_is_array || true,
            }],
            response_rust_type: format!("{}Response", target_schema.rust_type_name),
            fetch_method: format!("fetch_{alias_seg}_for_{source_module}"),
            batch_fetch_method: format!("fetch_{alias_seg}_batch_for_{source_module}"),
        });
    }

    // Source 2: Entity references (cross-refs from referenced_schemas).
    let schema_title_with_type = format!("{source_entity_name}Type");
    let referenced = db
        .get_referenced_schemas(&schema_title_with_type)
        .await
        .unwrap_or_default();

    // Collect already-discovered entity names to avoid duplicates.
    let existing_entity_names: std::collections::HashSet<String> =
        paths.iter().flat_map(|p| {
            p.segments
                .iter()
                .map(|s| s.entity_name.clone())
        }).collect();

    for ref_title in &referenced {
        if ref_title == schema_title {
            continue;
        }
        if existing_entity_names.contains(ref_title) {
            continue;
        }
        let Some(target_schema) = db.get_schema(ref_title).await? else {
            continue;
        };
        if target_schema.pg_table_name.is_empty() {
            continue;
        }
        let target_module = target_schema.pg_table_name.clone();
        let target_domain = target_schema
            .domain
            .clone()
            .unwrap_or_else(|| domain.to_string());
        let target_table = format!("\"{}\".\"{}\"", target_domain, target_module);

        // Find the FK property on source entity.
        let source_props = all_props
            .get(schema_title)
            .map(|v| v.as_slice())
            .unwrap_or_default();
        let ref_entity_name =
            super::router::strip_suffix(ref_title, &config.defaults.type_suffix);
        let (fk_column, is_array) = resolve_fk_for_target(
            source_props,
            schema_title,
            ref_title,
            &codegraph_naming::to_snake_case(ref_entity_name),
        );

        let reverse_fk_column = format!("{}_id", codegraph_naming::to_snake_case(
            super::router::strip_suffix(schema_title, &config.defaults.type_suffix),
        ));

        let alias_seg = codegraph_naming::to_snake_case(ref_entity_name);

        paths.push(ResolvedIncludePath {
            alias: alias_seg.clone(),
            segments: vec![IncludeSegment {
                entity_name: ref_title.clone(),
                module_name: target_module,
                domain: target_domain,
                table: target_table,
                fk_column,
                reverse_fk_column,
                is_array,
            }],
            response_rust_type: format!("{}Response", target_schema.rust_type_name),
            fetch_method: format!("fetch_{alias_seg}_for_{source_module}"),
            batch_fetch_method: format!("fetch_{alias_seg}_batch_for_{source_module}"),
        });
    }

    Ok(paths)
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Resolve a segment string to a target schema title by checking:
///
/// 1. PascalCase(seg) + "Type" (the standard entity suffix)
/// 2. PascalCase(seg) directly
/// 3. Any entity referenced by `current_source_title` whose stripped name matches `seg`
async fn resolve_target_title(
    db: &dyn GraphQuerier,
    current_source_title: &str,
    seg: &str,
) -> Result<String> {
    let pascal = codegraph_naming::to_pascal_case(seg);

    let candidates = [format!("{pascal}Type"), pascal.clone()];
    for title in &candidates {
        if let Ok(Some(_)) = db.get_schema(title).await {
            return Ok(title.clone());
        }
    }

    // Fallback: scan referenced schemas of the source for a name match.
    // Check both the raw source title and "{source}Type" if not already typed.
    let ref_sources = [
        current_source_title.to_string(),
        format!("{pascal}Type"),
    ];
    for ref_source in &ref_sources {
        if let Ok(refs) = db.get_referenced_schemas(ref_source).await {
            let seg_lower = seg.to_lowercase();
            for r in &refs {
                let stripped = r
                    .strip_suffix("Type")
                    .unwrap_or(r)
                    .to_lowercase();
                if stripped == seg_lower {
                    return Ok(r.clone());
                }
            }
        }
    }

    Err(crate::error::Error::RefResolution(format!(
        "cannot resolve include segment '{seg}' from '{current_source_title}'"
    )))
}

/// Inspect the source entity's properties to find the FK column and array flag
/// for a relationship to `target_title`.  Falls back to `{seg}_id` / no array
/// when no matching property is found.
fn resolve_fk_for_target(
    source_props: &[codegraph_core::types::PropertyNode],
    _source_title: &str,
    target_title: &str,
    seg: &str,
) -> (String, bool) {
    let seg_snake = codegraph_naming::to_snake_case(seg);

    // Priority 1: property whose ref_target exactly matches target_title.
    for prop in source_props {
        if prop.ref_target.as_deref() == Some(target_title) {
            return (prop.pg_column_name.clone(), prop.is_array);
        }
    }

    // Priority 2: property whose name or rust_field_name matches the segment.
    for prop in source_props {
        if prop.name.to_lowercase() == seg_snake
            || prop.rust_field_name.to_lowercase() == seg_snake
        {
            return (prop.pg_column_name.clone(), prop.is_array);
        }
    }

    // Priority 3: property whose pg_column_name is "{seg}_id".
    let seg_id = format!("{seg_snake}_id");
    for prop in source_props {
        if prop.pg_column_name.to_lowercase() == seg_id {
            return (prop.pg_column_name.clone(), prop.is_array);
        }
    }

    // Fallback: convention-based default.
    (seg_id, false)
}

/// Derive the response Rust type name for a resolved include path.
///
/// - Single segment: `{TargetEntity}Response`
/// - Multi segment:  `{FirstEntity}With{LastEntity}Response`
fn derive_response_type(segments: &[IncludeSegment]) -> String {
    if segments.len() == 1 {
        format!("{}Response", segments[0].entity_name)
    } else {
        format!(
            "{}With{}Response",
            segments[0].entity_name,
            segments.last().unwrap().entity_name,
        )
    }
}
