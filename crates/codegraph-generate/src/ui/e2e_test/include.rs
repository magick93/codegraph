use std::collections::HashSet;

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

use crate::ProjectConfig;
use crate::api::api_model::resolve_path_segment_with_config;
use crate::error::Result;

use super::context::{E2eIncludeConfig, IncludeSetupStep, IncludeTestPath};
use super::fixtures::build_test_data_json;

/// Build E2E include test configuration from resolved include paths.
/// Creates setup steps (entity creation in dependency order) and test path info.
pub(super) async fn resolve_e2e_include_config(
    db: &dyn GraphQuerier,
    _config: &DomainConfig,
    domain: &str,
    schema_title: &str,
    include_paths: &[crate::api::include_path::ResolvedIncludePath],
    has_list: bool,
    _project: &ProjectConfig,
) -> Result<Option<E2eIncludeConfig>> {
    let mut all_steps: Vec<IncludeSetupStep> = Vec::new();
    let mut test_paths: Vec<IncludeTestPath> = Vec::new();
    let mut seen_deps: HashSet<String> = HashSet::new();

    // Collect FK map for the main entity, deduplicated by FK column name
    let mut main_fk_map: Vec<[String; 2]> = Vec::new();
    let mut seen_main_fk_cols: HashSet<String> = HashSet::new();

    for path in include_paths {
        let mut prev_dep_id: Option<String> = None;
        // fk_column of the previously processed (deeper) segment — this is the FK
        // column on the CURRENT segment's entity pointing to the deeper entity.
        let mut prev_fk_column: Option<String> = None;

        // Process segments in REVERSE order (leaf entity first)
        for (seg_idx, seg) in path.segments.iter().enumerate().rev() {
            let dep_id = format!("{}_{}", seg.module_name, seg_idx);

            if seen_deps.contains(&dep_id) {
                prev_dep_id = Some(dep_id);
                prev_fk_column = Some(seg.fk_column.clone());
                continue;
            }
            seen_deps.insert(dep_id.clone());

            // Resolve the target schema for api_path using the canonical schema_title.
            let target_schema = db
                .get_schema_in_domain(&seg.schema_title, domain)
                .await?
                .ok_or_else(|| crate::error::Error::SchemaNotFound(seg.schema_title.clone()))?;
            let api_path = format!(
                "/{}/{}",
                seg.domain,
                resolve_path_segment_with_config(None, &target_schema, _config)
            );

            let fields_json =
                build_test_data_json(db, &seg.schema_title, Some(&seg.domain), _config).await;

            // FK map: this entity has a FK to the previously created (deeper) entity.
            // The FK column is the fk_column of the deeper segment — it describes
            // the column on this entity's table that references the deeper entity.
            let mut fk_map: Vec<[String; 2]> = Vec::new();
            if let Some(ref prev_id) = prev_dep_id
                && let Some(ref fk_col) = prev_fk_column
            {
                fk_map.push([fk_col.clone(), prev_id.clone()]);
            }

            all_steps.push(IncludeSetupStep {
                dep_id: dep_id.clone(),
                api_path,
                fields_json,
                fk_map,
            });

            prev_dep_id = Some(dep_id);
            prev_fk_column = Some(seg.fk_column.clone());
        }

        // Record the test path
        if let Some(_first_seg) = path.segments.first() {
            let last_idx = path.segments.len() - 1;
            let last_seg = &path.segments[last_idx];
            let target_dep_id = format!("{}_{}", last_seg.module_name, last_idx);

            test_paths.push(IncludeTestPath {
                alias: path.alias.clone(),
                target_dep_id,
                is_dot_path: path.segments.len() > 1,
                is_array: last_seg.is_array,
            });
        }

        // Add main entity FK to the first segment of this path (deduplicated)
        if let Some(first_seg) = path.segments.first() {
            let first_dep_id = format!("{}_{}", first_seg.module_name, 0);
            if seen_main_fk_cols.insert(first_seg.fk_column.clone()) {
                main_fk_map.push([first_seg.fk_column.clone(), first_dep_id]);
            }
        }
    }

    if test_paths.is_empty() {
        return Ok(None);
    }

    // Add main entity as the LAST setup step
    let source_schema = db
        .get_schema_in_domain(schema_title, domain)
        .await?
        .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;
    let main_dep_id = source_schema.pg_table_name.clone();

    if !seen_deps.contains(&main_dep_id) {
        let main_api_path = format!(
            "/{}/{}",
            domain,
            resolve_path_segment_with_config(None, &source_schema, _config)
        );
        let main_fields = build_test_data_json(db, schema_title, Some(domain), _config).await;

        all_steps.push(IncludeSetupStep {
            dep_id: main_dep_id.clone(),
            api_path: main_api_path,
            fields_json: main_fields,
            fk_map: main_fk_map,
        });
    }

    let has_multi = test_paths.len() >= 2;

    Ok(Some(E2eIncludeConfig {
        setup_steps: all_steps,
        main_entity_id_ref: main_dep_id,
        test_paths,
        has_multi_include: has_multi,
        test_list_include: has_list,
    }))
}
