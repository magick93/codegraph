//! Cornucopia `?include=` adapter emission (issue #781 P1).
//!
//! Split out of `cornucopia_repo.rs` (issue #751 file budget). This module owns
//! the three include-plane decisions the adapter file needs:
//!
//! * resolving the implementable include paths ([`resolve_cornucopia_include_targets`]),
//! * the import surface the target response + child hydration need
//!   ([`emit_include_imports`]),
//! * the inherent fetch methods themselves ([`emit_include_adapter`]).
//!
//! The response mapping is NOT re-implemented: [`emit_include_adapter`] calls
//! `cornucopia_repo::emit_response_expr`, the same chain the CRUD reads use, so
//! columns, codelists/numerics and child tables hydrate identically.

use codegraph_core::traits::GraphQuerier;

use crate::ProjectConfig;
use crate::code_writer::{CodeWriter, wln};
use crate::ddd::repository_emitter::{EntityTree, RepositoryImplEmitter};
use codegraph_config::DomainConfig;

use super::cornucopia_repo::{emit_response_expr, qmod};

/// Resolve the implemented include paths and each target's tree, in
/// declaration order (issue #781 P1).
///
/// The path set comes from the SAME gate the handler uses, and the support
/// verdict from the SAME predicate — so the adapter surface cannot advertise
/// a path the handler refuses (or omit one it calls).
pub(crate) async fn resolve_cornucopia_include_targets(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    project: &ProjectConfig,
    domain: &str,
    schema_title: &str,
) -> Vec<(crate::api::include_path::ResolvedIncludePath, EntityTree)> {
    let entity_cfg = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(schema_title));
    let paths = crate::api::include_path::resolve_include_paths_for_entity(
        db,
        config,
        domain,
        schema_title,
        entity_cfg,
        project.is_workers_topology(),
    )
    .await
    .unwrap_or_default();
    let support = crate::api::include_plan::include_support_for(project);

    let mut out = Vec::new();
    for path in support.supported_paths(&paths) {
        let Some(seg) = path.segments.first() else {
            continue;
        };
        // A self-referencing include resolves the source's own module; the
        // target tree is recomputed from the same title (the response
        // mapping only reads columns/children, so the parent-scope parameter
        // is irrelevant here).
        let Ok(target) = RepositoryImplEmitter
            .query_entity_tree(db, &seg.schema_title, &seg.domain, config, None)
            .await
        else {
            continue;
        };
        out.push((path.clone(), target));
    }
    out
}

/// One implemented `?include=` path's adapter facts (issue #781 P1).
///
/// Borrowed from the resolved path + the target's own `EntityTree` so the
/// response mapping reuses the SAME emitter the CRUD reads use
/// ([`emit_response_expr`] — columns, codelist/numeric parsing, child-table
/// hydration) instead of a second mapping that could drift.
pub(crate) struct IncludeAdapterPlan<'a> {
    // Fields are read by `cornucopia_repo`'s generator (same crate).
    /// The annotated query function (`include_person_for_worker`).
    pub(crate) query_fn: String,
    /// The batch query function (`include_person_batch_for_worker`).
    pub(crate) batch_query_fn: String,
    /// The adapter method name — `ResolvedIncludePath::fetch_method`.
    pub(crate) fetch_method: &'a str,
    /// The adapter batch method name — `ResolvedIncludePath::batch_fetch_method`.
    pub(crate) batch_fetch_method: &'a str,
    /// The target's response type (`PersonResponse`).
    pub(crate) response_type: &'a str,
    /// Target segment domain / module (import construction).
    pub(crate) target_domain: &'a str,
    pub(crate) target_module: &'a str,
    /// The target entity's tree.
    pub(crate) target: &'a EntityTree,
}

