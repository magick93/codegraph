use std::collections::HashSet;

use codegraph_config::{DomainConfig, SearchConfig};
use codegraph_core::traits::GraphQuerier;

use crate::db::ddl::types::{
    ChildTableDef, ColumnDef, EmbeddingContext, FtsColumnWeight, FtsContext,
};

pub(in crate::db::ddl) fn apply_workflow_defaults(
    config: &DomainConfig,
    domain: &str,
    schema_title: &str,
    columns: &mut [ColumnDef],
) -> bool {
    // Check if this entity has a workflow config
    let workflow_cfg = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(schema_title))
        .and_then(|ec| ec.workflow.as_ref());

    let has_workflow = workflow_cfg
        .map(|wf| wf.generate_action_endpoints)
        .unwrap_or(false);

    // Issue #311: the workflow plane owns the status field — the create DTO
    // excludes it, so every API INSERT omits the column and the row used to
    // land with a NULL status (list badges/chips rendered nothing until the
    // first transition). Emit the initial_state as the DB DEFAULT regardless
    // of column nullability, so creates materialize the workflow at its
    // initial state and both pipelines' list badges work on created rows
    // (matching the details/form view, which reads the {id}/workflow
    // endpoint). Single source: WorkflowConfig.initial_state. Explicit
    // schema/config defaults win. Fresh generates only — pre-existing
    // databases get no backfill; their NULL-status rows stay as they are.
    if let Some(wf) = workflow_cfg {
        for col in columns.iter_mut() {
            if col.name == wf.status_field && col.default.is_none() {
                col.default = Some(pg_string_default(&wf.initial_state));
            }
        }
    }

    has_workflow
}

