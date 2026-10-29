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
use super::types::{ChildTableInfo, EntityTree, TreeColumn};

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

/// The intermediate child table a grandchild EXISTS nests through (the
/// `NestedFilterFieldInfo::IntermediateJoin` shape).
#[derive(Debug, Clone)]
pub(crate) struct ChildJoin {
    pub sql_schema: String,
    pub sql_table_name: String,
    pub parent_fk_column: String,
}

/// A finder parameter bound to a contained-value column (issue #523):
/// equality rides an EXISTS subquery over the child table. Bindings
/// sharing `group` AND into ONE EXISTS (same-row semantics).
#[derive(Debug)]
pub(crate) struct ChildFinderBinding {
    /// Group identity — bindings sharing it belong to one child row.
    pub group: String,
    pub sql_schema: String,
    pub sql_table_name: String,
    pub parent_fk_column: String,
    pub pg_column_name: String,
    pub rust_type: String,
    /// Grandchild bindings nest through this intermediate table.
    pub intermediate: Option<ChildJoin>,
}

/// Where a finder parameter binds.
#[derive(Debug)]
pub(crate) enum FinderBinding {
    Direct(FinderColumn),
    Child(ChildFinderBinding),
}

impl FinderBinding {
    pub(crate) fn direct(&self) -> Option<&FinderColumn> {
        match self {
            FinderBinding::Direct(col) => Some(col),
            FinderBinding::Child(_) => None,
        }
    }

    pub(crate) fn child(&self) -> Option<&ChildFinderBinding> {
        match self {
            FinderBinding::Child(child) => Some(child),
            FinderBinding::Direct(_) => None,
        }
    }
}

/// One contained-column match candidate found while resolving a param.
struct ChildCandidate {
    binding: ChildFinderBinding,
    depth: usize,
}

/// Flatten the child/grandchild column universe into match candidates
/// (depth 1 = child, 2 = grandchild — the joinable levels; deeper levels
/// are collected only so the refusal can name them).
fn collect_child_candidates(
    children: &[ChildTableInfo],
    through: Option<ChildJoin>,
    depth: usize,
    out: &mut Vec<ChildCandidate>,
) {
    for child in children {
        let group = match &through {
            Some(i) => format!(
                "{}.{}>{}.{}",
                i.sql_schema, i.sql_table_name, child.sql_schema_name, child.sql_table_name
            ),
            None => format!("{}.{}", child.sql_schema_name, child.sql_table_name),
        };
        for col in &child.columns {
            out.push(ChildCandidate {
                binding: ChildFinderBinding {
                    group: group.clone(),
                    sql_schema: child.sql_schema_name.clone(),
                    sql_table_name: child.sql_table_name.clone(),
                    parent_fk_column: child.parent_fk_column.clone(),
                    pg_column_name: col.pg_column_name.clone(),
                    rust_type: col.rust_type.clone(),
                    intermediate: through.clone(),
                },
                depth,
            });
        }
        let nested_through = Some(ChildJoin {
            sql_schema: child.sql_schema_name.clone(),
            sql_table_name: child.sql_table_name.clone(),
            parent_fk_column: child.parent_fk_column.clone(),
        });
        collect_child_candidates(&child.child_tables, nested_through, depth + 1, out);
    }
}

