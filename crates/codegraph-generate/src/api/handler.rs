use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{ParentCandidate, PolicyKind};
use serde::Serialize;

use crate::ProjectConfig;
use crate::error::Result;
use crate::filter_fields::{
    FilterFieldInfo, NestedFilterFieldInfo, resolve_filter_fields, resolve_nested_filter_fields,
};
use crate::render_template_with_project;
use crate::traits::{EntityGenerator, EntityGeneratorKind, GeneratedFile};
use crate::type_registry::TypeRegistryHandle;
use codegraph_config::{DomainConfig, EntityConfig};

use super::include_path::{ResolvedIncludePath, resolve_include_paths_for_topology};
use super::router::{ChildInfo, CrossRefInfo};

use super::api_model::{resolve_entity_operations, resolve_path_segment};

/// A mox operation routed as an action endpoint (issue #508 R2):
/// `POST {id}/op/{snake}`. Only qualifying ops route — bodied AND with
/// all-primitive signature slots; body-less ops never route.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MoxOpRoute {
    /// Operation name as written in mox.
    pub name: String,
    /// Rust method name (and route segment).
    pub snake: String,
    /// Parameters: (snake name, Rust type).
    pub params: Vec<MoxOpParam>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MoxOpParam {
    pub name: String,
    pub rust_type: String,
}

/// Qualifying mox ops for `schema_title` (issue #508 R2): a body
/// (`rust` or `expr`) and only primitive signature slots.
pub async fn routable_mox_ops(
    db: &dyn GraphQuerier,
    schema_title: &str,
    entity_name: &str,
) -> Vec<MoxOpRoute> {
    let all = db.get_mox_operations().await.unwrap_or_default();
    all.iter()
        .filter(|op| op.class == schema_title || op.class == entity_name)
        .filter(|op| op.bodies.contains_key("rust") || op.bodies.contains_key("expr"))
        .filter_map(|op| {
            let primitive = |t: &str| crate::mox_expr::mox_primitive_rust_type(t).is_some();
            if !primitive(&op.return_type) || !op.params.iter().all(|p| primitive(&p.type_)) {
                return None;
            }
            Some(MoxOpRoute {
                name: op.name.clone(),
                snake: codegraph_naming::to_snake_case(&op.name),
                params: op
                    .params
                    .iter()
                    .map(|p| MoxOpParam {
                        name: codegraph_naming::to_snake_case(&p.name),
                        rust_type: crate::mox_expr::mox_primitive_rust_type(&p.type_)
                            .unwrap_or("String")
                            .to_string(),
                    })
                    .collect(),
            })
        })
        .collect()
}

#[derive(Debug, Serialize)]
pub struct HandlerContext {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub path_segment: String,
    pub tag: String,
    pub operations: Vec<String>,
    pub has_create: bool,
    pub has_read: bool,
    pub has_update: bool,
    pub has_delete: bool,
    pub has_list: bool,
    pub parent_ref: Option<String>,
    pub parent_entity: Option<String>,
    pub parent_path_segment: Option<String>,
    pub parent_module_name: Option<String>,
    /// tera 2's `default` filter only fires on undefined (not null), so None
    /// must OMIT the key for `parent_domain | default(value=domain)` routes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_domain: Option<String>,
    pub role: String,
    /// Named path parameter for this entity's ID (e.g. `"worker_id"`).
    pub param_name: String,
    /// Named path parameter for the parent's ID (e.g. `"worker_id"` when this entity is a child of Worker).
    pub parent_param_name: Option<String>,
    /// Named path parameter for the grandparent's ID (depth-2 children only).
    pub grandparent_param_name: Option<String>,
    /// Path segment for the grandparent entity (depth-2 children only, e.g. `"workers"`).
    pub grandparent_path_segment: Option<String>,
    /// Domain of the grandparent entity (depth-2 children only).
    /// Skipped when None: tera 2's `default` only fires on undefined keys.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grandparent_domain: Option<String>,
    pub children: Vec<ChildInfo>,
    pub cross_refs: Vec<CrossRefInfo>,
    /// When set, the list endpoint supports ?status= filtering on this column.
    pub status_field: Option<String>,
    /// Whether this entity has workflow support.
    pub has_workflow: bool,
    /// Whether this entity has full-text search enabled.
    pub has_fts: bool,
    /// REST surface for full-text search: "query_param", "dedicated", or "both".
    pub fts_rest_mode: String,
    /// Whether this entity has semantic search (pgvector embeddings) enabled.
    pub has_embeddings: bool,
    /// Fields exposed as JSON:API `?filter[field]=value` query params.
    pub filter_fields: Vec<FilterFieldInfo>,
    /// Nested (child/grandchild) fields exposed as `?filter[child.column]=value`.
    pub nested_filter_fields: Vec<NestedFilterFieldInfo>,
    /// Maximum number of items allowed in a bulk create request.
    pub max_bulk_size: usize,
    /// Number of ancestor path parameters captured before this handler's own routes.
    /// 0 for root entities, 1 for children, 2 for grandchildren, etc.
    pub ancestor_path_params: usize,
    /// When set, this entity supports the /tree endpoint (self-referencing hierarchy).
    pub hierarchy_field: Option<String>,
    /// When true, the find_tree method returns Vec<serde_json::Value> instead of Vec<Response>.
    #[serde(default)]
    pub tree_include: bool,
    /// When true, the get_by_id handler returns typed WithIncludeResponse instead of serde_json::Value.
    pub has_include: bool,
    /// Resolved include paths for `?include=` query parameter.
    pub include_paths: Vec<ResolvedIncludePath>,
    /// Resolved `use` import statements for types referenced by this handler.
    /// Populated through this run's type registry instead of hard-coded
    /// template paths.
    pub handler_imports: Vec<String>,
    /// When true, the entity has a soft-delete audit policy and the query layer
    /// expects an `include_deleted: bool` argument on all read methods.
    pub is_auditable: bool,
    /// Sortable-field allow-list for `?sort=` on the list endpoint
    /// (issue #306), derived from the ux plan. Empty = the ux sort plane
    /// is inactive: no sort params are emitted and the handler stays
    /// byte-identical.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ux_sort_fields: Vec<String>,
    /// Mox operations routed as action endpoints (issue #508 R2). Empty =
    /// no op routes and the handler stays byte-identical.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mox_ops: Vec<MoxOpRoute>,
    /// Service operations routed as action endpoints (issue #590): POST
    /// `{id}/actions/{snake}` with the actor policy enforced server-side.
    /// Child entities nest under the parent route (WS4). Empty = no op
    /// routes, byte-identical handler.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub service_ops: Vec<crate::ddd::service_ops::ServiceOpRoute>,
    /// Handler-site eligibility guards (issue #338): the compiled rule
    /// booleans evaluated before the gated operations. Empty = no guard
    /// surface and the handler stays byte-identical. Populated only when
    /// the plan carries the `rules` generator (the emitted `rules.rs`
    /// module must exist for the call sites to resolve) and the topology
    /// is monolith (the rules module is not worker-routed).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub eligibility_guards: Vec<crate::ddd::rules::EligibilityGuard>,
    /// Pre-rendered guard code blocks (issue #338), whitespace-exact so
    /// the template renders them inline; empty strings when inactive.
    /// `*_ownership_prefix` binds the existing ownership load (`let
    /// existing = `) when guards need the loaded record; `*_guard_block`
    /// carries the load-or-404 (root) and/or the guard call lines.
    #[serde(default)]
    pub update_ownership_prefix: String,
    #[serde(default)]
    pub update_guard_block: String,
    #[serde(default)]
    pub delete_ownership_prefix: String,
    #[serde(default)]
    pub delete_guard_block: String,
    #[serde(default)]
    pub read_guard_block: String,
    #[serde(default)]
    /// Media deletion cleanup (issue #631 R7): the pre-rendered post-delete
    /// key-recompute + storage.delete block. Empty unless the domain
    /// declares `[media]` and the topology is monolith.
    pub media_cleanup_block: String,
    #[serde(default)]
    pub create_guard_block: String,
    /// Pre-rendered design-invariant blocks (DD5, issue #666 slice 4d),
    /// whitespace-exact; empty strings when the entity carries no
    /// enforceable invariants (no plans and no invariant-carrying members),
    /// the plan lacks the `rules` generator, or the topology is workers
    /// (the rules plane is monolith-only — the gap is WARNed, never
    /// silent).
    #[serde(default)]
    pub create_invariant_block: String,
    #[serde(default)]
    pub update_invariant_block: String,
}