/// Imports the include hydration needs: the target's response DTO (and its
/// child DTOs) from the target module's `dto_response`, plus the target's
/// cornucopia query module for the child-table reads. Self-targets (a
/// self-referencing FK) already carry both through the header.
pub(crate) fn emit_include_imports(
    tree: &EntityTree,
    domain: &str,
    includes: &[IncludeAdapterPlan<'_>],
    code: &mut CodeWriter,
) {
    if includes.is_empty() {
        return;
    }
    let mut seen: std::collections::HashSet<String> =
        crate::ddd::repository_emitter::flatten_child_tables(&tree.child_tables)
            .into_iter()
            .map(|c| format!("{}Response", c.struct_name))
            .collect();
    seen.insert(format!("{}Response", tree.entity_name));
    let mut seen_modules: std::collections::HashSet<String> = std::collections::HashSet::new();
    for inc in includes {
        let same_module = inc.target_domain == domain && inc.target_module == tree.module_name;
        if same_module {
            continue;
        }
        if seen.insert(inc.response_type.to_string()) {
            wln!(
                code,
                "use crate::domain::{}::{}::dto_response::{};",
                inc.target_domain,
                inc.target_module,
                inc.response_type
            );
        }
        for child in crate::ddd::repository_emitter::flatten_child_tables(&inc.target.child_tables)
        {
            let name = format!("{}Response", child.struct_name);
            if seen.insert(name.clone()) {
                wln!(
                    code,
                    "use crate::domain::{}::{}::dto_response::{};",
                    inc.target_domain,
                    inc.target_module,
                    name
                );
            }
        }
        if seen_modules.insert(format!("{}::{}", inc.target_domain, inc.target_module)) {
            wln!(
                code,
                "use cornucopia_queries::queries::{}::{};",
                inc.target_domain,
                inc.target_module
            );
        }
    }
}

/// Emit the inherent include-fetch impl block.
///
/// GET: one target row addressed through the source's own FK column.
/// LIST: one row per source id, folded into
/// `HashMap<Uuid, Option<{Target}Response>>` — the same keying the SeaORM
/// batch fetch returns, so the `included` envelope serializes identically.
pub(crate) fn emit_include_adapter(
    tree: &EntityTree,
    includes: &[IncludeAdapterPlan<'_>],
    code: &mut CodeWriter,
) {
    let qmod = qmod(tree);
    wln!(code);
    wln!(
        code,
        "/// `?include=` hydration helpers (issue #781 P1).\n\
         ///\n\
         /// The response mapping is the SAME `emit_response_expr` chain the CRUD\n\
         /// reads use, so columns, codelists/numerics, and child tables hydrate\n\
         /// identically. Child-table reads run per row (one query per row/child),\n\
         /// matching the SeaORM include fetches' N+1 shape.\n\
         impl {entity_name}RepositoryImpl {{",
        entity_name = tree.entity_name
    );
    for inc in includes {
        let resp = inc.response_type;
        // ── single (GET) ────────────────────────────────────────────────
        wln!(code);
        wln!(
            code,
            "    pub async fn {}<C: GenericClient + Sync>(\n\
             \x20       &self,\n\
             \x20       db: &C,\n\
             \x20       source_id: Uuid,\n\
             \x20   ) -> Result<Option<{resp}>, Box<dyn std::error::Error>> {{",
            inc.fetch_method
        );
        wln!(
            code,
            "        let rows = {qmod}::{query_fn}().bind(db, &source_id).all().await.map_err(|e| e.to_string())?;",
            query_fn = inc.query_fn
        );
        wln!(code, "        let mut out: Option<{resp}> = None;");
        wln!(code, "        for row in rows {{");
        emit_response_expr(
            inc.target,
            code,
            "row",
            "            ",
            "            let resp = ",
        );
        wln!(code, "            ;");
        wln!(code, "            out = Some(resp);");
        wln!(code, "        }}");
        wln!(code, "        Ok(out)");
        wln!(code, "    }}");

        // ── batch (LIST) ────────────────────────────────────────────────
        wln!(code);
        wln!(
            code,
            "    pub async fn {}<C: GenericClient + Sync>(\n\
             \x20       &self,\n\
             \x20       db: &C,\n\
             \x20       source_ids: &[Uuid],\n\
             \x20   ) -> Result<std::collections::HashMap<Uuid, Option<{resp}>>, Box<dyn std::error::Error>> {{",
            inc.batch_fetch_method
        );
        wln!(
            code,
            "        let rows = {qmod}::{batch_query_fn}().bind(db, source_ids).all().await.map_err(|e| e.to_string())?;",
            batch_query_fn = inc.batch_query_fn
        );
        wln!(
            code,
            "        let mut result: std::collections::HashMap<Uuid, Option<{resp}>> = std::collections::HashMap::new();"
        );
        wln!(code, "        for id in source_ids {{");
        wln!(code, "            result.insert(*id, None);");
        wln!(code, "        }}");
        wln!(code, "        for row in rows {{");
        emit_response_expr(
            inc.target,
            code,
            "row",
            "            ",
            "            let resp = ",
        );
        wln!(code, "            ;");
        wln!(
            code,
            "            result.insert(row.source_id, Some(resp));"
        );
        wln!(code, "        }}");
        wln!(code, "        Ok(result)");
        wln!(code, "    }}");
    }
    wln!(code, "}}");
}

#[cfg(test)]
mod include_adapter_tests {
    use super::*;

    fn tree(entity: &str, table: &str) -> EntityTree {
        EntityTree {
            entity_name: entity.to_string(),
            module_name: table.to_string(),
            schema_name: "common".to_string(),
            table_name: table.to_string(),
            entity_module: format!("common_{table}"),
            namespace: None,
            direct_columns: vec![],
            child_tables: vec![],
            junction_tables: vec![],
            has_create: true,
            has_read: true,
            has_update: false,
            has_delete: false,
            has_list: true,
            has_workflow: false,
            has_fts: false,
            has_embeddings: false,
            fts_language: "english".to_string(),
            is_auditable: true,
            soft_delete_visibility: "exclude_by_default".to_string(),
            soft_delete_column: Some("deleted_at".to_string()),
            soft_delete_cascade: "restrict".to_string(),
            track_updated_user: false,
            track_deleted_user: false,
            append_only: false,
            filter_fields: vec![],
            nested_filter_fields: vec![],
            parent_ref: None,
            hierarchy_field: None,
            tree_include: vec![],
            design_finders: vec![],
        }
    }

    fn plan<'a>(target: &'a EntityTree, response: &'a str) -> IncludeAdapterPlan<'a> {
        IncludeAdapterPlan {
            query_fn: "include_person_for_worker".to_string(),
            batch_query_fn: "include_person_batch_for_worker".to_string(),
            fetch_method: "fetch_person_for_worker",
            batch_fetch_method: "fetch_person_batch_for_worker",
            response_type: response,
            target_domain: "common",
            target_module: "person",
            target,
        }
    }

    /// The adapter methods call the generated annotated queries and key the
    /// batch map by `source_id` (the LIST envelope's contract).
    #[test]
    fn include_adapter_calls_the_generated_queries() {
        let target = tree("Person", "person");
        let includes = vec![plan(&target, "PersonResponse")];
        let mut code = CodeWriter::new();
        emit_include_adapter(&tree("Worker", "worker"), &includes, &mut code);
        let out = code.as_str();

        assert!(
            out.contains("impl WorkerRepositoryImpl {"),
            "inherent impl (the trait stays CRUD-only):\n{out}"
        );
        assert!(
            out.contains("pub async fn fetch_person_for_worker<C: GenericClient + Sync>("),
            "{out}"
        );
        assert!(
            out.contains(
                "let rows = worker::include_person_for_worker().bind(db, &source_id).all().await.map_err(|e| e.to_string())?;"
            ),
            "{out}"
        );
        assert!(
            out.contains("let mut out: Option<PersonResponse> = None;"),
            "{out}"
        );
        assert!(
            out.contains(
                "let rows = worker::include_person_batch_for_worker().bind(db, source_ids).all().await.map_err(|e| e.to_string())?;"
            ),
            "{out}"
        );
        assert!(
            out.contains(
                "for id in source_ids {\n            result.insert(*id, None);\n        }"
            ),
            "batch seeds every requested id with None:\n{out}"
        );
        assert!(
            out.contains("result.insert(row.source_id, Some(resp));"),
            "{out}"
        );
        assert!(
            out.contains(
                "Result<std::collections::HashMap<Uuid, Option<PersonResponse>>, Box<dyn std::error::Error>>"
            ),
            "{out}"
        );
    }

    /// The import surface pulls the target's response DTO and query module —
    /// the two things the reused `emit_response_expr` chain needs.
    #[test]
    fn include_adapter_imports_the_target_surface() {
        let target = tree("Person", "person");
        let includes = vec![plan(&target, "PersonResponse")];
        let mut code = CodeWriter::new();
        emit_include_imports(&tree("Worker", "worker"), "common", &includes, &mut code);
        let out = code.as_str();

        assert!(
            out.contains("use crate::domain::common::person::dto_response::PersonResponse;"),
            "{out}"
        );
        assert!(
            out.contains("use cornucopia_queries::queries::common::person;"),
            "{out}"
        );
        // Idempotent across plans targeting the same module.
        let mut code2 = CodeWriter::new();
        emit_include_imports(
            &tree("Worker", "worker"),
            "common",
            &[
                plan(&target, "PersonResponse"),
                plan(&target, "PersonResponse"),
            ],
            &mut code2,
        );
        assert_eq!(
            code2
                .as_str()
                .matches("use crate::domain::common::person")
                .count(),
            1,
            "{}",
            code2.as_str()
        );
    }

    /// A self-referencing include already has both imports through the
    /// header — re-importing them is E0252.
    #[test]
    fn include_adapter_skips_self_target_imports() {
        let target = tree("Worker", "worker");
        let includes = vec![IncludeAdapterPlan {
            target_module: "worker",
            target_domain: "common",
            ..plan(&target, "WorkerResponse")
        }];
        let mut code = CodeWriter::new();
        emit_include_imports(&tree("Worker", "worker"), "common", &includes, &mut code);
        assert_eq!(code.as_str(), "");
    }
}
