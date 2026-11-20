//! Design-invariant enforcement blocks (DD5, issue #666 slice 4d), split
//! out of the #751 size-budget `api/handler.rs`.

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

use super::resolve_is_auditable;

/// One invariant-carrying aggregate MEMBER (issue #666 slice 4d): a child
/// entity whose invariants the root's save validates pre-write. One
/// routed level (the 4b depth cap — the index already drops deeper
/// chains).
#[derive(Debug, Clone)]
pub(super) struct InvariantMember {
    /// The member's owning domain (same-domain by construction).
    domain: String,
    /// The member's module (its `pg_table_name` — the AppState query slot).
    module: String,
    /// The member→root FK column the scoped fetch filters on.
    fk_column: String,
    /// The member's auditable flag (the query plane's `include_deleted`
    /// argument — the SAME resolution the DDL made).
    auditable: bool,
    /// `true` when the member's ux plan has sortable fields — the query
    /// plane widens `list_filtered` with a `sort: Option<(String, bool)>`
    /// argument (issue #306) that the member fetch must supply.
    ux_sort: bool,
    /// The member's DESIGNED class name (the 409 detail's `class`).
    class: String,
    /// The member's predicate plans, declaration order.
    plans: Vec<crate::ddd::invariants::InvariantPlan>,
}

/// The pre-rendered invariant blocks (issue #666 slice 4d). Empty fields
/// keep the handler byte-identical; every block is whitespace-exact and
/// carries NO trailing newline (the template supplies the line break).
pub(super) struct InvariantBlocks {
    pub(super) create_block: String,
    pub(super) update_block: String,
    /// The child-role update arm's `let existing = ` prefix, claimed by
    /// the invariant plane when the eligibility guards did not claim it.
    /// `None` keeps the guard-side behavior untouched.
    pub(super) update_ownership_prefix: Option<String>,
}

/// Escape a verbatim authored expression into a Rust string literal
/// (backslash, quote, newline) — the 409 message carries it verbatim.
fn rust_string_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

/// The 409 conflict naming the violated invariant: `error.code` is
/// `INVARIANT_VIOLATION` and `error.details` carries `class` /
/// `invariantIndex` / (for a member) `aggregateRoot` as field/message
/// entries — the AppError API's structured-detail surface. Built as a
/// struct literal (the `service_op_not_implemented` precedent): `code` is
/// `&'static str`, so no error.tera constructor is needed and flag-off
/// output stays byte-identical.
fn invariant_violation_error(
    class: &str,
    index: usize,
    expression: &str,
    aggregate_root: Option<&str>,
) -> String {
    let mut details = String::from(
        "Some(vec![\n                    crate::error::FieldError { field: \"class\".to_string(), message: \"",
    );
    details.push_str(class);
    details.push_str("\".to_string() },\n                    crate::error::FieldError { field: \"invariantIndex\".to_string(), message: \"");
    details.push_str(&index.to_string());
    details.push_str("\".to_string() },");
    if let Some(root) = aggregate_root {
        details.push_str("\n                    crate::error::FieldError { field: \"aggregateRoot\".to_string(), message: \"");
        details.push_str(root);
        details.push_str("\".to_string() },");
    }
    details.push_str("\n                ])");
    format!(
        "AppError {{\n                status: axum::http::StatusCode::CONFLICT,\n                code: \"INVARIANT_VIOLATION\",\n                message: \"Invariant {index} of `{class}` violated: {expr}\".to_string(),\n                details: {details},\n                correlation_id: Some(correlation_id),\n            }}",
        expr = rust_string_literal(expression),
    )
}

/// One predicate evaluation line: `if !{call}({var}) {{` … `return Err({err});` `}}`.
fn invariant_check(call: &str, var: &str, err: &str) -> String {
    format!("    if !{call}(&{var}) {{\n        return Err({err});\n    }}",)
}

