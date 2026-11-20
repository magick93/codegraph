use super::*;

/// One `execution_for` call with the test fixture's route shape.
fn lower_exec(
    model: &codegraph_core::types::DddModelGraph,
    config: &DomainConfig,
    surface_ops: &[String],
    op: &codegraph_core::types::DddServiceOperation,
    machine: Option<&phero_ir::ddd::StateMachine>,
    fields: &HashMap<String, FieldInfo>,
) -> ExecutionKind {
    try_lower_exec(model, config, surface_ops, op, machine, fields).expect("lowering")
}

fn try_lower_exec(
    model: &codegraph_core::types::DddModelGraph,
    config: &DomainConfig,
    surface_ops: &[String],
    op: &codegraph_core::types::DddServiceOperation,
    machine: Option<&phero_ir::ddd::StateMachine>,
    fields: &HashMap<String, FieldInfo>,
) -> Result<ExecutionKind, String> {
    let raises = crate::ddd::body_lower::RaiseTable::default();
    let shape = RouteShape {
        domain: "payroll",
        entity_title: "PayRunType",
        entity_name: "PayRunType",
        entity_table: "pay_run",
        id_param: "pay_run_id",
        scoped: None,
        auditable: false,
        domain_types_crate: "domain_types",
        dto_casing: crate::project_config::DtoKeyCasing::Snake,
        fields,
        surface_ops,
        record_load: true,
        machine,
        raises: &raises,
        params: &[],
        throws: &[],
    };
    execution_for(op, model, config, "PayRunService", &shape)
}

fn fields(pairs: &[(&str, FieldKind, bool)]) -> HashMap<String, FieldInfo> {
    let mut map = HashMap::new();
    for (name, kind, optional) in pairs {
        // DTO fields are snake_case; the authored (often camelCase)
        // name keys the lookup, mirroring `fields_for`.
        let rust_name = to_snake_case(name);
        let info = FieldInfo {
            rust_name,
            kind: *kind,
            optional: *optional,
        };
        map.insert(name.to_string(), info);
    }
    map
}

fn lower(source: &str, fields: &HashMap<String, FieldInfo>) -> Result<String, String> {
    lower_when_to_rust(source, "ApprovePayRun", "PayRunType", fields)
}

