//! Byte-identity oracle for `templates/api/handler.tera` (wave 0 / T0.1, issue #778).
//!
//! The handler template is being refactored in byte-identical slices. Every
//! slice needs a hard oracle that covers MUCH more than the individual
//! contains-pins scattered through the unit tests: this test renders the
//! template over a table of context permutations (root / child / grandchild,
//! every optional surface the template branches on, every service-op
//! execution kind) and pins a sha256 of the EXACT rendered bytes per
//! permutation in `tests/fixtures/handler_matrix.sha256`.
//!
//! Node-free, DB-free, graph-free: `HandlerContext` is a plain serde shape, so
//! the JSON here mirrors the real serializer's output (see `HandlerContext` in
//! `codegraph-generate/src/api/handler/mod.rs`). The one Tera subtlety that
//! matters: `{{ x | default(value=…) }}` only fires on an UNDEFINED key (not on
//! JSON null), so `Option` fields carrying `skip_serializing_if` are OMITTED
//! here exactly as the real serializer omits them.
//!
//! Refresh after an INTENTIONAL template output change (review the diff, then
//! commit the regenerated fixture enumerated in the PR):
//!
//! ```text
//! HANDLER_MATRIX_BLESS=1 cargo test -p codegraph --test handler_matrix_tests
//! cargo test -p codegraph --test handler_matrix_tests
//! ```
//!
//! On a mismatch a bless run also leaves per-permutation snapshots under
//! `<temp>/codegraph-handler-matrix/expected/`, so a later failing run can
//! print a real first-difference hint (line number + context) instead of only
//! two hashes.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use codegraph::generate::ProjectConfig;
use codegraph::generate::api::include_path::{IncludeSegment, ResolvedIncludePath};
use codegraph::generate::api::include_plan::{IncludePlanCtx, build_include_plan};
use codegraph::generate::ddd::service_ops::render::render_execution_block;
use codegraph::generate::ddd::service_ops::{ExecutionRenderFacts, ServiceOpRoute};
use codegraph::generate::profile::PersistenceProvider;
use codegraph::generate::render_template_with_project;
use codegraph::generate::template_engine::create_tera;

const FIXTURE_REL: &str = "tests/fixtures/handler_matrix.sha256";
const BLESS_ENV: &str = "HANDLER_MATRIX_BLESS";

/// One rendered permutation.
struct Case {
    name: &'static str,
    ctx: Value,
    project: ProjectConfig,
}

// ── Context builders ───────────────────────────────────────────────────

/// The minimal root context: `read` + `list`, no optional surfaces. Mirrors
/// the `HandlerContext` serde shape (see the module docs for the
/// undefined-vs-null rule).
fn root_base() -> Value {
    let mut ctx = json!({
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
        // Plain `Option<String>` fields: the real serializer emits null.
        "parent_ref": null,
        "parent_entity": null,
        "parent_path_segment": null,
        "parent_module_name": null,
        "role": "root",
        "param_name": "task_id",
        "parent_param_name": null,
        "grandparent_param_name": null,
        "grandparent_path_segment": null,
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
        // No `| default(...)` in the template's delete arm: the key must exist.
        "media_cleanup_block": "",
    });
    // Always-serialized booleans: `HandlerContext` derives the guard presence
    // flags from `eligibility_guards` in Rust, so the real serializer always
    // emits them (false without guards). Set outside `json!` to keep the
    // macro's recursion depth where it is.
    ctx["create_guarded"] = json!(false);
    ctx["read_guarded"] = json!(false);
    ctx["update_guarded"] = json!(false);
    ctx["delete_guarded"] = json!(false);
    // The `?include_deleted=` gate artifacts (issue #792) are pre-rendered
    // from Rust (the #338 block convention): the real serializer ALWAYS
    // emits the keys (empty when inactive), so the key must exist here.
    ctx["include_deleted_helper_fn"] = json!("");
    ctx["include_deleted_guard_block"] = json!("");
    attach_path_extractors(&mut ctx, "root", 0, "task_id");
    ctx
}

/// Mirror `HandlerContext`'s pre-rendered `Path(…)` bindings: the real
/// generator derives them from (role, ancestor_path_params, param_name) via
/// `path_extractors`, so the matrix feeds the same derivation instead of
/// re-spelling the whitespace-exact bytes (the helper's shapes are pinned by
/// its own unit tests).
fn attach_path_extractors(ctx: &mut Value, role: &str, depth: usize, param_name: &str) {
    let extractors = codegraph::generate::api::handler::path_extractors(role, depth, param_name);
    ctx["path_extractor_item"] = json!(extractors.item);
    ctx["path_extractor_item_named"] = json!(extractors.item_named);
    ctx["path_extractor_collection"] = json!(extractors.collection);
    ctx["path_extractor_collection_unused"] = json!(extractors.collection_unused);
    ctx["path_extractor_item_unused_parent"] = json!(extractors.item_unused_parent);
    ctx["path_extractor_child_op_add"] = json!(extractors.child_op_add);
}

fn full_crud(ctx: &mut Value) {
    ctx["operations"] = json!(["create", "read", "update", "delete", "list"]);
    ctx["has_create"] = json!(true);
    ctx["has_read"] = json!(true);
    ctx["has_update"] = json!(true);
    ctx["has_delete"] = json!(true);
    ctx["has_list"] = json!(true);
}

/// A child context: nested under `parent`, one ancestor path param.
fn child_base() -> Value {
    let mut ctx = root_base();
    ctx["entity_name"] = json!("TaskNote");
    ctx["module_name"] = json!("task_note");
    ctx["path_segment"] = json!("notes");
    ctx["tag"] = json!("TaskNote");
    ctx["role"] = json!("child");
    ctx["param_name"] = json!("task_note_id");
    ctx["parent_ref"] = json!("task_id");
    ctx["parent_entity"] = json!("Task");
    ctx["parent_path_segment"] = json!("tasks");
    ctx["parent_module_name"] = json!("task");
    ctx["parent_param_name"] = json!("task_id");
    ctx["ancestor_path_params"] = json!(1);
    attach_path_extractors(&mut ctx, "child", 1, "task_note_id");
    ctx
}

/// A grandchild context: two ancestor path params.
fn grandchild_base() -> Value {
    let mut ctx = child_base();
    ctx["entity_name"] = json!("TaskNoteRevision");
    ctx["module_name"] = json!("task_note_revision");
    ctx["path_segment"] = json!("revisions");
    ctx["tag"] = json!("TaskNoteRevision");
    ctx["param_name"] = json!("task_note_revision_id");
    ctx["parent_ref"] = json!("task_note_id");
    ctx["parent_entity"] = json!("TaskNote");
    ctx["parent_path_segment"] = json!("notes");
    ctx["parent_module_name"] = json!("task_note");
    ctx["parent_param_name"] = json!("task_note_id");
    // `grandparent_domain` carries skip_serializing_if: OMIT it so
    // `grandparent_domain | default(value=domain)` fires, as in real output.
    ctx["grandparent_param_name"] = json!("task_id");
    ctx["grandparent_path_segment"] = json!("tasks");
    ctx["ancestor_path_params"] = json!(2);
    attach_path_extractors(&mut ctx, "child", 2, "task_note_revision_id");
    ctx
}

