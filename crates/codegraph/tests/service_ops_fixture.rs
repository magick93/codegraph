//! Shared fixture harness for the service-ops tests (issue #590).
//!
//! Included via `#[path]` from `service_ops_tests.rs` (node-free pins) and
//! `service_ops_compile_gate.rs` (the ignored compile gate) so both drive
//! the SAME fixture — one recipe, no drift.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use codegraph_core::traits::GraphIngestor;
use tempfile::TempDir;

pub const PAY_RUN_SCHEMA: &str = r#"{
  "title": "PayRunType",
  "type": "object",
  "required": ["payRunId", "payRunStatus"],
  "properties": {
    "payRunId": { "type": "string" },
    "payRunStatus": { "type": "string" }
  }
}"#;

/// A refers-linked CHILD entity: `role = "child"` in the entity config with
/// `parent`/`parent_ref` — the unblocked target shape for child service ops
/// (the route-nested child with its OWN repository). The parent FK is the
/// synthetic `pay_run_id` column (config `parent_ref`), not a child-side
/// schema property.
pub const PAY_RUN_LINE_SCHEMA: &str = r#"{
  "title": "PayRunLineType",
  "type": "object",
  "required": ["lineId"],
  "properties": {
    "lineId": { "type": "string" },
    "notes": { "type": "string" }
  }
}"#;

/// The depth-2 GRANDCHILD entity (issue #629): refers-linked under the
/// CHILD — `role = "child"` with `parent = "PayRunLineType"`. The service-op
/// plane renders its action routes through the grandparent branch; the
/// #551 compile pin exists because that surface compiled NOWHERE before.
pub const PAY_RUN_LINE_ADJUSTMENT_SCHEMA: &str = r#"{
  "title": "PayRunLineAdjustmentType",
  "type": "object",
  "required": ["adjustmentId"],
  "properties": {
    "adjustmentId": { "type": "string" },
    "reason": { "type": "string" }
  }
}"#;

pub const PAY_RUN_MOX: &str = r#"
package payroll

enum PayRunStatus {
    Draft = 0
    Calculating = 1
    Approved = 2
    Reversed = 3
}

class PayRunType {
    String payRunId
    PayRunStatus payRunStatus
}

class PayRunLineType {
    String lineId
    String [0..1] notes
}

class PayRunLineAdjustmentType {
    String adjustmentId
    String [0..1] reason
}
"#;

/// Two entity designs (a root and its refers-linked CHILD, each with a full
/// repository), a lifecycle machine on the root, and services carrying every
/// lowering shape on both entities. The CHILD service ops delegate to the
/// child repository: `adjust` (save), `remove` (delete), a
/// capability-gated save (`reprice`), and a `findAll` delegation that keeps
/// the 501 seam fail-closed (`summarize`). The child machine surface stays
/// empty — child machines are title-keyed machinery, exercised by the root.
pub const PAYROLL_DDD: &str = r#"
import "pay_run.mox";

application Payroll {
    module payroll {
        entity PayRunType scaffold repository PayRunRepository {
            findById;
            findAll;
            save;
            delete;
        }
        machine payRunStatus: PayRunStatus {
            initial Draft;
            Draft -> Calculating via Submit;
            Calculating -> Approved via Approve;
            Approved -> Reversed via Reverse;
            terminal Reversed;
        }
        service PayRunService {
            inject PayRunRepository;
            purge => PayRunRepository.delete capability ApprovePayRun;
            inspect => PayRunRepository.findById capability InspectPayRun;
            archive => PayRunRepository.save;
            reverse => PayRunRepository.save capability ApprovePayRun;
            submit => PayRunRepository.save capability SubmitPayRun when (payRunStatus == Draft);
            approve => PayRunRepository.save capability ApprovePayRun;
            boolean notify(String note) capability SubmitPayRun;
            protected String internalAdjust();
        }
        entity PayRunLineType scaffold repository PayRunLineRepository {
            findById;
            findAll;
            save;
            delete;
        }
        service PayRunLineService {
            inject PayRunLineRepository;
            adjust => PayRunLineRepository.save;
            reprice => PayRunLineRepository.save capability AdjustPayRunLine;
            remove => PayRunLineRepository.delete;
            summarize => PayRunLineRepository.findAll;
        }
        entity PayRunLineAdjustmentType scaffold repository PayRunLineAdjustmentRepository {
            findById;
            findAll;
            save;
            delete;
        }
        service PayRunLineAdjustmentService {
            inject PayRunLineAdjustmentRepository;
            recalculate => PayRunLineAdjustmentRepository.save capability RecalculateAdjustment;
            voidOut => PayRunLineAdjustmentRepository.delete;
        }
    }
}
"#;