/// Resolve every finder parameter to its binding — a direct entity column
/// or a contained-value column (issue #523) joined via EXISTS — checking
/// the parameter's rust type against the column's. A parameter naming `id`
/// resolves to the synthetic UUID primary key (the `findByKeys` surrogate).
/// `Err(reason)` names why a parameter does not resolve (no column named
/// `y` / type mismatch / ambiguous contained match), so the caller's
/// skipped-finder warning is actionable.
pub(crate) fn finder_columns(
    finder: &DesignFinder,
    columns: &[TreeColumn],
    child_tables: &[ChildTableInfo],
) -> std::result::Result<Vec<FinderBinding>, String> {
    let mut candidates = Vec::new();
    collect_child_candidates(child_tables, None, 1, &mut candidates);
    let mut resolved = Vec::with_capacity(finder.params.len());
    for param in &finder.params {
        let col = columns.iter().find(|c| {
            let bare = c.field_name.strip_prefix("r#").unwrap_or(&c.field_name);
            bare == param.name
                || c.pg_column_name == param.name
                || c.dto_field_name.as_deref() == Some(param.name.as_str())
        });
        if let Some(c) = col {
            if !type_compatible(&param.rust_type, &c.rust_type) {
                return Err(format!(
                    "parameter `{}` type `{}` does not match column `{}` (`{}`)",
                    param.name, param.rust_type, param.name, c.rust_type
                ));
            }
            resolved.push(FinderBinding::Direct(FinderColumn {
                field_name: c.field_name.clone(),
                pg_column_name: c.pg_column_name.clone(),
            }));
            continue;
        }
        // The primary key is emitted outside `direct_columns`
        // (`row.id`); resolve the surrogate by name.
        if param.name == "id" && param.rust_type == "Uuid" {
            resolved.push(FinderBinding::Direct(FinderColumn {
                field_name: "id".to_string(),
                pg_column_name: "id".to_string(),
            }));
            continue;
        }
        // Contained-value columns: bare-name match against child and
        // grandchild columns (the documented resolution rule — direct
        // columns win, then a unique contained match). Deeper levels are
        // refused with a dedicated reason.
        let name_matches = |cand: &ChildCandidate| cand.binding.pg_column_name == param.name;
        let mut shallow: Vec<&ChildCandidate> = candidates
            .iter()
            .filter(|c| c.depth <= 2 && name_matches(c))
            .collect();
        shallow.sort_by_key(|c| c.depth);
        shallow.dedup_by(|a, b| {
            a.binding.group == b.binding.group
                && a.binding.pg_column_name == b.binding.pg_column_name
        });
        match shallow.len() {
            0 => {
                return Err(
                    if candidates.iter().any(|c| c.depth > 2 && name_matches(c)) {
                        format!(
                            "parameter `{}` reaches a nested contained value deeper than the \
                         supported grandchild level",
                            param.name
                        )
                    } else {
                        format!(
                            "parameter `{}` names no column of `{}`",
                            param.name, finder.entity_name
                        )
                    },
                );
            }
            1 => {
                let cand = &shallow[0].binding;
                if !type_compatible(&param.rust_type, &cand.rust_type) {
                    return Err(format!(
                        "parameter `{}` type `{}` does not match contained column `{}` (`{}`)",
                        param.name, param.rust_type, param.name, cand.rust_type
                    ));
                }
                resolved.push(FinderBinding::Child(ChildFinderBinding {
                    group: cand.group.clone(),
                    sql_schema: cand.sql_schema.clone(),
                    sql_table_name: cand.sql_table_name.clone(),
                    parent_fk_column: cand.parent_fk_column.clone(),
                    pg_column_name: cand.pg_column_name.clone(),
                    rust_type: cand.rust_type.clone(),
                    intermediate: cand.intermediate.clone(),
                }));
            }
            _ => {
                let groups: Vec<&str> = shallow
                    .iter()
                    .map(|c| c.binding.group.as_str())
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect();
                return Err(format!(
                    "parameter `{}` is ambiguous — contained columns of `{}`",
                    param.name,
                    groups.join("` and `")
                ));
            }
        }
    }
    Ok(resolved)
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
    // `From<T>` for owned values but not for references, so a `&param`
    // binding only moves for Copy types (primitives, Uuid). Everything
    // else — String, chrono dates/times, decimals, JSON, both bare and
    // crate-qualified spellings — binds owned via `.clone()`.
    const COPY: &[&str] = &[
        "bool", "f32", "f64", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "Uuid",
    ];
    if COPY.iter().any(|t| rust_type.starts_with(t)) {
        param.to_string()
    } else {
        format!("{param}.clone()")
    }
}

impl super::RepositoryImplEmitter {
    /// Emit every design finder method onto the repository impl. Gated on
    /// `tree.design_finders` being non-empty — flag-off emits nothing.
    pub(crate) fn emit_design_finders(&self, tree: &EntityTree, code: &mut CodeWriter) {
        for finder in &tree.design_finders {
            let Ok(bindings) = finder_columns(finder, &tree.direct_columns, &tree.child_tables)
            else {
                // Already warned at tree-build time (the trait filters the
                // same way) — skip silently so trait and impls stay in sync.
                continue;
            };
            self.emit_design_finder_fn(tree, finder, &bindings, code);
        }
    }

