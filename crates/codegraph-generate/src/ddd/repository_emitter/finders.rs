//! Emission of declared `.ddd` design finders (issue #449) onto the
//! SeaORM repository implementation.
//!
//! A finder's signature is the contract (Sculptor semantics): every
//! parameter lowers to an equality predicate on the entity column the
//! parameter names (`findByTitle(String title)` → `WHERE title = $1`),
//! AND-combined for multiple parameters. The return type resolves to the
//! designing entity, so the return rust type is always the entity's
//! `{Entity}Response` — `Vec` when the declared multiplicity is many,
//! `Option` otherwise. Soft-deleted rows are filtered out whenever the
//! entity is auditable (no `include_deleted` parameter: the declared
//! signature is the contract). Tenant isolation rides RLS exactly like
//! every other emitted method.

use crate::code_writer::{CodeWriter, wln};
use crate::ddd::design::{DesignFinder, FinderKind};

use super::child::emit_child_reads;
use super::dto::{
    emit_child_field_population, emit_entity_to_dto_field, emit_response_construction_for,
};
use super::junction::{emit_junction_field_population, emit_junction_reads};
use super::types::{EntityTree, TreeColumn};

/// The resolved column a finder parameter filters on: the entity column's
/// identity (owned — the `findByKeys` surrogate key resolves to the `id`
/// primary key, which is not a `direct_columns` entry).
#[derive(Debug)]
pub(crate) struct FinderColumn {
    pub field_name: String,
    pub pg_column_name: String,
}

impl FinderColumn {
    /// The SeaORM `Column` variant for this column.
    pub(crate) fn variant(&self) -> String {
        let bare = self
            .field_name
            .strip_prefix("r#")
            .unwrap_or(&self.field_name);
        codegraph_naming::to_pascal_case(bare)
    }
}

/// Resolve every finder parameter to its entity column, checking the
/// parameter's rust type against the column's. A parameter naming `id`
/// resolves to the synthetic UUID primary key (the `findByKeys` surrogate).
/// `None` when any other parameter is unresolvable — the caller drops the
/// finder so the trait and both impls stay consistent.
pub(crate) fn finder_columns(
    finder: &DesignFinder,
    columns: &[TreeColumn],
) -> Option<Vec<FinderColumn>> {
    let mut resolved = Vec::with_capacity(finder.params.len());
    for param in &finder.params {
        let col = columns.iter().find(|c| {
            let bare = c.field_name.strip_prefix("r#").unwrap_or(&c.field_name);
            bare == param.name
                || c.pg_column_name == param.name
                || c.dto_field_name.as_deref() == Some(param.name.as_str())
        });
        let (field_name, pg_column_name, column_type) = match col {
            Some(c) => (
                c.field_name.clone(),
                c.pg_column_name.clone(),
                c.rust_type.clone(),
            ),
            // The primary key is emitted outside `direct_columns`
            // (`row.id`); resolve the surrogate by name.
            None if param.name == "id" && param.rust_type == "Uuid" => {
                ("id".to_string(), "id".to_string(), "Uuid".to_string())
            }
            None => return None,
        };
        if !type_compatible(&param.rust_type, &column_type) {
            return None;
        }
        resolved.push(FinderColumn {
            field_name,
            pg_column_name,
        });
    }
    Some(resolved)
}

/// Whether a finder parameter type can bind the column's rust type.
/// `findByExample` params carry `optional: true` with the BARE type in
/// `rust_type`, so both sides are bare here.
fn type_compatible(param_type: &str, column_type: &str) -> bool {
    let base = column_type
        .trim_start_matches("Option<")
        .trim_end_matches('>')
        .trim_start_matches("Vec<")
        .trim_end_matches('>');
    normalize(param_type) == normalize(base)
}

