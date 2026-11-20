//! Handler-site eligibility-guard blocks (issue #338), split out of the
//! #751 size-budget `api/handler.rs`.

use codegraph_naming::to_pascal_case;

/// Pre-rendered eligibility-guard code blocks (issue #338). Empty fields
/// keep the handler byte-identical; every block is whitespace-exact and
/// carries NO trailing newline (the template supplies the line break).
pub(super) struct GuardBlocks {
    pub(super) update_ownership_prefix: String,
    pub(super) update_guard_block: String,
    pub(super) delete_ownership_prefix: String,
    pub(super) delete_guard_block: String,
    pub(super) read_guard_block: String,
    pub(super) create_guard_block: String,
}

pub(super) fn build_guard_blocks(
    role: &str,
    domain: &str,
    module_name: &str,
    entity_name: &str,
    is_auditable: bool,
    domain_types_crate: &str,
    guards: &[crate::ddd::rules::EligibilityGuard],
) -> GuardBlocks {
    let mut blocks = GuardBlocks {
        update_ownership_prefix: String::new(),
        update_guard_block: String::new(),
        delete_ownership_prefix: String::new(),
        delete_guard_block: String::new(),
        read_guard_block: String::new(),
        create_guard_block: String::new(),
    };
    if guards.is_empty() {
        return blocks;
    }
    let is_child = role == "child";
    let audit_arg = if is_auditable { ", false" } else { "" };
    let session_args = "api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()";

    let find_by_id =
        format!("state.{domain}_{module_name}_queries.find_by_id(id{audit_arg}, {session_args})");
    let load_or_404 = |load: &str| -> String {
        format!(
            "    // Eligibility guards (issue #338): load-or-404 before the operation.\n    let existing = {load}.await\n        .map_err(|e: {domain_pascal}Error| {{\n                    AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to find {entity}\")\n                        .with_correlation_id(correlation_id)\n                }})?\n        .ok_or_else(|| AppError::not_found(format!(\"{entity} {{id}} not found\"))\n            .with_correlation_id(correlation_id))?;",
            domain_pascal = to_pascal_case(domain),
            entity = entity_name,
            load = load,
        )
    };
    let guard_call = |g: &crate::ddd::rules::EligibilityGuard, var: &str| -> String {
        format!(
            "    // Eligibility rule '{rule}' (issue #338): deny with 403 when it fails.\n    if !{path}(&{var}) {{\n        return Err(AppError::forbidden(format!(\"Eligibility rule '{rule}' denied this {entity} operation\"))\n            .with_correlation_id(correlation_id));\n    }}",
            rule = g.rule_name,
            path = g.call_path,
            var = var,
            entity = entity_name,
        )
    };

    // update/delete evaluate over the loaded record: the root handler has
    // no load before the guard (the block adds load-or-404); the child
    // handler's existing ownership load gains the `let existing` binding
    // and the block is guard lines only.
    let mut update_guards = String::new();
    let mut delete_guards = String::new();
    let mut read_guards = String::new();
    let mut create_guards = String::new();
    for g in guards {
        if g.operations.iter().any(|op| op == "update") {
            update_guards.push_str(&guard_call(g, "existing"));
            update_guards.push('\n');
        }
        if g.operations.iter().any(|op| op == "delete") {
            delete_guards.push_str(&guard_call(g, "existing"));
            delete_guards.push('\n');
        }
        if g.operations.iter().any(|op| op == "read") {
            read_guards.push_str(&guard_call(g, "response"));
            read_guards.push('\n');
        }
        if g.operations.iter().any(|op| op == "create") {
            create_guards.push_str(&guard_call(g, "response"));
            create_guards.push('\n');
        }
    }

    if !update_guards.is_empty() {
        if is_child {
            blocks.update_ownership_prefix = "let existing = ".to_string();
            blocks.update_guard_block = update_guards.trim_end().to_string();
        } else {
            blocks.update_guard_block =
                format!("{}\n{}", load_or_404(&find_by_id), update_guards.trim_end());
        }
    }
    if !delete_guards.is_empty() {
        if is_child {
            blocks.delete_ownership_prefix = "let existing = ".to_string();
            blocks.delete_guard_block = delete_guards.trim_end().to_string();
        } else {
            blocks.delete_guard_block =
                format!("{}\n{}", load_or_404(&find_by_id), delete_guards.trim_end());
        }
    }
    if !read_guards.is_empty() {
        // Leading newline: the template glues the block onto the load
        // statement's `?;`, so the first guard line starts its own line.
        blocks.read_guard_block = format!("\n{}", read_guards.trim_end());
    }
    if !create_guards.is_empty() {
        // An ineligible create is compensated: the just-created row is
        // removed (best effort) and the caller is denied with 403.
        let compensation = format!(
            "state.{domain}_{module_name}_commands.delete(id, {crate_name}::SourceContext::api(), correlation_id, {session_args})",
            crate_name = domain_types_crate,
        );
        create_guards = create_guards.replace(
            "        return Err(AppError::forbidden",
            &format!(
                "        tracing::warn!(entity_id = %id, \"eligibility rule denied create — compensating\");\n        if let Err(comp_err) = {compensation}.await {{\n            tracing::error!(entity_id = %id, \"eligibility-rule compensation delete failed: {{comp_err}}\");\n        }}\n        return Err(AppError::forbidden"
            ),
        );
        blocks.create_guard_block = format!("\n{}", create_guards.trim_end());
    }
    blocks
}

// ── Design-invariant enforcement (DD5, issue #666 slice 4d) ────────────