/// Pre-rendered eligibility-guard code blocks (issue #338). Empty fields
/// keep the handler byte-identical; every block is whitespace-exact and
/// carries NO trailing newline (the template supplies the line break).
struct GuardBlocks {
    update_ownership_prefix: String,
    update_guard_block: String,
    delete_ownership_prefix: String,
    delete_guard_block: String,
    read_guard_block: String,
    create_guard_block: String,
}

fn build_guard_blocks(
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
            domain_pascal = codegraph_naming::to_pascal_case(domain),
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

/// One invariant-carrying aggregate MEMBER (issue #666 slice 4d): a child
/// entity whose invariants the root's save validates pre-write. One
/// routed level (the 4b depth cap — the index already drops deeper
/// chains).
#[derive(Debug, Clone)]
struct InvariantMember {
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
struct InvariantBlocks {
    create_block: String,
    update_block: String,
    /// The child-role update arm's `let existing = ` prefix, claimed by
    /// the invariant plane when the eligibility guards did not claim it.
    /// `None` keeps the guard-side behavior untouched.
    update_ownership_prefix: Option<String>,
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
fn build_invariant_blocks(
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
fn build_transition_invariant_block(
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
async fn collect_invariant_members(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    root_title: &str,
    parent_candidates: &[codegraph_core::types::ParentCandidate],
    project: &crate::ProjectConfig,
) -> Vec<InvariantMember> {
    let type_suffix = &config.defaults.type_suffix;
    let stripped_root = super::router::strip_suffix(root_title, type_suffix);
    let mut titles: Vec<String> = Vec::new();
    // Config children of this root.
    if let Some(entry) = config.domains.get(domain) {
        for (child_title, child_cfg) in &entry.entity_config {
            if child_cfg.role.as_deref() == Some("child")
                && super::router::strip_suffix(
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
            super::api_model::resolve_entity_operations(db, config, domain, &title).await;
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

pub struct HandlerGenerator {
    output_dir: PathBuf,
    parent_candidates: Vec<ParentCandidate>,
    /// This run's type registry (issue #746). Defaults to the ambient
    /// registry for direct construction; the pipeline injects the run's own
    /// handle via [`Self::with_type_registry`].
    type_registry: TypeRegistryHandle,
    /// Whether the plan carries the `rules` domain generator (registry
    /// wiring): handler-site eligibility guards (issue #338) resolve only
    /// when the emitted `rules.rs` module exists to answer the calls.
    rules_guards: bool,
}

impl HandlerGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            parent_candidates: Vec::new(),
            rules_guards: false,
            type_registry: TypeRegistryHandle::ambient(),
        }
    }

    /// Use `registry` for this generator's import resolution.
    pub fn with_type_registry(mut self, registry: TypeRegistryHandle) -> Self {
        self.type_registry = registry;
        self
    }

    pub fn with_parent_candidates(mut self, candidates: Vec<ParentCandidate>) -> Self {
        self.parent_candidates = candidates;
        self
    }

    /// Enable handler-site eligibility guards (issue #338). The registry
    /// derives this from the build plan's `rules` generator membership —
    /// guards call `crate::domain::{domain}::rules::…`, which only exists
    /// when the `RulesGenerator` runs.
    pub fn with_rules_guards(mut self, enabled: bool) -> Self {
        self.rules_guards = enabled;
        self
    }
}

#[async_trait]
impl EntityGenerator for HandlerGenerator {
    fn kind(&self) -> EntityGeneratorKind {
        EntityGeneratorKind::Handler
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let schema = db
            .get_schema_in_domain(schema_title, domain)
            .await?
            .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

        let entity_name = schema.rust_type_name.clone();
        let module_name = schema.pg_table_name.clone();
        let domain = domain.to_string();

        if module_name.is_empty() {
            return Ok(Vec::new());
        }

        let entity_cfg = config
            .domains
            .get(&domain)
            .and_then(|d| d.get_entity_config(&entity_name));

        let path_segment = resolve_path_segment(entity_cfg, &schema);

        let operations = resolve_entity_operations(db, config, &domain, &entity_name).await;

        let tag = entity_cfg
            .and_then(|ec| ec.tag.clone())
            .unwrap_or_else(|| entity_name.clone());

        let parent_entity = entity_cfg.and_then(|ec| ec.parent.clone());

        let workflow = entity_cfg.and_then(|ec| ec.workflow.as_ref());
        let status_field = workflow.map(|wf| wf.status_field.clone());
        let has_workflow = workflow
            .map(|wf| wf.generate_action_endpoints)
            .unwrap_or(false);

        let search = entity_cfg.map(|ec| &ec.search);
        let has_fts = search
            .and_then(|s| s.fts_columns.as_ref())
            .map(|cols| !cols.is_empty())
            .unwrap_or(false);
        let has_embeddings = search
            .map(|s| !s.embedding_columns.is_empty())
            .unwrap_or(false);
        let fts_rest_mode = entity_cfg
            .map(|ec| ec.search.fts_rest_mode.clone())
            .unwrap_or_else(|| "query_param".to_string());

        let filter_fields = resolve_filter_fields(
            db,
            schema_title,
            entity_cfg
                .and_then(|ec| ec.filter_fields.as_ref())
                .map(|v| v.as_slice()),
        )
        .await?;

        let max_bulk_size = entity_cfg
            .and_then(|ec| ec.max_bulk_size)
            .unwrap_or(config.defaults.max_bulk_size);

        // Resolve parent/child relationships from parent_candidates.
        let stripped_title =
            super::router::strip_suffix(schema_title, &config.defaults.type_suffix);
        let topo = resolve_handler_topology(
            db,
            config,
            &domain,
            &entity_name,
            schema_title,
            stripped_title,
            entity_cfg,
            &self.parent_candidates,
        )
        .await?;

        // Compute ancestor_path_params: how many {id} path segments Axum
        // accumulates before reaching this handler.  We walk up the resolved
        // parent chain, checking each ancestor's role in the domain it belongs to.
        let ancestor_path_params = compute_ancestor_path_params(
            config,
            &domain,
            stripped_title,
            entity_cfg,
            &self.parent_candidates,
            topo.parent_domain.as_deref(),
            &topo.role,
        );

        let param_name = super::router::param_name_from_path_segment(&path_segment);
        let parent_param_name = topo
            .parent_path_segment
            .as_deref()
            .map(super::router::param_name_from_path_segment);
        // For depth-2, resolve grandparent's param name, path segment, and domain.
        let (grandparent_param_name, grandparent_path_segment, grandparent_domain) =
            resolve_grandparent(
                db,
                config,
                &domain,
                entity_cfg,
                &self.parent_candidates,
                stripped_title,
                ancestor_path_params,
            )
            .await;

        // Resolve include paths from config. Skip non-root entities unless they
        // have explicit allow_include configuration.
        let include_paths =
            resolve_include_paths(db, config, &domain, schema_title, entity_cfg, project).await?;

        let nested_filter_fields =
            resolve_nested_filter_fields(db, schema_title, &module_name, &domain, config).await?;

        // Resolve handler imports via TypeRegistry instead of hard-coded template paths.
        let has_include = !include_paths.is_empty();
        let handler_imports = build_handler_imports(
            &self.type_registry,
            &operations,
            &entity_name,
            has_include,
            &include_paths,
            &domain,
            &module_name,
        );

        // Soft-delete audit policy: when the entity tracks deleted rows, the
        // query layer exposes an `include_deleted` argument on read methods.
        // Handlers must thread it through (passing `false` — APIs exclude
        // soft-deleted rows by default). Mirrors the logic in the query
        // generator (ddd/query.rs) so handler/query signatures stay in sync.
        let is_auditable = resolve_is_auditable(db, schema_title, config, &domain).await?;

        // ux sort plane (issue #306): the `?sort=` allow-list, resolved from
        // the SAME ux plan the list page renders. Empty when the flag is off
        // or no column is sortable — the handler then emits nothing.
        let ux_sort_fields = if operations.iter().any(|op| op == "list") {
            crate::ux::sort::resolve_ux_sort_plan(db, config, project, schema_title, &domain)
                .await?
                .fields
        } else {
            Vec::new()
        };

        // Mox op action routes (issue #508 R2).
        let mox_ops = routable_mox_ops(db, schema_title, &entity_name).await;

        // Handler-site eligibility guards (issue #338). Gated on the plan
        // carrying `rules` (the call targets must exist) and monolith
        // topology (rules.rs is not worker-routed, so a worker-crate
        // handler could not resolve the module). Empty otherwise —
        // byte-identical handler.
        let eligibility_guards = if self.rules_guards && !project.is_workers_topology() {
            crate::ddd::rules::eligibility_guards_for(db, config, &domain, schema_title).await?
        } else {
            Vec::new()
        };
        let guard_blocks = build_guard_blocks(
            &topo.role,
            &domain,
            &module_name,
            &entity_name,
            is_auditable,
            &project.identity.domain_types_crate,
            &eligibility_guards,
        );

        // Design-invariant enforcement (DD5, issue #666 slice 4d). The
        // SAME gating the eligibility guards use: the predicates live in
        // the `rules` generator's module (plan membership) and the rules
        // plane is monolith-only. Empty otherwise — byte-identical
        // handler. The blocks consume slice 2's lowering (`enforcement_for`)
        // — never re-derived.
        let invariant_enforcement = if self.rules_guards && !project.is_workers_topology() {
            crate::ddd::invariants::enforcement_for(db, config, &domain, schema_title).await
        } else {
            crate::ddd::invariants::InvariantEnforcement {
                class: entity_name.clone(),
                plans: Vec::new(),
            }
        };
        let invariant_members = if self.rules_guards && !project.is_workers_topology() {
            collect_invariant_members(
                db,
                config,
                &domain,
                schema_title,
                &self.parent_candidates,
                project,
            )
            .await
        } else {
            Vec::new()
        };
        let update_guards_bind_existing = eligibility_guards
            .iter()
            .any(|g| g.operations.iter().any(|op| op == "update"));
        let invariant_blocks = build_invariant_blocks(
            &topo.role,
            &domain,
            &module_name,
            &entity_name,
            is_auditable,
            operations.contains(&"delete".to_string()),
            &project.identity.domain_types_crate,
            &invariant_enforcement,
            &invariant_members,
            update_guards_bind_existing,
        );

        // Service-op action routes (issue #590) — with the transition-arm
        // invariant blocks attached (slice 4d): every executable
        // `save`/`update` op applies its patch to the candidate pre-write.
        // Child entities emit them too — the handler template nests the
        // utoipa path under the parent and loads the record through the
        // parent-scoped read (WS4). Lowering refusals (un-lowerable `when`
        // conditions) are hard generation errors naming the capability.
        let mut service_ops = crate::ddd::service_ops::routes_for_entity(
            db,
            config,
            &domain,
            schema_title,
            &crate::ddd::service_ops::BodyRenderContext::from_project(project),
        )
        .await
        .map_err(crate::error::Error::Config)?;
        if !invariant_enforcement.is_empty() {
            for route in service_ops.iter_mut() {
                if matches!(
                    route.execution,
                    crate::ddd::service_ops::ExecutionKind::Update { .. }
                ) && route.record_load
                {
                    route.invariant_block = Some(build_transition_invariant_block(
                        &domain,
                        &entity_name,
                        &invariant_enforcement,
                    ));
                    // The invariant 409 is now part of the surface's base
                    // set (the template documents it beside the machine
                    // arm), so a declared `throws Conflict` on this route
                    // must NOT double-document 409 (issue #665 review M2 —
                    // the routes-level `covered_statuses` cannot see the
                    // block, which attaches only here).
                    route.raise_responses.retain(|raise| raise.status != 409);
                }
            }
        }

        // Media deletion cleanup (issue #631 R7). Monolith topology only
        // (the media slot lives in the monolith AppState — worker placement
        // is the recorded deferral); the domain must declare `[media]` and
        // the plans resolve (the driver's pre-generation pass already
        // hard-failed on a broken config). Empty otherwise — byte-identical.
        let media_cleanup_block = if !project.is_workers_topology()
            && config
                .domains
                .get(&domain)
                .is_some_and(|d| d.media.is_some())
        {
            crate::media::build_media_plans(db, config)
                .await
                .ok()
                .and_then(|plans| plans.get(&domain).cloned())
                .map(|plan| {
                    crate::media::keys::render_cleanup_block(
                        &domain,
                        &crate::media::keys::cleanup_fields_for(&plan, schema_title),
                    )
                })
                .unwrap_or_default()
        } else {
            String::new()
        };

        // The child update arm's ownership prefix: at most one plane
        // claims it (the guards when they gate update, else the invariant
        // plane) — merge here so the template keeps its single key.
        let update_ownership_prefix = invariant_blocks
            .update_ownership_prefix
            .clone()
            .unwrap_or_else(|| guard_blocks.update_ownership_prefix.clone());

        let ctx = HandlerContext {
            has_create: operations.contains(&"create".to_string()),
            has_read: operations.contains(&"read".to_string()),
            has_update: operations.contains(&"update".to_string()),
            has_delete: operations.contains(&"delete".to_string()),
            has_list: operations.contains(&"list".to_string()),
            entity_name,
            module_name: module_name.clone(),
            domain: domain.clone(),
            path_segment,
            tag,
            operations,
            mox_ops,
            service_ops,
            eligibility_guards,
            parent_ref: topo.parent_ref,
            parent_entity,
            parent_path_segment: topo.parent_path_segment,
            parent_module_name: topo.parent_module_name,
            parent_domain: topo.parent_domain,
            role: topo.role,
            param_name,
            parent_param_name,
            grandparent_param_name,
            grandparent_path_segment,
            grandparent_domain,
            children: topo.children,
            cross_refs: topo.cross_refs,
            status_field,
            has_workflow,
            has_fts,
            fts_rest_mode,
            has_embeddings,
            filter_fields,
            nested_filter_fields,
            max_bulk_size,
            ancestor_path_params,
            hierarchy_field: entity_cfg.and_then(|ec| ec.hierarchy_field.clone()),
            tree_include: entity_cfg
                .and_then(|ec| ec.tree_include.as_ref())
                .map(|v| !v.is_empty())
                .unwrap_or(false),
            has_include,
            include_paths: include_paths.clone(),
            handler_imports,
            is_auditable,
            ux_sort_fields,
            update_ownership_prefix,
            update_guard_block: guard_blocks.update_guard_block,
            delete_ownership_prefix: guard_blocks.delete_ownership_prefix,
            delete_guard_block: guard_blocks.delete_guard_block,
            read_guard_block: guard_blocks.read_guard_block,
            create_guard_block: guard_blocks.create_guard_block,
            create_invariant_block: invariant_blocks.create_block,
            update_invariant_block: invariant_blocks.update_block,
            media_cleanup_block,
        };

        let content = render_template_with_project(tera, "api/handler.tera", &ctx, project)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("api")
                .join(&domain)
                .join(format!("{}_handler.rs", module_name)),
            content,
        }])
    }
}

/// Resolved parent/child topology for a handler, gathered from manual config
/// and graph parent candidates.
struct HandlerTopology {
    parent_ref: Option<String>,
    parent_path_segment: Option<String>,
    parent_module_name: Option<String>,
    parent_domain: Option<String>,
    role: String,
    children: Vec<ChildInfo>,
    cross_refs: Vec<CrossRefInfo>,
}

impl Default for HandlerTopology {
    fn default() -> Self {
        Self {
            parent_ref: None,
            parent_path_segment: None,
            parent_module_name: None,
            parent_domain: None,
            role: "root".into(),
            children: vec![],
            cross_refs: vec![],
        }
    }
}

#[allow(clippy::too_many_arguments)] // TODO(#751): gather the title spellings into one param struct
async fn resolve_handler_topology(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    entity_name: &str,
    schema_title: &str,
    stripped_title: &str,
    entity_cfg: Option<&EntityConfig>,
    parent_candidates: &[ParentCandidate],
) -> Result<HandlerTopology> {
    let mut topo = HandlerTopology {
        parent_ref: entity_cfg.and_then(|ec| ec.parent_ref.clone()),
        role: entity_cfg
            .and_then(|ec| ec.role.clone())
            .unwrap_or_else(|| "root".into()),
        ..Default::default()
    };

    // 1. Check manual config for parent (takes priority over graph)
    apply_config_parent(db, config, domain, entity_cfg, &mut topo).await;

    // 1.5 AUTHORED aggregate membership (issue #666 slice 4b): the design's
    // authored boundary wins over config for the parent — an authored
    // member re-points under its authored root; a class PROVEN root by a
    // member's authored declaration drops a config child role back to a
    // top-level root. Undecided boundaries change nothing.
    apply_authored_parent(db, config, domain, schema_title, &mut topo).await;

    // 2. Fall back to graph parent_candidates if manual config didn't resolve parent.
    // Only nest when the parent is in the same domain (matching router behavior).
    apply_graph_parent(
        db,
        config,
        domain,
        stripped_title,
        parent_candidates,
        &mut topo,
    )
    .await;

    // Check if this entity is a parent (graph detection)
    collect_graph_children(
        db,
        config,
        domain,
        stripped_title,
        parent_candidates,
        &mut topo.children,
    )
    .await;

    // Fallback: if entity is a parent in config but wasn't matched by parent_candidates
    if topo.children.is_empty() {
        collect_config_children(db, config, domain, stripped_title, &mut topo.children).await;
    }

    // Detect cross-aggregate entity references.
    topo.cross_refs =
        detect_cross_refs(db, config, domain, entity_name, entity_cfg, &topo.children).await;

    // Graph/config discovery order is not stable across processes (HashMap
    // iteration, unordered MATCH rows); sort so emitted handler code
    // (`with_child` links, route registrations) is byte-identical between runs.
    topo.children
        .sort_by(|a, b| a.path_segment.cmp(&b.path_segment));

    Ok(topo)
}

async fn apply_config_parent(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    entity_cfg: Option<&EntityConfig>,
    topo: &mut HandlerTopology,
) {
    if let Some(ec) = entity_cfg
        && ec.role.as_deref() == Some("child")
        && let Some(parent_title) = &ec.parent
    {
        let parent_name = super::router::strip_suffix(parent_title, &config.defaults.type_suffix);
        if topo.parent_ref.is_none() {
            topo.parent_ref = Some(format!(
                "{}_id",
                codegraph_naming::to_snake_case(parent_name)
            ));
        }
        if let Ok(Some(parent_schema)) = db.get_schema_in_domain(parent_title, domain).await {
            topo.parent_path_segment = Some(resolve_path_segment(None, &parent_schema));
            topo.parent_module_name = Some(parent_schema.pg_table_name.clone());
            topo.parent_domain = if config
                .domains
                .get(domain)
                .map(|d| d.entities.contains(parent_title))
                .unwrap_or(false)
            {
                Some(domain.to_string())
            } else {
                parent_schema.domain.or_else(|| Some(domain.to_string()))
            };
        } else {
            topo.parent_path_segment = Some(codegraph_naming::to_kebab_case(parent_name));
            topo.parent_module_name = Some(codegraph_naming::to_snake_case(parent_name));
            topo.parent_domain = Some(domain.to_string());
        }
        topo.role = "child".to_string();
    }
}

/// Issue #666 slice 4b: the AUTHORED aggregate boundary wins over config
/// for the handler's parent topology. An authored member re-points under
/// its authored root (config still owns the FK column when it names one —
/// `parent_ref` is seeded before this runs and preserved); a class PROVEN
/// root by a member's authored declaration drops a config child role back
/// to a top-level root. Undecided boundaries change nothing.
async fn apply_authored_parent(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    schema_title: &str,
    topo: &mut HandlerTopology,
) {
    let type_suffix = config.defaults.type_suffix.clone();
    if let Some(authored) =
        crate::ddd::aggregate::authored_routing_parent(db, config, domain, schema_title).await
    {
        if let Ok(Some(parent_schema)) = db
            .get_schema_in_domain(&authored.parent_title, domain)
            .await
        {
            topo.parent_path_segment = Some(resolve_path_segment(None, &parent_schema));
            topo.parent_module_name = Some(parent_schema.pg_table_name.clone());
            topo.parent_domain = Some(domain.to_string());
        } else {
            let parent_name = super::router::strip_suffix(&authored.parent_title, &type_suffix);
            topo.parent_path_segment = Some(codegraph_naming::to_kebab_case(parent_name));
            topo.parent_module_name = Some(codegraph_naming::to_snake_case(parent_name));
            topo.parent_domain = Some(domain.to_string());
        }
        if topo.parent_ref.is_none() {
            topo.parent_ref = Some(authored.fk_column);
        }
        topo.role = "child".to_string();
        return;
    }
    // Authored-root override: proven root + config child → top-level root.
    if topo.role == "child"
        && crate::ddd::aggregate::authored_root_proven(db, &type_suffix, schema_title).await
    {
        topo.role = "root".to_string();
        topo.parent_path_segment = None;
        topo.parent_module_name = None;
        topo.parent_domain = None;
        topo.parent_ref = None;
    }
}

async fn apply_graph_parent(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    stripped_title: &str,
    parent_candidates: &[ParentCandidate],
    topo: &mut HandlerTopology,
) {
    if topo.parent_path_segment.is_some() || topo.role == "root" {
        return;
    }
    for pc in parent_candidates {
        let child_name = super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix);
        if child_name == stripped_title {
            // Check if parent is in the same domain: either explicitly listed
            // or its schema is classified into this domain.
            let parent_in_domain = config
                .domains
                .get(domain)
                .map(|d| d.entities.contains(&pc.parent_title))
                .unwrap_or(false)
                || db
                    .get_schema_in_domain(&pc.parent_title, domain)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|s| s.domain.as_ref().map(|d| d == domain))
                    .unwrap_or(false);
            if !parent_in_domain {
                // Parent is in another domain — keep FK column but don't nest
                if topo.parent_ref.is_none() {
                    topo.parent_ref = Some(crate::fk_column_for_candidate(
                        pc,
                        &config.defaults.type_suffix,
                    ));
                }
                break;
            }
            let parent_name =
                super::router::strip_suffix(&pc.parent_title, &config.defaults.type_suffix);
            if topo.parent_ref.is_none() {
                topo.parent_ref = Some(crate::fk_column_for_candidate(
                    pc,
                    &config.defaults.type_suffix,
                ));
            }
            if let Ok(Some(parent_schema)) = db.get_schema_in_domain(&pc.parent_title, domain).await
            {
                topo.parent_path_segment = Some(resolve_path_segment(None, &parent_schema));
                topo.parent_module_name = Some(parent_schema.pg_table_name);
                topo.parent_domain = Some(domain.to_string());
            } else {
                topo.parent_path_segment = Some(codegraph_naming::to_kebab_case(parent_name));
                topo.parent_module_name = Some(codegraph_naming::to_snake_case(parent_name));
                topo.parent_domain = Some(domain.to_string());
            }
            topo.role = "child".to_string();
            break;
        }
    }
}

async fn collect_graph_children(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    stripped_title: &str,
    parent_candidates: &[ParentCandidate],
    resolved_children: &mut Vec<ChildInfo>,
) {
    for pc in parent_candidates {
        let parent_name =
            super::router::strip_suffix(&pc.parent_title, &config.defaults.type_suffix);
        if parent_name == stripped_title {
            let child_name =
                super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix);
            if let Ok(Some(child_schema)) = db.get_schema_in_domain(&pc.child_title, domain).await {
                resolved_children.push(ChildInfo {
                    entity_name: child_schema.rust_type_name.clone(),
                    module_name: child_schema.pg_table_name.clone(),
                    path_segment: resolve_path_segment(None, &child_schema),
                });
            } else {
                resolved_children.push(ChildInfo {
                    entity_name: child_name.to_string(),
                    module_name: codegraph_naming::to_snake_case(child_name),
                    path_segment: codegraph_naming::to_kebab_case(child_name),
                });
            }
        }
    }
}