/// A single-quoted SQL string literal for a column DEFAULT, with embedded
/// single quotes doubled. Workflow state names are identifiers in practice;
/// the escaping keeps a hostile config from breaking out of the literal.
fn pg_string_default(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Mapping from Postgres extension name to the column type patterns that require it.
const EXTENSION_TYPE_PATTERNS: &[(&str, &[&str])] = &[
    ("postgis", &["GEOMETRY", "GEOGRAPHY"]),
    ("vector", &["VECTOR"]),
];

/// Scan column pg_type values and return any Postgres extensions they require.
/// Generator-side safety net for transitive references the ingestion pass may miss.
pub(in crate::db::ddl) fn detect_extensions_from_columns(
    columns: &[ColumnDef],
    child_tables: &[ChildTableDef],
) -> Vec<String> {
    let mut found = Vec::new();
    let mut check = |pg_type: &str| {
        let upper = pg_type.to_uppercase();
        for &(ext, patterns) in EXTENSION_TYPE_PATTERNS {
            if patterns.iter().any(|p| upper.contains(p))
                && !found.iter().any(|s: &String| s == ext)
            {
                found.push(ext.to_string());
            }
        }
    };
    for col in columns {
        check(&col.pg_type);
    }
    for child in child_tables {
        for col in &child.columns {
            check(&col.pg_type);
        }
    }
    found
}

/// System-managed columns that should never be included in full-text search.
const FTS_EXCLUDED_COLUMNS: &[&str] =
    &["id", "platform_organization_id", "created_at", "updated_at"];

/// Build FTS context from search config and auto-discovered TEXT columns.
///
/// When `fts_columns` is `None` in config, auto-discovers all TEXT data columns.
/// When `fts_columns` is `Some([])` (explicit empty), FTS is disabled.
/// When `fts_columns` is `Some([...])`, uses the explicit list.
pub(super) fn build_fts_context(
    search_config: Option<&SearchConfig>,
    columns: &[ColumnDef],
    table_name: &str,
) -> Option<FtsContext> {
    let defaults = SearchConfig::default();
    let cfg = search_config.unwrap_or(&defaults);

    let column_names: HashSet<&str> = columns.iter().map(|c| c.name.as_str()).collect();

    let fts_columns: Vec<String> = match &cfg.fts_columns {
        // Explicit empty = FTS disabled
        Some(cols) if cols.is_empty() => return None,
        // Explicit list — filter to columns that actually exist in this table
        Some(cols) => cols
            .iter()
            .filter(|c| column_names.contains(c.as_str()))
            .cloned()
            .collect(),
        // When fts_weights are specified but fts_columns is None, use the weight
        // keys as the FTS columns — but only those that exist in this table
        None if !cfg.fts_weights.is_empty() => cfg
            .fts_weights
            .keys()
            .filter(|c| column_names.contains(c.as_str()))
            .cloned()
            .collect(),
        // Auto-discover: all TEXT columns that aren't system-managed or FKs
        None => columns
            .iter()
            .filter(|c| {
                c.pg_type == "TEXT"
                    && !c.is_primary_key
                    && !c.is_array
                    && !FTS_EXCLUDED_COLUMNS.contains(&c.name.as_str())
                    && !c.name.ends_with("_id")
                    && !c.name.ends_with("_code")
            })
            .map(|c| c.name.clone())
            .collect(),
    };

    if fts_columns.is_empty() {
        return None;
    }

    let weighted_columns: Vec<FtsColumnWeight> = fts_columns
        .iter()
        .map(|col| {
            let weight = cfg
                .fts_weights
                .get(col)
                .cloned()
                .unwrap_or_else(|| "D".to_string());
            FtsColumnWeight {
                column: col.clone(),
                weight,
            }
        })
        .collect();

    Some(FtsContext {
        tsvector_column: "search_tsv".to_string(),
        language: cfg.fts_language.clone(),
        weighted_columns,
        index_name: format!("idx_{}_search_tsv", table_name),
    })
}

/// Build embedding contexts from explicit search config.
/// Embedding columns are never auto-discovered — must be explicitly configured.
pub(super) fn build_embedding_contexts(
    search_config: Option<&SearchConfig>,
    table_name: &str,
) -> Vec<EmbeddingContext> {
    let cfg = match search_config {
        Some(c) if !c.embedding_columns.is_empty() => c,
        _ => return Vec::new(),
    };

    cfg.embedding_columns
        .iter()
        .map(|col| EmbeddingContext {
            source_column: col.clone(),
            vector_column: format!("{}_embedding", col),
            dimensions: cfg.embedding_dimensions,
            index_name: format!("idx_{}_{}_embedding", table_name, col),
        })
        .collect()
}

pub(super) fn is_global_entity(_table_name: &str, _config: &DomainConfig) -> bool {
    // TODO: check tenancy config for global tables
    false
}

/// Property-derived dto fields with no backing column in the DDL context —
/// the audit-only-skeleton signature (#446: DDL/entity resolve fewer
/// properties than dto/repository/command/query, and the emitted
/// repository reads columns the table and model lack).
///
/// Exclusions mirror planes that legitimately produce no column on this
/// table: `id` (hardcoded primary key), hierarchy fields (synthetic
/// self-referential FK, added by `add_hierarchy_artifacts`), and array
/// entity references (junction child tables). Codelist fields strip a
/// trailing `_code` on the Rust side (`status_code` column ↔ `status`
/// field), so the suffixed column name is accepted too.
pub(crate) fn ddl_dto_gap_fields(
    ddl_columns: &[ColumnDef],
    dto_fields: &[crate::ddd::dto::DtoField],
) -> Vec<String> {
    let column_names: HashSet<&str> = ddl_columns.iter().map(|c| c.name.as_str()).collect();
    let mut gaps = Vec::new();
    for field in dto_fields {
        if field.name == "id" || field.is_hierarchy_field {
            continue;
        }
        if field.is_array && field.is_entity_ref {
            continue;
        }
        if column_names.contains(field.name.as_str()) {
            continue;
        }
        let code_suffixed = format!("{}_code", field.name);
        if column_names.contains(code_suffixed.as_str()) {
            continue;
        }
        gaps.push(field.name.clone());
    }
    gaps
}

/// Loud advisory wiring for [`ddl_dto_gap_fields`] (debug builds only —
/// release runs skip the extra dto-context resolution).
#[cfg(debug_assertions)]
pub(super) async fn warn_on_ddl_dto_gap(
    db: &dyn GraphQuerier,
    schema_title: &str,
    domain: &str,
    config: &DomainConfig,
    columns: &[ColumnDef],
) {
    let Ok(dto_ctx) = crate::ddd::dto::build_dto_context(db, schema_title, domain, config).await
    else {
        return;
    };
    let gaps = ddl_dto_gap_fields(columns, &dto_ctx.fields);
    if gaps.is_empty() {
        return;
    }
    let message = format!(
        "ddl/dto column divergence for {domain}.{schema_title}: the dto context resolves {} \
         field(s) the DDL context has no column for: {} (audit-only skeleton signature, \
         issue #446)",
        gaps.len(),
        gaps.join(", ")
    );
    eprintln!("warning: ddl-dto-gap: {message}");
    tracing::warn!("{message}");
}
