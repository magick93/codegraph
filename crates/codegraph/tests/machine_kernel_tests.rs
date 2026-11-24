//! Machine kernel gate (issue #815 — Lane 1 of epic #812): the compiled
//! `.ddd` artifact's `machine_ir` becomes a first-class serialized
//! artifact.
//!
//! Four bands, node-free (no node/npm; the scratch-crate compile leg runs
//! cargo, the #551 precedent):
//!
//! 1. **xstate KATs** — the artifact's canonical IR JSON deserializes into
//!    the `xstate-ir` `Machine` (via `xstate_core`, which re-exports it),
//!    passes the fail-closed `validate_machine_json`, and round-trips
//!    through the interpreter's persisted-snapshot surface (the Stage 2/3
//!    seam).
//! 2. **Fail-closed validation** — a structurally broken machine and a
//!    machine state outside its bound codelist enum are REFUSED by name
//!    before any tree is written (`machine_kernel_refusals`, the DD9
//!    `query_refusals` posture); a machine-less graph is silent.
//! 3. **Emitter contract** — the machine kernel generator emits
//!    `machines/{domain}/{machine_id}.ir.json` + a registry module into
//!    the domain-types crate (static data, wasm32-safe); machine-less
//!    graphs emit ZERO files (byte identity).
//! 4. **The emitted module compiles** — scratch crate, `cargo check`
//!    native, carrying the domain-types machine dep set (xstate-core,
//!    tokio-free).

use std::fs;
use std::path::{Path, PathBuf};

use codegraph_core::mock::MockEngine;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{
    CodeList, DddApplicationNode, DddDesignFlags, DddDesignNode, DddModelGraph, DddModuleNode,
    EnumValue,
};
use codegraph_generate::machines::{self, MachineKernelGenerator};
use codegraph_generate::traits::GlobalGenerator;
use serde_json::Value;

// ── fixtures ──────────────────────────────────────────────────────────

/// The canonical `xstate_ir::Machine` JSON a referenced machine embeds as
/// `machineIr` (the shape pherolang#388 serializes — snake_case field
/// names, keys sorted recursively). The same literal the graph-mirror
/// round-trips in `ddd_ingest_tests`.
fn payrun_ir_json() -> serde_json::Value {
    serde_json::json!({
        "context": {"fields": []},
        "id": "payrun",
        "initial": "Draft",
        "setup": null,
        "states": [
            {
                "children": [], "description": null, "entry": [], "exit": [],
                "initial": null, "invoke": null, "name": "Draft",
                "on_done": [], "output": null, "tags": [],
                "transitions": [{
                    "actions": [], "description": null, "event": "CALCULATE",
                    "guard": null, "target": "Calculating", "targets": []
                }],
                "type": "atomic"
            },
            {
                "children": [], "description": null, "entry": [], "exit": [],
                "initial": null, "invoke": null, "name": "Calculating",
                "on_done": [], "output": null, "tags": [],
                "transitions": [],
                "type": "final"
            }
        ]
    })
}

fn provenance_json(machine_id: &str) -> serde_json::Value {
    serde_json::json!({
        "path": "machines/payrun.machine.ts",
        "machineId": machine_id,
        "contentSha256":
            "5aca796db97ff88b925f8bf6cdd2c2c50bc9aa7828ff9dc1422e1108238c2757"
    })
}

fn machine_design(
    domain: &str,
    ir: Option<serde_json::Value>,
    prov: Option<serde_json::Value>,
) -> DddDesignNode {
    DddDesignNode {
        application: "Payroll".to_string(),
        module: domain.to_string(),
        class: "nz.example.payroll::PayRun".to_string(),
        resolved_title: Some("PayRunType".to_string()),
        stereotype: "entity".to_string(),
        is_abstract: false,
        flags: DddDesignFlags::default(),
        // The DSL mirror binding the machine to its enum — the codelist
        // correspondence check reads the enum type from here.
        machine_json: Some(serde_json::json!({
            "field": "status",
            "enumType": {
                "type": "enum",
                "value": {"package": "nz.example.payroll", "name": "PayRunStatus"}
            },
            "initial": "Draft",
            "terminals": ["Calculating"],
            "transitions": [
                {"from": "Draft", "to": "Calculating", "trigger": "CALCULATE"}
            ]
        })),
        machine_ir_json: ir,
        machine_provenance_json: prov,
        child_operations: Vec::new(),
        aggregate_boundary: None,
        invariants: Vec::new(),
        ordinal: 0,
    }
}

/// A truly machine-less design: no artifact IR AND no DSL mirror (issue
/// #818 — a mirror alone IS an inline machine, so the machine-less
/// baseline carries neither).
fn bare_design(domain: &str) -> DddDesignNode {
    DddDesignNode {
        application: "Payroll".to_string(),
        module: domain.to_string(),
        class: "nz.example.payroll::PayRun".to_string(),
        resolved_title: Some("PayRunType".to_string()),
        stereotype: "entity".to_string(),
        is_abstract: false,
        flags: DddDesignFlags::default(),
        machine_json: None,
        machine_ir_json: None,
        machine_provenance_json: None,
        child_operations: Vec::new(),
        aggregate_boundary: None,
        invariants: Vec::new(),
        ordinal: 0,
    }
}

fn model_graph(designs: Vec<DddDesignNode>) -> DddModelGraph {
    DddModelGraph {
        source_path: "model/payroll.ddd".to_string(),
        application: DddApplicationNode {
            name: "Payroll".to_string(),
            base: Some("nz.example.payroll".to_string()),
            source_path: "model/payroll.ddd".to_string(),
            errors: Vec::new(),
        },
        modules: vec![DddModuleNode {
            application: "Payroll".to_string(),
            name: "payroll".to_string(),
            ordinal: 0,
            errors: Vec::new(),
        }],
        designs,
        repositories: Vec::new(),
        services: Vec::new(),
        searches: Vec::new(),
        dto_shapes: Vec::new(),
    }
}

async fn mock_with_payrun_codelist() -> MockEngine {
    let mock = MockEngine::builder().build();
    mock.ingest_codelist(&CodeList {
        name: "PayRunStatus".to_string(),
        description: None,
        pg_table_name: "pay_run_status".to_string(),
        render_as: "codelist".to_string(),
        check_expression: None,
    })
    .await
    .unwrap();
    for (value, order) in [("Draft", 0), ("Calculating", 1)] {
        mock.ingest_enum_value(
            "PayRunStatus",
            &EnumValue {
                value: value.to_string(),
                display_name: None,
                sort_order: order,
            },
        )
        .await
        .unwrap();
    }
    mock
}

// ── band 1: xstate KATs ───────────────────────────────────────────────