fn normalize(ty: &str) -> String {
    match ty {
        "Uuid" | "uuid::Uuid" => "Uuid".to_string(),
        "Decimal" | "rust_decimal::Decimal" => "Decimal".to_string(),
        "NaiveDate" | "chrono::NaiveDate" => "NaiveDate".to_string(),
        "DateTime<Utc>" | "chrono::DateTime<chrono::Utc>" => "DateTime".to_string(),
        other => other.to_string(),
    }
}

/// Whether a finder parameter type binds as text (with a `::text` cast on
/// the column) rather than its natural rust type on the cornucopia
/// provider — avoids depending on the generated queries crate's
/// numeric/chrono `ToSql` features. Shared with both cornucopia emitters.
pub(crate) fn finder_param_binds_as_text(rust_type: &str) -> bool {
    matches!(
        rust_type,
        "Decimal" | "rust_decimal::Decimal" | "NaiveDate" | "chrono::NaiveDate"
    )
}

/// The `.eq(...)` argument for a finder parameter of the given rust type.
fn eq_arg(param: &str, rust_type: &str) -> String {
    // `ColumnTrait::eq` takes `impl Into<Value>`; sea-orm implements
    // `From<T>` for owned values but not for references, and only Copy
    // types (primitives, Uuid) can move out of a `&param` binding. Every
    // non-Copy scalar (String, chrono dates/times, decimals, JSON) must
    // bind owned.
    const NON_COPY: &[&str] = &[
        "String",
        "NaiveDate",
        "NaiveTime",
        "NaiveDateTime",
        "DateTime",
        "Decimal",
        "BigDecimal",
        "JsonValue",
        "serde_json::Value",
    ];
    if rust_type == "String" || NON_COPY.iter().any(|t| rust_type.starts_with(t)) {
        format!("{param}.clone()")
    } else {
        param.to_string()
    }
}

impl super::RepositoryImplEmitter {
    /// Emit every design finder method onto the repository impl. Gated on
    /// `tree.design_finders` being non-empty — flag-off emits nothing.
    pub(crate) fn emit_design_finders(&self, tree: &EntityTree, code: &mut CodeWriter) {
        for finder in &tree.design_finders {
            let Some(columns) = finder_columns(finder, &tree.direct_columns) else {
                continue;
            };
            self.emit_design_finder_fn(tree, finder, &columns, code);
        }
    }

