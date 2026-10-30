//! Integration catalog manifests (issue #552).
//!
//! `tables.tera` promised a catalog "populated from manifests at deploy
//! time" but nothing generated or loaded manifests — consumers hand-wrote
//! `platform_integrations.catalog` INSERTs. This suite pins the
//! generate-time path:
//!
//! - `--integration-manifests <file>` (repeatable) seeds the catalog: the
//!   `IntegrationTablesGenerator` emits
//!   `migrations/0010_integration_catalog_seed.sql` with one idempotent
//!   upsert per manifest entry (worker_url `{app_name}` templating, a
//!   content hash in `manifest_hash`) plus the optional demo installations
//!   that replace hand-written bootstrap SQL
//! - absent manifests ⇒ no seed file (presence gating / byte-identity)
//! - manifests without `--extension-points` are a config error (the seed
//!   references extension point ids only the integration plane owns)
//! - the signing-secret write path (#511 rider): `InstallRequest` carries
//!   `signing_secret`, and a dedicated
//!   `PUT /integrations/installations/{id}/signing-secret` endpoint
//!   persists it on the installation row — the dispatcher SELECTs
//!   `i.signing_secret` fresh on every dispatch, so the next dispatch
//!   picks it up (already pinned in `extension_points_tests`)
//!
//! Both provider legs are pinned: the monolith SeaORM handler and the
//! cornucopia gateway handler (`catalog_handler_cornucopia.tera`), plus
//! the router route in both topologies.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use codegraph_ext_points::{
    IntegrationManifestSet, merge_integration_manifests, parse_extension_points_str,
    parse_integration_manifest_str,
};

use codegraph_grafeo::GrafeoEngine;

/// Extension point covering the fixture entity (CandidateType → table
/// `candidate` in domain `recruiting`), push direction.
const EXT_POINTS_TOML: &str = r#"
[payroll-sync]
name = "Payroll Sync"
description = "Synchronise employee and pay data"
cardinality = "exclusive"
entities = ["recruiting.candidate"]
directions = ["push"]
"#;

const MANIFEST_TOML: &str = r#"
[integration.xero-payroll]
name = "Xero Payroll"
vendor = "yestech"
version = "1.2.0"
description = "Push pay runs to Xero"
tier = "official"
categories = ["payroll"]
markets = ["nz"]
extension_point = "payroll-sync"
worker_url = "https://{app_name}-xero-payroll.workers.dev"

[[installation]]
catalog = "xero-payroll"
organization_id = "00000000-0000-0000-0000-000000000001"
"#;

/// Ingest the fixture JSON schemas (schema entities must exist for the
/// command generator).
async fn ingest_fixture_schemas(
    engine: &GrafeoEngine,
    config: &codegraph_config::config::DomainConfig,
) {
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
        engine,
        Path::new("tests/fixtures/schemas"),
        &classifier,
        &entity_names,
        &codegraph_config::UiOverrideConfig::default(),
        &config.defaults.type_suffix,
    )
    .await
    .unwrap();
}

fn profiles_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .join("profiles.toml")
}

/// Run the full generator suite (minus gRPC) over the fixture graph.
async fn run_generation(
    engine: &GrafeoEngine,
    ext_points: Option<&codegraph_ext_points::ExtensionPointsConfig>,
    manifests: Option<&IntegrationManifestSet>,
    output_dir: &Path,
    domain_types_dir: &Path,
    hooks_tmp: &Path,
) {
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

    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();

    let report =
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
            ext_points,
            integration_manifests: manifests,
            build_plan: Some(&plan),
            ifml_frameworks: vec![],
            ifml_components: None,
            ux_rules: None,
            project_config: Some(&project_config),
            emdash_plugins: None,
            domain_config_dir: None,
        })
        .await;
    if report.is_err() {
        return;
    }
    let report = report.unwrap();
    assert!(!report.has_errors(), "generation reported errors");
}

fn migrations_dir(output_dir: &Path) -> std::path::PathBuf {
    output_dir.join("migrations")
}