    fn emit_design_finder_fn(
        &self,
        tree: &EntityTree,
        finder: &DesignFinder,
        bindings: &[FinderBinding],
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
            for (param, binding) in finder.params.iter().zip(bindings.iter()) {
                let Some(col) = binding.direct() else {
                    continue;
                };
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
            // Filter chain: direct equality predicates in param order,
            // then one EXISTS per child group (issue #523 — same-child
            // params AND inside ONE EXISTS, same-row semantics).
            let mut exists_groups: Vec<(String, Vec<usize>)> = Vec::new();
            for (i, binding) in bindings.iter().enumerate() {
                if let Some(child) = binding.child() {
                    match exists_groups
                        .iter_mut()
                        .find(|(group, _)| group == &child.group)
                    {
                        Some((_, idxs)) => idxs.push(i),
                        None => exists_groups.push((child.group.clone(), vec![i])),
                    }
                }
            }
            let exists_filter_count = exists_groups.len();
            let direct_count = bindings.iter().filter(|b| b.direct().is_some()).count();
            let total_filters = direct_count + exists_filter_count;
            let mut emitted = 0usize;
            for (i, binding) in bindings.iter().enumerate() {
                let Some(col) = binding.direct() else {
                    continue;
                };
                let param = &finder.params[i];
                let eq = eq_arg(&param.name, &param.rust_type);
                emitted += 1;
                let terminator = if emitted == total_filters && !tree.is_auditable {
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
            for (_, idxs) in &exists_groups {
                let first = &bindings[idxs[0]];
                let child = first.child().expect("exists group holds child bindings");
                let (alias, preds) = match &child.intermediate {
                    None => (
                        "_child".to_string(),
                        idxs.iter()
                            .enumerate()
                            .map(|(n, i)| {
                                format!(
                                    "_child.\"{}\" = ${}",
                                    bindings[*i].child().expect("child binding").pg_column_name,
                                    n + 1
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(" AND "),
                    ),
                    Some(_) => (
                        "_gc".to_string(),
                        idxs.iter()
                            .enumerate()
                            .map(|(n, i)| {
                                format!(
                                    "_gc.\"{}\" = ${}",
                                    bindings[*i].child().expect("child binding").pg_column_name,
                                    n + 1
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(" AND "),
                    ),
                };
                let sql = match &child.intermediate {
                    None => format!(
                        "EXISTS (SELECT 1 FROM \"{}\".\"{}\" {alias} WHERE {alias}.\"{}\" = \
                         \"{}\".\"{}\".\"id\" AND {preds})",
                        child.sql_schema,
                        child.sql_table_name,
                        child.parent_fk_column,
                        tree.schema_name,
                        tree.table_name,
                        alias = alias,
                        preds = preds
                    ),
                    Some(ij) => format!(
                        "EXISTS (SELECT 1 FROM \"{}\".\"{}\" _intermediate WHERE \
                         _intermediate.\"{}\" = \"{}\".\"{}\".\"id\" AND EXISTS (SELECT 1 FROM \
                         \"{}\".\"{}\" {alias} WHERE {alias}.\"{}\" = _intermediate.\"id\" AND \
                         {preds}))",
                        ij.sql_schema,
                        ij.sql_table_name,
                        ij.parent_fk_column,
                        tree.schema_name,
                        tree.table_name,
                        child.sql_schema,
                        child.sql_table_name,
                        child.parent_fk_column,
                        alias = alias,
                        preds = preds
                    ),
                };
                let values = idxs
                    .iter()
                    .map(|i| {
                        let param = &finder.params[*i];
                        format!(
                            "sea_orm::Value::from({})",
                            eq_arg(&param.name, &param.rust_type)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                emitted += 1;
                let terminator = if emitted == total_filters && !tree.is_auditable {
                    ";"
                } else {
                    ""
                };
                wln!(
                    code,
                    "            .filter(sea_orm::sea_query::Expr::cust_with_values("
                );
                wln!(code, "                {sql:?},");
                wln!(code, "                vec![{values}],");
                wln!(code, "            )){terminator}");
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
            wln!(code, "    }}");
        } else {
            wln!(code, "        let row = query.one(db)");
            wln!(code, "            .await?;");
            wln!(code);
            wln!(code, "        let row = match row {{");
            wln!(code, "            Some(r) => r,");
            wln!(code, "            None => return Ok(None),");
            wln!(code, "        }};");
            // Child reads key on row.id — the `id` variable is not in a
            // finder's scope (its parameters are the declared ones). The
            // shared construction helper closes the fn itself (the
            // `find_by_id` contract) — do not close again.
            emit_response_construction_for(code, tree, "row.id");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddd::design::{DesignFinder, FinderKind, FinderParam};

    fn direct_col(name: &str, rust_type: &str) -> TreeColumn {
        TreeColumn {
            field_name: name.to_string(),
            pg_column_name: name.to_string(),
            dto_field_name: None,
            rust_type: rust_type.to_string(),
            is_nullable: false,
            is_entity_ref: false,
            dto_rust_type: None,
            is_workflow_managed: false,
            is_array: false,
            pg_cast: None,
            is_composite_range: false,
            is_structured_wrapper: false,
            is_media: false,
        }
    }

    fn child_col(name: &str, rust_type: &str) -> super::super::types::ChildColumn {
        super::super::types::ChildColumn {
            field_name: name.to_string(),
            pg_column_name: name.to_string(),
            rust_type: rust_type.to_string(),
            is_nullable: false,
            dto_rust_type: None,
            pg_cast: None,
        }
    }

    fn child_table(name: &str, columns: Vec<super::super::types::ChildColumn>) -> ChildTableInfo {
        ChildTableInfo {
            field_name: name.to_string(),
            struct_name: format!("Book{}", codegraph_naming::to_pascal_case(name)),
            sql_table_name: format!("book_{name}"),
            sql_schema_name: "library".to_string(),
            parent_fk_column: "book_id".to_string(),
            is_array: true,
            columns,
            child_tables: Vec::new(),
            is_back_ref: false,
        }
    }

    fn finder(params: Vec<(&str, &str)>) -> DesignFinder {
        DesignFinder {
            method_name: "find_by_thing".to_string(),
            name: "findByThing".to_string(),
            entity_name: "Book".to_string(),
            params: params
                .into_iter()
                .map(|(name, rust_type)| FinderParam {
                    name: name.to_string(),
                    rust_type: rust_type.to_string(),
                    optional: false,
                })
                .collect(),
            returns_many: false,
            return_rust_type: "BookResponse".to_string(),
            kind: FinderKind::Declared,
        }
    }

    /// The skipped-finder warning must name the reason (issue #523): no
    /// column named `y` and a type mismatch are distinct, actionable causes.
    #[test]
    fn finder_drop_reasons_name_the_cause() {
        let columns = vec![direct_col("title", "String")];
        let children = vec![child_table(
            "tax_identifiers",
            vec![child_col("number", "String")],
        )];

        let missing = finder_columns(&finder(vec![("ghost", "String")]), &columns, &children)
            .expect_err("no column named ghost");
        assert!(missing.contains("names no column of `Book`"), "{missing}");

        let mismatch = finder_columns(&finder(vec![("title", "i64")]), &columns, &children)
            .expect_err("type mismatch");
        assert!(
            mismatch.contains("type `i64` does not match column `title` (`String`)"),
            "{mismatch}"
        );

        // Ambiguous contained match names both candidates.
        let two_children = vec![
            child_table("tax_identifiers", vec![child_col("number", "String")]),
            child_table("permits", vec![child_col("number", "String")]),
        ];
        let ambiguous =
            finder_columns(&finder(vec![("number", "String")]), &columns, &two_children)
                .expect_err("number matches two child tables");
        assert!(ambiguous.contains("ambiguous"), "{ambiguous}");
        assert!(
            ambiguous.contains("book_tax_identifiers") && ambiguous.contains("book_permits"),
            "{ambiguous}"
        );
    }

    /// A unique contained-column match resolves to a Child binding carrying
    /// the EXISTS join metadata (issue #523).
    #[test]
    fn finder_resolves_unique_contained_column() {
        let columns = vec![direct_col("title", "String")];
        let mut grandchild = child_table("details", vec![child_col("issued", "String")]);
        grandchild.sql_table_name = "book_tax_identifiers_details".to_string();
        grandchild.parent_fk_column = "tax_identifiers_id".to_string();
        let mut child = child_table(
            "tax_identifiers",
            vec![
                child_col("number", "String"),
                child_col("jurisdiction", "String"),
            ],
        );
        child.child_tables = vec![grandchild];

        let bindings = finder_columns(
            &finder(vec![("number", "String"), ("title", "String")]),
            &columns,
            &[child],
        )
        .expect("contained params resolve");
        assert!(matches!(bindings[0], FinderBinding::Child(_)));
        assert!(matches!(bindings[1], FinderBinding::Direct(_)));
        let child_binding = bindings[0].child().expect("child binding");
        assert_eq!(child_binding.sql_table_name, "book_tax_identifiers");
        assert_eq!(child_binding.parent_fk_column, "book_id");
        assert_eq!(child_binding.pg_column_name, "number");
        assert!(child_binding.intermediate.is_none());

        // Grandchild params resolve through the intermediate join.
        let grand = finder_columns(
            &finder(vec![("issued", "String")]),
            &columns,
            &[{
                let mut c = child_table("tax_identifiers", vec![child_col("number", "String")]);
                c.child_tables = vec![{
                    let mut g = child_table("details", vec![child_col("issued", "String")]);
                    g.sql_table_name = "book_tax_identifiers_details".to_string();
                    g.parent_fk_column = "tax_identifiers_id".to_string();
                    g
                }];
                c
            }],
        )
        .expect("grandchild param resolves");
        let grand_binding = grand[0].child().expect("grandchild binding");
        assert_eq!(grand_binding.sql_table_name, "book_tax_identifiers_details");
        let ij = grand_binding
            .intermediate
            .as_ref()
            .expect("intermediate join");
        assert_eq!(ij.sql_table_name, "book_tax_identifiers");
        assert_eq!(ij.parent_fk_column, "book_id");
    }
}