async fn collect_config_children(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    stripped_title: &str,
    resolved_children: &mut Vec<ChildInfo>,
) {
    if let Some(domain_entry) = config.domains.get(domain) {
        // Iterate entity_config entries to find children of this entity
        {
            for (other_title, other_cfg) in &domain_entry.entity_config {
                if other_cfg.role.as_deref() == Some("child")
                    && let Some(parent_title) = &other_cfg.parent
                    && super::router::strip_suffix(parent_title, &config.defaults.type_suffix)
                        == stripped_title
                {
                    let child_name =
                        super::router::strip_suffix(other_title, &config.defaults.type_suffix);
                    if let Ok(Some(child_schema)) =
                        db.get_schema_in_domain(other_title, domain).await
                    {
                        resolved_children.push(ChildInfo {
                            entity_name: child_schema.rust_type_name.clone(),
                            module_name: child_schema.pg_table_name.clone(),
                            path_segment: resolve_path_segment(None, &child_schema),
                        });
                    } else {
                        resolved_children.push(ChildInfo {
                            entity_name: child_name.to_string(),
                            module_name: codegraph_naming::to_snake_case(child_name),
                            path_segment: codegraph_naming::to_kebab_case(child_name),
                        });
                    }
                }
            }
        }
    }
}

async fn detect_cross_refs(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    entity_name: &str,
    entity_cfg: Option<&EntityConfig>,
    resolved_children: &[ChildInfo],
) -> Vec<CrossRefInfo> {
    let mut resolved_cross_refs: Vec<CrossRefInfo> = vec![];
    {
        let schema_title_with_type = format!("{}Type", entity_name);
        if let Ok(referenced) = db.get_referenced_schemas(&schema_title_with_type).await {
            let parent_name = entity_cfg.and_then(|ec| ec.parent.as_deref()).unwrap_or("");
            let child_names: std::collections::HashSet<&str> = resolved_children
                .iter()
                .map(|c| c.entity_name.as_str())
                .collect();

            for ref_schema_node in &referenced {
                let ref_title = &ref_schema_node.title;
                let ref_entity_name =
                    super::router::strip_suffix(ref_title, &config.defaults.type_suffix);

                if ref_entity_name == entity_name {
                    continue;
                }
                if ref_entity_name == parent_name {
                    continue;
                }
                if child_names.contains(ref_entity_name) {
                    continue;
                }

                // Only include refs that are entities in any domain
                if let Ok(Some(ref_schema)) = db.get_schema_in_domain(ref_title, domain).await {
                    if ref_schema.pg_table_name.is_empty() {
                        continue;
                    }
                    // Find domain of referenced entity
                    let ref_domain = ref_schema
                        .domain
                        .clone()
                        .unwrap_or_else(|| domain.to_string());
                    let fk_col = codegraph_naming::to_snake_case(ref_entity_name) + "_id";
                    let link_rel = codegraph_naming::to_snake_case(ref_entity_name);
                    resolved_cross_refs.push(CrossRefInfo {
                        entity_name: ref_entity_name.to_string(),
                        module_name: ref_schema.pg_table_name.clone(),
                        domain: ref_domain,
                        path_segment: resolve_path_segment(None, &ref_schema),
                        fk_column: fk_col,
                        link_rel,
                    });
                }
            }
        }
    }
    resolved_cross_refs
}