    fn emit_design_finder_fn(
        &self,
        tree: &EntityTree,
        finder: &DesignFinder,
        columns: &[FinderColumn],
        code: &mut CodeWriter,
    ) {
        wln!(code);
        match finder.kind {
            FinderKind::Declared => wln!(
                code,
                "    /// Design finder `{}` (declared in the .ddd design): equality query.",
                finder.name
            ),
            FinderKind::FindByKeys => wln!(
                code,
                "    /// Design built-in `findByKeys`: equality on the natural key."
            ),
            FinderKind::FindByExample => wln!(
                code,
                "    /// Design built-in `findByExample`: `None` fields filter out."
            ),
        }
        wln!(
            code,
            "    #[tracing::instrument(skip(self, db), fields(db.operation = \"design_finder\", db.table = \"{}.{}\"))]",
            tree.schema_name,
            tree.table_name
        );
        wln!(code, "    async fn {}(", finder.method_name);
        wln!(code, "        &self,");
        wln!(code, "        db: &DatabaseTransaction,");
        for param in &finder.params {
            if param.optional {
                wln!(code, "        {}: Option<{}>,", param.name, param.rust_type);
            } else {
                wln!(code, "        {}: {},", param.name, param.rust_type);
            }
        }
        if finder.returns_many {
            wln!(
                code,
                "    ) -> Result<Vec<{}Response>, Box<dyn std::error::Error>> {{",
                tree.entity_name
            );
        } else {
            wln!(
                code,
                "    ) -> Result<Option<{}Response>, Box<dyn std::error::Error>> {{",
                tree.entity_name
            );
        }

        // Build the query: equality predicates AND-combined, plus the
        // soft-delete filter when the entity is auditable. `findByExample`'s
        // Option fields use the Condition builder — each `None` filters its
        // column out of the predicate entirely.
        if finder.kind == FinderKind::FindByExample {
            wln!(
                code,
                "        let mut conditions = sea_orm::Condition::all();"
            );
            for (param, col) in finder.params.iter().zip(columns.iter()) {
                let eq = eq_arg(&param.name, &param.rust_type);
                wln!(
                    code,
                    "        if let Some({name}) = &{name} {{",
                    name = param.name
                );
                wln!(
                    code,
                    "            conditions = conditions.add(crate::entity::{}::Column::{}.eq({eq}));",
                    tree.entity_module,
                    col.variant()
                );
                wln!(code, "        }}");
            }
            if tree.is_auditable {
                // Soft-deleted rows always stay invisible to design finders.
                wln!(
                    code,
                    "        conditions = \
                     conditions.add(crate::entity::{}::Column::DeletedAt.is_null());",
                    tree.entity_module
                );
            }
            wln!(
                code,
                "        let query = crate::entity::{}::Entity::find().filter(conditions);",
                tree.entity_module
            );
        } else {
            if tree.is_auditable {
                wln!(
                    code,
                    "        let mut query = crate::entity::{}::Entity::find()",
                    tree.entity_module
                );
            } else {
                wln!(
                    code,
                    "        let query = crate::entity::{}::Entity::find()",
                    tree.entity_module
                );
            }
            let params: Vec<&crate::ddd::design::FinderParam> = finder.params.iter().collect();
            for (i, (param, col)) in params.iter().zip(columns.iter()).enumerate() {
                let eq = eq_arg(&param.name, &param.rust_type);
                let terminator = if i + 1 == params.len() && !tree.is_auditable {
                    ";"
                } else {
                    ""
                };
                wln!(
                    code,
                    "            .filter(crate::entity::{}::Column::{}.eq({eq})){terminator}",
                    tree.entity_module,
                    col.variant()
                );
            }
            if tree.is_auditable {
                // The declared signature carries no include_deleted
                // parameter — soft-deleted rows always stay invisible.
                wln!(
                    code,
                    "            .filter(crate::entity::{}::Column::DeletedAt.is_null());",
                    tree.entity_module
                );
            }
        }

        if finder.returns_many {
            wln!(code, "        let rows = query.all(db)");
            wln!(code, "            .await?;");
            wln!(code);
            wln!(
                code,
                "        let mut results = Vec::with_capacity(rows.len());"
            );
            wln!(code, "        for row in rows {{");
            emit_child_reads(code, &tree.child_tables, "row.id", 3);
            emit_junction_reads(code, &tree.junction_tables, "row.id", 3);
            wln!(
                code,
                "            results.push({}Response {{",
                tree.entity_name
            );
            wln!(code, "                id: row.id,");
            for col in &tree.direct_columns {
                if col.is_composite_range
                    || col.field_name == "created_at"
                    || col.field_name == "updated_at"
                {
                    continue;
                }
                emit_entity_to_dto_field(code, col, "row", "                ");
            }
            emit_child_field_population(code, &tree.child_tables, "                ");
            emit_junction_field_population(code, &tree.junction_tables, "                ");
            if tree.has_workflow {
                wln!(code, "                workflow_state: None,");
            }
            wln!(code, "                created_at: row.created_at,");
            if !tree.append_only {
                wln!(code, "                updated_at: row.updated_at,");
            }
            wln!(code, "                ..Default::default()");
            wln!(code, "            }});");
            wln!(code, "        }}");
            wln!(code);
            wln!(code, "        Ok(results)");
        } else {
            wln!(code, "        let row = query.one(db)");
            wln!(code, "            .await?;");
            wln!(code);
            wln!(code, "        let row = match row {{");
            wln!(code, "            Some(r) => r,");
            wln!(code, "            None => return Ok(None),");
            wln!(code, "        }};");
            // Child reads key on row.id — the `id` variable is not in a
            // finder's scope (its parameters are the declared ones).
            emit_response_construction_for(code, tree, "row.id");
        }
        wln!(code, "    }}");
    }
}
