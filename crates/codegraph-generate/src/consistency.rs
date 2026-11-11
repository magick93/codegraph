//! Issue #460: cross-plane child-table consistency.
//!
//! Every table the repository plane persists or hydrates must be created by
//! the DDL plane in the same run. A drift means the generated app fails at
//! runtime — INSERT/SELECT against a table no migration creates (the
//! `person_legal_documents` 500 that surfaced this issue). Divergence is a
//! generation-time ERROR: refusing to emit divergent output beats a missing
//! table at runtime.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::ParentCandidate;

use crate::db::ddl::DdlGenerator;
use crate::ddd::repository_emitter::RepositoryImplEmitter;
use crate::error::Result;
use crate::project_config::GenerationEntry;

/// The consistency sweep. Returns one human-readable line per divergence
/// (empty = clean). Errors from the underlying context builders propagate —
/// an entity that cannot build its contexts fails generation anyway.
pub async fn check_child_table_consistency(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    order: &[GenerationEntry],
    parent_candidates: &[ParentCandidate],
) -> Result<Vec<String>> {
    // ── The DDL plane's table universe: every entry's own table plus
    // every child table of every context, flattened.
    let ddl = DdlGenerator::new(std::path::Path::new(""))
        .with_parent_candidates(parent_candidates.to_vec());
    let mut ddl_tables: HashSet<(String, String)> = HashSet::new();
    // One FK constraint name may exist on exactly ONE table in the emitted
    // migration set. A second emission is the duplicate-constraint class
    // (`fk_deployment_job_id`): apply-time `ADD CONSTRAINT` aborts.
    //
    // Root cause (observed consumer shape): two titles in one domain strip
    // to the same module/table (the `compute_generation_order` module
    // collision warning), so both DDL contexts emit the table and its FKs.
    // The fix is a model rename (the warning names both titles); this gate
    // is defense-in-depth — silently dropping one of two entities would
    // hide a real modeling conflict, so we refuse to emit instead.
    let mut fk_owners: BTreeMap<(String, String, String), String> = BTreeMap::new();
    let mut duplicate_fks: BTreeSet<String> = BTreeSet::new();
    for entry in generation_entries(config, order) {
        let ctx = ddl
            .query_ddl_context(db, &entry.schema_title, &entry.domain, config)
            .await?;
        let owner = format!("{}/{}", entry.domain, entry.schema_title);
        ddl_tables.insert((ctx.schema_name.clone(), ctx.table_name.clone()));
        for fk in &ctx.foreign_keys {
            record_fk(
                &mut fk_owners,
                &mut duplicate_fks,
                &ctx.schema_name,
                &ctx.table_name,
                &format!("fk_{}_{}", ctx.table_name, fk.column_name),
                &owner,
            );
        }
        for child in flatten_ddl_children(&ctx.child_tables) {
            ddl_tables.insert((child.schema_name.clone(), child.table_name.clone()));
            record_fk(
                &mut fk_owners,
                &mut duplicate_fks,
                &child.schema_name,
                &child.table_name,
                &format!("fk_{}_parent", child.table_name),
                &owner,
            );
            for fk in &child.foreign_keys {
                record_fk(
                    &mut fk_owners,
                    &mut duplicate_fks,
                    &child.schema_name,
                    &child.table_name,
                    &format!("fk_{}_{}", child.table_name, fk.column_name),
                    &owner,
                );
            }
        }
    }

    // ── The repository plane's table demands: every child and junction
    // table of every entity tree must exist in the DDL universe.
    let emitter = RepositoryImplEmitter;
    let mut divergences: BTreeSet<String> = BTreeSet::new();
    for entry in generation_entries(config, order) {
        let tree = emitter
            .query_entity_tree(db, &entry.schema_title, &entry.domain, config, None)
            .await?;
        // Only entries whose repository plane actually emits SQL (an
        // effective operation or a declared design finder) can diverge.
        // Ops-less trees (e.g. a contained value object whose `.ddd`
        // design carries no repository) generate repository files with no
        // SQL body — their child-table classification is inert.
        let emits_repo_sql = tree.has_create
            || tree.has_read
            || tree.has_update
            || tree.has_delete
            || tree.has_list
            || !tree.design_finders.is_empty();
        if !emits_repo_sql {
            continue;
        }
        for child in flatten_tree_children(&tree.child_tables) {
            let key = (child.sql_schema_name.clone(), child.sql_table_name.clone());
            if !ddl_tables.contains(&key) {
                divergences.insert(format!(
                    "{}/{}: repository {} targets `{}.{}` (field `{}`) but the DDL never creates it{}",
                    entry.domain,
                    entry.schema_title,
                    if child.is_back_ref { "back-ref child" } else { "child table" },
                    child.sql_schema_name,
                    child.sql_table_name,
                    child.field_name,
                    if child.is_back_ref {
                        " — the contains-target must project as a back-ref on its own entity table"
                    } else {
                        " — contained children must project as {parent}_{feature} in the DDL"
                    },
                ));
            }
        }
        for j in &tree.junction_tables {
            let key = (j.sql_schema_name.clone(), j.sql_table_name.clone());
            if !ddl_tables.contains(&key) {
                divergences.insert(format!(
                    "{}/{}: junction table `{}.{}` (field `{}`) has no DDL",
                    entry.domain,
                    entry.schema_title,
                    j.sql_schema_name,
                    j.sql_table_name,
                    j.field_name,
                ));
            }
        }

        // ── Ref-field agreement (issue #539): every entity-ref field the
        // DTO plane accepts must have a repository-plane counterpart with
        // the SAME name — arrays land as junction writers, scalars as FK
        // columns. A mismatch is the silent-drop shape: the API accepts
        // the ids (or a differently-named field) and every create/update
        // drops them. The DTO↔repo probes are shared helpers; this gate
        // pins that no plane ever drifts again.
        let dto_ctx =
            crate::ddd::dto::build_dto_context(db, &entry.schema_title, &entry.domain, config)
                .await?;
        for field in &dto_ctx.fields {
            if !field.is_entity_ref || field.name == "id" {
                continue;
            }
            if field.is_array {
                let written = tree
                    .junction_tables
                    .iter()
                    .any(|j| j.field_name == field.name);
                if !written {
                    divergences.insert(format!(
                        "{}/{}: create/update DTO accepts ref array `{}` but the repository plane has no junction writer for it — `refers` links would be silently dropped",
                        entry.domain, entry.schema_title, field.name,
                    ));
                }
            } else {
                let bound = tree
                    .direct_columns
                    .iter()
                    .any(|c| c.is_entity_ref && c.field_name == field.name);
                if !bound {
                    divergences.insert(format!(
                        "{}/{}: create/update DTO accepts ref field `{}` but the repository plane binds no FK column for it — the link would be silently dropped",
                        entry.domain, entry.schema_title, field.name,
                    ));
                }
            }
        }
        for j in &tree.junction_tables {
            let carried = dto_ctx
                .fields
                .iter()
                .any(|f| f.is_entity_ref && f.is_array && f.name == j.field_name);
            if !carried {
                divergences.insert(format!(
                    "{}/{}: repository junction writer `{}` has no matching create/update DTO field — the junction would be written by nothing",
                    entry.domain, entry.schema_title, j.field_name,
                ));
            }
        }
    }

    for duplicate in duplicate_fks {
        divergences.insert(duplicate);
    }
    Ok(divergences.into_iter().collect())
}