#[test]
fn string_equality_lowering() {
    let f = fields(&[("status", FieldKind::String, false)]);
    assert_eq!(
        lower(r#"status == "AwaitingApproval""#, &f).unwrap(),
        r#"(response.status == "AwaitingApproval")"#
    );
}

#[test]
fn optional_string_equality_lowering() {
    let f = fields(&[("status", FieldKind::String, true)]);
    assert_eq!(
        lower(r#"status != "Posted""#, &f).unwrap(),
        r#"(response.status.as_deref() != Some("Posted"))"#
    );
}

#[test]
fn codelist_and_date_compare_via_display() {
    let f = fields(&[
        ("status", FieldKind::Codelist, false),
        ("filedOn", FieldKind::Date, true),
    ]);
    assert_eq!(
        lower(r#"status == "AwaitingApproval""#, &f).unwrap(),
        r#"(response.status.to_string() == "AwaitingApproval")"#
    );
    assert_eq!(
        lower(r#"filedOn == date("2026-01-15")"#, &f).unwrap(),
        r#"(response.filed_on.as_ref().map(|v| v.to_string()).as_deref() == Some("2026-01-15"))"#
    );
}

#[test]
fn numeric_and_bool_orderings() {
    let f = fields(&[
        ("amount", FieldKind::Float, false),
        ("count", FieldKind::Integer, true),
        ("active", FieldKind::Bool, false),
    ]);
    assert_eq!(
        lower("amount >= 0.0", &f).unwrap(),
        "(response.amount >= 0.0)"
    );
    assert_eq!(
        lower("count < 5", &f).unwrap(),
        "(response.count.is_some_and(|v| v < 5))"
    );
    assert_eq!(
        lower("active == true", &f).unwrap(),
        "(response.active == true)"
    );
}

#[test]
fn logical_and_null_forms() {
    let f = fields(&[
        ("status", FieldKind::String, false),
        ("note", FieldKind::String, true),
        ("amount", FieldKind::Float, false),
    ]);
    assert_eq!(
        lower(
            r#"status == "Draft" && amount > 0.0 && !(note == null)"#,
            &f
        )
        .unwrap(),
        "(((response.status == \"Draft\") && (response.amount > 0.0)) && (!((response.note.is_none()))))"
    );
    assert_eq!(
        lower("note != null", &f).unwrap(),
        "(!(response.note.is_none()))"
    );
    assert_eq!(
        lower("status == null", &f).unwrap(),
        "false" // required field: vacuously false
    );
}

#[test]
fn refusals_name_the_capability() {
    let f = fields(&[("amount", FieldKind::Float, false)]);
    let err = lower("amount + 1.0 > 2.0", &f).unwrap_err();
    assert!(err.contains("capability `ApprovePayRun`"), "{err}");
    assert!(err.contains("arithmetic"), "{err}");
    let err = lower("total == 1.0", &f).unwrap_err();
    assert!(err.contains("is not a field of `PayRunType`"), "{err}");
    let err = lower("customer.name == \"x\"", &f).unwrap_err();
    assert!(err.contains("field access"), "{err}");
    let err = lower("date(\"15/01/2026\") == \"x\"", &f).unwrap_err();
    assert!(err.contains("not a calendar date"), "{err}");
}

#[test]
fn literal_flips_ordering_to_keep_field_on_the_left() {
    let f = fields(&[("amount", FieldKind::Float, false)]);
    assert_eq!(
        lower("0.0 < amount", &f).unwrap(),
        "(response.amount > 0.0)"
    );
}

fn policy_fixture() -> PolicySurface {
    let mut roles = BTreeMap::new();
    // owner holds ApprovePayRun when (status == "AwaitingApproval")
    let mut owner = BTreeMap::new();
    owner.insert(
        "ApprovePayRun".to_string(),
        vec![Some(r#"status == "AwaitingApproval""#.to_string())],
    );
    roles.insert("owner".to_string(), owner);
    // manager holds both sides of the never_both pair
    let mut manager = BTreeMap::new();
    manager.insert("ApprovePayRun".to_string(), vec![None]);
    manager.insert("PreparePayRun".to_string(), vec![None]);
    roles.insert("manager".to_string(), manager);
    // member holds nothing relevant
    roles.insert("member".to_string(), BTreeMap::new());
    PolicySurface {
        roles,
        never_both: vec![vec![
            "PreparePayRun".to_string(),
            "ApprovePayRun".to_string(),
        ]],
    }
}

#[test]
fn authorize_fn_encodes_capability_sod_and_when() {
    let f = fields(&[("status", FieldKind::String, false)]);
    let surface = policy_fixture();
    let rendered = render_authorize_fn(
        "approve",
        "approve",
        "PayRunService",
        &["ApprovePayRun".to_string()],
        None,
        "crate::domain::payroll::pay_run::dto_response::PayRunResponse",
        Some(&surface),
        &f,
        "PayRunType",
        true,
    )
    .unwrap()
    .expect("policy surface exists");
    // api_key bypass
    assert!(rendered.contains(r#"if role == "api_key""#), "{rendered}");
    // owner arm carries the when guard over the record
    assert!(
        rendered.contains(r#"if !((response.status == "AwaitingApproval"))"#),
        "{rendered}"
    );
    // manager arm is denied by separation of duties
    assert!(
        rendered.contains("separation of duties: role `manager` holds `PreparePayRun`"),
        "{rendered}"
    );
    // member falls to the deny-all arm
    assert!(
        rendered.contains("capability `ApprovePayRun` required"),
        "{rendered}"
    );
}

#[test]
fn authorize_fn_emits_the_op_level_transition_guard() {
    let f = fields(&[("status", FieldKind::String, false)]);
    let surface = policy_fixture();
    let rendered = render_authorize_fn(
        "approve",
        "approve",
        "PayRunService",
        &["ApprovePayRun".to_string()],
        Some(r#"status == "AwaitingApproval""#),
        "crate::domain::payroll::pay_run::dto_response::PayRunResponse",
        Some(&surface),
        &f,
        "PayRunType",
        true,
    )
    .unwrap()
    .expect("policy surface exists");
    // The transition guard is business semantics: it sits BEFORE the
    // api_key bypass and applies to every session.
    let api_key_pos = rendered.find(r#"if role == "api_key""#).unwrap();
    let cond_pos = rendered
        .find(r#"if !((response.status == "AwaitingApproval"))"#)
        .unwrap();
    assert!(cond_pos < api_key_pos, "{rendered}");
    assert!(rendered.contains("`when` condition not met"), "{rendered}");
}

#[test]
fn condition_only_op_emits_a_guard_without_a_policy() {
    let f = fields(&[("status", FieldKind::String, false)]);
    let rendered = render_authorize_fn(
        "submit",
        "submit",
        "TimeService",
        &[],
        Some(r#"status == "Draft""#),
        "crate::domain::time::timecard::dto_response::TimecardResponse",
        None,
        &f,
        "TimecardType",
        true,
    )
    .unwrap()
    .expect("a condition alone is enough to gate");
    assert!(
        rendered.contains(r#"if !((response.status == "Draft"))"#),
        "{rendered}"
    );
    // No role arms without a policy: everything but api_key denies.
    assert!(rendered.contains("match role"), "{rendered}");
    assert!(
        rendered.contains("capability ` ` required") || rendered.contains("required"),
        "{rendered}"
    );
}

#[test]
fn authorize_fn_none_without_capabilities() {
    let surface = policy_fixture();
    let rendered = render_authorize_fn(
        "prepare",
        "prepare",
        "PayRunService",
        &[],
        None,
        "crate::domain::payroll::pay_run::dto_response::PayRunResponse",
        Some(&surface),
        &HashMap::new(),
        "PayRunType",
        true,
    )
    .unwrap();
    assert!(rendered.is_none());
}

// ── Non-root record loads (the #595 follow-up; crewbase#71) ──────────

#[test]
fn no_record_load_renders_a_response_less_gate() {
    let f = fields(&[("status", FieldKind::String, false)]);
    let surface = policy_fixture();
    // `manager`'s permit is unconditional (`vec![None]`), so the gate
    // is capability membership + sod only — no record needed.
    let rendered = render_authorize_fn(
        "issue",
        "issue",
        "PaySlipService",
        &["IssuePaySlip".to_string()],
        None,
        "crate::domain::payroll::pay_slip::dto_response::PaySlipResponse",
        Some(&surface),
        &f,
        "PaySlipType",
        false,
    )
    .unwrap()
    .expect("membership gate renders without a record");
    // The response-less signature: no `response` parameter.
    assert!(!rendered.contains("response: &"), "{rendered}");
    // Still carries the deny-all membership arm.
    assert!(
        rendered.contains("capability `IssuePaySlip` required"),
        "{rendered}"
    );
}

#[test]
fn when_condition_without_a_record_is_a_generation_error() {
    let f = fields(&[("status", FieldKind::String, false)]);
    let surface = policy_fixture();
    // owner's permit carries `when (status == ...)` — unrenderable
    // without a record load; fail closed at generation time.
    let err = render_authorize_fn(
        "approve",
        "approve",
        "PaySlipService",
        &["ApprovePayRun".to_string()],
        None,
        "crate::domain::payroll::pay_slip::dto_response::PaySlipResponse",
        Some(&surface),
        &f,
        "PaySlipType",
        false,
    )
    .unwrap_err();
    assert!(err.contains("`when` conditions"), "{err}");
    assert!(err.contains("no record load"), "{err}");
}

#[test]
fn sod_conflicts_are_deterministic_and_deduped() {
    let surface = policy_fixture();
    assert_eq!(
        surface.sod_conflicts(&["ApprovePayRun".to_string()]),
        vec!["PreparePayRun".to_string()]
    );
    assert!(surface.sod_conflicts(&["Unrelated".to_string()]).is_empty());
}

#[test]
fn basejump_role_mapping_mirrors_the_hydration_convention() {
    let actors = vec![
        "PayrollManager".to_string(),
        "Owner".to_string(),
        "Member".to_string(),
    ];
    assert_eq!(
        basejump_role_actors("owner", &actors),
        vec!["Owner".to_string()]
    );
    assert_eq!(
        basejump_role_actors("member", &actors),
        vec!["Member".to_string()]
    );
    // No actor named Manager: the role maps to nobody.
    assert_eq!(
        basejump_role_actors("manager", &actors),
        Vec::<String>::new()
    );
    // Unnamed actors: owner falls to first, member to last.
    let anon = vec!["Auditor".to_string(), "Clerk".to_string()];
    assert_eq!(
        basejump_role_actors("owner", &anon),
        vec!["Auditor".to_string()]
    );
    assert_eq!(
        basejump_role_actors("member", &anon),
        vec!["Clerk".to_string()]
    );
}

fn empty_config() -> DomainConfig {
    toml::from_str(
        r#"
[domains.payroll]
label = "Payroll"
schema_dir = "schemas/payroll"
postgres_schema = "payroll"
"#,
    )
    .unwrap()
}

#[test]
fn execution_maps_repository_builtins_and_save() {
    let model = codegraph_core::types::DddModelGraph {
        source_path: "test.ddd".to_string(),
        application: codegraph_core::types::DddApplicationNode {
            name: "App".to_string(),
            base: None,
            source_path: "test.ddd".to_string(),
            errors: Vec::new(),
        },
        modules: Vec::new(),
        designs: Vec::new(),
        repositories: vec![codegraph_core::types::DddRepositoryNode {
            application: "App".to_string(),
            name: "PayRunRepository".to_string(),
            design_class: "PayRun".to_string(),
            operations: Vec::new(),
        }],
        services: Vec::new(),
        searches: Vec::new(),
        dto_shapes: Vec::new(),
    };
    let ops = [
        "delete".to_string(),
        "read".to_string(),
        "update".to_string(),
    ];
    let config = empty_config();
    let op = |target: Option<&str>, operation: Option<&str>| {
        codegraph_core::types::DddServiceOperation {
            name: "op".to_string(),
            return_type: None,
            return_multiplicity: None,
            params: Vec::new(),
            delegation_target: target.map(|t| t.to_string()),
            delegation_operation: operation.map(|o| o.to_string()),
            capabilities: Vec::new(),
            is_protected: false,
            condition: None,
            emits: Vec::new(),
            invokes: Vec::new(),
            throws: Vec::new(),
            bodies: Default::default(),
            body_receiver: None,
            transition: None,
            child_scope: None,
            condition_paths: Vec::new(),
            ordinal: 0,
        }
    };
    let lower = |op: &codegraph_core::types::DddServiceOperation, machine| {
        lower_exec(&model, &config, &ops, op, machine, &HashMap::new())
    };
    let delete = lower(&op(Some("PayRunRepository"), Some("delete")), None);
    assert!(matches!(delete, ExecutionKind::Delete));
    let read = lower(&op(Some("PayRunRepository"), Some("findById")), None);
    assert!(matches!(read, ExecutionKind::Read));
    // save delegates through the update command (P0)
    match lower(&op(Some("PayRunRepository"), Some("save")), None) {
        ExecutionKind::Update { state } => {
            assert!(state.is_none(), "no machine ⇒ no transition binding")
        }
        other => panic!("expected update lowering, got {other:?}"),
    }
    // non-repository delegation seams
    match lower(&op(Some("NotificationService"), Some("notify")), None) {
        ExecutionKind::Seam { reason } => assert!(reason.contains("not a repository")),
        other => panic!("expected seam, got {other:?}"),
    }
    // abstract op seams
    match lower(&op(None, None), None) {
        ExecutionKind::Seam { reason } => assert!(reason.contains("no delegation target")),
        other => panic!("expected seam, got {other:?}"),
    }
}

fn pay_run_machine() -> phero_ir::ddd::StateMachine {
    let transition = |from: &str, to: &str, trigger: &str| phero_ir::ddd::Transition {
        from: from.to_string(),
        to: to.to_string(),
        trigger: Some(trigger.to_string()),
        guard: None,
    };
    phero_ir::ddd::StateMachine {
        field: "runStatus".to_string(),
        enum_type: phero_ir::TypeRef::Enum {
            package: "payroll".to_string(),
            name: "PayRunStatus".to_string(),
        },
        initial: "Draft".to_string(),
        terminals: vec!["Reversed".to_string()],
        transitions: vec![
            transition("Draft", "Calculating", "Calculate"),
            transition("Calculating", "AwaitingApproval", "Submit"),
            transition("AwaitingApproval", "Approved", "Approve"),
            transition("Approved", "Posted", "Post"),
            transition("Posted", "Reversed", "Reverse"),
        ],
    }
}

fn pay_run_fields() -> HashMap<String, FieldInfo> {
    let mut fields = HashMap::new();
    fields.insert(
        "runStatus".to_string(),
        FieldInfo {
            rust_name: "run_status".to_string(),
            kind: FieldKind::Codelist,
            optional: false,
        },
    );
    fields.insert(
        "run_status".to_string(),
        FieldInfo {
            rust_name: "run_status".to_string(),
            kind: FieldKind::Codelist,
            optional: false,
        },
    );
    fields
}

fn service_op(
    name: &str,
    capabilities: &[&str],
    is_protected: bool,
    condition: Option<&str>,
) -> codegraph_core::types::DddServiceOperation {
    codegraph_core::types::DddServiceOperation {
        name: name.to_string(),
        return_type: None,
        return_multiplicity: None,
        params: Vec::new(),
        delegation_target: Some("PayRunRepository".to_string()),
        delegation_operation: Some("save".to_string()),
        capabilities: capabilities.iter().map(|c| c.to_string()).collect(),
        is_protected,
        condition: condition.map(str::to_string),
        emits: Vec::new(),
        invokes: Vec::new(),
        throws: Vec::new(),
        bodies: Default::default(),
        body_receiver: None,
        transition: None,
        child_scope: None,
        condition_paths: Vec::new(),
        ordinal: 0,
    }
}

fn pay_run_model(
    ops: Vec<codegraph_core::types::DddServiceOperation>,
) -> codegraph_core::types::DddModelGraph {
    codegraph_core::types::DddModelGraph {
        source_path: "test.ddd".to_string(),
        application: codegraph_core::types::DddApplicationNode {
            name: "App".to_string(),
            base: None,
            source_path: "test.ddd".to_string(),
            errors: Vec::new(),
        },
        modules: Vec::new(),
        designs: vec![codegraph_core::types::DddDesignNode {
            application: "App".to_string(),
            module: "payroll".to_string(),
            class: "PayRun".to_string(),
            resolved_title: Some("PayRunType".to_string()),
            stereotype: "entity".to_string(),
            is_abstract: false,
            flags: codegraph_core::types::DddDesignFlags::default(),
            machine_json: None,
            child_operations: Vec::new(),
            aggregate_boundary: None,
            invariants: Vec::new(),
            ordinal: 0,
        }],
        repositories: vec![codegraph_core::types::DddRepositoryNode {
            application: "App".to_string(),
            name: "PayRunRepository".to_string(),
            design_class: "PayRun".to_string(),
            operations: Vec::new(),
        }],
        services: vec![codegraph_core::types::DddServiceNode {
            application: "App".to_string(),
            module: "payroll".to_string(),
            name: "PayRunService".to_string(),
            description: None,
            dependencies: Vec::new(),
            decision_dependencies: Vec::new(),
            operations: ops,
            ordinal: 0,
        }],
        searches: Vec::new(),
        dto_shapes: Vec::new(),
    }
}

#[test]
fn save_delegation_binds_the_op_trigger_transition() {
    let machine = pay_run_machine();
    let model = pay_run_model(vec![service_op(
        "submitPayRun",
        &["SubmitPayRun"],
        false,
        None,
    )]);
    let config = empty_config();
    let ops = ["delete", "read", "update"].map(str::to_string);
    let lowering = lower_exec(
        &model,
        &config,
        &ops,
        &model.services[0].operations[0],
        Some(&machine),
        &pay_run_fields(),
    );
    let state = match lowering {
        ExecutionKind::Update { state: Some(state) } => state,
        other => panic!("expected machine-bound update, got {other:?}"),
    };
    assert_eq!(state.wire_key.as_str(), "run_status");
    assert_eq!(state.current_expr, "response.run_status.to_string()");
    assert_eq!(state.initial.as_str(), "Draft");
    assert_eq!(
        state.op,
        vec![ServiceOpTransition {
            from: StateName::new("Calculating"),
            to: StateName::new("AwaitingApproval"),
        }]
    );
}

#[test]
fn prepare_pay_run_binds_the_protected_internal_trigger() {
    // `preparePayRun` has no matching trigger; the protected `calculate`
    // shares its capability and matches `Calculate` (the crewbase shape).
    let machine = pay_run_machine();
    let model = pay_run_model(vec![
        service_op("preparePayRun", &["PreparePayRun"], false, None),
        service_op("calculate", &["PreparePayRun"], true, None),
    ]);
    let config = empty_config();
    let ops = ["delete", "read", "update"].map(str::to_string);
    let lowering = lower_exec(
        &model,
        &config,
        &ops,
        &model.services[0].operations[0],
        Some(&machine),
        &pay_run_fields(),
    );
    let state = match lowering {
        ExecutionKind::Update { state: Some(state) } => state,
        other => panic!("expected machine-bound update, got {other:?}"),
    };
    assert_eq!(
        state.op,
        vec![ServiceOpTransition {
            from: StateName::new("Draft"),
            to: StateName::new("Calculating"),
        }]
    );
}

#[test]
fn machine_states_lower_in_when_conditions() {
    let fields = pay_run_fields();
    let states = vec![
        "Draft".to_string(),
        "AwaitingApproval".to_string(),
        "Approved".to_string(),
    ];
    assert_eq!(
        lower_when_to_rust_with_states(
            "runStatus == AwaitingApproval",
            "ApprovePayRun",
            "PayRunType",
            &fields,
            &states,
        )
        .unwrap(),
        r#"(response.run_status.to_string() == "AwaitingApproval")"#
    );
    let err = lower_when_to_rust_with_states(
        "runStatus == Shelved",
        "ApprovePayRun",
        "PayRunType",
        &fields,
        &states,
    )
    .unwrap_err();
    assert!(err.contains("nor a declared state"), "{err}");
    assert!(err.contains("Draft"), "{err}");
}

#[test]
fn workflow_managed_state_field_keeps_the_update_but_seams_transitions() {
    let machine = pay_run_machine();
    let config = empty_config();
    let mut config = config;
    let entry = config.domains.get_mut("payroll").unwrap();
    entry.entity_config.insert(
        "PayRunType".to_string(),
        toml::from_str(
            r#"
workflow = { status_field = "run_status", initial_state = "Draft" }
"#,
        )
        .unwrap(),
    );
    let ops = ["read", "update"].map(str::to_string);
    // A non-transition delegation still updates the other body fields.
    let model = pay_run_model(vec![service_op("update", &[], false, None)]);
    let lowering = lower_exec(
        &model,
        &config,
        &ops,
        &model.services[0].operations[0],
        Some(&machine),
        &pay_run_fields(),
    );
    match lowering {
        ExecutionKind::Update { state } => assert!(
            state.is_none(),
            "workflow-managed state is not DTO-patchable"
        ),
        other => panic!("expected a plain update, got {other:?}"),
    }

    // A state transition cannot ride the update DTO — fail closed.
    let model = pay_run_model(vec![service_op("submitPayRun", &[], false, None)]);
    let lowering = lower_exec(
        &model,
        &config,
        &ops,
        &model.services[0].operations[0],
        Some(&machine),
        &pay_run_fields(),
    );
    match lowering {
        ExecutionKind::Seam { reason } => {
            assert!(reason.contains("workflow-managed"), "{reason}")
        }
        other => panic!("expected a fail-closed seam, got {other:?}"),
    }
}

fn lower_with_machine(
    op_name: &str,
    capabilities: &[&str],
    machine: &phero_ir::ddd::StateMachine,
    protected: Option<(&str, &[&str])>,
) -> ExecutionKind {
    let mut ops = vec![service_op(op_name, capabilities, false, None)];
    if let Some((name, caps)) = protected {
        ops.push(service_op(name, caps, true, None));
    }
    let model = pay_run_model(ops);
    let ops_surface = ["delete", "read", "update"].map(str::to_string);
    lower_exec(
        &model,
        &empty_config(),
        &ops_surface,
        &model.services[0].operations[0],
        Some(machine),
        &pay_run_fields(),
    )
}

#[test]
fn self_transition_is_a_declared_noop_edge() {
    let mut machine = pay_run_machine();
    machine.transitions.push(phero_ir::ddd::Transition {
        from: "Draft".to_string(),
        to: "Draft".to_string(),
        trigger: Some("Refresh".to_string()),
        guard: None,
    });
    let lowering = lower_with_machine("refresh", &[], &machine, None);
    let state = match lowering {
        ExecutionKind::Update { state: Some(state) } => state,
        other => panic!("expected a machine-bound update, got {other:?}"),
    };
    assert_eq!(
        state.op,
        vec![ServiceOpTransition {
            from: StateName::new("Draft"),
            to: StateName::new("Draft"),
        }],
        "a self transition is legal and does not advance the state"
    );
}

#[test]
fn body_driven_op_whitelists_every_transition_but_selects_none() {
    // `changeStatus` matches no trigger: the op has no transition of its
    // own, so the handler validates a body-supplied target against `all`
    // and an empty body is an idempotent read.
    let machine = pay_run_machine();
    let lowering = lower_with_machine("changeStatus", &[], &machine, None);
    let state = match lowering {
        ExecutionKind::Update { state: Some(state) } => state,
        other => panic!("expected a body-driven update, got {other:?}"),
    };
    assert!(state.op.is_empty(), "no transition is selected by name");
    assert_eq!(state.all.len(), machine.transitions.len());
    assert_eq!(state.initial.as_str(), "Draft");
}

// ── Issue #660: the authored binding is the primary selector ──

/// A machine with a trigger the op name does NOT resemble plus one the
/// name DOES resemble — so a binding that wins can only have won by
/// being authored.
fn pay_run_machine_with_abandon() -> phero_ir::ddd::StateMachine {
    let mut machine = pay_run_machine();
    machine.transitions.push(phero_ir::ddd::Transition {
        from: "Draft".to_string(),
        to: "Abandoned".to_string(),
        trigger: Some("Abandon".to_string()),
        guard: None,
    });
    machine.terminals.push("Abandoned".to_string());
    machine
}

/// The same machine carrying a bare arrow (`-> Posted`, no `via`): the
/// binding must accept its derived lowerCamel trigger (`posted`).
fn pay_run_machine_with_bare_arrow() -> phero_ir::ddd::StateMachine {
    let mut machine = pay_run_machine();
    machine.transitions.push(phero_ir::ddd::Transition {
        from: "Approved".to_string(),
        to: "Archived".to_string(),
        trigger: None,
        guard: None,
    });
    machine
}

/// A second machine-bearing design in the SAME module (the qualifier
/// gap's ambiguity set).
fn second_machine_design() -> codegraph_core::types::DddDesignNode {
    let mut machine = pay_run_machine();
    machine.field = "checkStatus".to_string();
    machine.enum_type = phero_ir::TypeRef::Enum {
        package: "payroll".to_string(),
        name: "PayRunCheckStatus".to_string(),
    };
    codegraph_core::types::DddDesignNode {
        application: "App".to_string(),
        module: "payroll".to_string(),
        class: "PayRunCheck".to_string(),
        resolved_title: Some("PayRunCheckType".to_string()),
        stereotype: "entity".to_string(),
        is_abstract: false,
        flags: codegraph_core::types::DddDesignFlags::default(),
        machine_json: Some(serde_json::to_value(machine).unwrap()),
        child_operations: Vec::new(),
        aggregate_boundary: None,
        invariants: Vec::new(),
        ordinal: 1,
    }
}

fn bound_op(
    name: &str,
    machine: Option<&str>,
    trigger: &str,
) -> codegraph_core::types::DddServiceOperation {
    let mut op = service_op(name, &[], false, None);
    op.transition = Some(codegraph_core::types::DddTransitionRef {
        trigger: trigger.to_string(),
        machine: machine.map(str::to_string),
    });
    op
}

fn machine_bound_update(lowering: ExecutionKind) -> ServiceOpState {
    match lowering {
        ExecutionKind::Update { state: Some(state) } => state,
        other => panic!("expected a machine-bound update, got {other:?}"),
    }
}

#[test]
fn authored_binding_wins_over_a_name_resembling_trigger() {
    // `abandonPayRun` matches the `Abandon` trigger by name, but the
    // authored binding selects `Submit` — the binding is the whole
    // selector, so the heuristic never runs (issue #660).
    let machine = pay_run_machine_with_abandon();
    let model = pay_run_model(vec![bound_op("abandonPayRun", None, "Submit")]);
    let state = machine_bound_update(lower_exec(
        &model,
        &empty_config(),
        &["delete", "read", "update"].map(str::to_string),
        &model.services[0].operations[0],
        Some(&machine),
        &pay_run_fields(),
    ));
    assert_eq!(
        state.op,
        vec![ServiceOpTransition {
            from: StateName::new("Calculating"),
            to: StateName::new("AwaitingApproval"),
        }],
        "the authored trigger must win over the name heuristic"
    );
}

#[test]
fn authored_binding_accepts_a_bare_arrows_derived_trigger() {
    // A bare arrow's trigger is the lowerCamel of the target state
    // upstream (`Archived` → `archived`); the binding resolves it.
    let machine = pay_run_machine_with_bare_arrow();
    let model = pay_run_model(vec![bound_op("storeRun", None, "archived")]);
    let state = machine_bound_update(lower_exec(
        &model,
        &empty_config(),
        &["delete", "read", "update"].map(str::to_string),
        &model.services[0].operations[0],
        Some(&machine),
        &pay_run_fields(),
    ));
    assert_eq!(
        state.op,
        vec![ServiceOpTransition {
            from: StateName::new("Approved"),
            to: StateName::new("Archived"),
        }]
    );
}

#[test]
fn a_single_machine_module_takes_the_unqualified_binding() {
    // One machine-bearing design ⇒ the qualifier is optional and the
    // unqualified spelling resolves against the bound machine.
    let machine = pay_run_machine();
    let model = pay_run_model(vec![bound_op("prepareRun", None, "Calculate")]);
    let state = machine_bound_update(lower_exec(
        &model,
        &empty_config(),
        &["delete", "read", "update"].map(str::to_string),
        &model.services[0].operations[0],
        Some(&machine),
        &pay_run_fields(),
    ));
    assert_eq!(
        state.op,
        vec![ServiceOpTransition {
            from: StateName::new("Draft"),
            to: StateName::new("Calculating"),
        }]
    );
}

#[test]
fn a_qualified_binding_resolves_in_a_multi_machine_module() {
    let machine = pay_run_machine();
    let mut model = pay_run_model(vec![bound_op(
        "prepareRun",
        Some("PayRunStatus"),
        "Calculate",
    )]);
    model.designs[0].machine_json = Some(serde_json::to_value(&machine).unwrap());
    model.designs.push(second_machine_design());
    let state = machine_bound_update(lower_exec(
        &model,
        &empty_config(),
        &["delete", "read", "update"].map(str::to_string),
        &model.services[0].operations[0],
        Some(&machine),
        &pay_run_fields(),
    ));
    assert_eq!(
        state.op,
        vec![ServiceOpTransition {
            from: StateName::new("Draft"),
            to: StateName::new("Calculating"),
        }]
    );
}

#[test]
fn an_unqualified_binding_in_a_multi_machine_module_names_both_machines() {
    let machine = pay_run_machine();
    let mut model = pay_run_model(vec![bound_op("prepareRun", None, "Calculate")]);
    model.designs[0].machine_json = Some(serde_json::to_value(&machine).unwrap());
    model.designs.push(second_machine_design());
    let err = try_lower_exec(
        &model,
        &empty_config(),
        &["delete", "read", "update"].map(str::to_string),
        &model.services[0].operations[0],
        Some(&machine),
        &pay_run_fields(),
    )
    .expect_err("an unqualified binding in a multi-machine module must fail");
    assert!(err.contains("prepareRun"), "{err}");
    assert!(err.contains("PayRun"), "{err}");
    assert!(
        err.contains("PayRunCheck"),
        "the error must name both machines: {err}"
    );
    assert!(err.contains("PayRunStatus"), "{err}");
    assert!(err.contains("PayRunCheckStatus"), "{err}");
}

#[test]
fn a_qualifier_naming_another_machine_is_refused() {
    let machine = pay_run_machine();
    let mut model = pay_run_model(vec![bound_op(
        "prepareRun",
        Some("PayRunCheckStatus"),
        "Calculate",
    )]);
    model.designs[0].machine_json = Some(serde_json::to_value(&machine).unwrap());
    model.designs.push(second_machine_design());
    let err = try_lower_exec(
        &model,
        &empty_config(),
        &["delete", "read", "update"].map(str::to_string),
        &model.services[0].operations[0],
        Some(&machine),
        &pay_run_fields(),
    )
    .expect_err("a qualifier naming another machine must fail");
    assert!(err.contains("prepareRun"), "{err}");
    assert!(err.contains("PayRunCheckStatus"), "{err}");
    assert!(err.contains("PayRunStatus"), "{err}");
}

#[test]
fn an_unknown_trigger_names_the_declared_vocabulary() {
    let machine = pay_run_machine();
    let model = pay_run_model(vec![bound_op("prepareRun", None, "SignOff")]);
    let err = try_lower_exec(
        &model,
        &empty_config(),
        &["delete", "read", "update"].map(str::to_string),
        &model.services[0].operations[0],
        Some(&machine),
        &pay_run_fields(),
    )
    .expect_err("an unknown trigger must fail");
    assert!(err.contains("SignOff"), "{err}");
    assert!(
        err.contains("Calculate"),
        "the error must list the vocabulary: {err}"
    );
    assert!(err.contains("Submit"), "{err}");
}

#[test]
fn a_binding_on_an_entity_without_a_machine_is_refused() {
    // Never degrade an authored binding to a plain update.
    let model = pay_run_model(vec![bound_op("prepareRun", None, "Calculate")]);
    let err = try_lower_exec(
        &model,
        &empty_config(),
        &["delete", "read", "update"].map(str::to_string),
        &model.services[0].operations[0],
        None,
        &pay_run_fields(),
    )
    .expect_err("a binding with no machine must fail");
    assert!(err.contains("prepareRun"), "{err}");
    assert!(err.contains("no lifecycle machine"), "{err}");
}

#[test]
fn a_binding_on_a_cross_aggregate_delegation_is_refused() {
    // The binding's machine belongs to the delegation target's class;
    // a route bound to another aggregate must not seam silently.
    let machine = pay_run_machine();
    let mut model = pay_run_model(vec![bound_op("addPayRunItem", None, "Submit")]);
    model.repositories[0].design_class = "PayRunItem".to_string();
    let err = try_lower_exec(
        &model,
        &empty_config(),
        &["delete", "read", "update"].map(str::to_string),
        &model.services[0].operations[0],
        Some(&machine),
        &pay_run_fields(),
    )
    .expect_err("a binding across aggregates must fail");
    assert!(err.contains("addPayRunItem"), "{err}");
    assert!(err.contains("another aggregate"), "{err}");
    assert!(err.contains("PayRunItem"), "{err}");
}

#[test]
fn capability_shared_op_without_a_trigger_stays_a_plain_update() {
    // Capability sharing is an explicit last-resort heuristic: the
    // protected op carries the trigger only when its NAME matches one.
    let machine = pay_run_machine();
    let lowering = lower_with_machine(
        "preparePayRun",
        &["PreparePayRun"],
        &machine,
        Some(("recalculate", &["PreparePayRun"])),
    );
    let state = match lowering {
        ExecutionKind::Update { state: Some(state) } => state,
        other => panic!("expected a plain body-driven update, got {other:?}"),
    };
    assert!(
        state.op.is_empty(),
        "no trigger match anywhere ⇒ no transition is invented"
    );
}

#[test]
fn cross_entity_child_delegation_fails_closed() {
    let machine = pay_run_machine();
    let mut model = pay_run_model(vec![service_op("addPayRunItem", &[], false, None)]);
    // The op routes through a repository for a DIFFERENT design class —
    // the child/aggregate shape. Patching the parent row would silently
    // drop the child intent.
    model.repositories[0].design_class = "PayRunItem".to_string();
    let ops_surface = ["delete", "read", "update"].map(str::to_string);
    let lowering = lower_exec(
        &model,
        &empty_config(),
        &ops_surface,
        &model.services[0].operations[0],
        Some(&machine),
        &pay_run_fields(),
    );
    match lowering {
        ExecutionKind::Seam { reason } => {
            // The improved message names the construct that WOULD lower
            // it (issue #661): a `children <Root>` block whose ops carry
            // `childScope`.
            assert!(reason.contains("children PayRun"), "{reason}");
            assert!(reason.contains("childScope"), "{reason}");
            assert!(reason.contains("PayRunItem"), "{reason}");
            assert!(
                reason.contains("Refusing to patch the parent row"),
                "{reason}"
            );
        }
        other => panic!("expected a fail-closed child seam, got {other:?}"),
    }
}

#[test]
fn no_record_load_route_fails_closed() {
    let machine = pay_run_machine();
    let model = pay_run_model(vec![service_op("submitPayRun", &[], false, None)]);
    let ops_surface = ["delete", "update"].map(str::to_string); // no read
    let shape_raises = crate::ddd::body_lower::RaiseTable::default();
    let shape = RouteShape {
        domain: "payroll",
        entity_title: "PayRunType",
        entity_name: "PayRunType",
        entity_table: "pay_run",
        id_param: "pay_run_id",
        scoped: None,
        auditable: false,
        domain_types_crate: "domain_types",
        dto_casing: crate::project_config::DtoKeyCasing::Snake,
        fields: &pay_run_fields(),
        surface_ops: &ops_surface,
        record_load: false,
        machine: Some(&machine),
        raises: &shape_raises,
        params: &[],
        throws: &[],
    };
    let lowering = execution_for(
        &model.services[0].operations[0],
        &model,
        &empty_config(),
        "PayRunService",
        &shape,
    )
    .expect("lowering");
    match lowering {
        ExecutionKind::Seam { reason } => {
            assert!(reason.contains("no record load"), "{reason}")
        }
        other => panic!("expected a no-record-load seam, got {other:?}"),
    }
}

// ── The pherolang TypeRef JSON shape (bug fix): the class-typed-signature
//    binding path must read the ADJACENT-tagged shape phero-ir serializes
//    (`{"type":"class","value":{"package":…,"name":…}}`), not a flat
//    top-level `name` that can never match. ───────────────────────────

fn op_with_type_json(return_json: serde_json::Value) -> codegraph_core::types::DddServiceOperation {
    codegraph_core::types::DddServiceOperation {
        name: "op".to_string(),
        return_type: Some(return_json),
        return_multiplicity: None,
        params: Vec::new(),
        delegation_target: None,
        delegation_operation: None,
        capabilities: Vec::new(),
        is_protected: false,
        condition: None,
        emits: Vec::new(),
        invokes: Vec::new(),
        throws: Vec::new(),
        bodies: Default::default(),
        body_receiver: None,
        transition: None,
        child_scope: None,
        condition_paths: Vec::new(),
        ordinal: 0,
    }
}

fn class_type_json(package: &str, name: &str) -> serde_json::Value {
    // The phero-ir TypeRef wire shape (adjacently tagged, camelCase tags).
    serde_json::json!({
        "type": "class",
        "value": { "package": package, "name": name }
    })
}

#[tokio::test]
async fn class_typed_signature_binds_the_real_phero_json_shape() {
    let engine = codegraph_core::mock::MockEngine::builder()
        .with_schema(codegraph_core::types::SchemaNode {
            namespace: None,
            schema_id: "payroll/json/PayRunType.json".to_string(),
            title: "PayRunType".to_string(),
            description: None,
            schema_type: "object".to_string(),
            classification: "entity_reference".to_string(),
            domain: Some("payroll".to_string()),
            rel_path: "payroll/json/PayRunType.json".to_string(),
            pg_type: "UUID".to_string(),
            rust_type: "PayRun".to_string(),
            sea_orm_type: "Uuid".to_string(),
            rust_type_name: "PayRun".to_string(),
            pg_table_name: "pay_run".to_string(),
            api_path_segment: "pay-run".to_string(),
            parent_schema: None,
            is_entity: true,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
            custom_annotations: Default::default(),
            access: None,
            annotations: None,
        })
        .build();

    // The REAL pherolang shape: `{"type":"class","value":{"package":…,"name":…}}`.
    let bound = bind_entity(
        &pay_run_model(Vec::new()),
        &op_with_type_json(class_type_json("payroll", "PayRun")),
        &engine,
        "Type",
    )
    .await;
    assert_eq!(
        bound.as_deref(),
        Some("PayRunType"),
        "the class-typed signature path must bind through the nested pherolang shape"
    );

    // The qualified name form keeps the `pkg::Name` resolution chain.
    let bound = bind_entity(
        &pay_run_model(Vec::new()),
        &op_with_type_json(class_type_json("other.pkg", "PayRun")),
        &engine,
        "Type",
    )
    .await;
    assert_eq!(bound.as_deref(), Some("PayRunType"));

    // Non-class shapes bind nothing.
    let primitive = bind_entity(
        &pay_run_model(Vec::new()),
        &op_with_type_json(serde_json::json!({ "type": "primitive", "value": "string" })),
        &engine,
        "Type",
    )
    .await;
    assert_eq!(primitive, None);

    // Unknown classes bind nothing (no fabricated titles).
    let unknown = bind_entity(
        &pay_run_model(Vec::new()),
        &op_with_type_json(class_type_json("payroll", "NoSuchClass")),
        &engine,
        "Type",
    )
    .await;
    assert_eq!(unknown, None);

    // Param-typed signatures ride the same path.
    let mut param_op = op_with_type_json(serde_json::Value::Null);
    param_op.return_type = None;
    param_op.params = vec![codegraph_core::types::DddParam {
        name: "run".to_string(),
        type_json: class_type_json("payroll", "PayRun"),
        multiplicity: None,
    }];
    let bound = bind_entity(&pay_run_model(Vec::new()), &param_op, &engine, "Type").await;
    assert_eq!(
        bound.as_deref(),
        Some("PayRunType"),
        "param-typed signature"
    );
}

// ── Injected-scope guard facts (issue #663) ────────────────────────

fn injected_scope(fk_optional: bool) -> InjectedScope {
    let mut fields = HashMap::new();
    fields.insert(
        "timecardStatus".to_string(),
        FieldInfo {
            rust_name: "timecard_status".to_string(),
            kind: FieldKind::Codelist,
            optional: false,
        },
    );
    fields.insert(
        "hours".to_string(),
        FieldInfo {
            rust_name: "hours".to_string(),
            kind: FieldKind::Integer,
            optional: false,
        },
    );
    InjectedScope {
        root: "TimecardRepository".to_string(),
        local: "timecard_repository".to_string(),
        slot: "guardpay_timecard".to_string(),
        auditable: false,
        fk_field: "timecard_id".to_string(),
        fk_optional,
        fields,
        machine: Some(InjectedMachine {
            field: "timecardStatus".to_string(),
            states: vec![
                "Pending".to_string(),
                "Approved".to_string(),
                "Rejected".to_string(),
            ],
            initial: "Pending".to_string(),
        }),
    }
}

fn guard_op(condition: &str) -> codegraph_core::types::DddServiceOperation {
    codegraph_core::types::DddServiceOperation {
        name: "submit".to_string(),
        return_type: None,
        return_multiplicity: None,
        params: Vec::new(),
        delegation_target: None,
        delegation_operation: None,
        capabilities: Vec::new(),
        is_protected: false,
        condition: Some(condition.to_string()),
        emits: Vec::new(),
        invokes: Vec::new(),
        throws: Vec::new(),
        bodies: Default::default(),
        body_receiver: None,
        transition: None,
        child_scope: None,
        condition_paths: Vec::new(),
        ordinal: 0,
    }
}

#[test]
fn fresh_fixture_verdicts_are_provable_never_guessed() {
    let scopes = vec![injected_scope(false)];
    // eq with the machine's initial state: the fresh row PASSES → the
    // contract keeps the normal legs.
    let facts = injected_guard_facts(
        &guard_op("payRunStatus == Draft && TimecardRepository.timecardStatus == Pending"),
        &scopes,
    )
    .expect("facts");
    assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Satisfied);
    // ne with a NON-initial state: `Pending != Approved` holds on the
    // fresh row → Satisfied.
    let facts = injected_guard_facts(
        &guard_op("payRunStatus == Draft && TimecardRepository.timecardStatus != Approved"),
        &scopes,
    )
    .expect("facts");
    assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Satisfied);
    // eq with a non-initial state: the fresh row (initial `Pending`)
    // FAILS the comparison → provable denial, pin 403.
    let facts = injected_guard_facts(
        &guard_op("payRunStatus == Draft && TimecardRepository.timecardStatus == Approved"),
        &scopes,
    )
    .expect("facts");
    assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Denied);
    // An OPTIONAL FK denies BEFORE any comparison is evaluated (the
    // create body omits nullable props → the fresh FK is NULL): 403
    // regardless of the comparison's content — including an
    // eq-initial comparison that would otherwise read `Satisfied`
    // (the exact reviewer case: planner 200 vs handler 403).
    let optional = vec![injected_scope(true)];
    let facts = injected_guard_facts(
        &guard_op("payRunStatus == Draft && TimecardRepository.timecardStatus == Pending"),
        &optional,
    )
    .expect("facts");
    assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Denied);
}

#[test]
fn undecided_comparisons_omit_the_leg_never_guess() {
    let scopes = vec![injected_scope(false)];
    // A NON-machine injected field (`hours > 8`): the fresh value
    // (fixture-seeded 42) is derivable but the planner does not model
    // it — Unverifiable, and the contract OMITS the leg (a guessed
    // 403 would come back 200; a guessed 200 would come back 403 —
    // a wrong pin fails the suite in either direction).
    let facts = injected_guard_facts(
        &guard_op("payRunStatus == Draft && TimecardRepository.hours > 8"),
        &scopes,
    )
    .expect("facts");
    assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Unverifiable);
    // Negation flips polarity: an un-negated collect would decide the
    // comparison upside down — Unverifiable.
    let facts = injected_guard_facts(
        &guard_op("!(TimecardRepository.timecardStatus == Approved)"),
        &scopes,
    )
    .expect("facts");
    assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Unverifiable);
    // `||` composition: the guard may pass via the own-record side —
    // the collected set no longer decides the whole condition.
    let facts = injected_guard_facts(
        &guard_op("payRunStatus == Posted || TimecardRepository.timecardStatus == Approved"),
        &scopes,
    )
    .expect("facts");
    assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Unverifiable);
    // No injected scopes at all → no guard facts.
    assert!(injected_guard_facts(&guard_op("payRunStatus == Draft"), &[]).is_none());
}

#[test]
fn mixed_machine_and_undecidable_conjuncts_never_misverdict() {
    // Round-2 review counterexamples: a machine comparison MIXED with
    // an undecidable one must not let the decided side guess the
    // verdict for the whole condition.
    let scopes = vec![injected_scope(false)];
    // Non-machine Eq/Ne conjunct: the fresh `hours` is fixture-seeded
    // (42) — `!= 8` PASSES on the fresh row, so a Denied fold would
    // pin 403 while the handler 200s (the reviewer's exact case).
    let facts = injected_guard_facts(
        &guard_op("TimecardRepository.timecardStatus == Pending && TimecardRepository.hours != 8"),
        &scopes,
    )
    .expect("facts");
    assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Unverifiable);
    // An ORDERING conjunct dropped beside a passing machine comparison:
    // fresh `hours` = 42 fails `> 100`, so a Satisfied fold would pin
    // 200 while the handler 403s.
    let facts = injected_guard_facts(
        &guard_op("TimecardRepository.timecardStatus == Pending && TimecardRepository.hours > 100"),
        &scopes,
    )
    .expect("facts");
    assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Unverifiable);
    // A failing machine conjunct beside an undecided sibling is still
    // PROVABLE: under plain AND the failing conjunct decides the whole
    // guard — Denied stands.
    let facts = injected_guard_facts(
        &guard_op("TimecardRepository.timecardStatus == Approved && TimecardRepository.hours > 8"),
        &scopes,
    )
    .expect("facts");
    assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Denied);
}

#[test]
fn cross_scope_field_comparisons_refuse() {
    let mut fields = HashMap::new();
    fields.insert(
        "status".to_string(),
        FieldInfo {
            rust_name: "status".to_string(),
            kind: FieldKind::String,
            optional: false,
        },
    );
    let mut own = scopes_for_test(&fields);
    let scopes = vec![injected_scope(false)];
    let _ = &mut own;
    // An own-record field vs an injected field: no honest root for
    // both sides — named refusal (the message covers two injected
    // scopes too).
    let err = lower_when_scoped(
        "status == TimecardRepository.timecardStatus",
        "SubmitPayRun",
        "PayRunType",
        &fields,
        &[],
        &scopes,
    )
    .unwrap_err();
    assert!(err.contains("across different records"), "{err}");
}

fn scopes_for_test(
    fields: &HashMap<String, FieldInfo>,
) -> (&str, &HashMap<String, FieldInfo>, &[InjectedScope]) {
    ("PayRunType", fields, &[])
}

// ── Declared error vocabulary resolution (DD7, issue #665) ─────────────

fn param(
    name: &str,
    type_json: serde_json::Value,
    multiplicity: Option<serde_json::Value>,
) -> codegraph_core::types::DddParam {
    codegraph_core::types::DddParam {
        name: name.to_string(),
        type_json,
        multiplicity,
    }
}

fn declared_error_with_fields(
    name: &str,
    fields: &[&str],
) -> crate::ddd::body_lower::DeclaredError {
    crate::ddd::body_lower::DeclaredError {
        variant: name.to_string(),
        code: "PAYMENT_DECLINED".to_string(),
        status: 422,
        fields: fields
            .iter()
            .map(|f| crate::ddd::body_lower::DeclaredErrorField {
                name: (*f).to_string(),
            })
            .collect(),
    }
}

#[test]
fn throws_responses_group_by_status_and_filter_covered_statuses() {
    let mut table = crate::ddd::body_lower::RaiseTable::default();
    table.declared.insert(
        "PaymentDeclined".to_string(),
        declared_error_with_fields("PaymentDeclined", &["reason", "reference"]),
    );

    // Prelude + declared group under their own statuses; 404 (covered by
    // the base set) drops out entirely.
    let responses = crate::ddd::service_ops::routes::throws_responses(
        &table,
        "declines",
        "PayRunService",
        &["NotFound", "PaymentDeclined"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>(),
        &[403, 404],
    )
    .expect("resolvable vocabulary");
    assert_eq!(
        responses.len(),
        1,
        "404 is covered; only 422 documents: {responses:?}"
    );
    assert_eq!(responses[0].status, 422);
    assert_eq!(responses[0].errors, "PaymentDeclined");

    // An uncovered prelude (409 via a machine Conflict) groups alone.
    let responses = crate::ddd::service_ops::routes::throws_responses(
        &table,
        "submit",
        "PayRunService",
        &["Conflict"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>(),
        &[403, 404],
    )
    .expect("resolvable vocabulary");
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].status, 409);
    assert_eq!(responses[0].errors, "Conflict");
}

#[test]
fn unresolvable_throws_entry_refuses_naming_the_op_and_entry() {
    let table = crate::ddd::body_lower::RaiseTable::default();
    let error = crate::ddd::service_ops::routes::throws_responses(
        &table,
        "declines",
        "PayRunService",
        &["Bogus".to_string()],
        &[],
    )
    .expect_err("an unresolvable entry refuses");
    assert!(error.contains("declines"), "{error}");
    assert!(error.contains("`Bogus`"), "{error}");
    assert!(error.contains("no declared errors are in scope"), "{error}");
}

#[test]
fn error_payload_fields_map_enums_classes_and_refuse_collections() {
    // Enum payload field → the generated codelist enum.
    let fields = crate::ddd::errors::error_field_defs(
        "PaymentDeclined",
        &[param(
            "reason",
            serde_json::json!({"type": "enum", "value": {"package": "payroll", "name": "DeclineReason"}}),
            None,
        )],
    )
    .expect("enum payload maps");
    assert_eq!(fields[0].rust_type, "crate::codelist::DeclineReason");

    // Class reference payload field → the `{Class}Id` Uuid slot (#517).
    let fields = crate::ddd::errors::error_field_defs(
        "PaymentDeclined",
        &[param("run", class_type_json("payroll", "PayRun"), None)],
    )
    .expect("class payload maps to the Uuid slot");
    assert_eq!(fields[0].rust_type, "uuid::Uuid");

    // Optional primitive field → Option.
    let fields = crate::ddd::errors::error_field_defs(
        "PaymentDeclined",
        &[param(
            "reference",
            serde_json::json!({"type": "primitive", "value": "string"}),
            Some(serde_json::json!({"lower": 0, "upper": {"finite": 1}})),
        )],
    )
    .expect("optional maps");
    assert_eq!(fields[0].rust_type, "Option<String>");
    assert!(fields[0].optional);

    // A collection payload field refuses, naming the error and field.
    let error = crate::ddd::errors::error_field_defs(
        "PaymentDeclined",
        &[param(
            "reasons",
            serde_json::json!({"type": "primitive", "value": "string"}),
            Some(serde_json::json!({"lower": 0, "upper": "unbounded"})),
        )],
    )
    .expect_err("a collection payload refuses");
    assert!(error.contains("PaymentDeclined"), "{error}");
    assert!(error.contains("reasons"), "{error}");
    assert!(error.contains("collection"), "{error}");
}

// ── Issue #767: the create-shaped predicate ──────────────────────────────

/// A minimal `DddServiceOperation` for the predicate pins.
fn create_op(
    name: &str,
    params: Vec<codegraph_core::types::DddParam>,
    return_json: serde_json::Value,
) -> codegraph_core::types::DddServiceOperation {
    codegraph_core::types::DddServiceOperation {
        name: name.to_string(),
        params,
        return_type: Some(return_json),
        return_multiplicity: None,
        is_protected: false,
        capabilities: Vec::new(),
        condition: None,
        condition_paths: Vec::new(),
        bodies: std::collections::BTreeMap::new(),
        body_receiver: None,
        throws: Vec::new(),
        emits: Vec::new(),
        transition: None,
        delegation_target: None,
        delegation_operation: None,
        child_scope: None,
        invokes: Vec::new(),
        ordinal: 0,
    }
}

fn class_json(name: &str) -> serde_json::Value {
    serde_json::json!({"type": "class", "value": {"package": "nz.example.library", "name": name}})
}

fn dto_json(name: &str) -> serde_json::Value {
    serde_json::json!({ "dto": name })
}

fn dto_param(name: &str, boundary: &str, optional: bool) -> codegraph_core::types::DddParam {
    codegraph_core::types::DddParam {
        name: name.to_string(),
        type_json: dto_json(boundary),
        multiplicity: optional.then(|| serde_json::json!({"lower": 0, "upper": {"finite": 1}})),
    }
}

fn predicate_model() -> codegraph_core::types::DddModelGraph {
    codegraph_core::types::DddModelGraph {
        source_path: "design.ddd".to_string(),
        application: codegraph_core::types::DddApplicationNode {
            name: "Library".to_string(),
            base: Some("nz.example.library".to_string()),
            source_path: "design.ddd".to_string(),
            errors: Vec::new(),
        },
        modules: Vec::new(),
        designs: vec![codegraph_core::types::DddDesignNode {
            application: "Library".to_string(),
            module: "catalog".to_string(),
            class: "Book".to_string(),
            resolved_title: Some("BookType".to_string()),
            stereotype: "entity".to_string(),
            ordinal: 0,
            is_abstract: false,
            flags: Default::default(),
            machine_json: None,
            aggregate_boundary: None,
            invariants: Vec::new(),
            child_operations: Vec::new(),
        }],
        repositories: Vec::new(),
        services: Vec::new(),
        searches: Vec::new(),
        dto_shapes: Vec::new(),
    }
}

fn create_surface() -> Vec<String> {
    vec!["create".to_string(), "read".to_string()]
}

#[test]
fn create_shaped_predicate_accepts_the_issue_shape() {
    let model = predicate_model();
    let op = create_op(
        "registerBook",
        vec![dto_param("input", "NewBookDraft", false)],
        class_json("Book"),
    );
    assert!(super::routes::create_shape_candidate(
        &op,
        &model,
        "BookType",
        &create_surface(),
        None
    ));
}

#[test]
fn create_shaped_predicate_refuses_every_non_create_shape() {
    let model = predicate_model();
    let surface = create_surface();
    let base = || {
        create_op(
            "registerBook",
            vec![dto_param("input", "NewBookDraft", false)],
            class_json("Book"),
        )
    };

    // No `create` operation on the entity.
    let no_create = create_op(
        "registerBook",
        vec![dto_param("input", "NewBookDraft", false)],
        class_json("Book"),
    );
    assert!(!super::routes::create_shape_candidate(
        &no_create,
        &model,
        "BookType",
        &["read".to_string()],
        None
    ));

    // A nested child entity's create is the child-op plane's.
    assert!(!super::routes::create_shape_candidate(
        &base(),
        &model,
        "BookType",
        &surface,
        Some("parent_id")
    ));

    // An op-level `when` guard has no record to evaluate (keeps its seam).
    let mut guarded = base();
    guarded.condition = Some("title == title".to_string());
    assert!(!super::routes::create_shape_candidate(
        &guarded, &model, "BookType", &surface, None
    ));

    // A statement body wins over the create shape.
    let mut bodied = base();
    bodied.bodies.insert("stmt".to_string(), String::new());
    assert!(!super::routes::create_shape_candidate(
        &bodied, &model, "BookType", &surface, None
    ));

    // Two parameters, a primitive parameter, a DTO return, a foreign return
    // and a non-save delegation all stay non-create.
    let mut two_params = base();
    two_params
        .params
        .push(dto_param("other", "NewBookDraft", false));
    assert!(!super::routes::create_shape_candidate(
        &two_params,
        &model,
        "BookType",
        &surface,
        None
    ));
    let primitive_param = create_op(
        "importBook",
        vec![codegraph_core::types::DddParam {
            name: "title".to_string(),
            type_json: serde_json::json!({"type": "primitive", "value": "string"}),
            multiplicity: None,
        }],
        class_json("Book"),
    );
    assert!(!super::routes::create_shape_candidate(
        &primitive_param,
        &model,
        "BookType",
        &surface,
        None
    ));
    let dto_return = create_op(
        "summarise",
        vec![dto_param("input", "NewBookDraft", false)],
        dto_json("BookSummary"),
    );
    assert!(!super::routes::create_shape_candidate(
        &dto_return,
        &model,
        "BookType",
        &surface,
        None
    ));
    let mut delete_delegation = base();
    delete_delegation.delegation_target = Some("BookRepository".to_string());
    delete_delegation.delegation_operation = Some("delete".to_string());
    assert!(!super::routes::create_shape_candidate(
        &delete_delegation,
        &model,
        "BookType",
        &surface,
        None
    ));
    // ... while the `save` delegation IS accepted (the widened arm).
    let mut save_delegation = base();
    save_delegation.delegation_target = Some("BookRepository".to_string());
    save_delegation.delegation_operation = Some("save".to_string());
    assert!(super::routes::create_shape_candidate(
        &save_delegation,
        &model,
        "BookType",
        &surface,
        None
    ));
}
