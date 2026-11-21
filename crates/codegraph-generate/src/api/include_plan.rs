//! `?include=` hydration lowering (issue #781, wave 2 / T2.1).
//!
//! The handler template used to branch inline on
//! `project.persistence_provider` to decide whether `?include=` hydration is
//! available (`handler.tera`'s two provider branches: GET-by-id and LIST).
//! This module makes that decision in Rust and pre-renders the provider's
//! hydration block so the template renders one already-computed fragment.
//!
//! The SeaORM blocks are a verbatim port of the template's SeaORM branches
//! (including comments and the dot-notation accumulation merge).
//!
//! ## Cornucopia: fail closed (issue #781 P0)
//!
//! The cornucopia SQL-first repositories do not implement include hydration,
//! so [`build_include_plan`] returns [`IncludeSupport::Unsupported`] for a
//! cornucopia project: a request that ASKS for `?include=` answers a 501
//! naming the provider and issue #781 instead of silently returning empty
//! include data. The guard is `if !include_paths.is_empty()` — a request
//! without includes proceeds normally (the handler region is emitted whenever
//! the ENTITY models include paths, not only when the request asks for them).
//! [`IncludeSupport::Cornucopia`] remains reachable through
//! [`build_include_plan_with_support`] and keeps the historical empty-data
//! shape (no production path produces it any more).
//!
//! ## What the blocks do NOT contain
//!
//! The template keeps the parts that are provider-independent and must run
//! for every provider:
//!
//! * GET — the `let include_paths: Vec<String> = params.include…` parsing,
//!   the `ALLOWED_INCLUDE_KEYS` / max-depth validation loops, and the
//!   `let mut included = {Entity}IncludedData { … };` initialisation.
//! * LIST — the `let include_paths: Vec<String> = params.include…` parsing
//!   and the `ALLOWED_INCLUDE_KEYS` validation loop (hoisted out of the
//!   provider branch when the block is wired).
//!
//! ## Whitespace contract
//!
//! The blocks are inserted into a generated Rust file, so they are
//! whitespace-exact:
//!
//! * the first line carries the handler-fn-body indentation (4 spaces), and
//!   nested lines carry the indentation the template branch emitted;
//! * a block has NO trailing newline (the template supplies the line break,
//!   matching the `GuardBlocks` / invariant-block convention);
//! * the Tera tag artifacts of the old inline branches (the blank lines the
//!   `{% if %}` / `{% for %}` lines produced in the rendered output) are not
//!   reproduced — the wiring task deliberately emits clean Rust, so its
//!   fixture re-bless is enumerated.

use crate::api::include_path::ResolvedIncludePath;
use crate::project_config::ProjectConfig;

/// Which persistence provider backed the pre-rendered hydration blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncludeSupport {
    /// The SeaORM repository plane: per-id fetch on GET, batch fetch on LIST.
    SeaOrm,
    /// The cornucopia SQL-first plane: hydration is not implemented yet. No
    /// production path produces this verdict any more (issue #781 makes
    /// cornucopia `Unsupported`); the historical empty-data block is kept for
    /// callers that inject the verdict explicitly.
    Cornucopia,
    /// The provider cannot hydrate `?include=` at all: a request that asks
    /// for includes fails closed with a 501 naming the provider and issue
    /// #781. A request WITHOUT includes proceeds normally.
    Unsupported { reason: String },
}

impl IncludeSupport {
    /// True when the provider renders real hydration code.
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::SeaOrm)
    }
}

/// The pre-rendered `?include=` hydration blocks for one entity handler.
#[derive(Debug, Clone)]
pub struct IncludePlan {
    /// Which provider's blocks were rendered.
    pub support: IncludeSupport,
    /// The exact Rust lines the GET-by-id handler renders for include
    /// hydration (the provider-selected block, no surrounding template
    /// conditionals, no trailing newline). Empty when no include paths are
    /// modelled — the handler needs no hydration.
    pub get_block: String,
    /// The exact Rust lines the LIST handler renders for include hydration.
    /// Empty when no include paths are modelled.
    pub list_block: String,
    /// The Rust doc-comment LINE (leading `    /// ` included) that replaces
    /// the `?include=` description on the shared query-params struct. Empty
    /// when no include paths are modelled. SeaORM keeps the historical
    /// "eager-load …" line byte-for-byte; an unsupported provider states
    /// that includes are modelled but answer 501 (issue #781).
    pub include_doc: String,
}

