//! Issue #460: cross-plane child-table consistency.
//!
//! Every table the repository plane persists or hydrates must be created by
//! the DDL plane in the same run. A drift means the generated app fails at
//! runtime — INSERT/SELECT against a table no migration creates (the
//! `person_legal_documents` 500 that surfaced this issue). Divergence is a
//! generation-time ERROR: refusing to emit divergent output beats a missing
//! table at runtime.

use std::collections::{BTreeSet, HashSet};

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
    for entry in generation_entries(config, order) {
        let ctx = ddl
            .query_ddl_context(db, &entry.schema_title, &entry.domain, config)
            .await?;
        ddl_tables.insert((ctx.schema_name.clone(), ctx.table_name.clone()));
        for child in flatten_ddl_children(&ctx.child_tables) {
            ddl_tables.insert((child.schema_name.clone(), child.table_name.clone()));
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
    }

    Ok(divergences.into_iter().collect())
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