/// Manager holds InspectPayRun unconditionally, plus the CHILD entity's
/// `AdjustPayRunLine` (unconditional — child grant-`when` typing is
/// upstream-blocked with the module's single machine; the capability gate
/// itself is what the child handler pin exercises).
pub const POLICY_ACTOR: &str = r#"
import "pay_run.mox"

actors Payroll {
    actor Owner
    actor Manager

    capability ApprovePayRun on PayRunType
    capability InspectPayRun on PayRunType
    capability SubmitPayRun on PayRunType
    capability PreparePayRun on PayRunType
    capability AdjustPayRunLine on PayRunLineType
    capability RecalculateAdjustment on PayRunLineAdjustmentType

    grant Owner {
        permit ApprovePayRun
        permit SubmitPayRun
    }

    grant Manager {
        permit InspectPayRun
        permit AdjustPayRunLine
        permit RecalculateAdjustment
    }

    never_both { PreparePayRun, ApprovePayRun }
}
"#;

/// The second import: valid alone, violating after the merge — Owner gains
/// `PreparePayRun`, the `never_both` partner of the `ApprovePayRun` the
/// first file granted.
pub const POLICY_B_ACTOR: &str = r#"
import "pay_run.mox"

actors Payroll {
    actor Owner

    capability PreparePayRun on PayRunType

    grant Owner {
        permit PreparePayRun
    }
}
"#;

pub const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.payroll]
label = "Payroll"
schema_dir = "model-schemas"
postgres_schema = "payroll"
entities = ["PayRunType", "PayRunLineType", "PayRunLineAdjustmentType"]

[domains.payroll.entity_config.PayRunLineType]
role = "child"
parent = "PayRunType"
parent_ref = "pay_run_id"

[domains.payroll.entity_config.PayRunLineAdjustmentType]
role = "child"
parent = "PayRunLineType"
parent_ref = "pay_run_line_id"
"#;

/// The mox-first twin: NO `entities` key (author-declarative mox), no JSON
/// schemas — the mox bridge owns the graph. Same child declaration. Pins
/// the openapi service-op registration in mox-first mode (the pre-WS4 loop
/// iterated `domain_entry.entities`, which is empty here).
pub const DOMAINS_TOML_MOX_FIRST: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.payroll]
label = "Payroll"
schema_dir = "model-schemas"
postgres_schema = "payroll"

[domains.payroll.entity_config.PayRunLineType]
role = "child"
parent = "PayRunType"
parent_ref = "pay_run_id"

[domains.payroll.entity_config.PayRunLineAdjustmentType]
role = "child"
parent = "PayRunLineType"
parent_ref = "pay_run_line_id"
"#;

// ── Harness ────────────────────────────────────────────────────────────

pub struct Fixture {
    pub schemas_dir: PathBuf,
    pub mox: PathBuf,
    pub ddd: PathBuf,
    pub actor: PathBuf,
    pub config: PathBuf,
}

pub fn write_fixture_in(_prefix: &str, ddd_source: &str) -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();
    std::mem::forget(dir); // the fixture outlives the guard on purpose
    // SchemaLoader layout: <schema_dir>/<domain>/json/*.json.
    let schemas_dir = root.join("model-schemas").join("payroll").join("json");
    fs::create_dir_all(&schemas_dir).unwrap();
    fs::write(schemas_dir.join("PayRunType.json"), PAY_RUN_SCHEMA).unwrap();
    fs::write(schemas_dir.join("PayRunLineType.json"), PAY_RUN_LINE_SCHEMA).unwrap();
    fs::write(
        schemas_dir.join("PayRunLineAdjustmentType.json"),
        PAY_RUN_LINE_ADJUSTMENT_SCHEMA,
    )
    .unwrap();
    let schemas_dir = root.join("model-schemas");
    let mox = root.join("pay_run.mox");
    fs::write(&mox, PAY_RUN_MOX).unwrap();
    let ddd = root.join("payroll.ddd");
    fs::write(&ddd, ddd_source).unwrap();
    let actor = root.join("policy.actor");
    fs::write(&actor, POLICY_ACTOR).unwrap();
    let actor_b = root.join("policy_b.actor");
    fs::write(&actor_b, POLICY_B_ACTOR).unwrap();
    let config = root.join("domains.toml");
    fs::write(&config, DOMAINS_TOML).unwrap();
    Fixture {
        schemas_dir,
        mox,
        ddd,
        actor,
        config,
    }
}