fn record_fk(
    owners: &mut BTreeMap<(String, String, String), String>,
    duplicates: &mut BTreeSet<String>,
    schema: &str,
    table: &str,
    name: &str,
    owner: &str,
) {
    let name = codegraph_naming::truncate_pg_identifier(name);
    let key = (schema.to_string(), table.to_string(), name.clone());
    if let Some(previous) = owners.insert(key, owner.to_string()) {
        duplicates.insert(format!(
            "`{schema}.{table}` constraint `{name}` is emitted twice (by {previous} and {owner}) — a duplicate ADD CONSTRAINT aborts at apply time"
        ));
    }
}

fn generation_entries<'a>(
    config: &DomainConfig,
    order: &'a [GenerationEntry],
) -> impl Iterator<Item = &'a GenerationEntry> {
    order.iter().filter(|entry| {
        let mode = config
            .domains
            .get(&entry.domain)
            .and_then(|d| d.get_entity_config(&entry.schema_title))
            .and_then(|ec| ec.generation_mode.as_deref())
            .unwrap_or(&config.defaults.generation_mode);
        mode != "none"
    })
}

fn flatten_ddl_children(
    children: &[crate::db::ddl::ChildTableDef],
) -> Vec<&crate::db::ddl::ChildTableDef> {
    let mut out = Vec::new();
    for child in children {
        out.push(child);
        out.extend(flatten_ddl_children(&child.child_tables));
    }
    out
}

fn flatten_tree_children(
    children: &[crate::ddd::repository_emitter::ChildTableInfo],
) -> Vec<&crate::ddd::repository_emitter::ChildTableInfo> {
    let mut out = Vec::new();
    for child in children {
        out.push(child);
        out.extend(flatten_tree_children(&child.child_tables));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_fk_names_are_reported() {
        let mut owners = BTreeMap::new();
        let mut duplicates = BTreeSet::new();
        record_fk(
            &mut owners,
            &mut duplicates,
            "common",
            "deployment",
            "fk_deployment_job_id",
            "payroll/PayRun",
        );
        assert!(duplicates.is_empty());
        record_fk(
            &mut owners,
            &mut duplicates,
            "common",
            "deployment",
            "fk_deployment_job_id",
            "core/Deployment",
        );
        assert_eq!(duplicates.len(), 1);
        let message = duplicates.iter().next().unwrap();
        assert!(message.contains("fk_deployment_job_id"), "{message}");
        assert!(message.contains("payroll/PayRun"), "{message}");
    }

    #[test]
    fn same_fk_name_on_a_different_table_is_not_a_duplicate() {
        let mut owners = BTreeMap::new();
        let mut duplicates = BTreeSet::new();
        record_fk(&mut owners, &mut duplicates, "common", "a", "fk_a_id", "x");
        record_fk(&mut owners, &mut duplicates, "common", "b", "fk_a_id", "y");
        assert!(duplicates.is_empty());
    }
}
