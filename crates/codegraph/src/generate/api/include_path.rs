use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::resolve_field;
use codegraph_core::types::SchemaNode;
use codegraph_type_contracts::RefClassificationKind;
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
    /// The target entity display name, e.g. "Worker" (from rust_type_name).
    pub entity_name: String,
    /// The canonical schema title used as the graph node key, e.g. "WorkerType".
    pub schema_title: String,
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
        .get_schema_in_domain(schema_title, domain)
        .await?
        .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

    let source_entity_name = &source_schema.rust_type_name;
    let source_module = &source_schema.pg_table_name;

    match allow_include {
        Some(paths) if paths.is_empty() => Ok(Vec::new()),
        Some(paths) => {
            resolve_explicit_paths(db, config, domain, schema_title, source_entity_name, source_module, paths).await
        }
        None => {
            resolve_auto_paths(db, config, domain, schema_title, source_entity_name, source_module).await
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
        // `current_source_title` always holds the canonical schema title so that
        // graph queries (get_referenced_schemas, get_properties) work correctly
        // at every depth level.
        let mut current_source_title: &str = schema_title;

        for &seg in &segment_strs {
            // Resolve the target schema via graph identity (schema_id).
            let target_schema = resolve_schema_target(db, current_source_title, seg, domain).await?;
            let target_title = target_schema.title.clone();

            // Skip force_value_objects — they don't have standalone entity or DTO
            // generation, so fetch methods referencing {Entity}Response would fail.
            let is_force_vo = config
                .domains
                .get(domain)
                .map(|d| d.force_value_objects.contains(&target_title))
                .unwrap_or(false);
            if is_force_vo {
                tracing::warn!(
                    "include path '{path}' targets force_value_object '{target_title}' — skipping"
                );
                break;
            }

            // Skip codelists — they have standalone enum generation but no entity
            // module or response DTO, so fetch methods referencing {Entity}Response
            // would fail.
            if target_schema.is_codelist {
                tracing::warn!(
                    "include path '{path}' targets codelist '{target_title}' — skipping (no DTO/entity)"
                );
                break;
            }

            // Skip non-entity types — they don't have standalone entity generation
            // or response DTOs, so include paths referencing them would fail.
            if !target_schema.is_entity {
                tracing::warn!(
                    "include path '{path}' targets non-entity '{target_title}' — skipping"
                );
                break;
            }

            let target_entity_name = target_schema.rust_type_name.clone();
            let target_schema_title = target_schema.title.clone();
            let target_module = target_schema.pg_table_name.clone();
            let target_domain = target_schema
                .domain
                .clone()
                .unwrap_or_else(|| domain.to_string());
            let target_table = format!("\"{}\".\"{}\"", target_domain, target_module);

            // Resolve FK column and array flag via graph query — uses
            // db.get_properties() which runs GQL internally.
            let (fk_column, is_array) =
                resolve_fk_via_graph(db, current_source_title, &target_title, seg).await?;

            let source_entity_name = super::router::strip_suffix(current_source_title, &config.defaults.type_suffix);
            let (reverse_fk_column, _) = resolve_fk_via_graph(
                db, &target_title, current_source_title, &source_entity_name,
            ).await?;

            segments.push(IncludeSegment {
                entity_name: target_entity_name,
                schema_title: target_schema_title,
                module_name: target_module,
                domain: target_domain,
                table: target_table,
                fk_column,
                reverse_fk_column,
                is_array,
            });

            // Use the canonical schema title for the next iteration so graph
            // queries at depth ≥ 2 resolve correctly.
            current_source_title = &segments.last().unwrap().schema_title;
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
) -> Result<Vec<ResolvedIncludePath>> {
    let mut paths: Vec<ResolvedIncludePath> = Vec::new();

    // Source 1: Children from parent_candidates.
    let parent_candidates = db.get_parent_candidates().await?;
    for pc in &parent_candidates {
        if pc.parent_title != schema_title {
            continue;
        }
        let target_title = &pc.child_title;
        let Some(target_schema) = db.get_schema_in_domain(target_title, domain).await? else {
            continue;
        };
        // Skip child entities and inline definitions — they don't have standalone
        // entity .rs files, so repository code referencing crate::entity::<module>::
        // would fail with E0583.
        if target_schema.parent_schema.is_some() {
            continue;
        }
        // Skip force_value_objects — they won't have standalone entity generation.
        let is_force_vo = config
            .domains
            .get(domain)
            .map(|d| d.force_value_objects.contains(target_title))
            .unwrap_or(false);
        if is_force_vo {
            continue;
        }
        // Skip codelists — no standalone entity generation.
        if target_schema.is_codelist {
            continue;
        }
        // Skip non-entity types — no standalone entity generation.
        if !target_schema.is_entity {
            continue;
        }
        let target_module = target_schema.pg_table_name.clone();
        let target_schema_title = target_schema.title.clone();
        let target_entity_name = target_schema.rust_type_name.clone();
        let target_domain = target_schema
            .domain
            .clone()
            .unwrap_or_else(|| domain.to_string());
        let target_table = format!("\"{}\".\"{}\"", target_domain, target_module);

        // Resolve FK column from the child entity's domain config (parent_ref)
        // or from graph properties, falling back to convention-based naming.
        // Both fk_column and reverse_fk_column resolve to the same FK on the
        // child entity that references the parent.
        let fk_column = resolve_child_fk_column(config, domain, target_title, schema_title, db).await?;
        let reverse_fk_column = fk_column.clone();

        let alias_seg = codegraph_naming::to_snake_case(super::router::strip_suffix(
            target_title,
            &config.defaults.type_suffix,
        ));

        // Resolve is_array from the graph: does the parent have an array property
        // pointing to this child via ItemsOf?
        let is_array = {
            let props = db.get_properties(schema_title).await.unwrap_or_default();
            props.iter().any(|p| p.is_array && p.effective_kind() == Some(RefClassificationKind::ValueObject))
                || true
        };

        paths.push(ResolvedIncludePath {
            alias: alias_seg.clone(),
            segments: vec![IncludeSegment {
                entity_name: target_entity_name,
                schema_title: target_schema_title,
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
                .map(|s| s.schema_title.clone())
        }).collect();

    for ref_schema in &referenced {
        let ref_title = &ref_schema.title;
        if ref_title == schema_title {
            continue;
        }
        if existing_entity_names.contains(ref_title) {
            continue;
        }
        let Some(target_schema) = db.get_schema_in_domain(ref_title, domain).await? else {
            continue;
        };
        if target_schema.pg_table_name.is_empty() {
            continue;
        }
        // Skip child entities and inline definitions — they don't have standalone
        // entity .rs files, so repository code referencing crate::entity::<module>::
        // would fail with E0583.
        if target_schema.parent_schema.is_some() {
            continue;
        }
        // Skip force_value_objects — they won't have standalone entity generation.
        let is_force_vo = config
            .domains
            .get(domain)
            .map(|d| d.force_value_objects.contains(ref_title))
            .unwrap_or(false);
        if is_force_vo {
            continue;
        }
        // Skip codelists — no standalone entity generation.
        if target_schema.is_codelist {
            continue;
        }
        // Skip non-entity types — no standalone entity generation.
        if !target_schema.is_entity {
            continue;
        }
        let target_entity_name = target_schema.rust_type_name.clone();
        let target_schema_title = target_schema.title.clone();
        let target_module = target_schema.pg_table_name.clone();
        let target_domain = target_schema
            .domain
            .clone()
            .unwrap_or_else(|| domain.to_string());
        let target_table = format!("\"{}\".\"{}\"", target_domain, target_module);

        // Resolve FK property via graph query.
        let ref_entity_name =
            super::router::strip_suffix(ref_title, &config.defaults.type_suffix);
        let (fk_column, is_array) = resolve_fk_via_graph(
            db, schema_title, ref_title,
            &codegraph_naming::to_snake_case(ref_entity_name),
        ).await?;

        let source_entity_name = super::router::strip_suffix(schema_title, &config.defaults.type_suffix);
        let (reverse_fk_column, _) = resolve_fk_via_graph(
            db, ref_title, schema_title, &source_entity_name,
        ).await?;

        let alias_seg = codegraph_naming::to_snake_case(ref_entity_name);

        paths.push(ResolvedIncludePath {
            alias: alias_seg.clone(),
            segments: vec![IncludeSegment {
                entity_name: target_entity_name,
                schema_title: target_schema_title,
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

/// Resolve a segment string to a target schema node using graph identity
/// (schema_id) for cross-domain collision safety.
///
/// Primary path: query the graph for schemas referenced by the current source
/// via `HasProperty → ReferencesSchema` edges. This is authoritative because
/// the graph stores the actual `$ref` relationships from ingestion.
///
/// Fallback: PascalCase naming convention for schemas not yet linked by refs.
async fn resolve_schema_target(
    db: &dyn GraphQuerier,
    current_source_title: &str,
    seg: &str,
    domain: &str,
) -> Result<SchemaNode> {
    let seg_lower = seg.to_lowercase();

    // 1. Query the graph for schemas the current source actually references.
    if let Ok(refs) = db.get_referenced_schemas(current_source_title).await {
        for ref_schema in &refs {
            let stripped = ref_schema
                .title
                .strip_suffix("Type")
                .unwrap_or(&ref_schema.title)
                .to_lowercase();
            if stripped == seg_lower {
                if let Some(auth_node) = db.get_schema_by_id(&ref_schema.schema_id).await? {
                    return Ok(auth_node);
                }
            }
        }
    }

    // 2. Also check ItemsOf references (array items the source holds).
    //    These are discovered via the parent_candidates query (one-to-many direction).
    if let Ok(candidates) = db.get_parent_candidates().await {
        for pc in &candidates {
            if pc.parent_title == current_source_title {
                let child_stripped = pc.child_title
                    .strip_suffix("Type")
                    .unwrap_or(&pc.child_title)
                    .to_lowercase();
                if child_stripped == seg_lower {
                    if let Some(node) = db.get_schema_in_domain(&pc.child_title, domain).await? {
                        if let Some(auth_node) = db.get_schema_by_id(&node.schema_id).await? {
                            return Ok(auth_node);
                        }
                        return Ok(node);
                    }
                }
            }
        }
    }

    // 3. Fallback: try PascalCase naming convention.
    let pascal = codegraph_naming::to_pascal_case(seg);
    let candidates = [format!("{pascal}Type"), pascal.clone()];
    for title in &candidates {
        if let Ok(Some(node)) = db.get_schema_in_domain(title, domain).await {
            if let Some(auth_node) = db.get_schema_by_id(&node.schema_id).await? {
                return Ok(auth_node);
            }
            return Ok(node);
        }
    }

    Err(crate::error::Error::RefResolution(format!(
        "cannot resolve include segment '{seg}' from '{current_source_title}'"
    )))
}

/// Resolve the FK column and array flag for a source→target relationship
/// by querying the source entity's properties from the graph.
///
/// Uses `db.get_properties()` which runs GQL internally (`HasProperty` edges),
/// then matches properties by `ref_target` or field name.  This is the same
/// pattern used by `build_composition_node()` in the Grafeo querier.
async fn resolve_fk_via_graph(
    db: &dyn GraphQuerier,
    source_title: &str,
    target_title: &str,
    seg: &str,
) -> Result<(String, bool)> {
    let seg_snake = codegraph_naming::to_snake_case(seg);
    let source_props = db.get_properties(source_title).await.unwrap_or_default();

    // Priority 1: property whose ref_target matches target_title (exact).
    for prop in &source_props {
        let matches = prop.ref_target.as_deref().map(|rt| {
            // Handle both plain title refs ("PersonType") and path refs
            // ("common/json/person/PersonType.json").
            let rt_clean = rt.rsplit('/').next().unwrap_or(rt)
                .strip_suffix(".json#").or_else(|| rt.strip_suffix(".json"))
                .unwrap_or(rt);
            rt_clean == target_title
        }).unwrap_or(false);
        if matches {
            let fd = resolve_field(prop);
            return Ok((fd.column_name, prop.is_array));
        }
    }

    // Priority 2: property whose name or rust_field_name matches the segment.
    for prop in &source_props {
        if prop.name.to_lowercase() == seg_snake
            || prop.rust_field_name.to_lowercase() == seg_snake
        {
            let fd = resolve_field(prop);
            return Ok((fd.column_name, prop.is_array));
        }
    }

    // Priority 3: property whose pg_column_name is "{seg}_id".
    let seg_id = format!("{seg_snake}_id");
    for prop in &source_props {
        if prop.pg_column_name.to_lowercase() == seg_id {
            let fd = resolve_field(prop);
            return Ok((fd.column_name, prop.is_array));
        }
    }

    // Fallback: convention-based default.
    Ok((seg_id, false))
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

/// Resolve the FK column on a child entity that references its parent.
/// Priority: 1) domain config `parent_ref`, 2) graph properties, 3) convention.
async fn resolve_child_fk_column(
    config: &codegraph_config::DomainConfig,
    domain: &str,
    child_title: &str,
    parent_title: &str,
    db: &dyn GraphQuerier,
) -> Result<String> {
    // Priority 1: parent_ref from the child entity's domain config.
    if let Some(fk) = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(child_title))
        .and_then(|ec| ec.parent_ref.clone())
    {
        return Ok(fk);
    }

    // Priority 2: graph properties — find the property on the child that
    // references the parent.
    let seg = codegraph_naming::to_snake_case(
        super::router::strip_suffix(child_title, &config.defaults.type_suffix),
    );
    let (fk, _) = resolve_fk_via_graph(db, child_title, parent_title, &seg).await?;
    if !fk.ends_with("_id") {
        // The resolved column name doesn't look like an FK — fall back.
        return Ok(format!(
            "{}_id",
            codegraph_naming::to_snake_case(
                super::router::strip_suffix(child_title, &config.defaults.type_suffix),
            )
        ));
    }
    Ok(fk)
}