/// A mox-FIRST variant of the fixture: no JSON schemas at all, and the
/// config carries NO `entities` key (author-declarative mox). The mox
/// bridge owns the graph; the `.ddd` + `.actor` inputs are unchanged.
pub fn write_mox_first_fixture_in(_prefix: &str, ddd_source: &str) -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();
    std::mem::forget(dir); // the fixture outlives the guard on purpose
    let schemas_dir = root.join("model-schemas");
    let mox = root.join("pay_run.mox");
    fs::write(&mox, PAY_RUN_MOX).unwrap();
    let ddd = root.join("payroll.ddd");
    fs::write(&ddd, ddd_source).unwrap();
    let actor = root.join("policy.actor");
    fs::write(&actor, POLICY_ACTOR).unwrap();
    let actor_b = root.join("policy_b.actor");
    fs::write(&actor_b, POLICY_B_ACTOR).unwrap();
    let config = root.join("domains.toml");
    fs::write(&config, DOMAINS_TOML_MOX_FIRST).unwrap();
    Fixture {
        schemas_dir,
        mox,
        ddd,
        actor,
        config,
    }
}

pub fn tempdir() -> TempDir {
    TempDir::new().unwrap()
}

pub async fn ingest(fixture: &Fixture) -> codegraph_grafeo::GrafeoEngine {
    let engine = codegraph_grafeo::GrafeoEngine::in_memory().unwrap();
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let classifier = codegraph_classifier::config::parse_classifier_config(Path::new(
        "tests/fixtures/classifier.toml",
    ))
    .unwrap();
    let entity_names: std::collections::HashSet<String> = config
        .domains
        .values()
        .flat_map(|d| d.entities.iter().cloned())
        .collect();
    if !entity_names.is_empty() {
        codegraph::ingest::async_ingest::ingest_schemas(
            &engine,
            &fixture.schemas_dir,
            &classifier,
            &entity_names,
            &codegraph_config::UiOverrideConfig::default(),
            &config.defaults.type_suffix,
        )
        .await
        .unwrap();
    }
    let mox_stats = codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        std::slice::from_ref(&fixture.mox),
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap()
    .stats;
    assert_eq!(mox_stats.skipped, 0, "mox classes must bridge the schemas");
    let ddd_stats = codegraph::ingest::ddd_ingest::ingest_ddd_files(
        &engine,
        &engine,
        std::slice::from_ref(&fixture.ddd),
        &config,
    )
    .await
    .unwrap();
    assert_eq!(
        ddd_stats.services, 3,
        "all three services must ingest (root, child, grandchild)"
    );
    let policy = codegraph::ifml_actor_import::resolve_actor_policy(
        &["policy.actor".to_string(), "policy_b.actor".to_string()],
        fixture.ddd.parent().unwrap(),
    )
    .expect("the actor policies must resolve");
    engine
        .ingest_actor_policy(&policy)
        .await
        .expect("policy ingests");
    engine
}

pub fn profiles_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .join("profiles.toml")
}

pub async fn run_generation(
    engine: &codegraph_grafeo::GrafeoEngine,
    fixture: &Fixture,
    output_dir: &Path,
    domain_types_dir: &Path,
    hooks_tmp: &Path,
) -> codegraph_generate::report::GenerationReport {
    let tera = codegraph::generate::template_engine::create_tera(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
    )
    .unwrap();

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let type_contracts_path = workspace_root
        .join("crates")
        .join("codegraph-type-contracts");
    let workflow_path = workspace_root.join("crates").join("codegraph-workflow");
    let profiles = profiles_path();

    let registry = codegraph::profile::CapabilityRegistry::new();
    let mut resolved =
        codegraph::profile::load_and_resolve_profile(&profiles, "default", None).unwrap();
    for section in resolved.sections.values_mut() {
        section.generators.retain(|g| !g.starts_with("grpc_"));
    }
    let plan = codegraph::profile::BuildPlan::from_profile(&resolved, &registry).unwrap();

    let project_config = codegraph::generate::ProjectConfig {
        identity: codegraph::generate::IdentityConfig {
            app_name: "test-app".into(),
            domain_types_crate: "domain_types".into(),
            generator_name: "codegraph-test".into(),
            ..Default::default()
        },
        paths: codegraph::generate::PathsConfig {
            type_contracts_base: type_contracts_path.to_string_lossy().to_string(),
            codegraph_workflow_base: workflow_path.to_string_lossy().to_string(),
            domain_types_base: "domain-types".into(),
            ..Default::default()
        },
        codegen: codegraph::generate::CodegenConfig {
            types_import_prefix: "crate::types".into(),
            ..Default::default()
        },
        ..Default::default()
    };

    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();

    codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
        db: engine,
        config: &config,
        output_dir,
        tera: &tera,
        ui_overrides: &Default::default(),
        ui_domains: &Default::default(),
        schema_base_dir: Path::new(""),
        seed_config: None,
        domain_types_base: Some(domain_types_dir),
        hooks_base: Some(hooks_tmp),
        ext_points: None,
        integration_manifests: None,
        build_plan: Some(&plan),
        profile_name: "default",
        ifml_frameworks: vec![],
        ifml_components: None,
        ux_rules: None,
        project_config: Some(&project_config),
        emdash_plugins: None,
        domain_config_dir: None,
    })
    .await
    .unwrap()
}

pub fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}