/// The handler context the hydration blocks are rendered against.
pub struct IncludePlanCtx<'a> {
    /// The entity's Rust type stem, e.g. `Worker` (the template's
    /// `entity_name`).
    pub entity_name: &'a str,
    /// The bounded-context domain key, e.g. `common`.
    pub domain: &'a str,
    /// The entity's Rust module name, e.g. `worker`.
    pub module_name: &'a str,
    /// The entity role (`root` / `child` / …). Carried for parity with the
    /// handler context; the current blocks do not branch on it.
    pub role: &'a str,
    /// Whether the entity is auditable. Carried for parity with the handler
    /// context; the current blocks do not branch on it.
    pub is_auditable: bool,
}

/// The provider's `?include=` support verdict, independent of any entity —
/// the ONE predicate every consumer reads (the handler's hydration plan and
/// the generated hurl contract's include legs), so a contract can never probe
/// a surface the handler refuses.
pub fn include_support_for(project: &ProjectConfig) -> IncludeSupport {
    if project.is_cornucopia() {
        IncludeSupport::Unsupported {
            reason: CORNUCOPIA_UNSUPPORTED_REASON.to_string(),
        }
    } else {
        IncludeSupport::SeaOrm
    }
}

/// Build the include plan for one entity, deriving provider support from
/// `project` (`PersistenceProvider` has exactly two variants today).
///
/// Cornucopia is UNSUPPORTED (issue #781 P0): its SQL-first repositories have
/// no include fetch plane, so asking for `?include=` must fail closed rather
/// than silently return empty data.
pub fn build_include_plan(
    paths: &[ResolvedIncludePath],
    ctx: &IncludePlanCtx<'_>,
    project: &ProjectConfig,
) -> IncludePlan {
    build_include_plan_with_support(include_support_for(project), paths, ctx)
}

/// The cornucopia verdict's reason (rendered into the 501 message).
const CORNUCOPIA_UNSUPPORTED_REASON: &str = "the cornucopia SQL-first persistence provider does not implement `?include=` hydration (issue #781)";

/// [`build_include_plan`] with an explicit support verdict — the entry point
/// for a provider the caller cannot lower (see [`IncludeSupport::Unsupported`]).
pub fn build_include_plan_with_support(
    support: IncludeSupport,
    paths: &[ResolvedIncludePath],
    ctx: &IncludePlanCtx<'_>,
) -> IncludePlan {
    if paths.is_empty() {
        // No modelled include paths ⇒ no hydration code at all. The provider
        // verdict is still reported so callers can assert the dependency.
        return IncludePlan {
            support,
            get_block: String::new(),
            list_block: String::new(),
            include_doc: String::new(),
        };
    }
    IncludePlan {
        get_block: render_get_block(paths, ctx, &support),
        list_block: render_list_block(paths, ctx, &support),
        include_doc: include_doc_for(&support).to_string(),
        support,
    }
}

// =============================================================================
// GET-by-id
// =============================================================================

fn render_get_block(
    paths: &[ResolvedIncludePath],
    ctx: &IncludePlanCtx<'_>,
    support: &IncludeSupport,
) -> String {
    match support {
        IncludeSupport::SeaOrm => render_get_sea_orm(paths, ctx),
        IncludeSupport::Cornucopia => CORNUCOPIA_GET_BLOCK.to_string(),
        IncludeSupport::Unsupported { reason } => render_unsupported_get_block(reason),
    }
}