/// One `ResolvedIncludePath` segment (the template reads `module_name`,
/// `is_array` and, for the merge branch, `response_rust_type`).
fn include_segment(module: &str, entity: &str, is_array: bool) -> IncludeSegment {
    IncludeSegment {
        entity_name: entity.to_string(),
        schema_title: format!("{entity}Type"),
        module_name: module.to_string(),
        domain: "common".to_string(),
        table: format!("\"common\".\"{module}\""),
        fk_column: format!("{module}_id"),
        reverse_fk_column: "task_id".to_string(),
        fk_is_required: false,
        reverse_fk_is_required: false,
        is_array,
        child_table_override: None,
    }
}

fn include_path(
    alias: &str,
    segments: Vec<IncludeSegment>,
    response_rust_type: &str,
    fetch_method: &str,
    batch_fetch_method: &str,
) -> ResolvedIncludePath {
    ResolvedIncludePath {
        alias: alias.to_string(),
        segments,
        response_rust_type: response_rust_type.to_string(),
        fetch_method: fetch_method.to_string(),
        batch_fetch_method: batch_fetch_method.to_string(),
    }
}

/// Mirror PRODUCTION for the two pre-rendered hydration blocks: they come
/// from the real [`build_include_plan`] over the same resolved paths, never
/// hand-copied (issue #778 wave 2). The provider verdict also selects the
/// `?include=` doc line (issue #781).
fn apply_include_blocks(ctx: &mut Value, paths: &[ResolvedIncludePath], project: &ProjectConfig) {
    let plan = build_include_plan(
        paths,
        &IncludePlanCtx {
            entity_name: ctx["entity_name"].as_str().expect("entity_name"),
            domain: ctx["domain"].as_str().expect("domain"),
            module_name: ctx["module_name"].as_str().expect("module_name"),
            role: ctx["role"].as_str().expect("role"),
            is_auditable: ctx["is_auditable"].as_bool().unwrap_or(false),
        },
        project,
    );
    ctx["include_get_block"] = json!(plan.get_block);
    ctx["include_list_block"] = json!(plan.list_block);
    ctx["include_doc"] = json!(plan.include_doc);
}

fn cornucopia_project() -> ProjectConfig {
    let mut project = ProjectConfig::default();
    project.database.persistence_provider = PersistenceProvider::Cornucopia;
    project
}

/// Mirror PRODUCTION for the `?include_deleted=` gate artifacts (issue #792):
/// they come from the real [`build_include_deleted_gate`] over the domain's
/// role hierarchy, never hand-copied. Production resolves the hierarchy from
/// `[rbac] roles_hierarchy` (default basejump); the matrix feeds the same
/// default and an explicit custom hierarchy for the fail-closed permutation.
fn apply_include_deleted_gate(ctx: &mut Value) {
    let hierarchy = codegraph_config::config::default_roles_hierarchy();
    apply_include_deleted_gate_with(ctx, &hierarchy);
}

fn apply_include_deleted_gate_with(ctx: &mut Value, hierarchy: &[String]) {
    let gate = codegraph::generate::api::handler::include_deleted::build_include_deleted_gate(
        ctx["is_auditable"].as_bool().unwrap_or(false),
        ctx["has_read"].as_bool().unwrap_or(false),
        ctx["has_list"].as_bool().unwrap_or(false),
        ctx["has_fts"].as_bool().unwrap_or(false),
        hierarchy,
    );
    ctx["include_deleted_helper_fn"] = json!(gate.helper_fn);
    ctx["include_deleted_guard_block"] = json!(gate.guard_block);
}

// ── Pre-rendered blocks (opaque to the template — hashed bytes) ─────────