/// The candidate-construction lines shared by the update and transition
/// arms: the record's JSON with the request patch applied, decoded back
/// into the record type (issue #666 slice 4d — candidate BEFORE write).
fn candidate_lines(record_var: &str, patch_var: &str, entity: &str) -> String {
    format!(
        "    let mut candidate_json = serde_json::to_value(&{record_var})\n        .map_err(|e| {{\n            AppError::internal(format!(\"invariant candidate serialization failed: {{e}}\"))\n                .with_correlation_id(correlation_id)\n        }})?;\n    if let (Some(obj), Ok(patch)) = (\n        candidate_json.as_object_mut(),\n        serde_json::to_value(&{patch_var}),\n    ) {{\n        if let Some(patch_obj) = patch.as_object() {{\n            for (key, value) in patch_obj {{\n                obj.insert(key.clone(), value.clone());\n            }}\n        }}\n    }}\n    let candidate: {entity}Response = serde_json::from_value(candidate_json)\n        .map_err(|e| {{\n            AppError::internal(format!(\"invariant candidate decode failed: {{e}}\"))\n                .with_correlation_id(correlation_id)\n        }})?;",
        entity = entity,
        record_var = record_var,
        patch_var = patch_var,
    )
}

/// The load-or-404 lines a ROOT update arm needs when no eligibility
/// guard already bound `existing` (the #338 block's shape, invariant
/// wording).
fn invariant_load_or_404(
    domain: &str,
    module_name: &str,
    entity: &str,
    is_auditable: bool,
    domain_pascal: &str,
) -> String {
    let audit_arg = if is_auditable { ", false" } else { "" };
    let session_args = "api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()";
    format!(
        "    // Invariant enforcement (issue #666 slice 4d): the candidate is the\n    // loaded record with the request patch applied — load-or-404 first.\n    let existing = state.{domain}_{module}_queries.find_by_id(id{audit}, {session}).await\n        .map_err(|e: {domain_pascal}Error| {{\n                    AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to find {entity}\")\n                        .with_correlation_id(correlation_id)\n                }})?\n        .ok_or_else(|| AppError::not_found(format!(\"{entity} {{id}} not found\"))\n            .with_correlation_id(correlation_id))?;",
        domain = domain,
        module = module_name,
        audit = audit_arg,
        session = session_args,
        domain_pascal = domain_pascal,
        entity = entity,
    )
}