#[test]
fn machine_ir_deserializes_into_the_xstate_machine_and_validates() {
    let json = serde_json::to_string(&payrun_ir_json()).unwrap();
    // The typed form parses…
    let machine: xstate_core::xstate_ir::Machine =
        serde_json::from_str(&json).expect("the artifact IR is an xstate-ir Machine");
    assert_eq!(machine.id, "payrun");
    assert_eq!(machine.initial, "Draft");
    let state_names: Vec<&str> = machine.states.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(state_names, vec!["Draft", "Calculating"]);
    // …and passes the fail-closed structural boundary.
    xstate_core::validate_machine_json(&json)
        .expect("a compiler-produced IR is structurally valid");
}

#[test]
fn persisted_snapshot_round_trips_through_a_fresh_interpreter() {
    let json = serde_json::to_string(&payrun_ir_json()).unwrap();
    let machine: xstate_core::xstate_ir::Machine = serde_json::from_str(&json).unwrap();

    let interpreter = xstate_core::Interpreter::new(machine.clone());
    let state = interpreter.initial_state();
    assert!(
        matches!(&state, xstate_core::StateValue::Atomic(name) if name == "Draft"),
        "the initial state is the declared one: {state:?}"
    );
    let context = serde_json::json!({"total": 42});
    let snapshot = interpreter.persisted_snapshot(&state, &context);

    // A fresh interpreter over the SAME machine restores the persisted
    // state — the Stage 2/3 runtime seam over the embedded artifact.
    let fresh = xstate_core::Interpreter::new(machine);
    let (restored_state, restored_context) = fresh.restore_persisted(&snapshot).unwrap();
    assert_eq!(
        serde_json::to_value(&restored_state).unwrap(),
        serde_json::to_value(&state).unwrap()
    );
    assert_eq!(restored_context, context);

    // A state the machine does not declare is REFUSED — the persisted
    // surface is fail-closed, never silently accepted.
    let bogus = serde_json::json!({"value": "NotAState", "context": null});
    assert!(
        fresh.restore_persisted(&bogus).is_err(),
        "an undeclared state value must not restore"
    );
}

#[test]
fn validate_machine_json_refuses_a_broken_machine() {
    // The transition targets a state the machine does not declare — the
    // restingstate structural check must refuse it, naming both states.
    let broken = serde_json::json!({
        "context": {"fields": []},
        "id": "payrun",
        "initial": "Draft",
        "setup": null,
        "states": [
            {
                "children": [], "description": null, "entry": [], "exit": [],
                "initial": null, "invoke": null, "name": "Draft",
                "on_done": [], "output": null, "tags": [],
                "transitions": [{
                    "actions": [], "description": null, "event": "CALCULATE",
                    "guard": null, "target": "Nowhere", "targets": []
                }],
                "type": "atomic"
            }
        ]
    });
    let err = xstate_core::validate_machine_json(&serde_json::to_string(&broken).unwrap())
        .expect_err("an unknown transition target must refuse");
    let text = err.to_string();
    assert!(
        text.contains("Nowhere"),
        "the refusal names the target: {text}"
    );
    assert!(
        text.contains("Draft"),
        "the refusal names the source state: {text}"
    );
}

// ── band 2: fail-closed validation ────────────────────────────────────

#[tokio::test]
async fn machine_less_graph_is_silent() {
    let mock = MockEngine::builder().build();
    mock.ingest_ddd_model(&model_graph(vec![bare_design("payroll")]))
        .await
        .unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let refusals = machines::machine_kernel_refusals(&mock, &config)
        .await
        .unwrap();
    assert!(
        refusals.is_empty(),
        "a design with neither a referenced machine nor an inline mirror validates to nothing: \
         {refusals:?}"
    );
}

#[tokio::test]
async fn a_valid_machine_refuses_nothing() {
    let mock = mock_with_payrun_codelist().await;
    mock.ingest_ddd_model(&model_graph(vec![machine_design(
        "payroll",
        Some(payrun_ir_json()),
        Some(provenance_json("payrun")),
    )]))
    .await
    .unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let refusals = machines::machine_kernel_refusals(&mock, &config)
        .await
        .unwrap();
    assert!(
        refusals.is_empty(),
        "a compiler-produced machine over a bound codelist is valid: {refusals:?}"
    );
}

#[tokio::test]
async fn a_structurally_broken_machine_is_refused_by_name() {
    let broken = serde_json::json!({
        "context": {"fields": []},
        "id": "payrun",
        "initial": "Draft",
        "setup": null,
        "states": [
            {
                "children": [], "description": null, "entry": [], "exit": [],
                "initial": null, "invoke": null, "name": "Draft",
                "on_done": [], "output": null, "tags": [],
                "transitions": [{
                    "actions": [], "description": null, "event": "CALCULATE",
                    "guard": null, "target": "Nowhere", "targets": []
                }],
                "type": "atomic"
            }
        ]
    });
    let mock = mock_with_payrun_codelist().await;
    mock.ingest_ddd_model(&model_graph(vec![machine_design(
        "payroll",
        Some(broken),
        Some(provenance_json("payrun")),
    )]))
    .await
    .unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let refusals = machines::machine_kernel_refusals(&mock, &config)
        .await
        .unwrap();
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    let refusal = &refusals[0];
    assert!(
        refusal.contains("payrun"),
        "the refusal names the machine: {refusal}"
    );
    assert!(
        refusal.contains("Nowhere"),
        "the refusal carries the xstate diagnostic: {refusal}"
    );
}

#[tokio::test]
async fn a_machine_ir_that_is_not_machine_json_is_refused_by_name() {
    let mock = mock_with_payrun_codelist().await;
    mock.ingest_ddd_model(&model_graph(vec![machine_design(
        "payroll",
        Some(serde_json::json!({"not": "a machine"})),
        Some(provenance_json("payrun")),
    )]))
    .await
    .unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let refusals = machines::machine_kernel_refusals(&mock, &config)
        .await
        .unwrap();
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert!(
        refusals[0].contains("payrun"),
        "the refusal names the machine even for a non-machine payload: {:?}",
        refusals[0]
    );
}

#[tokio::test]
async fn a_machine_state_outside_the_bound_codelist_is_refused() {
    // `Calculating` is NOT a value of the PayRunStatus codelist — the
    // enum correspondence is OUR check (the artifact validates
    // machine-internal structure only).
    let mut ir = payrun_ir_json();
    // Rename `Calculating` -> `Voided` INCLUDING the inbound transition
    // target, so the machine stays structurally valid and only the enum
    // correspondence (our check) can refuse it.
    ir["states"][1]["name"] = serde_json::json!("Voided");
    ir["states"][0]["transitions"][0]["target"] = serde_json::json!("Voided");
    let mock = mock_with_payrun_codelist().await;
    mock.ingest_ddd_model(&model_graph(vec![machine_design(
        "payroll",
        Some(ir),
        Some(provenance_json("payrun")),
    )]))
    .await
    .unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let refusals = machines::machine_kernel_refusals(&mock, &config)
        .await
        .unwrap();
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    let refusal = &refusals[0];
    assert!(refusal.contains("Voided"), "names the state: {refusal}");
    assert!(
        refusal.contains("PayRunStatus"),
        "names the bound enum: {refusal}"
    );
    assert!(refusal.contains("payrun"), "names the machine: {refusal}");
}