const CREATE_GUARD_BLOCK: &str = r#"
    // Eligibility rule 'PositiveTotal' (issue #338): deny with 403 when it fails.
    tracing::warn!(entity_id = %id, "eligibility rule denied create — compensating");
    if let Err(comp_err) = state.common_task_commands.delete(id, domain_types::SourceContext::api(), correlation_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await {
        tracing::error!(entity_id = %id, "eligibility-rule compensation delete failed: {comp_err}");
    }
    if !crate::domain::common::rules::task_positive_total(&response) {
        return Err(AppError::forbidden(format!("Eligibility rule 'PositiveTotal' denied this Task operation"))
            .with_correlation_id(correlation_id));
    }"#;

const UPDATE_GUARD_BLOCK: &str = r#"    // Eligibility guards (issue #338): load-or-404 before the operation.
    let existing = state.common_task_queries.find_by_id(id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await
        .map_err(|e: CommonError| {
                    AppError::from_domain_error(e.http_status(), e.to_string(), "Failed to find Task")
                        .with_correlation_id(correlation_id)
                })?
        .ok_or_else(|| AppError::not_found(format!("Task {id} not found"))
            .with_correlation_id(correlation_id))?;
    // Eligibility rule 'PositiveTotal' (issue #338): deny with 403 when it fails.
    if !crate::domain::common::rules::task_positive_total(&existing) {
        return Err(AppError::forbidden(format!("Eligibility rule 'PositiveTotal' denied this Task operation"))
            .with_correlation_id(correlation_id));
    }"#;

const DELETE_GUARD_BLOCK: &str = r#"    // Eligibility guards (issue #338): load-or-404 before the operation.
    let existing = state.common_task_queries.find_by_id(id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await
        .map_err(|e: CommonError| {
                    AppError::from_domain_error(e.http_status(), e.to_string(), "Failed to find Task")
                        .with_correlation_id(correlation_id)
                })?
        .ok_or_else(|| AppError::not_found(format!("Task {id} not found"))
            .with_correlation_id(correlation_id))?;
    // Eligibility rule 'PositiveTotal' (issue #338): deny with 403 when it fails.
    if !crate::domain::common::rules::task_positive_total(&existing) {
        return Err(AppError::forbidden(format!("Eligibility rule 'PositiveTotal' denied this Task operation"))
            .with_correlation_id(correlation_id));
    }"#;

const READ_GUARD_BLOCK: &str = r#"
    // Eligibility rule 'PositiveTotal' (issue #338): deny with 403 when it fails.
    if !crate::domain::common::rules::task_positive_total(&response) {
        return Err(AppError::forbidden(format!("Eligibility rule 'PositiveTotal' denied this Task operation"))
            .with_correlation_id(correlation_id));
    }"#;

const CREATE_INVARIANT_BLOCK: &str = r#"
    // Design invariants (DD5, issue #666 slice 4d): server-generated
    // fields exist only post-insert, so CREATE evaluates the CREATED
    // record. A violation COMPENSATES before the 409.
    if !crate::domain::common::rules::task_invariant_0(&response) {
        tracing::warn!(entity_id = %id, "design invariant violated on create — compensating");
        if let Err(comp_err) = state.common_task_commands.delete(id, domain_types::SourceContext::api(), correlation_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await {
            tracing::error!(entity_id = %id, "invariant compensation delete failed: {comp_err}");
        }
        return Err(AppError { status: axum::http::StatusCode::CONFLICT, code: "INVARIANT_VIOLATION", message: format!("Invariant {} of `Task` violated: total > 0", 0usize), details: Some(vec![crate::error::FieldError { field: "Task".to_string(), message: "invariantIndex: 0".to_string() }]), correlation_id: Some(correlation_id) });
    }"#;

const UPDATE_INVARIANT_BLOCK: &str = r#"
    // Design invariants (DD5, issue #666 slice 4d): the candidate is the
    // loaded record with the update patch applied — evaluated BEFORE the
    // write, so a violating update answers 409 and nothing persists.
    let candidate: domain_types::common::task::dto_response::TaskResponse = serde_json::from_value({
        let mut candidate_json = serde_json::to_value(&response).unwrap_or_default();
        if let (Some(candidate_obj), Ok(patch)) = (candidate_json.as_object_mut(), serde_json::to_value(&body)) {
            if let Some(patch_obj) = patch.as_object() {
                for (key, value) in patch_obj {
                    candidate_obj.insert(key.clone(), value.clone());
                }
            }
        }
        candidate_json
    }).map_err(|e| AppError::internal(format!("Failed to build the invariant candidate: {e}")))?;
    if !crate::domain::common::rules::task_invariant_0(&candidate) {
        return Err(AppError { status: axum::http::StatusCode::CONFLICT, code: "INVARIANT_VIOLATION", message: format!("Invariant {} of `Task` violated: total > 0", 0usize), details: Some(vec![crate::error::FieldError { field: "Task".to_string(), message: "invariantIndex: 0".to_string() }]), correlation_id: Some(correlation_id) });
    }"#;

const MEDIA_CLEANUP_BLOCK: &str = r#"
    // Media cleanup (issue #631): overwrite-in-place keys make the object
    // key a pure function of (org, entity, id, field) — recompute and
    // delete best-effort; the `testkit storage --gc` sweep is the backstop.
    if let Some(storage) = &state.common_media_storage {
        let org = api_key_info.organization_id.to_string();
        for key in [
            format!("{org}/common/task/{id}/attachment"),
        ] {
            if let Err(e) = storage.delete(&key).await {
                tracing::warn!(key = %key, error = %e, "media cleanup failed (gc sweep will retry)");
            }
        }
    }
"#;

const OP_AUTHORIZE_FN: &str = r#"/// Actor-policy gate for service operation `submit` (issue #590).
async fn service_op_submit_authorize(state: &AppState, _api_key_info: &ApiKeyInfo, response: &domain_types::common::task::dto_response::TaskResponse) -> Result<(), String> {
    if !_api_key_info.capabilities.iter().any(|c| c == "TaskSubmit") {
        return Err("operation `submit` requires capability `TaskSubmit`".to_string());
    }
    if response.status.as_deref() == Some("Archived") {
        return Err("operation `submit` cannot move `status` from `Archived`".to_string());
    }
    Ok(())
}"#;

const OP_INVARIANT_BLOCK: &str = r#"
    // Design invariants (DD5, issue #666 slice 4d): the candidate is
    // the loaded record with the action patch — the machine state move
    // included — applied; evaluated BEFORE the write.
    let candidate: domain_types::common::task::dto_response::TaskResponse = serde_json::from_value({
        let mut candidate_json = serde_json::to_value(&response).unwrap_or_default();
        if let (Some(candidate_obj), Some(patch)) = (candidate_json.as_object_mut(), value.as_object()) {
            for (key, item) in patch {
                candidate_obj.insert(key.clone(), item.clone());
            }
        }
        candidate_json
    }).map_err(|e| AppError::internal(format!("Failed to build the invariant candidate: {e}")))?;
    if !crate::domain::common::rules::task_invariant_0(&candidate) {
        return Err(AppError { status: axum::http::StatusCode::CONFLICT, code: "INVARIANT_VIOLATION", message: format!("Invariant {} of `Task` violated: total > 0", 0usize), details: Some(vec![crate::error::FieldError { field: "Task".to_string(), message: "invariantIndex: 0".to_string() }]), correlation_id: Some(correlation_id) });
    }"#;

const BODY_BLOCK: &str = r#"    let current = self.findById("current").await.map_err(|e| e)?;
    guard current.status != "Archived" else error Conflict("archived tasks cannot be submitted");
    local.status = "Submitted";
    let latest = self.save(local).await?;
    return Ok(serde_json::json!({ "data": latest }));"#;

// ── Service-op fixtures ────────────────────────────────────────────────

/// A service op with only the always-serialized fields (the `skip_serializing_if`
/// keys are omitted exactly as the real serializer omits them).
fn service_op(service: &str, op_name: &str, snake: &str, execution: Value) -> Value {
    json!({
        "service": service,
        "op_name": op_name,
        "snake": snake,
        "fn_name": format!("service_op_{snake}"),
        "capabilities": [],
        "execution": execution,
        "record_load": true,
    })
}

fn root_service_ops() -> Value {
    let state = json!({
        "current_expr": "response.status.clone().unwrap_or_default()",
        "wire_key": "status",
        "initial": "Draft",
        "all": [
            { "from": "Draft", "to": "Submitted" },
            { "from": "Submitted", "to": "Approved" },
        ],
        // Named transitions for THIS op: the runtime `match` arm.
        "op": [
            { "from": "Draft", "to": "Submitted" },
        ],
    });

    let mut guarded_submit = service_op(
        "TaskService",
        "submit",
        "submit",
        json!({ "kind": "update", "state": state }),
    );
    guarded_submit["authorize_fn"] = json!(OP_AUTHORIZE_FN);
    guarded_submit["authorize_async"] = json!(true);
    guarded_submit["invariant_block"] = json!(OP_INVARIANT_BLOCK);
    guarded_submit["throws"] = json!(["Conflict"]);

    // The same update kind with an EMPTY named-transition list: the
    // body-driven whitelist branch (`state.all`) instead of the `match`.
    let mut body_driven = service_op(
        "TaskService",
        "changeStatus",
        "change_status",
        json!({
            "kind": "update",
            "state": {
                "current_expr": "response.status.clone().unwrap_or_default()",
                "wire_key": "status",
                "initial": "Draft",
                "all": [
                    { "from": "Draft", "to": "Submitted" },
                    { "from": "Submitted", "to": "Approved" },
                ],
                "op": [],
            },
        }),
    );
    body_driven["record_load"] = json!(true);
    body_driven["authorize_fn"] = json!(OP_AUTHORIZE_FN);

    // A stateless update (no machine binding): the plain patch arm.
    let mut plain_update = service_op("TaskService", "touch", "touch", json!({ "kind": "update" }));
    // The `update` + invariant, NO machine binding arm (the third 409 wording).
    plain_update["invariant_block"] = json!(OP_INVARIANT_BLOCK);

    // A seam with a pre-rendered gate and a declared raise (the
    // `raise_responses` documentation loop).
    let mut seamed = service_op(
        "TaskService",
        "export",
        "export",
        json!({ "kind": "seam", "reason": "the body delegates to a service (`Exporter`) — no lowering yet" }),
    );
    seamed["authorize_fn"] = json!(OP_AUTHORIZE_FN);
    seamed["raise_responses"] = json!([
        { "status": 404, "errors": "NotFound" },
        { "status": 422, "errors": "InvalidState, MissingField" },
    ]);
    seamed["throws"] = json!(["NotFound", "InvalidState", "MissingField"]);

    // A seam WITHOUT a record load: the capability-membership-only route.
    // The fixture spells the seam reason the template's `default` filter
    // supplied before the arm was pre-rendered (`no executable lowering`).
    let mut stateless_seam = service_op(
        "TaskService",
        "purgeCache",
        "purge_cache",
        json!({ "kind": "seam", "reason": "no executable lowering" }),
    );
    stateless_seam["record_load"] = json!(false);
    stateless_seam["authorize_fn"] = json!(OP_AUTHORIZE_FN);
    stateless_seam["authorize_async"] = json!(true);

    let read = service_op("TaskService", "peek", "peek", json!({ "kind": "read" }));
    let delete = service_op("TaskService", "purge", "purge", json!({ "kind": "delete" }));

    // A checked statement body, with per-status documented raises.
    let body = service_op(
        "TaskService",
        "submitAndSave",
        "submit_and_save",
        json!({
            "kind": "body",
            "plan": {
                "block": BODY_BLOCK,
                "params": [],
                "raises": [],
                "saves": true,
                "may_refuse": true,
                "responses": [
                    { "status": 409, "errors": "Conflict" },
                ],
            },
        }),
    );

    // A body whose LONE parameter IS the request body (the modeled DTO
    // boundary shape): `plan.params[0].direct_body` drives the utoipa
    // `request_body` line.
    let body_direct = service_op(
        "TaskService",
        "submitBoundary",
        "submit_boundary",
        json!({
            "kind": "body",
            "plan": {
                "block": BODY_BLOCK,
                "params": [
                    {
                        "name": "payload",
                        "rust_type": "domain_types::dto::submission::Submission",
                        "optional": false,
                        "direct_body": true,
                        "sample": { "note": "sample" },
                        "request_body": "domain_types::dto::submission::Submission",
                    },
                ],
                "raises": [],
                "saves": true,
                "may_refuse": false,
                "responses": [],
            },
        }),
    );

    // A collection-level create whose body is the modeled boundary.
    let create = service_op(
        "TaskService",
        "openTask",
        "open_task",
        json!({
            "kind": "create",
            "plan": {
                "dto_prefix": "Create",
                "extraction": "    let payload: domain_types::dto::intake::Intake = Json::<domain_types::dto::intake::Intake>::from_value(serde_json::json!({}))?;\n",
                "construct": "        title: payload.title,\n        status: None,\n",
                "defaulted": ["status", "assignee_id"],
                "request_body": "domain_types::dto::intake::Intake",
                "sample": { "title": "sample" },
                "required_class_refs": [],
            },
        }),
    );

    // Item-addressed projection read.
    let projection_item = service_op(
        "TaskService",
        "summarise",
        "summarise",
        json!({
            "kind": "projection_read",
            "plan": {
                "module": "summary",
                "projection": "TaskSummary",
                "collection": false,
                "derived": [
                    { "wire_key": "score", "expression": "total / 2" },
                ],
            },
        }),
    );

    // Collection-addressed projection read (`Vec` return).
    let projection_collection = service_op(
        "TaskService",
        "summariseAll",
        "summarise_all",
        json!({
            "kind": "projection_read",
            "plan": {
                "module": "summary",
                "projection": "TaskSummary",
                "collection": true,
                "derived": [
                    { "wire_key": "score", "expression": "total / 2" },
                ],
            },
        }),
    );

    json!([
        guarded_submit,
        body_driven,
        plain_update,
        seamed,
        stateless_seam,
        read,
        delete,
        body,
        body_direct,
        create,
        projection_item,
        projection_collection,
    ])
}

/// Child-role service ops: every `child_op` action, with and without a
/// modeled boundary payload, plus the child-scoped read/update arms.
fn child_service_ops() -> Value {
    let add_with_payload = {
        let mut op = service_op(
            "TaskService",
            "addNote",
            "add_note",
            json!({
                "kind": "child_op",
                "action": "add",
                "feature": "notes",
                "child_class": "common::TaskNote",
                "read_back": true,
                "parent_load": {
                    "entity": "Task",
                    "module": "task",
                    "domain": "common",
                    "auditable": false,
                },
                "payload": {
                    "dto_prefix": "Create",
                    "extraction": "    let payload: domain_types::dto::notes::NoteIntake = Json::<domain_types::dto::notes::NoteIntake>::from_value(serde_json::json!({}))?;\n",
                    "construct": "        body: payload.body,\n        task_id: Some(parent_id),\n",
                    "defaulted": ["task_id"],
                    "request_body": "domain_types::dto::notes::NoteIntake",
                    "sample": { "body": "sample" },
                    "required_class_refs": [],
                },
            }),
        );
        op["authorize_fn"] = json!(OP_AUTHORIZE_FN);
        op
    };

    // The same action WITHOUT a modeled boundary: the generated
    // `CreateTaskNoteBody` extractor path.
    let add_plain = {
        let mut op = service_op(
            "TaskService",
            "appendNote",
            "append_note",
            json!({
                "kind": "child_op",
                "action": "add",
                "feature": "notes",
                "child_class": "common::TaskNote",
                "read_back": true,
                "parent_load": {
                    "entity": "Task",
                    "module": "task",
                    "domain": "common",
                    "auditable": true,
                },
            }),
        );
        op["record_load"] = json!(false);
        op
    };

    let save_payload = service_op(
        "TaskService",
        "saveNote",
        "save_note",
        json!({
            "kind": "child_op",
            "action": "save",
            "feature": "notes",
            "child_class": "common::TaskNote",
            "read_back": true,
            "payload": {
                "dto_prefix": "Update",
                "extraction": "    let payload: domain_types::dto::notes::NotePatch = Json::<domain_types::dto::notes::NotePatch>::from_value(serde_json::json!({}))?;\n",
                "construct": "        body: payload.body,\n",
                "defaulted": [],
                "request_body": "domain_types::dto::notes::NotePatch",
                "sample": { "body": "patched" },
                "required_class_refs": [],
            },
        }),
    );

    let save_plain = service_op(
        "TaskService",
        "renameNote",
        "rename_note",
        json!({
            "kind": "child_op",
            "action": "save",
            "feature": "notes",
            "child_class": "common::TaskNote",
            "read_back": true,
        }),
    );

    let remove = service_op(
        "TaskService",
        "removeNote",
        "remove_note",
        json!({
            "kind": "child_op",
            "action": "remove",
            "feature": "notes",
            "child_class": "common::TaskNote",
            "read_back": false,
        }),
    );

    // A child-scoped update with a machine binding (the `find_by_id_scoped`
    // reload arm).
    let update = service_op(
        "TaskService",
        "approveNote",
        "approve_note",
        json!({
            "kind": "update",
            "state": {
                "current_expr": "response.status.clone().unwrap_or_default()",
                "wire_key": "status",
                "initial": "Draft",
                "all": [
                    { "from": "Draft", "to": "Approved" },
                ],
                "op": [
                    { "from": "Draft", "to": "Approved" },
                ],
            },
        }),
    );

    let read = {
        let mut op = service_op(
            "TaskService",
            "peekNote",
            "peek_note",
            json!({ "kind": "read" }),
        );
        op["authorize_fn"] = json!(OP_AUTHORIZE_FN);
        op
    };

    json!([
        add_with_payload,
        add_plain,
        save_payload,
        save_plain,
        remove,
        update,
        read
    ])
}

// ── The permutation table ──────────────────────────────────────────────

/// Fill every service-op fixture's derived `execution_block` (issue #778).
///
/// The matrix mirrors `ServiceOpRoute`'s serde shape by hand; the execution
/// block is DERIVED from the rest of the route, so the mirror derives it the
/// same way the handler plane does — one renderer, never a second copy of the
/// arm text. The facts come from the case's own context (they are exactly the
/// names the template's prelude binds).
fn with_execution_blocks(ctx: &mut Value) {
    // Read the facts out FIRST: the ops array is borrowed mutably below.
    let is_auditable = ctx["is_auditable"].as_bool().unwrap_or(false);
    let nested_child = ctx["role"].as_str() == Some("child");
    let ux_sort_fields = ctx["ux_sort_fields"]
        .as_array()
        .is_some_and(|fields| !fields.is_empty());
    let facts = ExecutionRenderFacts {
        domain: ctx["domain"].as_str().unwrap_or_default().to_string(),
        entity_name: ctx["entity_name"].as_str().unwrap_or_default().to_string(),
        entity_table: ctx["module_name"].as_str().unwrap_or_default().to_string(),
        id_param: ctx["param_name"].as_str().unwrap_or_default().to_string(),
        domain_types_crate: ctx["domain_types_crate"]
            .as_str()
            .unwrap_or("domain_types")
            .to_string(),
        nested_child,
        auditable: is_auditable,
        ux_sort_fields,
    };
    // The handler generator's post-pass copies the handler-level create
    // blocks onto every create-shaped route (the arm splices them).
    let create_guard_block = ctx["create_guard_block"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let create_invariant_block = ctx["create_invariant_block"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let Some(ops) = ctx.get_mut("service_ops").and_then(Value::as_array_mut) else {
        return;
    };
    for op in ops.iter_mut() {
        let mut route: ServiceOpRoute =
            serde_json::from_value(op.clone()).expect("service-op fixture deserializes");
        route.render_facts = facts.clone();
        if matches!(
            route.execution,
            codegraph::generate::ddd::service_ops::ExecutionKind::Create { .. }
        ) {
            route.create_guard_block = create_guard_block.clone();
            route.create_invariant_block = create_invariant_block.clone();
        }
        op["execution_block"] = serde_json::json!(render_execution_block(&route));
    }
}

fn cases() -> Vec<Case> {
    let mut out: Vec<Case> = Vec::new();

    // 1. root: read + list only, no optional surfaces.
    out.push(Case {
        name: "root_read_list_only",
        ctx: root_base(),
        project: ProjectConfig::default(),
    });

    // 2. root: full CRUD, plus the filter / nested-filter planes and the
    //    child / cross-ref link loops.
    let mut ctx = root_base();
    full_crud(&mut ctx);
    ctx["filter_fields"] = json!([
        { "field_name": "status", "pg_column_name": "status", "rust_type": "String", "is_nullable": true },
        { "field_name": "assignee_id", "pg_column_name": "assignee_id", "rust_type": "Uuid", "is_nullable": true },
    ]);
    ctx["nested_filter_fields"] = json!([
        {
            "filter_key": "assignee_name.given_name",
            "sql_table": "\"common\".\"task_assignee_name\"",
            "sql_schema": "common",
            "sql_table_name": "task_assignee_name",
            "parent_fk_column": "task_type_id",
            "pg_column_name": "given_name",
            "rust_type": "String",
            "is_nullable": true,
            "intermediate_join": null,
        },
    ]);
    ctx["children"] = json!([
        { "entity_name": "TaskNote", "module_name": "task_note", "path_segment": "notes" },
    ]);
    ctx["cross_refs"] = json!([
        {
            "entity_name": "Project",
            "module_name": "project",
            "domain": "common",
            "path_segment": "projects",
            "fk_column": "project_id",
            "link_rel": "project",
        },
    ]);
    out.push(Case {
        name: "root_full_crud",
        ctx,
        project: ProjectConfig::default(),
    });

    // 3. root + `?sort=` allow-list.
    let mut ctx = root_base();
    ctx["ux_sort_fields"] = json!(["name", "total_amount"]);
    out.push(Case {
        name: "root_ux_sort",
        ctx,
        project: ProjectConfig::default(),
    });

    // 4. root + FTS via `?q=` on the list endpoint.
    let mut ctx = root_base();
    ctx["has_fts"] = json!(true);
    ctx["fts_rest_mode"] = json!("query_param");
    out.push(Case {
        name: "root_fts_query_param",
        ctx,
        project: ProjectConfig::default(),
    });

    // 5. root + FTS via the dedicated `/search` endpoint, audit arity on
    //    (the search endpoint's `, false` include-deleted argument).
    let mut ctx = root_base();
    ctx["has_fts"] = json!(true);
    ctx["fts_rest_mode"] = json!("dedicated");
    ctx["is_auditable"] = json!(true);
    out.push(Case {
        name: "root_fts_dedicated",
        ctx,
        project: ProjectConfig::default(),
    });

    // 6. root + semantic search (pgvector embeddings).
    let mut ctx = root_base();
    ctx["has_embeddings"] = json!(true);
    ctx["is_auditable"] = json!(true);
    out.push(Case {
        name: "root_semantic_search",
        ctx,
        project: ProjectConfig::default(),
    });

    // 7. root + self-referencing hierarchy: the `/tree` endpoint, both the
    //    typed and the `serde_json::Value` return shapes.
    let mut ctx = root_base();
    ctx["hierarchy_field"] = json!("parent_id");
    ctx["tree_include"] = json!(true);
    out.push(Case {
        name: "root_hierarchy_tree",
        ctx,
        project: ProjectConfig::default(),
    });
    let mut ctx = root_base();
    ctx["hierarchy_field"] = json!("parent_id");
    ctx["tree_include"] = json!(false);
    out.push(Case {
        name: "root_hierarchy_tree_typed",
        ctx,
        project: ProjectConfig::default(),
    });

    // 8. root + workflow status filtering.
    let mut ctx = root_base();
    ctx["has_workflow"] = json!(true);
    ctx["status_field"] = json!("status");
    out.push(Case {
        name: "root_workflow_status",
        ctx,
        project: ProjectConfig::default(),
    });

    // 9. root + `?include=` with a single-segment scalar path and an array path.
    let mut ctx = root_base();
    full_crud(&mut ctx);
    ctx["has_include"] = json!(true);
    let paths = vec![
        include_path(
            "person",
            vec![include_segment("person", "Person", false)],
            "PersonResponse",
            "fetch_person_for_task",
            "fetch_person_batch_for_task",
        ),
        include_path(
            "attachments",
            vec![include_segment("attachment", "Attachment", true)],
            "AttachmentResponse",
            "fetch_attachments_for_task",
            "fetch_attachments_batch_for_task",
        ),
    ];
    ctx["include_paths"] = json!(paths);
    // `included_default_fields` mirrors `HandlerContext`: the unique
    // first-segment module names, in first-occurrence order.
    ctx["included_default_fields"] = json!(["person", "attachment"]);
    let project = ProjectConfig::default();
    apply_include_blocks(&mut ctx, &paths, &project);
    out.push(Case {
        name: "root_include_single_segment",
        ctx,
        project,
    });

    // 10. root + `?include=` with two dot-notation paths sharing the first
    //     segment (the sibling-accumulation merge).
    let mut ctx = root_base();
    full_crud(&mut ctx);
    ctx["has_include"] = json!(true);
    let paths = vec![
        include_path(
            "assignee.person",
            vec![
                include_segment("assignee", "Assignee", false),
                include_segment("person", "Person", false),
            ],
            "AssigneeCombinedResponse",
            "fetch_assignee_person_for_task",
            "fetch_assignee_person_batch_for_task",
        ),
        include_path(
            "assignee.organization",
            vec![
                include_segment("assignee", "Assignee", false),
                include_segment("organization", "Organization", false),
            ],
            "AssigneeCombinedResponse",
            "fetch_assignee_organization_for_task",
            "fetch_assignee_organization_batch_for_task",
        ),
    ];
    ctx["include_paths"] = json!(paths);
    // Sibling dot paths share the `assignee` first segment: ONE default field.
    ctx["included_default_fields"] = json!(["assignee"]);
    let project = ProjectConfig::default();
    apply_include_blocks(&mut ctx, &paths, &project);
    out.push(Case {
        name: "root_include_dot_siblings",
        ctx,
        project,
    });

    // 11. cornucopia provider + `?include=`: issue #781 P0 — the include
    //     surface FAILS CLOSED (guarded 501 + the honest doc line) instead of
    //     silently returning empty include data. A no-include request still
    //     succeeds (the `if !include_paths.is_empty()` guard).
    let mut ctx = root_base();
    full_crud(&mut ctx);
    ctx["has_include"] = json!(true);
    let paths = vec![include_path(
        "person",
        vec![include_segment("person", "Person", false)],
        "PersonResponse",
        "fetch_person_for_task",
        "fetch_person_batch_for_task",
    )];
    ctx["include_paths"] = json!(paths);
    ctx["included_default_fields"] = json!(["person"]);
    let project = cornucopia_project();
    apply_include_blocks(&mut ctx, &paths, &project);
    out.push(Case {
        name: "cornucopia_include",
        ctx,
        project,
    });

    // 12. child: nested under its parent, one ancestor path param, full CRUD,
    //     audit arity on (the `, false` include-deleted argument).
    let mut ctx = child_base();
    full_crud(&mut ctx);
    ctx["is_auditable"] = json!(true);
    out.push(Case {
        name: "child_full_crud",
        ctx,
        project: ProjectConfig::default(),
    });

    // 13. grandchild: two ancestor path params, full CRUD, plus a child-op
    //     `add` so the depth-2 service-op path/param branches are pinned.
    let mut ctx = grandchild_base();
    full_crud(&mut ctx);
    // A cross-domain parent chain: `grandparent_domain` PRESENT covers the
    // other arm of `grandparent_domain | default(value=domain)`.
    ctx["grandparent_domain"] = json!("people");
    ctx["service_ops"] = json!([{
        "service": "TaskNoteService",
        "op_name": "addRevisionImage",
        "snake": "add_revision_image",
        "fn_name": "service_op_add_revision_image",
        "capabilities": [],
        "execution": {
            "kind": "child_op",
            "action": "add",
            "feature": "images",
            "child_class": "common::RevisionImage",
            "read_back": true,
            "parent_load": {
                "entity": "TaskNote",
                "module": "task_note",
                "domain": "common",
                "auditable": false,
            },
        },
        "record_load": false,
    }]);
    out.push(Case {
        name: "grandchild_full_crud_with_service_ops",
        ctx,
        project: ProjectConfig::default(),
    });

    // 14. root + eligibility guards: the guard surface flag, the four
    //     operation gates and their pre-rendered blocks.
    let mut ctx = root_base();
    full_crud(&mut ctx);
    ctx["eligibility_guards"] = json!([
        {
            "rule_name": "PositiveTotal",
            "fn_name": "task_positive_total",
            "call_path": "crate::domain::common::rules::task_positive_total",
            "operations": ["create", "read", "update", "delete"],
        },
    ]);
    ctx["create_guard_block"] = json!(CREATE_GUARD_BLOCK);
    ctx["update_guard_block"] = json!(UPDATE_GUARD_BLOCK);
    ctx["delete_guard_block"] = json!(DELETE_GUARD_BLOCK);
    ctx["read_guard_block"] = json!(READ_GUARD_BLOCK);
    // The guard presence flags `HandlerContext` derives from the one guard
    // above: every gated operation documents its 403.
    ctx["create_guarded"] = json!(true);
    ctx["read_guarded"] = json!(true);
    ctx["update_guarded"] = json!(true);
    ctx["delete_guarded"] = json!(true);
    out.push(Case {
        name: "root_eligibility_guards",
        ctx,
        project: ProjectConfig::default(),
    });

    // 15. root + mox ops routed as `POST {id}/op/{snake}` (audit arity on:
    //     the op's `find_by_id` carries the include-deleted argument).
    let mut ctx = root_base();
    full_crud(&mut ctx);
    ctx["is_auditable"] = json!(true);
    ctx["mox_ops"] = json!([
        {
            "name": "Reopen",
            "snake": "reopen",
            "params": [
                { "name": "reason", "rust_type": "String" },
                { "name": "priority", "rust_type": "i32" },
            ],
        },
        {
            "name": "Touch",
            "snake": "touch",
            "params": [],
        },
    ]);
    out.push(Case {
        name: "root_mox_ops",
        ctx,
        project: ProjectConfig::default(),
    });

    // 16. root + service ops covering every execution kind the template
    //     branches on (root role), with the invariant blocks that ride them.
    let mut ctx = root_base();
    full_crud(&mut ctx);
    // The sort allow-list also rides the collection projection read (the
    // `ux_sort` argument slot) — one more interaction the table pins.
    ctx["ux_sort_fields"] = json!(["name", "total_amount"]);
    ctx["service_ops"] = root_service_ops();
    ctx["create_invariant_block"] = json!(CREATE_INVARIANT_BLOCK);
    ctx["update_invariant_block"] = json!(UPDATE_INVARIANT_BLOCK);
    out.push(Case {
        name: "root_service_ops",
        ctx,
        project: ProjectConfig::default(),
    });

    // 17. root + media cleanup + auditable arity.
    let mut ctx = root_base();
    full_crud(&mut ctx);
    ctx["is_auditable"] = json!(true);
    ctx["media_cleanup_block"] = json!(MEDIA_CLEANUP_BLOCK);
    out.push(Case {
        name: "root_media_cleanup_auditable",
        ctx,
        project: ProjectConfig::default(),
    });

    // 18. child + service ops: every `child_op` action (with and without a
    //     modeled boundary payload), plus the child-scoped read/update arms.
    //     A cross-domain parent covers the PRESENT arm of
    //     `parent_domain | default(value=domain)`.
    let mut ctx = child_base();
    full_crud(&mut ctx);
    ctx["parent_domain"] = json!("people");
    ctx["service_ops"] = child_service_ops();
    out.push(Case {
        name: "service_ops_child_roles",
        ctx,
        project: ProjectConfig::default(),
    });

    // 19. child + eligibility guards: the `*_ownership_prefix` keys (only
    //     interpolated on the child arms) plus the child-shaped guard blocks.
    let mut ctx = child_base();
    full_crud(&mut ctx);
    ctx["eligibility_guards"] = json!([
        {
            "rule_name": "PositiveTotal",
            "fn_name": "task_note_positive_total",
            "call_path": "crate::domain::common::rules::task_note_positive_total",
            "operations": ["read", "update", "delete"],
        },
    ]);
    ctx["update_ownership_prefix"] = json!("let existing = ");
    ctx["delete_ownership_prefix"] = json!("let existing = ");
    ctx["update_guard_block"] = json!(
        r#"    // Eligibility rule 'PositiveTotal' (issue #338): deny with 403 when it fails.
    if !crate::domain::common::rules::task_note_positive_total(&existing) {
        return Err(AppError::forbidden(format!("Eligibility rule 'PositiveTotal' denied this TaskNote operation"))
            .with_correlation_id(correlation_id));
    }"#
    );
    ctx["delete_guard_block"] = json!(
        r#"    // Eligibility rule 'PositiveTotal' (issue #338): deny with 403 when it fails.
    if !crate::domain::common::rules::task_note_positive_total(&existing) {
        return Err(AppError::forbidden(format!("Eligibility rule 'PositiveTotal' denied this TaskNote operation"))
            .with_correlation_id(correlation_id));
    }"#
    );
    ctx["read_guard_block"] = json!(READ_GUARD_BLOCK);
    // `read`/`update`/`delete` are gated; `create` is not (no create block).
    ctx["read_guarded"] = json!(true);
    ctx["update_guarded"] = json!(true);
    ctx["delete_guarded"] = json!(true);
    out.push(Case {
        name: "child_eligibility_guards",
        ctx,
        project: ProjectConfig::default(),
    });

    // 20. root, auditable, NO include surface: the `?include_deleted=` toggle
    //     (issue #792). The GET extractor widens from `has_include` to
    //     `has_include or is_auditable`, ListParams gains the toggle field,
    //     and GET/LIST forward `params.include_deleted.unwrap_or(false)`
    //     where `false` used to be hardcoded. The pre-rendered role gate
    //     (manager-or-better; api-key bypass) rides both read endpoints.
    let mut ctx = root_base();
    full_crud(&mut ctx);
    ctx["is_auditable"] = json!(true);
    out.push(Case {
        name: "root_auditable_include_deleted_toggle",
        ctx,
        project: ProjectConfig::default(),
    });

    // 21. the same auditable shape under a hierarchy WITHOUT `manager`: the
    //     emitted helper keeps its fail-closed arm (every JWT role refuses;
    //     API-key sessions still bypass). The hierarchy is not serialized
    //     into the template context (the gate is pre-rendered), so this
    //     case's gate is re-applied with the custom hierarchy after the
    //     shared loop below.
    let mut ctx = root_base();
    full_crud(&mut ctx);
    ctx["is_auditable"] = json!(true);
    out.push(Case {
        name: "root_auditable_toggle_no_manager_hierarchy",
        ctx,
        project: ProjectConfig::default(),
    });

    // 22. provider parity (issue #792): the cornucopia project renders the
    //     SAME toggle surface — identical forward + guard — for the same
    //     auditable context. Both providers' query planes take
    //     `include_deleted: bool`; only the persistence plane differs.
    let mut ctx = root_base();
    full_crud(&mut ctx);
    ctx["is_auditable"] = json!(true);
    out.push(Case {
        name: "cornucopia_auditable_include_deleted_toggle",
        ctx,
        project: cornucopia_project(),
    });

    for case in out.iter_mut() {
        with_execution_blocks(&mut case.ctx);
    }

    // The gate artifacts mirror production for every case (the builder's own
    // gating decides active vs empty); the no-manager case re-applies with
    // its custom hierarchy.
    let custom = no_manager_hierarchy();
    for case in out.iter_mut() {
        if case.name == "root_auditable_toggle_no_manager_hierarchy" {
            apply_include_deleted_gate_with(&mut case.ctx, &custom);
        } else {
            apply_include_deleted_gate(&mut case.ctx);
        }
    }

    out.sort_by(|a, b| a.name.cmp(b.name));
    out
}

/// A hierarchy without `manager`: the fail-closed permutation's hierarchy.
fn no_manager_hierarchy() -> Vec<String> {
    ["owner", "admin", "member"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

// ── Rendering + hashing ────────────────────────────────────────────────

fn sha256_hex(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_path() -> PathBuf {
    manifest_dir().join(FIXTURE_REL)
}

/// `<temp>/codegraph-handler-matrix/` — bless snapshots so a later failing run
/// can print a real first-difference hint instead of two opaque hashes. Kept
/// out of the repo tree: these are scratch oracle data, never committed.
fn snapshot_root() -> PathBuf {
    std::env::temp_dir().join("codegraph-handler-matrix")
}

/// First line-level divergence between the blessed snapshot and the current
/// render, with a few lines of context around it.
fn first_difference(expected: &str, actual: &str) -> String {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let first = expected_lines
        .iter()
        .zip(actual_lines.iter())
        .position(|(e, a)| e != a);

    let mut out = String::new();
    match first {
        Some(index) => {
            out.push_str(&format!(
                "first difference at line {} (1-based); expected {} lines / {} bytes, actual {} lines / {} bytes\n",
                index + 1,
                expected_lines.len(),
                expected.len(),
                actual_lines.len(),
                actual.len()
            ));
            let start = index.saturating_sub(2);
            for (i, line) in expected_lines.iter().enumerate().take(index).skip(start) {
                out.push_str(&format!("  {:>5}   {}\n", i + 1, line));
            }
            out.push_str(&format!("  {:>5} - {}\n", index + 1, expected_lines[index]));
            out.push_str(&format!("  {:>5} + {}\n", index + 1, actual_lines[index]));
            let end = std::cmp::min(index + 2, actual_lines.len().saturating_sub(1));
            for (i, line) in actual_lines
                .iter()
                .enumerate()
                .take(end + 1)
                .skip(index + 1)
            {
                out.push_str(&format!("  {:>5} + {}\n", i + 1, line));
            }
        }
        None if expected_lines.len() != actual_lines.len() => {
            out.push_str(&format!(
                "common prefix identical; line counts differ: expected {}, actual {}\n",
                expected_lines.len(),
                actual_lines.len()
            ));
        }
        None => out.push_str("no line-level difference (byte-level difference only)\n"),
    }
    out
}

/// Read the committed fixture as `(name, hash)` pairs, rejecting malformed
/// or duplicated lines (a fixture that cannot be trusted is a hard failure).
fn read_fixture(path: &Path) -> Vec<(String, String)> {
    let raw =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut entries: Vec<(String, String)> = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, hash) = line.split_once('\t').unwrap_or_else(|| {
            panic!(
                "{}:{}: expected `<name>\\t<hex>`, got {line:?}",
                path.display(),
                index + 1
            )
        });
        assert!(
            hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()),
            "{}:{}: hash for {name:?} is not a sha256 hex digest: {hash:?}",
            path.display(),
            index + 1
        );
        assert!(
            !entries.iter().any(|(existing, _)| existing == name),
            "{}:{}: duplicate permutation {name:?}",
            path.display(),
            index + 1
        );
        entries.push((name.to_string(), hash.to_ascii_lowercase()));
    }
    entries
}

fn fixture_text(entries: &[(String, String)]) -> String {
    let mut out = String::new();
    out.push_str("# Handler output matrix — `api/handler.tera` byte-identity oracle (wave 0 / T0.1, issue #778).\n");
    out.push_str("# One line per rendered permutation: <name>\\t<sha256-hex>, sorted by name.\n");
    out.push_str("# Refresh ONLY with an enumerated, reviewed output diff:\n");
    out.push_str(
        "#   HANDLER_MATRIX_BLESS=1 cargo test -p codegraph --test handler_matrix_tests\n",
    );
    for (name, hash) in entries {
        out.push_str(name);
        out.push('\t');
        out.push_str(hash);
        out.push('\n');
    }
    out
}

// ── The gate ───────────────────────────────────────────────────────────

#[test]
fn handler_output_matrix_is_byte_identical() {
    let cases = cases();

    // The table itself must stay sane: unique names, sorted.
    let mut names: Vec<&str> = cases.iter().map(|case| case.name).collect();
    let sorted = {
        let mut copy = names.clone();
        copy.sort_unstable();
        copy
    };
    assert_eq!(
        names, sorted,
        "the permutation table must be sorted by name"
    );
    names.dedup();
    assert_eq!(
        names.len(),
        cases.len(),
        "the permutation table must have unique names"
    );

    let tera = create_tera(Path::new(".")).expect("embedded templates load");

    let mut computed: Vec<(String, String)> = Vec::with_capacity(cases.len());
    let mut rendered: Vec<(&str, String)> = Vec::with_capacity(cases.len());
    for case in &cases {
        let output =
            render_template_with_project(&tera, "api/handler.tera", &case.ctx, &case.project)
                .unwrap_or_else(|e| panic!("permutation {:?} failed to render: {e}", case.name));
        assert!(
            output.contains("pub async fn"),
            "permutation {:?} rendered no handler body — the context is wrong:\n{output}",
            case.name
        );
        let hash = sha256_hex(&output);
        computed.push((case.name.to_string(), hash));
        rendered.push((case.name, output));
    }
    let computed_text = fixture_text(&computed);

    let path = fixture_path();
    let bless = std::env::var(BLESS_ENV).is_ok_and(|value| value == "1");

    if bless {
        std::fs::write(&path, &computed_text)
            .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        // Keep per-permutation snapshots so a later failing run can print a
        // real first-difference hint (see `first_difference`).
        let expected_dir = snapshot_root().join("expected");
        std::fs::create_dir_all(&expected_dir)
            .unwrap_or_else(|e| panic!("create {}: {e}", expected_dir.display()));
        for (name, output) in &rendered {
            let snapshot = expected_dir.join(format!("{name}.rs"));
            std::fs::write(&snapshot, output)
                .unwrap_or_else(|e| panic!("write {}: {e}", snapshot.display()));
        }
        eprintln!(
            "blessed {} permutations into {} (snapshots: {})",
            computed.len(),
            path.display(),
            expected_dir.display()
        );
        return;
    }

    assert!(
        path.exists(),
        "missing fixture {} — generate it with {BLESS_ENV}=1 cargo test -p codegraph --test handler_matrix_tests",
        path.display()
    );
    let expected = read_fixture(&path);

    // The fixture cannot rot: names must match EXACTLY in both directions.
    let expected_names: Vec<&str> = expected.iter().map(|(name, _)| name.as_str()).collect();
    let computed_names: Vec<&str> = computed.iter().map(|(name, _)| name.as_str()).collect();
    let missing: Vec<&&str> = computed_names
        .iter()
        .filter(|name| !expected_names.contains(name))
        .collect();
    let extra: Vec<&&str> = expected_names
        .iter()
        .filter(|name| !computed_names.contains(name))
        .collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "permutation table and fixture disagree.\n  missing from {}: {missing:?}\n  stale in {}: {extra:?}",
        path.display(),
        path.display()
    );

    let mut failures: Vec<String> = Vec::new();
    for ((name, expected_hash), (_, actual_hash)) in expected.iter().zip(computed.iter()) {
        if expected_hash == actual_hash {
            continue;
        }
        let mut message = format!(
            "permutation {name:?} drifted:\n  fixture sha256: {expected_hash}\n  current sha256: {actual_hash}"
        );
        let expected_snapshot = snapshot_root().join("expected").join(format!("{name}.rs"));
        if let Ok(expected_output) = std::fs::read_to_string(&expected_snapshot) {
            if let Some((_, actual_output)) = rendered.iter().find(|(n, _)| n == name) {
                message.push_str(&format!(
                    "\nhint (against the last bless snapshot {}):\n{}",
                    expected_snapshot.display(),
                    first_difference(&expected_output, actual_output)
                ));
            }
        } else {
            message.push_str(&format!(
                "\n(no bless snapshot at {} — run the bless command in a scratch checkout to capture a line-level diff)",
                expected_snapshot.display()
            ));
        }
        failures.push(message);
    }

    assert!(
        failures.is_empty(),
        "{} of {} handler permutations drifted from the committed matrix.\n\
         If the output change is INTENTIONAL, review the diff and refresh the fixture with\n\
         {BLESS_ENV}=1 cargo test -p codegraph --test handler_matrix_tests\n\n{}",
        failures.len(),
        computed.len(),
        failures.join("\n\n")
    );
}