/// The member-validation lines: load each member's rows through the
/// scoped fetch the repository already exposes (`list_filtered` under the
/// parent-FK filter) and evaluate the member predicates pre-write (issue
/// #666 slice 4d — one routed level, the 4b depth cap).
fn member_lines(members: &[InvariantMember], root_entity: &str) -> String {
    let mut out = String::new();
    for member in members {
        out.push_str(&format!(
            "\n    // Aggregate member validation (DD4+DD5, issue #666 slice 4d): the\n    // root save proves `{member}` — every member row's invariants hold\n    // BEFORE the write; a violation names the member class, its invariant\n    // index, and this aggregate root.\n",
            member = member.class,
        ));
        out.push_str(&format!(
            "    let mut member_filters_{module} = std::collections::HashMap::new();\n    member_filters_{module}.insert(\"{fk}\".to_string(), id.to_string());\n    let (member_rows_{module}, _) = state.{domain}_{module}_queries.list_filtered(0, 1000, &member_filters_{module}{audit}{sort}, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await\n        .map_err(|e: {domain_pascal}Error| {{\n            AppError::internal(format!(\"Failed to load {class} members for invariant validation: {{e}}\"))\n                .with_correlation_id(correlation_id)\n        }})?;\n    for member_row in &member_rows_{module} {{\n{checks}\n    }}",
            module = member.module,
            fk = member.fk_column,
            audit = if member.auditable { ", false" } else { "" },
            domain = member.domain,
            domain_pascal = codegraph_naming::to_pascal_case(&member.domain),
            class = member.class,
            checks = member
                .plans
                .iter()
                .map(|plan| {
                    format!(
                        "    {}",
                        invariant_check(
                            &format!("crate::domain::{}::rules::{}", member.domain, plan.fn_name),
                            "member_row",
                            &invariant_violation_error(
                                &member.class,
                                plan.index,
                                &plan.expression,
                                Some(root_entity),
                            ),
                        )
                        .replace('\n', "\n    ")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
            sort = if member.ux_sort { ", None" } else { "" },
        ));
    }
    out.trim_start_matches('\n').to_string()
}

/// Build the pre-rendered invariant blocks for one entity (issue #666
/// slice 4d). `update_existing_bound` mirrors the #338 coordination: when
/// the eligibility guards' update block (root) or their ownership prefix
/// (child) already binds `existing`, the invariant block reuses it;
/// otherwise the block carries its own load (root) or claims the prefix
/// (child).
#[allow(clippy::too_many_arguments)] // TODO(#751): the block facts arrive as one context struct
pub(super) fn build_invariant_blocks(
    role: &str,
    domain: &str,
    module_name: &str,
    entity_name: &str,
    is_auditable: bool,
    has_delete: bool,
    domain_types_crate: &str,
    enforcement: &crate::ddd::invariants::InvariantEnforcement,
    members: &[InvariantMember],
    update_existing_bound: bool,
) -> InvariantBlocks {
    let mut blocks = InvariantBlocks {
        create_block: String::new(),
        update_block: String::new(),
        update_ownership_prefix: None,
    };
    if enforcement.is_empty() && members.is_empty() {
        return blocks;
    }
    let domain_pascal = codegraph_naming::to_pascal_case(domain);
    let crate_name = domain_types_crate;
    let session_args = "api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()";

    // The member 409s name the ROOT's designed class (the schema title
    // when this root carries no invariants of its own — for mox-first
    // models the two spellings coincide).
    let member_block = member_lines(members, &enforcement.class);

    // CREATE: the server-generated fields exist only post-insert, so the
    // CREATED record is evaluated and the write is COMPENSATED (the #338
    // auditable soft-delete shape) before the 409.
    if !enforcement.is_empty() || !members.is_empty() {
        let compensation = if has_delete {
            format!(
                "        tracing::warn!(entity_id = %id, \"design invariant violated on create — compensating\");\n        if let Err(comp_err) = state.{domain}_{module}_commands.delete(id, {crate}::SourceContext::api(), correlation_id, {session}).await {{\n            tracing::error!(entity_id = %id, \"invariant compensation delete failed: {{comp_err}}\");\n        }}\n",
                domain = domain,
                module = module_name,
                crate = crate_name,
                session = session_args,
            )
        } else {
            "        // No delete command on this entity — the violating row is NOT\n        // compensated (recorded v1 gap, issue #666).\n".to_string()
        };
        let mut lines = String::from(
            "\n    // Design invariants (DD5, issue #666 slice 4d): server-generated\n    // fields exist only post-insert, so CREATE evaluates the CREATED\n    // record. The generated command is a single transaction, so this\n    // handler-level evaluation is observably identical to in-transaction\n    // rollback for the shapes we support; a violation COMPENSATES (the\n    // auditable soft-delete shape) before the 409.",
        );
        for plan in &enforcement.plans {
            let check = format!(
                "    if !crate::domain::{domain}::rules::{fn_name}(&response) {{",
                fn_name = plan.fn_name,
            );
            lines.push_str(&format!(
                "\n{}\n{}return Err({});\n    }}",
                check,
                compensation,
                invariant_violation_error(&enforcement.class, plan.index, &plan.expression, None),
            ));
        }
        if !member_block.is_empty() {
            lines.push('\n');
            lines.push_str(&member_block);
        }
        blocks.create_block = lines;
    }

    // UPDATE: the candidate is the loaded record with the request patch
    // applied, evaluated BEFORE the write — a violation answers 409 and
    // the write never starts.
    if !enforcement.is_empty() || !members.is_empty() {
        let mut lines = String::new();
        let is_child = role == "child";
        if !update_existing_bound && !is_child {
            lines.push('\n');
            lines.push_str(&invariant_load_or_404(
                domain,
                module_name,
                entity_name,
                is_auditable,
                &domain_pascal,
            ));
        }
        if !update_existing_bound && is_child {
            // The child ownership load already exists in the template —
            // claim its binding (the guards would have, had they gated
            // update).
            blocks.update_ownership_prefix = Some("let existing = ".to_string());
        }
        lines.push('\n');
        lines.push_str(
            "    // Design invariants (DD5, issue #666 slice 4d): the candidate is\n    // the loaded record with the request patch applied; evaluated BEFORE\n    // the write — a violation answers 409 and the write never starts (the\n    // generated command is a single transaction, so handler-level\n    // pre-write evaluation is observably identical to in-transaction\n    // rollback for the shapes we support).",
        );
        lines.push('\n');
        lines.push_str(&candidate_lines("existing", "body", entity_name));
        for plan in &enforcement.plans {
            lines.push('\n');
            lines.push_str(&invariant_check(
                &format!("crate::domain::{domain}::rules::{}", plan.fn_name),
                "candidate",
                &invariant_violation_error(&enforcement.class, plan.index, &plan.expression, None),
            ));
        }
        if !member_block.is_empty() {
            lines.push('\n');
            lines.push_str(&member_block);
        }
        blocks.update_block = lines;
    }
    blocks
}

/// The transition-arm block (issue #666 slice 4d): the state-aware action
/// applies its patch — including the machine state move — to the CANDIDATE
/// before the write, so a transition that would leave an invariant false
/// answers 409 pre-write. Rendered against the loaded `response` and the
/// action's `value` patch.
pub(super) fn build_transition_invariant_block(
    domain: &str,
    entity_name: &str,
    enforcement: &crate::ddd::invariants::InvariantEnforcement,
) -> String {
    // Leading newline: the template glues the block onto the idempotent-
    // read early return's `}`, so the first comment line starts its own
    // line (the `read_guard_block` pattern).
    let mut lines = String::from("\n");
    lines.push_str(
        "    // Design invariants (DD5, issue #666 slice 4d): the candidate is\n    // the loaded record with the action patch — the machine state move\n    // included — applied; evaluated BEFORE the write, so a transition\n    // that would leave an invariant false answers 409 and nothing\n    // persists.",
    );
    lines.push('\n');
    lines.push_str(&candidate_lines("response", "value", entity_name));
    for plan in &enforcement.plans {
        lines.push('\n');
        lines.push_str(&invariant_check(
            &format!("crate::domain::{domain}::rules::{}", plan.fn_name),
            "candidate",
            &invariant_violation_error(&enforcement.class, plan.index, &plan.expression, None),
        ));
    }
    lines
}

/// The invariant-carrying aggregate members of one ROOT entity (issue
/// #666 slice 4d): config-declared children (`role = "child"` + `parent`)
/// PLUS the authored-boundary members (the 4b resolver — consumed, never
/// re-derived), deduped by title, one routed level.
pub(super) async fn collect_invariant_members(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    root_title: &str,
    parent_candidates: &[codegraph_core::types::ParentCandidate],
    project: &crate::ProjectConfig,
) -> Vec<InvariantMember> {
    let type_suffix = &config.defaults.type_suffix;
    let stripped_root = crate::api::router::strip_suffix(root_title, type_suffix);
    let mut titles: Vec<String> = Vec::new();
    // Config children of this root.
    if let Some(entry) = config.domains.get(domain) {
        for (child_title, child_cfg) in &entry.entity_config {
            if child_cfg.role.as_deref() == Some("child")
                && crate::api::router::strip_suffix(
                    child_cfg.parent.as_deref().unwrap_or_default(),
                    type_suffix,
                ) == stripped_root
            {
                titles.push(child_title.clone());
            }
        }
    }
    // Authored members (the 4b index). Early-out on an empty index — the
    // common graph pays one models read and nothing else.
    let index = crate::ddd::aggregate::AuthoredBoundaryIndex::from_graph(db, type_suffix).await;
    for member in index.members_of(root_title, domain) {
        if !titles.iter().any(|t| t == member) {
            titles.push(member.to_string());
        }
    }
    titles.sort();
    titles.dedup();

    let mut members = Vec::new();
    for title in titles {
        let enforcement = crate::ddd::invariants::enforcement_for(db, config, domain, &title).await;
        if enforcement.is_empty() {
            continue;
        }
        let Ok(Some(schema)) = db.get_schema_in_domain(&title, domain).await else {
            continue;
        };
        // The scoped fetch must exist: `list_filtered` is emitted for the
        // member's own `list` operation; the FK column rides the canonical
        // resolver (config parent_ref → authored → candidate).
        let operations =
            crate::api::api_model::resolve_entity_operations(db, config, domain, &title).await;
        if !operations.iter().any(|op| op == "list") {
            continue;
        }
        let entity_cfg = config
            .domains
            .get(domain)
            .and_then(|entry| entry.get_entity_config(&title));
        let Some(fk_column) = crate::fk::resolve_parent_fk_column_same_domain(
            &title,
            parent_candidates,
            entity_cfg,
            domain,
            config,
            db,
        )
        .await
        else {
            continue;
        };
        let Ok(auditable) = resolve_is_auditable(db, &title, config, domain).await else {
            continue;
        };
        // The ux sort plane widens `list_filtered` (issue #306) — the
        // member fetch must supply the widened argument.
        let ux_sort = !crate::ux::sort::resolve_ux_sort_plan(db, config, project, &title, domain)
            .await
            .map(|plan| plan.fields.is_empty())
            .unwrap_or(true);
        members.push(InvariantMember {
            domain: domain.to_string(),
            module: schema.pg_table_name.clone(),
            fk_column,
            auditable,
            ux_sort,
            class: enforcement.class,
            plans: enforcement.plans,
        });
    }
    members
}