fn compute_ancestor_path_params(
    config: &DomainConfig,
    domain: &str,
    stripped_title: &str,
    entity_cfg: Option<&EntityConfig>,
    parent_candidates: &[ParentCandidate],
    resolved_parent_domain: Option<&str>,
    resolved_role: &str,
) -> usize {
    if resolved_role != "child" {
        return 0;
    }
    let parent_title_for_lookup = entity_cfg.and_then(|ec| ec.parent.clone()).or_else(|| {
        parent_candidates
            .iter()
            .find(|pc| {
                super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix)
                    == stripped_title
            })
            .map(|pc| pc.parent_title.clone())
    });
    let mut depth = 1usize;
    if let Some(parent_title) = parent_title_for_lookup {
        // Prefer the current entity's domain for looking up the parent's config,
        // since the parent may exist in multiple domains with different roles.
        let parent_dom = if config
            .domains
            .get(domain)
            .map(|d| d.entities.contains(&parent_title))
            .unwrap_or(false)
        {
            domain
        } else {
            resolved_parent_domain.unwrap_or(domain)
        };
        // Check if parent has explicit role=root in its domain config
        let parent_explicit_root = config
            .domains
            .get(parent_dom)
            .and_then(|d| d.get_entity_config(&parent_title))
            .map(|ec| ec.role.as_deref() == Some("root"))
            .unwrap_or(false);
        if !parent_explicit_root {
            // Check if parent is a child (either via manual config or graph)
            let parent_is_child_in_config = config
                .domains
                .get(parent_dom)
                .and_then(|d| d.get_entity_config(&parent_title))
                .map(|ec| ec.role.as_deref() == Some("child"))
                .unwrap_or(false);
            let parent_is_child_in_graph = parent_candidates
                .iter()
                .any(|pc| pc.child_title == parent_title);
            if parent_is_child_in_config || parent_is_child_in_graph {
                depth += 1;
            }
        }
    }
    depth
}

