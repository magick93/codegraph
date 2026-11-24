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
        "a design without a referenced machine validates to nothing: {refusals:?}"
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

#[tokio::test]
async fn a_machine_less_graph_emits_zero_files() {
    let dir = tempfile::tempdir().unwrap();
    let mock = MockEngine::builder().build();
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