#[tokio::test]
async fn a_machine_without_an_inline_mirror_skips_the_codelist_check() {
    // The codelist correspondence reads the bound enum from the DSL
    // mirror (`machine_json`). A design with only the IR (a hand-built /
    // stale graph) validates structure but has no enum to check against —
    // the correspondence arm is skipped, never guessed.
    let mut design = machine_design("payroll", Some(payrun_ir_json()), None);
    design.machine_json = None;
    let mock = MockEngine::builder().build();
    mock.ingest_ddd_model(&model_graph(vec![design]))
        .await
        .unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let refusals = machines::machine_kernel_refusals(&mock, &config)
        .await
        .unwrap();
    assert!(refusals.is_empty(), "{refusals:?}");
}

#[tokio::test]
async fn duplicate_machine_ids_are_refused() {
    let mock = mock_with_payrun_codelist().await;
    let first = machine_design(
        "payroll",
        Some(payrun_ir_json()),
        Some(provenance_json("payrun")),
    );
    let mut second = machine_design(
        "payroll",
        Some(payrun_ir_json()),
        Some(provenance_json("payrun")),
    );
    second.ordinal = 1;
    mock.ingest_ddd_model(&model_graph(vec![first, second]))
        .await
        .unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let refusals = machines::machine_kernel_refusals(&mock, &config)
        .await
        .unwrap();
    assert!(
        refusals
            .iter()
            .any(|r| r.contains("payrun") && r.to_lowercase().contains("duplicate")),
        "a duplicate machine id is refused by name: {refusals:?}"
    );
}

/// The mox bridge's `PayRunType` schema node (the entity title the
/// design's class resolves against through the +Type suffix).
fn payrun_schema() -> codegraph_core::types::SchemaNode {
    codegraph_core::types::SchemaNode {
        namespace: None,
        schema_id: "payroll/json/PayRunType.json".to_string(),
        title: "PayRunType".to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("payroll".to_string()),
        rel_path: "payroll/json/PayRunType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "PayRun".to_string(),
        pg_table_name: "pay_run".to_string(),
        api_path_segment: "pay_run".to_string(),
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
    }
}

// ── band 2.5: the end-to-end artifact flow (the TS-authored front door) ─

/// The fixture at `tests/fixtures/machine_kernel/` authors the machine in
/// TypeScript/XState and declares it through the production front door
/// (`machine status: PayRunStatus from "machines/payrun.machine.ts";`).
/// This test is the artifact → graph acceptance: the compiled embed
/// carries `machine_ir` + provenance onto the design node, resolved
/// against the mox-bridged `PayRunType` title. The codelist rides the
/// mox bridge (a separate pass), so the test pre-ingests it the way the
/// driver's Pass 1 mox would.
#[tokio::test]
async fn a_ts_authored_machine_ingests_with_ir_and_provenance() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/machine_kernel");
    let design = fixture.join("design.ddd");
    let domain_config =
        codegraph_config::config::parse_domain_config(&fixture.join("domains.toml")).unwrap();

    // The mox bridge's product: the PayRunType schema title + the
    // PayRunStatus codelist with its two literals.
    let mock = MockEngine::builder().with_schema(payrun_schema()).build();
    mock.ingest_codelist(&CodeList {
        name: "PayRunStatus".to_string(),
        description: None,
        pg_table_name: "pay_run_status".to_string(),
        render_as: "codelist".to_string(),
        check_expression: None,
    })
    .await
    .unwrap();
    for (value, order) in [("Draft", 0), ("Calculated", 1)] {
        mock.ingest_enum_value(
            "PayRunStatus",
            &EnumValue {
                value: value.to_string(),
                display_name: None,
                sort_order: order,
            },
        )
        .await
        .unwrap();
    }

    let stats =
        codegraph::ingest::ddd_ingest::ingest_ddd_files(&mock, &mock, &[design], &domain_config)
            .await
            .unwrap();
    assert_eq!(stats.files, 1);
    assert_eq!(stats.designs, 1);

    let models = mock.get_ddd_models().await.unwrap();
    assert_eq!(models.len(), 1);
    let design_node = &models[0].designs[0];
    assert_eq!(
        design_node.resolved_title.as_deref(),
        Some("PayRunType"),
        "the design class resolves through the +Type suffix"
    );

    // The compiled embed rides: the canonical xstate-ir Machine + the
    // provenance triple — and the embed passes the fail-closed boundary.
    let ir = design_node
        .machine_ir_json
        .as_ref()
        .expect("machine IR lands");
    assert_eq!(ir.get("id").and_then(Value::as_str), Some("payrun"));
    assert_eq!(
        ir.get("initial").and_then(Value::as_str),
        Some("Draft"),
        "the IR is the compiled xstate form, not the DSL mirror"
    );
    let prov = design_node
        .machine_provenance_json
        .as_ref()
        .expect("provenance lands");
    assert_eq!(
        prov.get("path").and_then(Value::as_str),
        Some("machines/payrun.machine.ts")
    );
    assert_eq!(
        prov.get("machineId").and_then(Value::as_str),
        Some("payrun")
    );
    let sha = prov
        .get("contentSha256")
        .and_then(Value::as_str)
        .expect("the content hash lands");
    assert_eq!(sha.len(), 64, "a lowercase-hex SHA-256: {sha}");
    let canonical = machines::canonical_ir_json(ir);
    xstate_core::validate_machine_json(&canonical)
        .expect("the compiled embed is structurally valid");

    // And the kernel validates it against the mox-bridged codelist: the
    // TS states are PayRunStatus literals.
    let config = domain_config;
    let refusals = machines::machine_kernel_refusals(&mock, &config)
        .await
        .unwrap();
    assert!(
        refusals.is_empty(),
        "the fixture machine validates clean: {refusals:?}"
    );
}

// ── band 3: the emitter contract ──────────────────────────────────────

