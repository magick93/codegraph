use codegraph_core::types::ColumnInfo;
use codegraph_type_contracts::RefClassificationKind;

use crate::db::ddl::types::{CheckConstraint, ColumnComment, ColumnDef, ForeignKeyDef};

use super::keywords::{dollar_quote, quote_if_reserved, strip_rsharp};

/// DDL artifacts produced from a single column classification.
type DdlArtifacts = (
    Vec<ColumnDef>,
    Vec<ForeignKeyDef>,
    Vec<CheckConstraint>,
    Vec<ColumnComment>,
);

/// Convert a `ColumnInfo` from the composition tree into DDL columns, FKs, and check constraints.
///
/// Returns `None` for ValueObject-classified columns — those are represented as child
/// `CompositionNode`s in the tree, not as columns.
pub(crate) fn column_info_to_ddl(col: &ColumnInfo, table_name: &str) -> Option<DdlArtifacts> {
    let raw_name = &col.name;
    let prop_name = raw_name.clone();
    let description = col.description.as_deref().unwrap_or("");

    let mut columns = Vec::new();
    let mut foreign_keys = Vec::new();
    let mut check_constraints = Vec::new();
    let mut comments = Vec::new();

    match col.classification.as_ref() {
        Some(RefClassificationKind::PrimitiveWrapper)
        | Some(RefClassificationKind::StructuredWrapper)
        | Some(RefClassificationKind::RangeWrapper) => {
            if !description.is_empty() {
                comments.push(ColumnComment {
                    column: prop_name.clone(),
                    comment: description.to_string(),
                });
            }
            let pg_type = if col.postgres_type.is_empty() {
                "TEXT".to_string()
            } else {
                col.postgres_type.clone()
            };
            columns.push(ColumnDef {
                name: prop_name,
                pg_type,
                nullable: col.is_optional,
                default: None,
                is_primary_key: false,
                is_array: false,
            });
        }
        Some(RefClassificationKind::ArrayWrapper) => {
            if !description.is_empty() {
                comments.push(ColumnComment {
                    column: prop_name.clone(),
                    comment: description.to_string(),
                });
            }
            let raw_base = col
                .postgres_type
                .strip_suffix("[]")
                .unwrap_or(&col.postgres_type);
            let pg_type = if raw_base.is_empty() {
                "TEXT".to_string()
            } else {
                raw_base.to_string()
            };
            columns.push(ColumnDef {
                name: prop_name,
                pg_type,
                nullable: col.is_optional,
                default: None,
                is_primary_key: false,
                is_array: true,
            });
        }
        Some(RefClassificationKind::CodelistReference) => {
            // Array codelists are represented as child CompositionNodes, not columns.
            if col.is_array {
                return None;
            }
            let col_name = prop_name;
            if !description.is_empty() {
                comments.push(ColumnComment {
                    column: col_name.clone(),
                    comment: description.to_string(),
                });
            }
            columns.push(ColumnDef {
                name: col_name.clone(),
                pg_type: "TEXT".to_string(),
                nullable: col.is_optional,
                default: None,
                is_primary_key: false,
                is_array: false,
            });
            if let Some(ref fk) = col.fk_target {
                foreign_keys.push(ForeignKeyDef {
                    column_name: strip_rsharp(raw_name),
                    column: col_name,
                    references_schema: fk.schema.clone(),
                    references_table: fk.table.clone(),
                    references_column: fk.column.clone(),
                    on_delete: fk.on_delete.clone(),
                    is_codelist: true,
                });
            }
        }
        Some(RefClassificationKind::EntityReference) => {
            // Array entity refs are represented as junction child tables
            // (CompositionNodes), never as columns on the parent table.
            if col.is_array {
                return None;
            }
            // Only append `_id` when the schema-side column name doesn't
            // already carry the suffix (e.g. `tenantId` -> `tenant_id`).
            let col_name = if raw_name.ends_with("_id") {
                raw_name.to_string()
            } else {
                format!("{}_id", raw_name)
            };
            if !description.is_empty() {
                comments.push(ColumnComment {
                    column: col_name.clone(),
                    comment: description.to_string(),
                });
            }
            columns.push(ColumnDef {
                name: col_name.clone(),
                pg_type: "UUID".to_string(),
                nullable: col.is_optional,
                default: None,
                is_primary_key: false,
                is_array: false,
            });
            if let Some(ref fk) = col.fk_target {
                foreign_keys.push(ForeignKeyDef {
                    column_name: strip_rsharp(&col_name),
                    column: col_name,
                    references_schema: fk.schema.clone(),
                    references_table: fk.table.clone(),
                    references_column: fk.column.clone(),
                    on_delete: fk.on_delete.clone(),
                    is_codelist: false,
                });
            }
        }
        Some(RefClassificationKind::CodelistCheck) | Some(RefClassificationKind::InlineEnum) => {
            // Array CodelistCheck properties are child CompositionNodes, not columns.
            if col.is_array && col.classification == Some(RefClassificationKind::CodelistCheck) {
                return None;
            }
            if !description.is_empty() {
                comments.push(ColumnComment {
                    column: prop_name.clone(),
                    comment: description.to_string(),
                });
            }
            columns.push(ColumnDef {
                name: prop_name.clone(),
                pg_type: "TEXT".to_string(),
                nullable: col.is_optional,
                default: None,
                is_primary_key: false,
                is_array: col.is_array,
            });
            if !col.is_array && !col.check_values.is_empty() {
                let expr = col
                    .check_values
                    .iter()
                    .map(|v| dollar_quote(v))
                    .collect::<Vec<_>>()
                    .join(", ");
                check_constraints.push(CheckConstraint {
                    name: codegraph_naming::truncate_pg_identifier(&format!(
                        "chk_{}_{}",
                        table_name, raw_name
                    )),
                    column: prop_name.clone(),
                    expression: format!("{} IN ({})", quote_if_reserved(raw_name), expr),
                });
            }
        }
        Some(RefClassificationKind::CompositeWrapper)
        | Some(RefClassificationKind::MediaWrapper) => {
            let is_media = col.classification == Some(RefClassificationKind::MediaWrapper);
            let mut col_names_for_check = Vec::new();
            for comp_col in &col.composite_columns {
                let composite_col_name = format!("{}{}", raw_name, comp_col.suffix);
                if !description.is_empty() {
                    comments.push(ColumnComment {
                        column: composite_col_name.clone(),
                        comment: description.to_string(),
                    });
                }
                if is_media {
                    col_names_for_check.push(quote_if_reserved(&composite_col_name));
                }
                columns.push(ColumnDef {
                    name: composite_col_name,
                    pg_type: comp_col.pg_type.clone(),
                    nullable: col.is_optional,
                    default: None,
                    is_primary_key: false,
                    is_array: false,
                });
            }
            if is_media && col_names_for_check.len() == 2 {
                let nulls = col_names_for_check
                    .iter()
                    .map(|c| format!("{c} IS NULL"))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                let not_nulls = col_names_for_check
                    .iter()
                    .map(|c| format!("{c} IS NOT NULL"))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                check_constraints.push(CheckConstraint {
                    name: format!("chk_{}_complete", raw_name),
                    column: raw_name.clone(),
                    expression: format!("({nulls}) OR ({not_nulls})"),
                });
            }
        }
        Some(RefClassificationKind::ValueObject) => {
            // ValueObjects are represented as child CompositionNodes, not columns.
            return None;
        }
        None => {
            if !description.is_empty() {
                comments.push(ColumnComment {
                    column: prop_name.clone(),
                    comment: description.to_string(),
                });
            }
            let pg_type = if col.postgres_type.is_empty() {
                "TEXT".to_string()
            } else {
                col.postgres_type.clone()
            };
            columns.push(ColumnDef {
                name: prop_name,
                pg_type,
                nullable: col.is_optional,
                default: None,
                is_primary_key: false,
                is_array: false,
            });
        }
    }

    Some((columns, foreign_keys, check_constraints, comments))
}

/// FK column a child table/entity uses to reference its parent. Suffix-aware
/// so parents whose table name already ends in `_id` don't double it
/// (`evidence_extracted_field_id` → `evidence_extracted_field_id`, not
/// `evidence_extracted_field_id_id`). Delegates to the codegraph-naming
/// single source of truth (issue #460) shared with entity/repository
/// generation.
pub(crate) fn child_parent_fk_column(parent_table_name: &str) -> String {
    codegraph_naming::child_parent_fk_column(parent_table_name)
}