async fn resolve_grandparent(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    entity_cfg: Option<&EntityConfig>,
    parent_candidates: &[ParentCandidate],
    stripped_title: &str,
    ancestor_path_params: usize,
) -> (Option<String>, Option<String>, Option<String>) {
    if ancestor_path_params < 2 {
        return (None, None, None);
    }
    let parent_title = entity_cfg.and_then(|ec| ec.parent.clone()).or_else(|| {
        parent_candidates
            .iter()
            .find(|pc| {
                super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix)
                    == stripped_title
            })
            .map(|pc| pc.parent_title.clone())
    });
    let gp_title = parent_title.and_then(|pt| {
        config
            .domains
            .get(domain)
            .and_then(|d| d.get_entity_config(&pt))
            .and_then(|ec| ec.parent.clone())
            .or_else(|| {
                let parent_stripped =
                    super::router::strip_suffix(&pt, &config.defaults.type_suffix);
                parent_candidates
                    .iter()
                    .find(|pc| {
                        super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix)
                            == parent_stripped
                    })
                    .map(|pc| pc.parent_title.clone())
            })
    });
    if let Some(ref gpt) = gp_title {
        if let Ok(Some(gp_schema)) = db.get_schema_in_domain(gpt, domain).await {
            let gp_seg = resolve_path_segment(None, &gp_schema);
            let gp_param = super::router::param_name_from_path_segment(&gp_seg);
            // Grandparent domain: look up which domain owns it.
            let gp_domain = config
                .domains
                .iter()
                .find_map(|(d, dc)| {
                    if dc.get_entity_config(gpt).is_some() {
                        Some(d.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| domain.to_string());
            (Some(gp_param), Some(gp_seg), Some(gp_domain))
        } else {
            let gp_name = super::router::strip_suffix(gpt, &config.defaults.type_suffix);
            let gp_seg = codegraph_naming::to_kebab_case(gp_name);
            let gp_param = super::router::param_name_from_path_segment(&gp_seg);
            (Some(gp_param), Some(gp_seg), Some(domain.to_string()))
        }
    } else {
        (None, None, None)
    }
}

async fn resolve_include_paths(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    schema_title: &str,
    entity_cfg: Option<&EntityConfig>,
    project: &ProjectConfig,
) -> Result<Vec<ResolvedIncludePath>> {
    // Resolve include paths from config. Skip non-root entities unless they
    // have explicit allow_include configuration.
    let has_explicit_include = entity_cfg
        .and_then(|ec| ec.allow_include.as_ref())
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    let is_root = entity_cfg
        .and_then(|ec| ec.role.as_deref())
        .map(|r| r == "root")
        .unwrap_or(true);
    let include_paths = if has_explicit_include || is_root {
        if let Some(ec) = entity_cfg {
            resolve_include_paths_for_topology(
                db,
                config,
                domain,
                schema_title,
                ec.allow_include.as_ref(),
                project.is_workers_topology(),
            )
            .await?
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Deduplicate include paths by alias to prevent duplicate struct fields
    // in the generated handler code (auto-discover can produce the same child
    // entity through multiple FK relationships).
    let include_paths = {
        let mut seen = std::collections::HashSet::new();
        include_paths
            .into_iter()
            .filter(|path| seen.insert(path.alias.clone()))
            .collect::<Vec<_>>()
    };
    Ok(include_paths)
}

fn build_handler_imports(
    type_registry: &TypeRegistryHandle,
    operations: &[String],
    entity_name: &str,
    has_include: bool,
    include_paths: &[ResolvedIncludePath],
    domain: &str,
    module_name: &str,
) -> Vec<String> {
    let mut handler_refs: Vec<String> = vec![
        "AppState".into(),
        "AppError".into(),
        "ApiKeyInfo".into(),
        format!("{}Response", entity_name),
        format!("{}LinkedResponse", entity_name),
    ];
    if operations.contains(&"create".to_string()) {
        handler_refs.push("BulkItemError".into());
    }
    if operations.contains(&"create".to_string()) {
        handler_refs.push(format!("Create{}Request", entity_name));
    }
    if operations.contains(&"update".to_string()) {
        handler_refs.push(format!("Update{}Request", entity_name));
    }
    if has_include {
        handler_refs.push(format!("{}WithIncludeResponse", entity_name));
        handler_refs.push(format!("{}IncludedData", entity_name));
        // Add combined response types for dot-notation paths and VO response
        // types for child_table_override paths — the handler's match arms
        // reference these types directly in merge patterns.
        for path in include_paths {
            if path.segments.len() > 1 {
                handler_refs.push(format!("{}CombinedResponse", path.segments[0].entity_name));
            }
            // The override response type is only referenced textually in the
            // dot-path merge arm (handler.tera), so only register it for
            // multi-segment paths; single-segment includes use inference.
            if path.segments.len() > 1
                && let Some(over) = path
                    .segments
                    .first()
                    .and_then(|s| s.child_table_override.as_ref())
            {
                handler_refs.push(over.response_type.clone());
            }
        }
    }
    // Note: Create{entity_name}Body and {entity_name}BulkCreateResponse are
    // defined inline by handler.tera and must NOT be registered in the type
    // registry or added to handler_refs. Doing so causes E0432/E0255 when
    // multiple domains share an entity name (e.g. "Order" in screening AND
    // assessments): the second handler's registration silently fails (name
    // collision), resolve_imports generates wrong cross-module imports for
    // types that should be local, producing duplicate definitions and
    // non-existent type references in the generated code.
    let handler_caller: Vec<String> = vec![
        "crate".into(),
        "api".into(),
        domain.to_string(),
        format!("{}_handler", module_name),
    ];
    type_registry.resolve_imports(&handler_refs, &handler_caller)
}

pub(crate) async fn resolve_is_auditable(
    db: &dyn GraphQuerier,
    schema_title: &str,
    config: &DomainConfig,
    domain: &str,
) -> Result<bool> {
    let policies = db.get_policies_for_schema(schema_title).await?;
    // Issue #449: a `.ddd` design overrides both the operation set and the
    // auditable flag — the handler's call shape must match the design-mapped
    // query layer exactly (include_deleted on read methods exists only when
    // the query layer accepts it), so mirror ddd/query.rs's resolution.
    let design_surface = crate::ddd::design::DddDesignSurface::from_graph(db).await?;
    let mut operations =
        crate::api::api_model::resolve_entity_operations(db, config, domain, schema_title).await;
    if let Some(design_ops) = design_surface.operations_for(schema_title) {
        operations = design_ops;
    }
    // Append-only snapshot semantics (issue #284) — same inference as
    // db/ddl.rs: insert-only entities carry no audit columns, so the API
    // layer never passes `include_deleted` to the query/repository traits.
    let append_only = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(schema_title))
        .as_ref()
        .is_some_and(|ec| ec.is_append_only())
        || !operations.iter().any(|op| op == "update" || op == "delete");
    let is_auditable = (if let Some(design) = design_surface.design_for(schema_title) {
        design.flags.auditable
    } else if policies.is_empty() {
        config
            .domains
            .get(domain)
            .and_then(|d| d.auditable)
            .unwrap_or(true)
    } else {
        policies
            .iter()
            .any(|p| matches!(&p.kind, PolicyKind::Audit(a) if a.track_deleted))
    }) && !append_only;
    Ok(is_auditable)
}

/// Template pins for the `?sort=` list surface (issue #306): the
/// generated handler parses/validates `sort`/`order` against the plan's
/// allow-list and names the valid fields in the 400; flag-off renders no
/// sort surface at all.
#[cfg(test)]
mod ux_sort_handler_template_tests {
    use super::*;
    use serde_json::json;

    fn base_ctx(ux_sort_fields: Vec<&str>) -> serde_json::Value {
        json!({
            "entity_name": "Task",
            "module_name": "task",
            "domain": "common",
            "path_segment": "tasks",
            "tag": "Task",
            "operations": ["list", "read"],
            "has_create": false,
            "has_read": true,
            "has_update": false,
            "has_delete": false,
            "has_list": true,
            "role": "root",
            "param_name": "task_id",
            "children": [],
            "cross_refs": [],
            "status_field": null,
            "has_workflow": false,
            "has_fts": false,
            "fts_rest_mode": "query_param",
            "has_embeddings": false,
            "filter_fields": [],
            "nested_filter_fields": [],
            "max_bulk_size": 100,
            "ancestor_path_params": 0,
            "hierarchy_field": null,
            "tree_include": false,
            "has_include": false,
            "include_paths": [],
            "handler_imports": [],
            "is_auditable": false,
            "ux_sort_fields": ux_sort_fields,
        })
    }

    fn render(ctx: &serde_json::Value) -> String {
        let tera = crate::template_engine::create_tera(std::path::Path::new("."))
            .expect("embedded templates");
        crate::render_template_with_project(
            &tera,
            "api/handler.tera",
            ctx,
            &ProjectConfig::default(),
        )
        .expect("handler renders")
    }

    #[test]
    fn flag_on_handler_validates_sort_params_against_allow_list() {
        let out = render(&base_ctx(vec!["name", "total_amount"]));

        // ListParams gains optional sort/order with serde defaults (absent
        // params keep the default created_at DESC ordering).
        assert!(
            out.contains("#[serde(default)]\n    pub sort: Option<String>,"),
            "{out}"
        );
        assert!(
            out.contains("#[serde(default)]\n    pub order: Option<String>,"),
            "{out}"
        );
        // The allow-list const carries the plan's sortable fields.
        assert!(
            out.contains("const ALLOWED_SORT_FIELDS: &[&str] = &["),
            "{out}"
        );
        assert!(out.contains("\"name\","), "{out}");
        assert!(out.contains("\"total_amount\","), "{out}");
        // 400 texts name the valid sortable fields (include-path style).
        assert!(
            out.contains("Unknown sort field: {sort}. Valid sortable fields: {}"),
            "{out}"
        );
        assert!(
            out.contains("order requires sort; valid sortable fields: {}"),
            "{out}"
        );
        // `order` is restricted to asc|desc.
        assert!(
            out.contains("Invalid order: {other}. Valid values: asc, desc"),
            "{out}"
        );
        // The validated spec threads into the query layer.
        assert!(out.contains("let ux_sort: Option<(String, bool)>"), "{out}");
        assert!(
            out.contains(
                "list_filtered(params.page, params.page_size, &filters, ux_sort, api_key_info"
            ),
            "{out}"
        );
    }

    #[test]
    fn flag_off_handler_emits_no_sort_surface() {
        let out = render(&base_ctx(Vec::new()));
        for needle in [
            "ALLOWED_SORT_FIELDS",
            "pub sort:",
            "pub order:",
            "Unknown sort field",
            "Invalid order",
            "order requires sort",
            "ux_sort",
        ] {
            assert!(
                !out.contains(needle),
                "flag-off handler must not contain {needle:?}:\n{out}"
            );
        }
        // The pre-#306 call shape stays intact.
        assert!(
            out.contains("list_filtered(params.page, params.page_size, &filters, api_key_info"),
            "{out}"
        );
    }

    /// Issue #479: create/update bodies extract through a
    /// `serde_path_to_error`-wrapped extractor so 422s name
    /// `path.to.field: cause` instead of axum's opaque untagged-enum
    /// rejection. The create body branches on the payload shape BEFORE
    /// deserializing (the untagged enum discards per-variant serde
    /// errors, so path-tracking the enum is not enough); the extractor is
    /// emitted only when a write body exists.
    #[test]
    fn write_bodies_extract_through_serde_path_to_error() {
        // has_create renders the extractor + the create signature.
        let mut ctx = base_ctx(Vec::new());
        ctx["has_create"] = json!(true);
        let out = render(&ctx);
        for needle in [
            // The shared rejection mapper names the failing path + cause.
            "fn json_body_rejection(e: serde_path_to_error::Error<serde_json::Error>) -> AppError {",
            "field: e.path().to_string(),",
            "message: e.inner().to_string(),",
            // Non-JSON content types keep the 415 contract axum's Json had.
            "StatusCode::UNSUPPORTED_MEDIA_TYPE",
            "code: \"UNSUPPORTED_MEDIA_TYPE\"",
            // The create extractor branches on the payload shape and
            // path-tracks EACH attempt (Bulk|Single semantics preserved).
            "pub struct CreateBody(pub CreateTaskBody);",
            "let body = if value.is_array() {",
            "Vec<CreateTaskRequest> =\n                serde_path_to_error::deserialize(value)",
            "CreateTaskBody::Bulk(items)",
            "CreateTaskBody::Single(item)",
            "CreateBody(body): CreateBody,",
            "#[serde(untagged)]",
        ] {
            assert!(
                out.contains(needle),
                "handler must contain {needle:?}:\n{out}"
            );
        }
        // Update rides the generic path-tracked extractor.
        let mut ctx = base_ctx(Vec::new());
        ctx["has_update"] = json!(true);
        let out = render(&ctx);
        assert!(out.contains("pub struct JsonPathBody<T>(pub T);"), "{out}");
        assert!(
            out.contains("JsonPathBody(body): JsonPathBody<UpdateTaskRequest>,"),
            "{out}"
        );
        // Read-only entities emit neither extractor nor wrapper noise.
        let out = render(&base_ctx(Vec::new()));
        for needle in ["JsonPathBody", "CreateBody", "json_body_rejection"] {
            assert!(
                !out.contains(needle),
                "read-only handler must not emit {needle:?}:\n{out}"
            );
        }
    }
}