async fn emit_into(dir: &Path) -> Vec<PathBuf> {
    let mock = mock_with_payrun_codelist().await;
    mock.ingest_ddd_model(&model_graph(vec![machine_design(
        "payroll",
        Some(payrun_ir_json()),
        Some(provenance_json("payrun")),
    )]))
    .await
    .unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let generator = MachineKernelGenerator::new_with_base(dir.to_path_buf());
    generator
        .generate(
            &mock,
            &config,
            &[],
            &tera::Tera::default(),
            &Default::default(),
        )
        .await
        .expect("the machine kernel generator must emit")
        .into_iter()
        .map(|f| {
            // The pipeline's write step: the generated files land on disk
            // (the module's include_str! paths resolve against them).
            if let Some(parent) = f.path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&f.path, &f.content).unwrap();
            f.path
        })
        .collect()
}

#[tokio::test]
async fn the_emitter_writes_data_files_and_a_registry_module() {
    let dir = tempfile::tempdir().unwrap();
    let paths = emit_into(dir.path()).await;
    let rels: Vec<String> = paths
        .iter()
        .map(|p| p.strip_prefix(dir.path()).unwrap().display().to_string())
        .collect();

    // The issue's layout, inside the domain-types crate src.
    assert!(
        rels.iter()
            .any(|r| r == "src/machines/payroll/payrun.ir.json"),
        "the IR data file rides machines/{{domain}}/{{machine_id}}.ir.json: {rels:?}"
    );
    assert!(
        rels.iter().any(|r| r == "src/machines/mod.rs"),
        "the registry module is machines/mod.rs: {rels:?}"
    );

    // The data file is CANONICAL JSON (sorted keys recursively) — the
    // Stage 2 drift checks re-canonicalize and must get a fixed point.
    let data = fs::read_to_string(dir.path().join("src/machines/payroll/payrun.ir.json")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&data).unwrap();
    assert_eq!(
        data.trim(),
        machines::canonical_ir_json(&value),
        "the embedded bytes are the canonical serialization"
    );

    // The registry module: include_str! over the sibling data file,
    // sorted ids, provenance consts.
    let module = fs::read_to_string(dir.path().join("src/machines/mod.rs")).unwrap();
    assert!(
        module.contains(r#"include_str!("payroll/payrun.ir.json")"#),
        "the IR embeds through include_str!: {module}"
    );
    assert!(
        module.contains(r#""payrun""#),
        "the registry keys by machine id: {module}"
    );
    assert!(
        module.contains("PAYRUN_SOURCE_PATH") && module.contains("machines/payrun.machine.ts"),
        "the authored path rides as a provenance const: {module}"
    );
    assert!(
        module.contains("PAYRUN_CONTENT_SHA256")
            && module.contains("5aca796db97ff88b925f8bf6cdd2c2c50bc9aa7828ff9dc1422e1108238c2757"),
        "the content hash rides as a provenance const: {module}"
    );
    assert!(
        !module.contains("sea_orm"),
        "the module is static data — no persistence handle: {module}"
    );
}

// ── band 5: inline machines — the generation-time lowering (issue #818) ──

/// An INLINE machine (a DSL mirror, no artifact IR) lowers into a valid
/// kernel IR and passes the same fail-closed doors as a referenced one.
/// This is the door the route plane's kernel probes load through: inline
/// machines execute through the MachineRuntime exactly like referenced
/// machines.
#[tokio::test]
async fn an_inline_machine_lowers_into_a_valid_kernel_ir() {
    let mock = mock_with_payrun_codelist().await;
    mock.ingest_ddd_model(&model_graph(vec![machine_design("payroll", None, None)]))
        .await
        .unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let refusals = machines::machine_kernel_refusals(&mock, &config)
        .await
        .unwrap();
    assert!(
        refusals.is_empty(),
        "an inline machine over a bound codelist is valid: {refusals:?}"
    );

    // The lowering itself: pure, deterministic, structurally valid.
    let design = machine_design("payroll", None, None);
    let ir = machines::machine_ir_for_design(&design)
        .expect("a mirror lowers")
        .expect("a mirror is a machine");
    let machine: xstate_core::xstate_ir::Machine = serde_json::from_value(ir.clone()).unwrap();
    let canonical = machines::canonical_ir_json(&ir);
    xstate_core::validate_machine_json(&canonical)
        .expect("the lowered IR passes the fail-closed boundary");
    // The derived registry id and the shape: states, terminals, events.
    assert_eq!(machine.id, "payroll_status");
    assert_eq!(machine.initial, "Draft");
    let names: Vec<&str> = machine.states.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["Draft", "Calculating"]);
    let calculating = machine
        .states
        .iter()
        .find(|s| s.name == "Calculating")
        .unwrap();
    assert_eq!(
        calculating.node_type,
        xstate_core::xstate_ir::StateType::Final
    );
    let draft = machine.states.iter().find(|s| s.name == "Draft").unwrap();
    let edge = &draft.transitions[0];
    assert_eq!(edge.event, "CALCULATE", "the authored `via` rides verbatim");
    assert_eq!(edge.target.as_deref(), Some("Calculating"));
    assert!(
        edge.guard.is_none(),
        "inline `when` conditions are not lowered (recorded deferral)"
    );
}

/// The lowering is DETERMINISTIC: two lowerings of the same mirror
/// produce byte-identical canonical IR — the embed cannot drift run to
/// run.
#[test]
fn the_inline_machine_lowering_is_deterministic() {
    let design = machine_design("payroll", None, None);
    let a = machines::machine_ir_for_design(&design).unwrap().unwrap();
    let b = machines::machine_ir_for_design(&design).unwrap().unwrap();
    assert_eq!(
        machines::canonical_ir_json(&a),
        machines::canonical_ir_json(&b)
    );
}

/// A bare-arrow transition's event is the DERIVED lowerCamel of its
/// target — the same vocabulary the #660 binding plane resolves against,
/// so the kernel's events and the binding plane's triggers cannot drift.
#[test]
fn inline_bare_arrows_derive_the_binding_plane_vocabulary() {
    let mirror = serde_json::json!({
        "field": "status",
        "enumType": {"type": "enum", "value": {"package": "nz.example.payroll", "name": "PayRunStatus"}},
        "initial": "Draft",
        "terminals": ["Approved"],
        "transitions": [
            {"from": "Draft", "to": "Calculating"},
            {"from": "Calculating", "to": "Approved", "trigger": "Approve"}
        ]
    });
    let design = DddDesignNode {
        machine_json: Some(mirror),
        ..machine_design("payroll", None, None)
    };
    let ir = machines::machine_ir_for_design(&design).unwrap().unwrap();
    let machine: xstate_core::xstate_ir::Machine = serde_json::from_value(ir).unwrap();
    let draft = machine.states.iter().find(|s| s.name == "Draft").unwrap();
    assert_eq!(
        draft.transitions[0].event, "calculating",
        "a bare arrow derives the target's lowerCamel — `declared_triggers`' spelling"
    );
    let calculating = machine
        .states
        .iter()
        .find(|s| s.name == "Calculating")
        .unwrap();
    assert_eq!(calculating.transitions[0].event, "Approve");
}

/// The emitter covers inline machines: the lowered IR lands as
/// `machines/{domain}/{id}.ir.json` + a registry entry keyed by the
/// derived id, with CANONICAL bytes.
#[tokio::test]
async fn an_inline_machine_emits_the_registry_entry() {
    let dir = tempfile::tempdir().unwrap();
    let mock = mock_with_payrun_codelist().await;
    mock.ingest_ddd_model(&model_graph(vec![machine_design("payroll", None, None)]))
        .await
        .unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let generator = MachineKernelGenerator::new_with_base(dir.path().to_path_buf());
    let files = generator
        .generate(
            &mock,
            &config,
            &[],
            &tera::Tera::default(),
            &Default::default(),
        )
        .await
        .expect("the machine kernel generator must emit for an inline machine");
    let rels: Vec<String> = files
        .iter()
        .map(|f| {
            f.path
                .strip_prefix(dir.path())
                .unwrap()
                .display()
                .to_string()
        })
        .collect();
    assert!(
        rels.iter()
            .any(|r| r == "src/machines/payroll/payroll_status.ir.json"),
        "the lowered IR rides machines/{{domain}}/{{id}}.ir.json: {rels:?}"
    );
    let data_path = dir
        .path()
        .join("src/machines/payroll/payroll_status.ir.json");
    let _ = files;
    // The write step landed the files (the module include_str!s resolve
    // against them) — the emitter test above asserts the same contract
    // for referenced machines.
    let written = std::fs::read_to_string(&data_path).unwrap_or_else(|_| {
        // `generate` returns the files without writing; write here so
        // the module's include_str!s resolve, then read the data back.
        for f in &files {
            if let Some(parent) = f.path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&f.path, &f.content).unwrap();
        }
        fs::read_to_string(&data_path).unwrap()
    });
    let value: serde_json::Value = serde_json::from_str(&written).unwrap();
    assert_eq!(
        written.trim(),
        machines::canonical_ir_json(&value),
        "the lowered embed is the canonical serialization"
    );
    let module = fs::read_to_string(dir.path().join("src/machines/mod.rs")).unwrap();
    assert!(
        module.contains(r#"include_str!("payroll/payroll_status.ir.json")"#),
        "the inline machine embeds through include_str!: {module}"
    );
    assert!(
        module.contains(r#""payroll_status""#),
        "the registry keys by the derived machine id: {module}"
    );
    // An inline machine carries no authored provenance: the consts stay
    // empty (never a guessed path or hash).
    assert!(
        module.contains("PAYROLL_STATUS_SOURCE_PATH: &str = \"\""),
        "inline provenance stays empty: {module}"
    );
}

/// An inline machine whose states miss the bound codelist is refused —
/// the enum door applies to the LOWERED IR (issue #818).
#[tokio::test]
async fn an_inline_machine_state_outside_the_codelist_is_refused() {
    let mock = mock_with_payrun_codelist().await;
    let mirror = serde_json::json!({
        "field": "status",
        "enumType": {
            "type": "enum",
            "value": {"package": "nz.example.payroll", "name": "PayRunStatus"}
        },
        "initial": "Draft",
        "terminals": ["Shipped"],
        "transitions": [{"from": "Draft", "to": "Shipped", "trigger": "SHIP"}]
    });
    let design = DddDesignNode {
        machine_json: Some(mirror),
        ..machine_design("payroll", None, None)
    };
    mock.ingest_ddd_model(&model_graph(vec![design]))
        .await
        .unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let refusals = machines::machine_kernel_refusals(&mock, &config)
        .await
        .unwrap();
    let refusal = refusals
        .iter()
        .find(|r| r.contains("Shipped") && r.contains("PayRunStatus"))
        .unwrap_or_else(|| panic!("expected the enum-correspondence refusal, got: {refusals:?}"));
    assert!(refusal.contains("payroll_status"), "{refusal}");
}

/// A mirror that does not deserialize as a StateMachine refuses the run
/// by name — fail-closed, never a silent no-machine.
#[tokio::test]
async fn a_stale_inline_mirror_is_refused_by_name() {
    let mock = MockEngine::builder().build();
    let design = DddDesignNode {
        machine_json: Some(serde_json::json!({ "field": 42 })),
        ..machine_design("payroll", None, None)
    };
    mock.ingest_ddd_model(&model_graph(vec![design]))
        .await
        .unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let refusals = machines::machine_kernel_refusals(&mock, &config)
        .await
        .unwrap();
    assert!(
        refusals
            .iter()
            .any(|r| r.contains("does not deserialize as a StateMachine")),
        "a stale mirror refuses by name: {refusals:?}"
    );
}

/// The derived inline id is deterministic, snake_cased, and a safe file
/// stem — the emitter and the route plane share one derivation.
#[test]
fn inline_machine_ids_are_deterministic_and_file_safe() {
    assert_eq!(
        machines::inline_machine_id("payroll", "payRunStatus"),
        "payroll_pay_run_status"
    );
    assert_eq!(
        machines::inline_machine_id("payroll", "payRunStatus"),
        machines::inline_machine_id("payroll", "payRunStatus"),
        "the derivation is pure"
    );
    // The route plane resolves the same id through the kernel-id helper.
    let design = machine_design("payroll", None, None);
    assert_eq!(
        machines::machine_kernel_id(&design).as_deref(),
        Some("payroll_status")
    );
    // A referenced machine keeps its authored id.
    let referenced = machine_design(
        "payroll",
        Some(payrun_ir_json()),
        Some(provenance_json("payrun")),
    );
    assert_eq!(
        machines::machine_kernel_id(&referenced).as_deref(),
        Some("payrun")
    );
}

#[tokio::test]
async fn a_machine_less_graph_emits_zero_files() {
    let dir = tempfile::tempdir().unwrap();
    let mock = MockEngine::builder().build();
    mock.ingest_ddd_model(&model_graph(vec![bare_design("payroll")]))
        .await
        .unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let generator = MachineKernelGenerator::new_with_base(dir.path().to_path_buf());
    let files = generator
        .generate(
            &mock,
            &config,
            &[],
            &tera::Tera::default(),
            &Default::default(),
        )
        .await
        .unwrap();
    assert!(
        files.is_empty(),
        "presence gating: no machine IR ⇒ no files (byte identity)"
    );
    assert!(
        !dir.path().join("src").exists(),
        "nothing is written at all"
    );
}

#[tokio::test]
async fn an_invalid_machine_fails_the_emitter_before_any_tree() {
    // The emitter ALSO refuses (belt + braces beside the pre-generation
    // validation pass): a broken machine never reaches the tree.
    let broken = serde_json::json!({
        "context": {"fields": []},
        "id": "payrun",
        "initial": "Draft",
        "setup": null,
        "states": [
            {
                "children": [], "description": null, "entry": [], "exit": [],
                "initial": null, "invoke": null, "name": "Draft",
                "on_done": [], "output": null, "tags": [],
                "transitions": [{
                    "actions": [], "description": null, "event": "CALCULATE",
                    "guard": null, "target": "Nowhere", "targets": []
                }],
                "type": "atomic"
            }
        ]
    });
    let dir = tempfile::tempdir().unwrap();
    let mock = mock_with_payrun_codelist().await;
    mock.ingest_ddd_model(&model_graph(vec![machine_design(
        "payroll",
        Some(broken),
        Some(provenance_json("payrun")),
    )]))
    .await
    .unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let generator = MachineKernelGenerator::new_with_base(dir.path().to_path_buf());
    let err = generator
        .generate(
            &mock,
            &config,
            &[],
            &tera::Tera::default(),
            &Default::default(),
        )
        .await
        .expect_err("a broken machine must fail the emitter");
    let text = err.to_string();
    assert!(text.contains("payrun"), "names the machine: {text}");
    assert!(!dir.path().join("src").exists(), "no tree was written");
}

// ── band 4: the emitted module compiles (the #551 pin) ────────────────
/// Scratch crate over the emitted machines module: xstate-core (the
/// generated domain-types dep set — tokio-free, `default-features =
/// false`) + serde_json. Native only here; the wasm32 story rides the
/// worker compile gate (`worker_wasm_compile_tests`) — the domain-types
/// crate is sea_orm-free and xstate-core's default features are off
/// exactly so the wasm target links.
#[tokio::test]
async fn the_emitted_machines_module_compiles_in_a_scratch_crate() {
    let dir = tempfile::tempdir().unwrap();
    emit_into(dir.path()).await;
    let module = dir.path().join("src/machines/mod.rs");
    assert!(module.exists(), "the emitter must have run");

    let scratch = tempfile::tempdir().unwrap();
    let crate_root = scratch.path();
    fs::create_dir_all(crate_root.join("src/machines/payroll")).unwrap();
    fs::copy(&module, crate_root.join("src/machines/mod.rs")).unwrap();
    fs::copy(
        dir.path().join("src/machines/payroll/payrun.ir.json"),
        crate_root.join("src/machines/payroll/payrun.ir.json"),
    )
    .unwrap();
    fs::write(crate_root.join("src/lib.rs"), "pub mod machines;\n").unwrap();

    // The xstate-core pin, exactly as the generated domain-types Cargo.toml
    // spells it (the restingstate rev; default features off = tokio-free).
    fs::write(
        crate_root.join("Cargo.toml"),
        concat!(
            "[package]\nname = \"machines_scratch\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n",
            "[dependencies]\nserde_json = \"1\"\n",
            "xstate-core = { git = \"https://github.com/yestechgroup/restingstate.git\", ",
            "rev = \"da0c804d3c30a0bcc938562e39a4b7d7505c663b\", default-features = false }\n"
        ),
    )
    .unwrap();

    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("check")
        .arg("--manifest-path")
        .arg(crate_root.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", scratch.path().join("target"))
        .env("CARGO_TERM_COLOR", "never");
    let out = cmd.output().expect("cargo check must run");
    assert!(
        out.status.success(),
        "the emitted machines module must compile:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

// ── band 5: the WorkflowConfig → IR lowering (issue #816, Decision 2) ──

/// A WorkflowConfig fixture: a four-state leave machine with a data guard,
/// a dual-status guard, and an explicit transition map.
fn leave_workflow_config() -> codegraph_config::WorkflowConfig {
    let toml = r#"
status_field = "status"
initial_state = "Draft"
states = ["Draft", "Submitted", "Approved", "Rejected"]
terminal_states = ["Approved", "Rejected"]

[transitions]
Draft = ["Submitted", "Rejected"]
Submitted = ["Approved", "Rejected"]

[[data_guards]]
transition_to = "Submitted"
rule = "days > 0"
message = "leave days must be positive"

[dual_status_guards]
Submitted = "Reviewed"
"#;
    toml::from_str(toml).expect("the workflow fixture parses")
}

#[test]
fn the_workflow_config_lowers_into_a_structurally_valid_machine() {
    let config = leave_workflow_config();
    let machine = machines::workflow_config_to_machine(&config, "payroll.leave_request");

    assert_eq!(machine.id, "payroll.leave_request");
    assert_eq!(machine.initial, "Draft");
    let names: Vec<&str> = machine.states.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Approved", "Draft", "Rejected", "Submitted"],
        "states are the config's declared union, sorted"
    );
    // Terminal states lower as final nodes.
    for name in ["Approved", "Rejected"] {
        let state = machine.states.iter().find(|s| s.name == name).unwrap();
        assert!(
            matches!(state.node_type, xstate_core::xstate_ir::StateType::Final),
            "{name} is terminal"
        );
    }
    // The lowered machine passes the fail-closed structural boundary.
    let json = serde_json::to_value(&machine).unwrap();
    xstate_core::validate_machine_json(&machines::canonical_ir_json(&json))
        .expect("a lowered config machine is structurally valid");

    // Draft → Submitted carries the composed engine guard (data + dual).
    let draft = machine.states.iter().find(|s| s.name == "Draft").unwrap();
    let submitted = draft
        .transitions
        .iter()
        .find(|t| t.target.as_deref() == Some("Submitted"))
        .expect("Draft → Submitted exists");
    assert_eq!(
        submitted.guard.as_ref().map(|g| g.name.as_str()),
        Some("guards:Submitted"),
        "a target with both guard kinds composes one name"
    );

    // Draft → Rejected is dual-free and data-free: unguarded.
    let rejected = draft
        .transitions
        .iter()
        .find(|t| t.target.as_deref() == Some("Rejected"))
        .unwrap();
    assert!(rejected.guard.is_none());
}

#[test]
fn the_lowering_is_byte_deterministic() {
    let config = leave_workflow_config();
    let a = machines::workflow_config_to_machine(&config, "payroll.leave_request");
    let b = machines::workflow_config_to_machine(&config, "payroll.leave_request");
    let ja = serde_json::to_value(&a).unwrap();
    let jb = serde_json::to_value(&b).unwrap();
    assert_eq!(
        machines::canonical_ir_json(&ja),
        machines::canonical_ir_json(&jb),
        "the same config lowers to the same canonical machine JSON"
    );
}

#[test]
fn an_empty_transition_map_lowers_the_documented_default() {
    // "When empty, any non-terminal state can transition to any other
    // state" — the lowered machine carries the complete edges.
    let toml = r#"
status_field = "status"
initial_state = "New"
states = ["New", "Open", "Done"]
terminal_states = ["Done"]
"#;
    let config: codegraph_config::WorkflowConfig = toml::from_str(toml).unwrap();
    let machine = machines::workflow_config_to_machine(&config, "t.thing");
    let new = machine.states.iter().find(|s| s.name == "New").unwrap();
    let mut targets: Vec<&str> = new
        .transitions
        .iter()
        .filter_map(|t| t.target.as_deref())
        .collect();
    targets.sort_unstable();
    assert_eq!(
        targets,
        vec!["Done", "Open"],
        "New reaches every other state"
    );
    // Done is final: no outbound edges.
    let done = machine.states.iter().find(|s| s.name == "Done").unwrap();
    assert!(done.transitions.is_empty());
}

// ── band 6: the two-representation cross-check ─────────────────────────

fn leave_artifact_ir() -> serde_json::Value {
    serde_json::json!({
        "context": {"fields": []},
        "id": "leave",
        "initial": "Draft",
        "setup": null,
        "states": [
            {
                "children": [], "description": null, "entry": [], "exit": [],
                "initial": null, "invoke": null, "name": "Draft",
                "on_done": [], "output": null, "tags": [],
                "transitions": [{
                    "actions": [], "description": null, "event": "SUBMIT",
                    "guard": null, "target": "Submitted", "targets": []
                }, {
                    "actions": [], "description": null, "event": "REJECT",
                    "guard": null, "target": "Rejected", "targets": []
                }],
                "type": "atomic"
            },
            {
                "children": [], "description": null, "entry": [], "exit": [],
                "initial": null, "invoke": null, "name": "Submitted",
                "on_done": [], "output": null, "tags": [],
                "transitions": [{
                    "actions": [], "description": null, "event": "APPROVE",
                    "guard": null, "target": "Approved", "targets": []
                }, {
                    "actions": [], "description": null, "event": "REJECT",
                    "guard": null, "target": "Rejected", "targets": []
                }],
                "type": "atomic"
            },
            {
                "children": [], "description": null, "entry": [], "exit": [],
                "initial": null, "invoke": null, "name": "Approved",
                "on_done": [], "output": null, "tags": [],
                "transitions": [],
                "type": "final"
            },
            {
                "children": [], "description": null, "entry": [], "exit": [],
                "initial": null, "invoke": null, "name": "Rejected",
                "on_done": [], "output": null, "tags": [],
                "transitions": [],
                "type": "final"
            }
        ]
    })
}

#[test]
fn the_cross_check_refuses_disagreement_naming_both_sources() {
    let config = leave_workflow_config();
    // The artifact machine lacks Draft → Rejected (the config declares it):
    // a two-representation drift.
    let mut artifact = leave_artifact_ir();
    artifact["states"][0]["transitions"] = serde_json::json!([{
        "actions": [], "description": null, "event": "SUBMIT",
        "guard": null, "target": "Submitted", "targets": []
    }]);
    let lowered = machines::workflow_config_to_machine(&config, "payroll.leave_request");
    let err = machines::compare_machine_surfaces(
        &artifact,
        &serde_json::to_value(&lowered).unwrap(),
        "machines/leave.machine.ts",
        "domains.toml [domains.payroll.entities.LeaveRequest.workflow]",
    )
    .expect_err("a state-surface disagreement must refuse");
    let text = err.to_string();
    assert!(
        text.contains("machines/leave.machine.ts"),
        "the artifact source is named: {text}"
    );
    assert!(
        text.contains("domains.toml"),
        "the config source is named: {text}"
    );
    assert!(
        text.contains("Rejected"),
        "the disagreeing state is named: {text}"
    );
}

#[test]
fn the_cross_check_accepts_agreement() {
    let config = leave_workflow_config();
    let artifact = leave_artifact_ir();
    let lowered = machines::workflow_config_to_machine(&config, "payroll.leave_request");
    machines::compare_machine_surfaces(
        &artifact,
        &serde_json::to_value(&lowered).unwrap(),
        "machines/leave.machine.ts",
        "domains.toml [domains.payroll]",
    )
    .expect("identical state + transition surfaces agree");
}

// ── band 7: the machine seed migration (0007) ──────────────────────────

async fn seed_into(dir: &Path) -> Vec<PathBuf> {
    let mock = MockEngine::builder().build();
    mock.ingest_ddd_model(&model_graph(Vec::new()))
        .await
        .unwrap();
    let mut config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    // Attach a workflow to the fixture domain's entity config. EntityConfig
    // is Deserialize-all-optional, so an empty table + the workflow parses.
    let mut entity_cfg: codegraph_config::EntityConfig =
        toml::from_str("").expect("an empty entity config parses");
    let mut wf = leave_workflow_config();
    wf.generate_action_endpoints = false;
    entity_cfg.workflow = Some(wf);
    if let Some(entry) = config.domains.get_mut("common") {
        entry.entity_config.insert("Thing".to_string(), entity_cfg);
    }
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[]).unwrap();
    let generator = codegraph_generate::db::workflow_seed::WorkflowSeedGenerator::new(dir);
    generator
        .generate(&mock, &config, &[], &tera, &Default::default())
        .await
        .expect("the workflow seed generator emits")
        .into_iter()
        .map(|f| {
            if let Some(parent) = f.path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&f.path, &f.content).unwrap();
            f.path
        })
        .collect()
}

#[tokio::test]
async fn the_seed_emits_the_machine_envelope_migration_and_reconciliation() {
    let dir = tempfile::tempdir().unwrap();
    let paths = seed_into(dir.path()).await;
    let names: Vec<String> = paths
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();

    assert!(
        names.iter().any(|n| n == "0007_workflow_machine_seed.sql"),
        "the machine seed rides 0007 (0006 stays byte-identical for existing bands): {names:?}"
    );

    let seed =
        fs::read_to_string(dir.path().join("migrations/0007_workflow_machine_seed.sql")).unwrap();
    // The envelope carries the machine + its FNV hash.
    assert!(
        seed.contains("\"machine\""),
        "the envelope embeds the machine: {seed}"
    );
    assert!(
        seed.contains("\"machine_hash\""),
        "the envelope records the drift hash: {seed}"
    );
    assert!(
        seed.contains("guards:Submitted"),
        "the lowered machine's composed guard rides the transition: {seed}"
    );
    // The reconciliation step: entity column pulled from the instance.
    assert!(
        seed.contains("IS DISTINCT FROM"),
        "the reconcile UPDATE is idempotent: {seed}"
    );
    assert!(
        !seed.contains("pending_approval:") || seed.to_lowercase().contains("not like"),
        "parked states never reconcile into the column"
    );
}

// ── band 8: the guard seam (the ManualImpls precedent) ─────────────────

/// A machine whose transition names a guard — the artifact-machine shape
/// that needs an app-side implementation.
fn guarded_payrun_ir() -> serde_json::Value {
    let mut ir = payrun_ir_json();
    ir["states"][0]["transitions"][0]["guard"] = serde_json::json!({
        "name": "amount_is_positive",
        "params": {"floor": 1}
    });
    ir
}

async fn emit_guarded_into(dir: &Path) {
    let mock = mock_with_payrun_codelist().await;
    mock.ingest_ddd_model(&model_graph(vec![machine_design(
        "payroll",
        Some(guarded_payrun_ir()),
        Some(provenance_json("payrun")),
    )]))
    .await
    .unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let generator = MachineKernelGenerator::new_with_base(dir.to_path_buf());
    let files = generator
        .generate(
            &mock,
            &config,
            &[],
            &tera::Tera::default(),
            &Default::default(),
        )
        .await
        .expect("the machine kernel generator must emit");
    for f in files {
        if let Some(parent) = f.path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&f.path, &f.content).unwrap();
    }
}

#[tokio::test]
async fn the_guard_seam_trait_and_write_once_impl_are_emitted() {
    let dir = tempfile::tempdir().unwrap();
    emit_guarded_into(dir.path()).await;

    let module = fs::read_to_string(dir.path().join("src/machines/mod.rs")).unwrap();
    // The trait: one method per named guard, context/event/params signature.
    assert!(
        module.contains("pub trait MachineGuardImpls"),
        "the seam trait is emitted: {module}"
    );
    assert!(
        module.contains("fn guard_amount_is_positive"),
        "one method per named guard: {module}"
    );
    assert!(
        module.contains("params: &serde_json::Value"),
        "guard params ride the signature: {module}"
    );
    // The regenerated dispatch routes the guard name to the trait method
    // and fails closed on anything else.
    assert!(
        module.contains(r#""amount_is_positive" =>"#),
        "the dispatch routes the name: {module}"
    );

    // The write-once hand file: stubs return the NotImplemented error.
    let guards_path = dir.path().join("src/machines/guards.rs");
    let guards = fs::read_to_string(&guards_path).unwrap();
    assert!(
        guards.contains("pub struct DefaultMachineGuards"),
        "the write-once impl struct: {guards}"
    );
    assert!(
        guards.contains("GuardImplError::NotImplemented"),
        "stubs fail closed (the 501), never silently allow: {guards}"
    );

    // A second run keeps the hand file (write-once).
    let before = fs::read_to_string(&guards_path).unwrap();
    fs::write(&guards_path, format!("{before}\n// hand edit survives\n")).unwrap();
    emit_guarded_into(dir.path()).await;
    let after = fs::read_to_string(&guards_path).unwrap();
    assert!(
        after.contains("// hand edit survives"),
        "hand edits survive regeneration (write-once)"
    );
}

#[tokio::test]
async fn a_machine_without_guards_emits_no_seam() {
    // Byte identity: a guard-less machine keeps Lane 1's module shape —
    // no trait, no write-once file, no codegraph_workflow dependency.
    let dir = tempfile::tempdir().unwrap();
    emit_into(dir.path()).await;
    let module = fs::read_to_string(dir.path().join("src/machines/mod.rs")).unwrap();
    assert!(
        !module.contains("MachineGuardImpls"),
        "no guards ⇒ no seam trait"
    );
    assert!(
        !module.contains("codegraph_workflow"),
        "no guards ⇒ no workflow dependency in the machines module"
    );
    assert!(
        !dir.path().join("src/machines/guards.rs").exists(),
        "no guards ⇒ no write-once file"
    );
}

#[tokio::test]
async fn losing_the_last_guard_sweeps_the_stale_seam() {
    // First run with a guard, then regenerate WITHOUT it: guards.rs is
    // swept and the declaration disappears (a stale hand file would not
    // compile against the vanished trait).
    let dir = tempfile::tempdir().unwrap();
    emit_guarded_into(dir.path()).await;
    assert!(dir.path().join("src/machines/guards.rs").exists());

    emit_into(dir.path()).await;
    assert!(
        !dir.path().join("src/machines/guards.rs").exists(),
        "the emitter sweeps the stale write-once file when no machine carries guards"
    );
    let module = fs::read_to_string(dir.path().join("src/machines/mod.rs")).unwrap();
    assert!(!module.contains("pub mod guards;"));
    assert!(!module.contains("MachineGuardImpls"));
}

/// Band 4b (the #551 pin, guarded shape): the emitted machines module WITH
/// the guard seam compiles in a scratch crate carrying the domain-types
/// guard dep set — xstate-core + serde_json + codegraph-workflow (path).
/// The dispatch implements the workflow crate's `MachineGuards` trait, so
/// the module cannot compile without it.
#[tokio::test]
async fn the_guarded_machines_module_compiles_with_the_workflow_dep() {
    let dir = tempfile::tempdir().unwrap();
    emit_guarded_into(dir.path()).await;

    let scratch = tempfile::tempdir().unwrap();
    let crate_root = scratch.path();
    fs::create_dir_all(crate_root.join("src/machines/payroll")).unwrap();
    fs::copy(
        dir.path().join("src/machines/mod.rs"),
        crate_root.join("src/machines/mod.rs"),
    )
    .unwrap();
    fs::copy(
        dir.path().join("src/machines/guards.rs"),
        crate_root.join("src/machines/guards.rs"),
    )
    .unwrap();
    fs::copy(
        dir.path().join("src/machines/payroll/payrun.ir.json"),
        crate_root.join("src/machines/payroll/payrun.ir.json"),
    )
    .unwrap();
    fs::write(crate_root.join("src/lib.rs"), "pub mod machines;\n").unwrap();

    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the workspace root")
        .display()
        .to_string();
    fs::write(
        crate_root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"machines_guarded_scratch\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nserde_json = \"1\"\n\
             xstate-core = {{ git = \"https://github.com/yestechgroup/restingstate.git\", \
             rev = \"da0c804d3c30a0bcc938562e39a4b7d7505c663b\", default-features = false }}\n\
             codegraph-workflow = {{ path = \"{workspace}/crates/codegraph-workflow\" }}\n"
        ),
    )
    .unwrap();

    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("check")
        .arg("--manifest-path")
        .arg(crate_root.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", scratch.path().join("target"))
        .env("CARGO_TERM_COLOR", "never");
    let out = cmd.output().expect("cargo check must run");
    assert!(
        out.status.success(),
        "the guarded machines module must compile against the workflow crate:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}
