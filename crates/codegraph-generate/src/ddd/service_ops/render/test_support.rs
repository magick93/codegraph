//! Shared `ServiceOpRoute` fixtures for the arm KATs (issue #778).
//!
//! Each arm's tests build routes from these: the fields the renderer never
//! reads are fixed here, so a KAT states only what its own arm consumes.

use crate::ddd::service_ops::types::{ExecutionKind, ExecutionRenderFacts, ServiceOpRoute};

/// The render facts one route's execution block renders with.
pub(super) fn facts(
    domain: &str,
    entity_name: &str,
    entity_table: &str,
    id_param: &str,
    nested_child: bool,
    auditable: bool,
) -> ExecutionRenderFacts {
    ExecutionRenderFacts {
        domain: domain.to_string(),
        entity_name: entity_name.to_string(),
        entity_table: entity_table.to_string(),
        id_param: id_param.to_string(),
        domain_types_crate: "domain_types".to_string(),
        nested_child,
        auditable,
        // The arm KATs that exercise the sort arity set it explicitly.
        ux_sort_fields: false,
    }
}

/// A `ServiceOpRoute` fixture: the renderer reads `execution`, `record_load`
/// (the seam arm) and `render_facts`, so those are the parameters; every
/// other field is a fixed plausible value.
pub(super) fn route(
    service: &str,
    op_name: &str,
    render_facts: ExecutionRenderFacts,
    execution: ExecutionKind,
    record_load: bool,
) -> ServiceOpRoute {
    let snake = codegraph_naming::to_snake_case(op_name);
    ServiceOpRoute {
        service: service.to_string(),
        op_name: op_name.to_string(),
        snake: snake.clone(),
        fn_name: format!("service_op_{snake}"),
        capabilities: Vec::new(),
        authorize_fn: None,
        execution,
        record_load,
        authorize_async: false,
        api_key: Default::default(),
        injected_guard: None,
        invariant_block: None,
        create_guard_block: String::new(),
        create_invariant_block: String::new(),
        throws: Vec::new(),
        raise_responses: Vec::new(),
        render_facts,
        execution_block: String::new(),
        publish_arg: None,
    }
}

/// The `payroll` (`basic` arm) facts.
pub(super) fn payroll_facts() -> ExecutionRenderFacts {
    facts(
        "payroll",
        "PayRunType",
        "pay_run",
        "pay_run_id",
        false,
        false,
    )
}

/// The handler-matrix `root_service_ops` (`common`/`task`) facts.
pub(super) fn matrix_facts() -> ExecutionRenderFacts {
    facts("common", "Task", "task", "task_id", false, false)
}

/// The handler-matrix `service_ops_child_roles` (`common`/`task_note`) facts.
pub(super) fn child_facts() -> ExecutionRenderFacts {
    facts(
        "common",
        "TaskNote",
        "task_note",
        "task_note_id",
        true,
        false,
    )
}