/// TDD mandate 1 (issue #552): a fixture manifest produces the catalog
/// seed with worker_url templating.
#[tokio::test]
async fn manifest_seeds_catalog_row() {
    let dir = tempfile::tempdir().unwrap();
    let output_dir = dir.path().join("out");
    let domain_types_dir = dir.path().join("domain-types");
    fs::create_dir_all(&output_dir).unwrap();

    let engine = GrafeoEngine::in_memory().unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    ingest_fixture_schemas(&engine, &config).await;

    let ext = parse_extension_points_str(EXT_POINTS_TOML).unwrap();
    let manifests =
        merge_integration_manifests(&[parse_integration_manifest_str(MANIFEST_TOML).unwrap()])
            .unwrap();

    let hooks_tmp = tempfile::tempdir().unwrap();
    run_generation(
        &engine,
        Some(&ext),
        Some(&manifests),
        &output_dir,
        &domain_types_dir,
        hooks_tmp.path(),
    )
    .await;

    let seed_path = migrations_dir(&output_dir).join("0010_integration_catalog_seed.sql");
    let seed = fs::read_to_string(&seed_path)
        .unwrap_or_else(|e| panic!("catalog seed migration must exist at {seed_path:?}: {e}"));

    // The catalog row upsert (idempotent — regeneration updates in place).
    assert!(
        seed.contains("INSERT INTO platform_integrations.catalog"),
        "seed must INSERT catalog rows:\n{seed}"
    );
    assert!(
        seed.contains("ON CONFLICT (id) DO UPDATE"),
        "seed upserts so a changed manifest re-seeds cleanly:\n{seed}"
    );
    for expected in [
        "xero-payroll",
        "Xero Payroll",
        "yestech",
        "1.2.0",
        "official",
        "payroll-sync",
    ] {
        assert!(
            seed.contains(expected),
            "seed must carry `{expected}`:\n{seed}"
        );
    }
    // Array columns.
    assert!(
        seed.contains("ARRAY['payroll']") && seed.contains("ARRAY['nz']"),
        "categories/markets must render as TEXT arrays:\n{seed}"
    );
    // worker_url templating: {app_name} → the project app name.
    assert!(
        seed.contains("https://test-app-xero-payroll.workers.dev"),
        "worker_url template must substitute {{app_name}}:\n{seed}"
    );
    assert!(
        !seed.contains("{app_name}"),
        "no unresolved template placeholder may reach SQL:\n{seed}"
    );
    // Content hash column is populated.
    assert!(
        seed.contains("manifest_hash"),
        "manifest_hash must be recorded:\n{seed}"
    );
    // The demo installation replaces hand-written bootstrap SQL.
    assert!(
        seed.contains("INSERT INTO platform_integrations.installations"),
        "demo installations must seed:\n{seed}"
    );
    assert!(
        seed.contains("00000000-0000-0000-0000-000000000001"),
        "demo installation org id must appear:\n{seed}"
    );
}

/// Presence gating (the byte-identity contract): ext points WITHOUT
/// manifests emit no catalog seed — the three pre-existing integration
/// migrations are the entire SQL surface.
#[tokio::test]
async fn absent_manifests_emits_no_catalog_seed() {
    let dir = tempfile::tempdir().unwrap();
    let output_dir = dir.path().join("out");
    let domain_types_dir = dir.path().join("domain-types");
    fs::create_dir_all(&output_dir).unwrap();

    let engine = GrafeoEngine::in_memory().unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    ingest_fixture_schemas(&engine, &config).await;

    let ext = parse_extension_points_str(EXT_POINTS_TOML).unwrap();
    let hooks_tmp = tempfile::tempdir().unwrap();
    run_generation(
        &engine,
        Some(&ext),
        None,
        &output_dir,
        &domain_types_dir,
        hooks_tmp.path(),
    )
    .await;

    let migrations = migrations_dir(&output_dir);
    assert!(
        !migrations
            .join("0010_integration_catalog_seed.sql")
            .exists(),
        "no manifest ⇒ no catalog seed migration"
    );
    // The pre-existing plane is untouched.
    for existing in [
        "0007_integration_tables.sql",
        "0008_integration_rls.sql",
        "0009_integration_seed.sql",
    ] {
        assert!(
            migrations.join(existing).exists(),
            "the base integration plane must still emit {existing}"
        );
    }
}

/// A manifest referencing an unknown extension point is a generation-time
/// config error — the FK would fail at migrate time, so it fails earlier
/// with a message naming the offender.
#[tokio::test]
async fn unknown_extension_point_in_manifest_fails_generation() {
    let dir = tempfile::tempdir().unwrap();
    let output_dir = dir.path().join("out");
    let domain_types_dir = dir.path().join("domain-types");
    fs::create_dir_all(&output_dir).unwrap();

    let engine = GrafeoEngine::in_memory().unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    ingest_fixture_schemas(&engine, &config).await;

    let ext = parse_extension_points_str(EXT_POINTS_TOML).unwrap();
    let manifest = r#"
[integration.ghost]
name = "Ghost"
vendor = "v"
version = "1.0.0"
tier = "community"
extension_point = "no-such-point"
"#;
    let manifests =
        merge_integration_manifests(&[parse_integration_manifest_str(manifest).unwrap()]).unwrap();

    let hooks_tmp = tempfile::tempdir().unwrap();
    let report =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            db: &engine,
            config: &config,
            output_dir: &output_dir,
            tera: &codegraph::generate::template_engine::create_tera(
                &Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
            )
            .unwrap(),
            ui_overrides: &Default::default(),
            ui_domains: &Default::default(),
            schema_base_dir: Path::new(""),
            seed_config: None,
            domain_types_base: Some(&domain_types_dir),
            hooks_base: Some(hooks_tmp.path()),
            ext_points: Some(&ext),
            integration_manifests: Some(&manifests),
            build_plan: None,
            ifml_frameworks: vec![],
            ifml_components: None,
            ux_rules: None,
            project_config: None,
            emdash_plugins: None,
            domain_config_dir: None,
        })
        .await;

    let err = report.expect_err("an unknown extension point ref must fail generation");
    let msg = err.to_string();
    assert!(
        msg.contains("no-such-point"),
        "error must name the unknown point: {msg}"
    );
}