fn render_get_sea_orm(paths: &[ResolvedIncludePath], ctx: &IncludePlanCtx<'_>) -> String {
    let mut out = String::new();
    out.push_str("    use sea_orm::TransactionTrait;\n");
    out.push_str("    let tx = state.db.begin().await\n");
    out.push_str(
        "        .map_err(|e| AppError::internal(format!(\"Failed to begin transaction: {e}\"))\n",
    );
    out.push_str("            .with_correlation_id(correlation_id))?;\n");
    out.push_str(&format!(
        "    let repo = crate::domain::{}::{}::repository_impl::{}RepositoryImpl;\n",
        ctx.domain, ctx.module_name, ctx.entity_name
    ));
    out.push_str("    for path in &include_paths {\n");
    out.push_str("        match path.as_str() {\n");
    for ipath in paths {
        let Some(first) = ipath.segments.first() else {
            continue;
        };
        out.push_str(&format!("            \"{}\" => {{\n", ipath.alias));
        if ipath.segments.len() == 1 {
            if first.is_array {
                out.push_str(&format!(
                    "                included.{} = Some(repo.{}(&tx, id).await\n",
                    first.module_name, ipath.fetch_method
                ));
                out.push_str(&map_err_line(20, &ipath.alias));
                out.push_str("                        .with_correlation_id(correlation_id))?);\n");
            } else {
                out.push_str(&format!(
                    "                included.{} = repo.{}(&tx, id).await\n",
                    first.module_name, ipath.fetch_method
                ));
                out.push_str(&map_err_line(20, &ipath.alias));
                out.push_str("                        .with_correlation_id(correlation_id))?;\n");
            }
        } else {
            // The template merges only the first two segments (depth > 2
            // paths keep the same leaf); ported verbatim.
            let leaf = &ipath.segments[1];
            out.push_str("                // Dot-notation path: merge into combined response\n");
            out.push_str(&format!(
                "                let leaf_resp = repo.{}(&tx, id).await\n",
                ipath.fetch_method
            ));
            out.push_str(&map_err_line(20, &ipath.alias));
            out.push_str("                        .with_correlation_id(correlation_id))?;\n");
            out.push_str("                if let Some(val) = leaf_resp {\n");
            out.push_str(&format!(
                "                    if let Some(ref mut combined) = included.{} {{\n",
                first.module_name
            ));
            out.push_str(&format!(
                "                        combined.{} = val.{};\n",
                leaf.module_name, leaf.module_name
            ));
            out.push_str("                    } else {\n");
            out.push_str(&format!(
                "                        included.{} = Some({} {{\n",
                first.module_name, ipath.response_rust_type
            ));
            out.push_str(&format!(
                "                            {}: val.{},\n",
                leaf.module_name, leaf.module_name
            ));
            out.push_str("                            ..Default::default()\n");
            out.push_str("                        });\n");
            out.push_str("                    }\n");
            out.push_str("                }\n");
        }
        out.push_str("            }\n");
    }
    out.push_str("            _ => {}\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("    tx.commit().await\n");
    out.push_str(
        "        .map_err(|e| AppError::internal(format!(\"Failed to commit transaction: {e}\"))\n",
    );
    out.push_str("            .with_correlation_id(correlation_id))?;");
    out
}

// =============================================================================
// LIST
// =============================================================================

fn render_list_block(
    paths: &[ResolvedIncludePath],
    ctx: &IncludePlanCtx<'_>,
    support: &IncludeSupport,
) -> String {
    match support {
        IncludeSupport::SeaOrm => render_list_sea_orm(paths, ctx),
        IncludeSupport::Cornucopia => CORNUCOPIA_LIST_BLOCK.to_string(),
        IncludeSupport::Unsupported { reason } => render_unsupported_list_block(reason),
    }
}

fn render_list_sea_orm(paths: &[ResolvedIncludePath], ctx: &IncludePlanCtx<'_>) -> String {
    let mut out = String::new();
    out.push_str("    let included = if !include_paths.is_empty() {\n");
    out.push_str("        use sea_orm::TransactionTrait;\n");
    out.push_str("        let tx = state.db.begin().await\n");
    out.push_str(
        "            .map_err(|e| AppError::internal(format!(\"Failed to begin transaction for includes: {e}\"))\n",
    );
    out.push_str("                .with_correlation_id(correlation_id))?;\n");
    out.push_str(&format!(
        "        let repo = crate::domain::{}::{}::repository_impl::{}RepositoryImpl;\n",
        ctx.domain, ctx.module_name, ctx.entity_name
    ));
    out.push_str("        let ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();\n");
    out.push_str(
        "        let mut included_map = std::collections::HashMap::<String, serde_json::Value>::new();\n",
    );
    // NOTE: the template's `ALLOWED_INCLUDE_KEYS` loop is NOT part of the
    // block — the wiring task keeps it (provider-independent validation).
    for ipath in paths {
        let Some(first) = ipath.segments.first() else {
            continue;
        };
        if ipath.segments.len() == 1 {
            out.push_str(&format!(
                "        if include_paths.contains(&\"{}\".to_string()) {{\n",
                ipath.alias
            ));
            out.push_str("            included_map.insert(\n");
            out.push_str(&format!(
                "                \"{}\".to_string(),\n",
                first.module_name
            ));
            out.push_str(&format!(
                "                serde_json::to_value(repo.{}(&tx, &ids).await\n",
                ipath.batch_fetch_method
            ));
            out.push_str(&map_err_line(20, &ipath.alias));
            out.push_str(
                "                        .with_correlation_id(correlation_id))?).unwrap_or_default(),\n",
            );
            out.push_str("            );\n");
            out.push_str("        }\n");
        } else {
            let leaf = &ipath.segments[1];
            out.push_str(
                "        // Dot-notation path: batch-fetch the combined struct per source id and\n",
            );
            out.push_str(
                "        // merge each leaf field into `included.<segments[0]>[<source_id>]` so\n",
            );
            out.push_str(
                "        // sibling dot paths sharing the first segment accumulate instead of\n",
            );
            out.push_str("        // clobbering each other.\n");
            out.push_str(&format!(
                "        if include_paths.contains(&\"{}\".to_string()) {{\n",
                ipath.alias
            ));
            out.push_str(&format!(
                "            let batch = repo.{}(&tx, &ids).await\n",
                ipath.batch_fetch_method
            ));
            out.push_str(&map_err_line(16, &ipath.alias));
            out.push_str("                    .with_correlation_id(correlation_id))?;\n");
            out.push_str("            let entry = included_map\n");
            out.push_str(&format!(
                "                .entry(\"{}\".to_string())\n",
                first.module_name
            ));
            out.push_str("                .or_insert_with(|| serde_json::json!({}));\n");
            out.push_str("            if let Some(by_source) = entry.as_object_mut() {\n");
            out.push_str("                for (source_id, combined) in batch {\n");
            out.push_str(&format!(
                "                    let leaf = serde_json::json!(combined.{});\n",
                leaf.module_name
            ));
            out.push_str("                    let leaf_entry = by_source\n");
            out.push_str("                        .entry(source_id.to_string())\n");
            out.push_str("                        .or_insert_with(|| serde_json::json!({}));\n");
            out.push_str(
                "                    if let Some(leaf_obj) = leaf_entry.as_object_mut() {\n",
            );
            out.push_str("                        leaf_obj.insert(\n");
            out.push_str(&format!(
                "                            \"{}\".to_string(),\n",
                leaf.module_name
            ));
            out.push_str("                            leaf,\n");
            out.push_str("                        );\n");
            out.push_str("                    }\n");
            out.push_str("                }\n");
            out.push_str("            }\n");
            out.push_str("        }\n");
        }
    }
    out.push_str("        tx.commit().await\n");
    out.push_str(
        "            .map_err(|e| AppError::internal(format!(\"Failed to commit transaction: {e}\"))\n",
    );
    out.push_str("                .with_correlation_id(correlation_id))?;\n");
    out.push_str("        included_map\n");
    out.push_str("    } else {\n");
    out.push_str("        std::collections::HashMap::new()\n");
    out.push_str("    };");
    out
}

// =============================================================================
// Provider branches without hydration / without support
// =============================================================================

/// The current cornucopia GET block: hydration is not implemented on the
/// SQL-first repositories, so `included` stays all-`None`. Ported verbatim
/// from `handler.tera` (the comment lines are the contract the wiring task
/// keeps byte-identical).
///
/// NB: the literal must NOT use a `"\` line continuation for its first line —
/// Rust strips the following line's indentation, which would emit the comment
/// at column 0.
const CORNUCOPIA_GET_BLOCK: &str =
    "    // Cornucopia provider: include fetch methods are not implemented on the
    // SQL-first repositories yet; `included` stays all-None (validation of
    // the requested paths above still applies).
    // TODO(worker-cornucopia-includes): resolve includes through the
    // cornucopia query layer (later epic step).";

/// The current cornucopia LIST block: includes stay empty, with the
/// historical comment marker (`handler.tera`). The template's
/// `ALLOWED_INCLUDE_KEYS` loop is NOT reproduced (validation is hoisted out
/// of the provider branch by the wiring task).
const CORNUCOPIA_LIST_BLOCK: &str =
    "    // Cornucopia provider: batch include fetches are not implemented on the
    // SQL-first repositories yet; includes stay empty (path validation above
    // still applies).
    // TODO(worker-cornucopia-includes): resolve includes through the
    // cornucopia query layer (later epic step).
    let included = std::collections::HashMap::<String, serde_json::Value>::new();";

/// A fail-closed 501 (issue #781): a provider that cannot hydrate includes
/// must never answer with silently-empty include data.
///
/// The block lives in the handler's `has_include` region, which is emitted
/// whenever the ENTITY models include paths — so it must be GUARDED on the
/// request actually asking for some: a request without `?include=` proceeds
/// normally.
fn render_unsupported_get_block(reason: &str) -> String {
    let mut out = render_unsupported_guard(reason);
    out.push_str("\n    }");
    out
}

/// The LIST arm: same guard, then the empty map the Ok envelope always
/// references (a request with includes never reaches it — it returned 501).
fn render_unsupported_list_block(reason: &str) -> String {
    let mut out = render_unsupported_guard(reason);
    out.push_str("\n    }");
    out.push_str(
        "\n    let included = std::collections::HashMap::<String, serde_json::Value>::new();",
    );
    out
}

/// The shared guarded 501 body: `if !include_paths.is_empty() { … return
/// Err(501) … }` at the handler-fn-body indent, WITHOUT the closing brace
/// (the caller appends it, plus any provider-specific tail).
fn render_unsupported_guard(reason: &str) -> String {
    // The reason names issue #781 already for the cornucopia verdict; keep
    // the reference exactly once for both paths.
    let message = if reason.contains("#781") {
        format!("?include= hydration is not supported: {reason}")
    } else {
        format!("?include= hydration is not supported: {reason} (issue #781)")
    };
    let mut out = String::new();
    out.push_str("    if !include_paths.is_empty() {\n");
    out.push_str(
        "        // `?include=` hydration is not supported by this persistence provider\n",
    );
    out.push_str(
        "        // (issue #781): fail closed instead of silently returning empty data.\n",
    );
    out.push_str("        // A request without includes proceeds normally.\n");
    out.push_str("        return Err(AppError {\n");
    out.push_str("            status: axum::http::StatusCode::NOT_IMPLEMENTED,\n");
    out.push_str("            code: \"NOT_IMPLEMENTED\",\n");
    out.push_str(&format!(
        "            message: {}.to_string(),\n",
        rust_string_literal(&message)
    ));
    out.push_str("            details: None,\n");
    out.push_str("            correlation_id: Some(correlation_id),\n");
    out.push_str("        });");
    out
}

/// The `?include=` doc-comment line rendered into the query-params struct.
///
/// * SeaORM / the historical cornucopia arm: the pre-#781 line, byte-exact.
/// * Unsupported: honest — includes are modelled but the deployment's
///   persistence provider cannot serve them, so requests answer 501.
fn include_doc_for(support: &IncludeSupport) -> &'static str {
    match support {
        IncludeSupport::SeaOrm | IncludeSupport::Cornucopia => {
            "    /// Eager-load related entities, e.g. `?include=person,deployment.position`."
        }
        IncludeSupport::Unsupported { .. } => {
            "    /// Includes are modelled for this entity but are not supported by this deployment's persistence provider; a request that sends `?include=` answers 501 (issue #781)."
        }
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// The shared `.map_err(|e| AppError::internal(format!("Failed to include
/// {alias}: {e}"))` line at `indent` spaces.
fn map_err_line(indent: usize, alias: &str) -> String {
    format!(
        "{}.map_err(|e| AppError::internal(format!(\"Failed to include {}: {{e}}\"))\n",
        " ".repeat(indent),
        alias
    )
}

/// Escape a value for embedding in a generated Rust string literal.
fn rust_string_literal(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::include_path::IncludeSegment;

    fn segment(module_name: &str, is_array: bool) -> IncludeSegment {
        IncludeSegment {
            entity_name: module_name.to_string(),
            schema_title: format!("{module_name}Type"),
            module_name: module_name.to_string(),
            domain: "common".to_string(),
            table: format!("\"common\".\"{module_name}\""),
            fk_column: format!("{module_name}_id"),
            reverse_fk_column: "worker_id".to_string(),
            fk_is_required: false,
            reverse_fk_is_required: false,
            is_array,
            child_table_override: None,
        }
    }

    fn path(alias: &str, segments: Vec<IncludeSegment>) -> ResolvedIncludePath {
        let snake = alias.replace('.', "_");
        ResolvedIncludePath {
            alias: alias.to_string(),
            segments,
            response_rust_type: format!("{}CombinedResponse", pascal(alias)),
            fetch_method: format!("fetch_{snake}_for_worker"),
            batch_fetch_method: format!("fetch_{snake}_batch_for_worker"),
        }
    }

    fn pascal(alias: &str) -> String {
        alias
            .split('.')
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect()
    }

    fn ctx() -> IncludePlanCtx<'static> {
        IncludePlanCtx {
            entity_name: "Worker",
            domain: "common",
            module_name: "worker",
            role: "root",
            is_auditable: true,
        }
    }

    fn cornucopia_project() -> ProjectConfig {
        use crate::profile::PersistenceProvider;

        ProjectConfig {
            database: crate::project_config::DatabaseConfig {
                persistence_provider: PersistenceProvider::Cornucopia,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn sea_orm_plan(paths: &[ResolvedIncludePath]) -> IncludePlan {
        build_include_plan(paths, &ctx(), &ProjectConfig::default())
    }

    // ── GET: one-segment scalar ──────────────────────────────────────────

    #[test]
    fn get_single_scalar_path_uses_the_per_id_fetch() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let plan = sea_orm_plan(&paths);
        assert!(plan.support.is_supported());
        assert!(
            plan.get_block
                .contains("included.person = repo.fetch_person_for_worker(&tx, id).await"),
            "{}",
            plan.get_block
        );
        assert!(plan.get_block.contains(
            ".map_err(|e| AppError::internal(format!(\"Failed to include person: {e}\"))"
        ));
        // No batch fetch / map on the GET arm.
        assert!(!plan.get_block.contains("included_map"));
        assert!(!plan.get_block.contains("batch_for_worker"));
        // No trailing newline (the template supplies the line break).
        assert!(!plan.get_block.ends_with('\n'));
    }

    #[test]
    fn get_single_scalar_path_block_is_exact() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let plan = sea_orm_plan(&paths);
        let expected = [
            "    use sea_orm::TransactionTrait;",
            "    let tx = state.db.begin().await",
            "        .map_err(|e| AppError::internal(format!(\"Failed to begin transaction: {e}\"))",
            "            .with_correlation_id(correlation_id))?;",
            "    let repo = crate::domain::common::worker::repository_impl::WorkerRepositoryImpl;",
            "    for path in &include_paths {",
            "        match path.as_str() {",
            "            \"person\" => {",
            "                included.person = repo.fetch_person_for_worker(&tx, id).await",
            "                    .map_err(|e| AppError::internal(format!(\"Failed to include person: {e}\"))",
            "                        .with_correlation_id(correlation_id))?;",
            "            }",
            "            _ => {}",
            "        }",
            "    }",
            "    tx.commit().await",
            "        .map_err(|e| AppError::internal(format!(\"Failed to commit transaction: {e}\"))",
            "            .with_correlation_id(correlation_id))?;",
        ]
        .join("\n");
        assert_eq!(plan.get_block, expected);
    }

    // ── GET: one-segment array ──────────────────────────────────────────

    #[test]
    fn get_single_array_path_wraps_the_fetch_in_some() {
        let paths = vec![path("deployment", vec![segment("deployment", true)])];
        let plan = sea_orm_plan(&paths);
        assert!(
            plan.get_block.contains(
                "included.deployment = Some(repo.fetch_deployment_for_worker(&tx, id).await"
            ),
            "{}",
            plan.get_block
        );
        assert!(
            plan.get_block
                .contains("                        .with_correlation_id(correlation_id))?);")
        );
        assert!(!plan.get_block.contains("batch_for_worker"));
    }

    // ── GET: dot-notation merge ─────────────────────────────────────────

    #[test]
    fn get_dot_path_merges_into_the_combined_struct() {
        let paths = vec![path(
            "deployment.position",
            vec![segment("deployment", false), segment("position", false)],
        )];
        let plan = sea_orm_plan(&paths);
        assert!(
            plan.get_block
                .contains("// Dot-notation path: merge into combined response")
        );
        assert!(
            plan.get_block.contains(
                "let leaf_resp = repo.fetch_deployment_position_for_worker(&tx, id).await"
            )
        );
        assert!(
            plan.get_block
                .contains("if let Some(ref mut combined) = included.deployment {")
        );
        assert!(plan.get_block.contains("combined.position = val.position;"));
        assert!(
            plan.get_block
                .contains("included.deployment = Some(DeploymentPositionCombinedResponse {")
        );
        assert!(plan.get_block.contains("position: val.position,"));
        assert!(plan.get_block.contains("..Default::default()"));
    }

    #[test]
    fn get_two_dot_paths_sharing_a_segment_accumulate_on_the_same_entry() {
        let paths = vec![
            path(
                "deployment.position",
                vec![segment("deployment", false), segment("position", false)],
            ),
            path(
                "deployment.assignment",
                vec![segment("deployment", false), segment("assignment", false)],
            ),
        ];
        let plan = sea_orm_plan(&paths);
        assert!(plan.get_block.contains("combined.position = val.position;"));
        assert!(
            plan.get_block
                .contains("combined.assignment = val.assignment;")
        );
        // Both arms bind `included.deployment` (accumulation, not clobbering).
        assert_eq!(
            plan.get_block
                .matches("if let Some(ref mut combined) = included.deployment {")
                .count(),
            2
        );
        assert_eq!(
            plan.get_block
                .matches("included.deployment = Some(")
                .count(),
            2
        );
    }

    // ── LIST ────────────────────────────────────────────────────────────

    #[test]
    fn list_single_path_uses_the_batch_fetch() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let plan = sea_orm_plan(&paths);
        assert!(
            plan.list_block
                .contains("let included = if !include_paths.is_empty() {")
        );
        assert!(
            plan.list_block
                .contains("let ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();")
        );
        assert!(
            plan.list_block
                .contains("if include_paths.contains(&\"person\".to_string()) {")
        );
        assert!(
            plan.list_block.contains(
                "serde_json::to_value(repo.fetch_person_batch_for_worker(&tx, &ids).await"
            )
        );
        assert!(plan.list_block.contains(".unwrap_or_default(),"));
        assert!(
            plan.list_block
                .contains("        std::collections::HashMap::new()")
        );
        // The template keeps the ALLOWED_INCLUDE_KEYS loop — never emitted here.
        assert!(!plan.list_block.contains("ALLOWED_INCLUDE_KEYS"));
        assert!(!plan.get_block.contains("ALLOWED_INCLUDE_KEYS"));
        assert!(!plan.list_block.ends_with('\n'));
    }

    #[test]
    fn list_single_path_block_is_exact() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let plan = sea_orm_plan(&paths);
        let expected = [
            "    let included = if !include_paths.is_empty() {",
            "        use sea_orm::TransactionTrait;",
            "        let tx = state.db.begin().await",
            "            .map_err(|e| AppError::internal(format!(\"Failed to begin transaction for includes: {e}\"))",
            "                .with_correlation_id(correlation_id))?;",
            "        let repo = crate::domain::common::worker::repository_impl::WorkerRepositoryImpl;",
            "        let ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();",
            "        let mut included_map = std::collections::HashMap::<String, serde_json::Value>::new();",
            "        if include_paths.contains(&\"person\".to_string()) {",
            "            included_map.insert(",
            "                \"person\".to_string(),",
            "                serde_json::to_value(repo.fetch_person_batch_for_worker(&tx, &ids).await",
            "                    .map_err(|e| AppError::internal(format!(\"Failed to include person: {e}\"))",
            "                        .with_correlation_id(correlation_id))?).unwrap_or_default(),",
            "            );",
            "        }",
            "        tx.commit().await",
            "            .map_err(|e| AppError::internal(format!(\"Failed to commit transaction: {e}\"))",
            "                .with_correlation_id(correlation_id))?;",
            "        included_map",
            "    } else {",
            "        std::collections::HashMap::new()",
            "    };",
        ]
        .join("\n");
        assert_eq!(plan.list_block, expected);
    }

    #[test]
    fn list_dot_path_accumulates_into_included_map() {
        let paths = vec![
            path(
                "deployment.position",
                vec![segment("deployment", false), segment("position", false)],
            ),
            path(
                "deployment.assignment",
                vec![segment("deployment", false), segment("assignment", false)],
            ),
        ];
        let plan = sea_orm_plan(&paths);
        // Both leaf inserts key the same first-segment entry.
        assert_eq!(
            plan.list_block
                .matches(".entry(\"deployment\".to_string())")
                .count(),
            2
        );
        assert!(
            plan.list_block
                .contains("let leaf = serde_json::json!(combined.position);")
        );
        assert!(
            plan.list_block
                .contains("let leaf = serde_json::json!(combined.assignment);")
        );
        assert!(
            plan.list_block
                .contains("                            \"position\".to_string(),")
        );
        assert!(
            plan.list_block
                .contains("                            \"assignment\".to_string(),")
        );
        assert!(plan.list_block.contains(
            "        // sibling dot paths sharing the first segment accumulate instead of"
        ));
    }

    // ── Empty / cornucopia / unsupported ────────────────────────────────

    #[test]
    fn empty_paths_produce_empty_blocks() {
        let plan = sea_orm_plan(&[]);
        assert!(plan.get_block.is_empty());
        assert!(plan.list_block.is_empty());
        assert!(plan.include_doc.is_empty());
        assert_eq!(plan.support, IncludeSupport::SeaOrm);
    }

    /// Issue #781 P0: a cornucopia project's include surface FAILS CLOSED.
    #[test]
    fn cornucopia_project_fails_closed_with_a_guarded_501() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let plan = build_include_plan(&paths, &ctx(), &cornucopia_project());
        assert!(!plan.support.is_supported());
        match &plan.support {
            IncludeSupport::Unsupported { reason } => {
                assert!(reason.contains("cornucopia"), "{reason}");
                assert!(reason.contains("issue #781"), "{reason}");
                assert!(reason.contains("does not implement"), "{reason}");
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
        // Both arms are GUARDED on the request asking for includes: a request
        // without `?include=` must proceed normally.
        for block in [&plan.get_block, &plan.list_block] {
            assert!(
                block.starts_with("    if !include_paths.is_empty() {\n"),
                "{block}"
            );
            assert!(block.contains("        return Err(AppError {"), "{block}");
            assert!(
                block.contains("status: axum::http::StatusCode::NOT_IMPLEMENTED,"),
                "{block}"
            );
            assert!(block.contains("code: \"NOT_IMPLEMENTED\","), "{block}");
            assert!(
                block.contains("correlation_id: Some(correlation_id),"),
                "{block}"
            );
            assert!(block.contains("A request without includes proceeds normally."));
            // No silent empty-data shape left on either arm.
            assert!(!block.contains("TODO(worker-cornucopia-includes)"));
        }
        // The GET arm ends at the guard's closing brace (the template's
        // `included` initialiser + Ok envelope follow).
        assert!(plan.get_block.ends_with("\n    }"), "{}", plan.get_block);
        // The LIST arm defines the empty map the Ok envelope always reads.
        assert!(
            plan.list_block.ends_with(
                "\n    let included = std::collections::HashMap::<String, serde_json::Value>::new();"
            ),
            "{}",
            plan.list_block
        );
    }

    #[test]
    fn explicit_cornucopia_verdict_keeps_the_historical_empty_blocks() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let plan = build_include_plan_with_support(IncludeSupport::Cornucopia, &paths, &ctx());
        assert!(
            matches!(plan.support, IncludeSupport::Cornucopia),
            "historical arm is not the 501 arm"
        );
        // The comment starts at the handler-fn-body indent (4 spaces) — the
        // blocks are inserted verbatim into a generated fn body.
        assert!(plan.get_block.starts_with("    // Cornucopia provider"));
        assert!(plan.list_block.starts_with("    // Cornucopia provider"));
        assert!(plan.get_block.contains("`included` stays all-None"));
        assert!(
            plan.get_block
                .contains("// TODO(worker-cornucopia-includes): resolve includes through the")
        );
        // No SeaORM fetch code leaks into the cornucopia blocks.
        assert!(!plan.get_block.contains("sea_orm"));
        assert!(!plan.list_block.contains("sea_orm"));
        assert!(plan.list_block.contains(
            "let included = std::collections::HashMap::<String, serde_json::Value>::new();"
        ));
    }

    #[test]
    fn unsupported_renders_a_guarded_fail_closed_501() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let support = IncludeSupport::Unsupported {
            reason: "persistence provider `diesel` has no include fetch plane".to_string(),
        };
        let plan = build_include_plan_with_support(support.clone(), &paths, &ctx());
        assert_eq!(plan.support, support);
        for block in [&plan.get_block, &plan.list_block] {
            assert!(block.contains("    if !include_paths.is_empty() {"));
            assert!(block.contains("status: axum::http::StatusCode::NOT_IMPLEMENTED,"));
            assert!(block.contains("code: \"NOT_IMPLEMENTED\","));
            // An injected reason that does not name the issue gets the
            // reference appended exactly once (the comment above is separate).
            assert!(block.contains("(issue #781)"));
            let message_line = block
                .lines()
                .find(|line| line.trim_start().starts_with("message:"))
                .expect("message line");
            assert_eq!(message_line.matches("#781").count(), 1, "{message_line}");
            assert!(block.contains("persistence provider `diesel` has no include fetch plane"));
            assert!(block.contains("correlation_id: Some(correlation_id),"));
        }
        assert!(plan.list_block.ends_with(
            "let included = std::collections::HashMap::<String, serde_json::Value>::new();"
        ));
    }

    // ── Doc line (issue #781) ───────────────────────────────────────────

    #[test]
    fn sea_orm_doc_line_is_the_historical_text() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let plan = sea_orm_plan(&paths);
        assert_eq!(
            plan.include_doc,
            "    /// Eager-load related entities, e.g. `?include=person,deployment.position`."
        );
    }

    #[test]
    fn unsupported_doc_line_is_honest_about_the_501() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let plan = build_include_plan(&paths, &ctx(), &cornucopia_project());
        assert!(
            plan.include_doc
                .starts_with("    /// Includes are modelled for this entity")
        );
        assert!(
            plan.include_doc
                .contains("not supported by this deployment's persistence provider")
        );
        assert!(plan.include_doc.contains("answers 501 (issue #781)"));
        // The historical "eager-load" claim is gone.
        assert!(!plan.include_doc.contains("Eager-load related entities"));
    }
}
