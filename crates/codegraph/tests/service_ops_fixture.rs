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
  "required": ["payRunId", "runStatus"],
  "properties": {
    "payRunId": { "type": "string" },
    "runStatus": { "type": "string" }
  }
}"#;

pub const PAY_RUN_MOX: &str = r#"
package payroll

class PayRunType {
    String payRunId
    String runStatus
}
"#;

/// One entity design with a full repository, and a service carrying every
/// lowering shape: capability-gated delete delegation, conditional-grant
/// read delegation, an ungated seam (`save` delegation), an abstract seam
/// (declared signature), and a protected op. Transition guards ride the
/// GRANT `when`s (the `.actor` policy); the ddd op-level `when (...)`
/// condition requires an upstream machine-bearing design (typed
/// lifecycles) and is pinned at the unit level.
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
        service PayRunService {
            inject PayRunRepository;
            approve => PayRunRepository.delete capability ApprovePayRun;
            inspect => PayRunRepository.findById capability InspectPayRun;
            archive => PayRunRepository.save;
            boolean submit(String note) capability SubmitPayRun;
            protected String internalAdjust();
        }
    }
}
"#;

/// Owner holds ApprovePayRun + SubmitPayRun unconditionally; Manager holds
/// InspectPayRun `when (runStatus == "Draft")`. The `never_both` pair is
/// split across TWO policy files — each compiles clean, but the MERGED
/// policy has Owner holding both sides, which is exactly the runtime
/// separation-of-duties gate's reason to exist (single-file violations
/// are rejected statically by the rex actor compiler).
pub const POLICY_ACTOR: &str = r#"
import "pay_run.mox"

actors Payroll {
    actor Owner
    actor Manager

    capability ApprovePayRun on PayRunType
    capability InspectPayRun on PayRunType
    capability SubmitPayRun on PayRunType
    capability PreparePayRun on PayRunType

    grant Owner {
        permit ApprovePayRun
        permit SubmitPayRun
    }

    grant Manager {
        permit InspectPayRun when (runStatus == "Draft")
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
entities = ["PayRunType"]
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
    assert_eq!(mox_stats.skipped, 0, "mox class must bridge the schema");
    let ddd_stats = codegraph::ingest::ddd_ingest::ingest_ddd_files(
        &engine,
        &engine,
        std::slice::from_ref(&fixture.ddd),
        &config,
    )
    .await
    .unwrap();
    assert_eq!(ddd_stats.services, 1, "the service must ingest");
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