/// Manifests without `--extension-points` are a config error — silently
/// ignoring them would drop the seed on the floor.
#[tokio::test]
async fn manifest_without_extension_points_is_a_config_error() {
    let dir = tempfile::tempdir().unwrap();
    let output_dir = dir.path().join("out");
    let domain_types_dir = dir.path().join("domain-types");
    fs::create_dir_all(&output_dir).unwrap();

    let engine = GrafeoEngine::in_memory().unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    ingest_fixture_schemas(&engine, &config).await;

    let manifests =
        merge_integration_manifests(&[parse_integration_manifest_str(MANIFEST_TOML).unwrap()])
            .unwrap();
    let hooks_tmp = tempfile::tempdir().unwrap();
    let report =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            db: &engine,
            config: &config,
            output_dir: &output_dir,
            tera: &codegraph::generate::template_engine::create_tera(
                &Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
            )
            .unwrap(),
            ui_overrides: &Default::default(),
            ui_domains: &Default::default(),
            schema_base_dir: Path::new(""),
            seed_config: None,
            domain_types_base: Some(&domain_types_dir),
            hooks_base: Some(hooks_tmp.path()),
            ext_points: None,
            integration_manifests: Some(&manifests),
            build_plan: None,
            ifml_frameworks: vec![],
            ifml_components: None,
            ux_rules: None,
            project_config: None,
            emdash_plugins: None,
            domain_config_dir: None,
        })
        .await;

    let err = report.expect_err("manifests without extension points must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("--extension-points"),
        "error must point at the missing flag: {msg}"
    );
}

/// Render one integration template with a minimal project context — the
/// handler/router templates only read `project.generator_name` and
/// `project.api_version`.
fn render_integration_template(tera: &tera::Tera, name: &str) -> String {
    let mut ctx = tera::Context::new();
    let mut project = HashMap::new();
    project.insert("generator_name", "codegraph-test");
    project.insert("api_version", "v1");
    ctx.insert("project", &project);
    tera.render(name, &ctx).unwrap()
}

fn signing_secret_pins(content: &str, label: &str) {
    // InstallRequest carries the optional secret (#511 rider).
    assert!(
        content.contains("pub signing_secret: Option<String>"),
        "{label}: InstallRequest must accept signing_secret:\n{content}"
    );
    // The install INSERT persists it on the installation row.
    assert!(
        content.contains("signing_secret"),
        "{label}: install path must reference signing_secret"
    );
    // The dedicated provisioning endpoint — no raw SQL to rotate a secret.
    assert!(
        content.contains("pub async fn set_signing_secret"),
        "{label}: set_signing_secret handler must exist:\n{content}"
    );
    assert!(
        content.contains("UPDATE platform_integrations.installations")
            && content.contains("SET signing_secret"),
        "{label}: provisioning must UPDATE installations.signing_secret (the column the \
         dispatcher SELECTs on every dispatch):\n{content}"
    );
}

/// TDD mandate 2 (issue #552): the API write path persists the signing
/// secret; the dispatcher picks it up on the next dispatch (its SELECT
/// reads `i.signing_secret` at dispatch time — pinned in
/// `extension_points_tests::dispatcher_reads_worker_url_and_signing_secret`).
#[tokio::test]
async fn installation_settings_accepts_signing_secret() {
    let tera = codegraph::generate::template_engine::create_tera(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
    )
    .unwrap();

    // Monolith (SeaORM) leg.
    let rendered = render_integration_template(&tera, "integration/catalog_handler.tera");
    signing_secret_pins(&rendered, "monolith handler");
    assert!(
        rendered.contains("Statement::from_sql_and_values"),
        "monolith leg goes through SeaORM statements"
    );

    // Cornucopia gateway leg (workers topology).
    let cornucopia =
        render_integration_template(&tera, "integration/catalog_handler_cornucopia.tera");
    signing_secret_pins(&cornucopia, "gateway handler");
    assert!(
        cornucopia.contains("GenericClient"),
        "gateway leg goes through the cornucopia client"
    );

    // The router mounts the provisioning endpoint (one template serves
    // both topologies).
    let router = render_integration_template(&tera, "integration/catalog_router.tera");
    assert!(
        router.contains("signing-secret") && router.contains("set_signing_secret"),
        "router must mount the signing-secret endpoint:\n{router}"
    );
}
