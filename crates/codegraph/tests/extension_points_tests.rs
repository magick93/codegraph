//! Push-integration plane tests (issue #503).
//!
//! `--extension-points` generates the full integration infrastructure —
//! catalog/tables/config structs and the `IntegrationDispatcher` — but
//! nothing ever called `notify`, so a catalog-registered integration
//! worker never learned that anything happened. This suite pins:
//!
//! - command-layer notify call sites for covered entities (create/update/
//!   delete), gated on the parsed extension-points config so absent config
//!   ⇒ byte-identical output
//! - the documented POST payload shape (`{event, entity_id, org_id,
//!   installation_id, correlation_id}`, `event = "{entity_type}.{event}"`,
//!   entity_type = the full `domain.table` ref the installations SQL
//!   matches against `ep.allowed_entities`)
//! - the dispatcher's topology/provider legs: SeaORM + reqwest (monolith),
//!   cornucopia `ClientSource` + `GenericClient` (cornucopia monolith and
//!   workers), `worker::Fetch` (wasm32 workers)
//!
//! The closed event vocabulary is `created` / `updated` / `deleted` /
//! `state_changed`.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use codegraph_ext_points::parse_extension_points_str;

use codegraph_grafeo::GrafeoEngine;

/// Extension point covering the fixture entity (CandidateType → table
/// `candidate` in domain `recruiting`), push direction — the shape the
/// command-layer call sites key on.
const EXT_POINTS_TOML: &str = r#"
[payroll-sync]
name = "Payroll Sync"
description = "Synchronise employee and pay data"
cardinality = "exclusive"
entities = ["recruiting.candidate"]
directions = ["push"]
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
            build_plan: Some(&plan),
            ifml_frameworks: vec![],
            ifml_components: None,
            ux_rules: None,
            project_config: Some(&project_config),
            emdash_plugins: None,
            domain_config_dir: None,
        })
        .await
        .unwrap();
    assert!(!report.has_errors(), "generation reported errors");
}

fn read_command_module(output_dir: &Path) -> String {
    fs::read_to_string(
        output_dir
            .join("src")
            .join("domain")
            .join("recruiting")
            .join("candidate")
            .join("command.rs"),
    )
    .expect("candidate command module must be generated")
}

// ── Command-layer call sites (the B5-style payoff) ──

#[tokio::test]
async fn extension_point_coverage_emits_notify_call_sites() {
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
        &output_dir,
        &domain_types_dir,
        hooks_tmp.path(),
    )
    .await;

    // The dispatcher module exists (the plane is installed)…
    assert!(
        output_dir
            .join("src")
            .join("integration_dispatch.rs")
            .exists(),
        "integration_dispatch.rs must be generated when extension points are configured"
    );

    // …and the covered entity's command layer FIRES it: one notify call
    // site per EMITTED mutation, entity_type = the full domain.table ref,
    // closed event vocabulary. The fixture entity carries create/update
    // (no delete op) — the delete site must follow the delete fn.
    let command = read_command_module(&output_dir);
    let has_delete = command.contains("pub async fn delete");
    for (op_fn, event) in [
        ("pub async fn create", "created"),
        ("pub async fn update", "updated"),
        ("pub async fn delete", "deleted"),
    ] {
        assert_eq!(
            command.contains(op_fn),
            command.contains(&format!(
                "dispatcher.notify(organization_id, \"recruiting.candidate\", \"{event}\""
            )),
            "the `{event}` notify site must exactly follow the `{op_fn}` fn"
        );
    }
    assert!(
        command
            .contains("dispatcher.notify(organization_id, \"recruiting.candidate\", \"created\""),
        "command module must carry the create notify site:\n{command}"
    );
    let _ = has_delete;
}

/// Presence gating (the byte-identity contract): with NO extension-points
/// config the command layer carries no dispatcher field and no notify call
/// sites, and the integration plane is not generated at all.
#[tokio::test]
async fn without_extension_points_output_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let output_dir = dir.path().join("out");
    let domain_types_dir = dir.path().join("domain-types");
    fs::create_dir_all(&output_dir).unwrap();

    let engine = GrafeoEngine::in_memory().unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    ingest_fixture_schemas(&engine, &config).await;

    let hooks_tmp = tempfile::tempdir().unwrap();
    run_generation(
        &engine,
        None,
        &output_dir,
        &domain_types_dir,
        hooks_tmp.path(),
    )
    .await;

    assert!(
        !output_dir
            .join("src")
            .join("integration_dispatch.rs")
            .exists(),
        "integration_dispatch.rs must not be generated without extension points"
    );
    let command = read_command_module(&output_dir);
    assert!(
        !command.contains("dispatcher"),
        "command module must not reference a dispatcher without extension points:\n{command}"
    );
    assert!(
        !command.contains("notify("),
        "command module must not carry notify call sites without extension points:\n{command}"
    );
}

// ── Dispatcher contract ──

fn test_project(provider: &str) -> codegraph::generate::ProjectConfig {
    let mut project = codegraph::generate::ProjectConfig::default();
    project.identity.generator_name = "codegraph-test".into();
    if provider == "cornucopia" {
        project.database.persistence_provider =
            codegraph::generate::profile::PersistenceProvider::Cornucopia;
    }
    project
}

fn render_dispatcher(provider: &str) -> String {
    let tera = codegraph::generate::template_engine::create_tera(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
    )
    .unwrap();
    let ctx: HashMap<String, String> = HashMap::new();
    codegraph::generate::render_template_with_project(
        &tera,
        "integration/dispatcher.tera",
        &ctx,
        &test_project(provider),
    )
    .expect("dispatcher.tera must render")
}

/// Issue #511: both provider legs select the installation's
/// signing_secret, and both HTTP legs (native reqwest, wasm
/// `worker::Fetch`) carry the HMAC headers. The verification helper and
/// the replay window ride the same module.
#[test]
fn dispatcher_signs_events_with_the_installation_secret_on_both_legs() {
    for dispatcher in [
        render_dispatcher("sea_orm"),
        render_dispatcher("cornucopia"),
    ] {
        // Shared signing machinery.
        for fragment in [
            "use hmac::{Hmac, Mac};",
            "type HmacSha256 = Hmac<Sha256>;",
            "fn event_signature(secret: &str, timestamp: i64, body: &[u8]) -> String",
            "pub fn verify_event_signature(",
            "pub const EVENT_SIGNATURE_MAX_AGE_SECS: i64 = 300;",
            // Both provider legs select + thread the secret.
            "i.signing_secret",
            "signing_secret: Option<&str>",
            "signing_secret.as_deref()",
            // Signature header shape matches the webhook plane.
            "X-Webhook-Signature",
            "X-Webhook-Timestamp",
        ] {
            assert!(
                dispatcher.contains(fragment),
                "dispatcher must carry `{fragment}`:\n{dispatcher}"
            );
        }
        // Native leg signs the exact reqwest body bytes.
        let native_idx = dispatcher
            .find("#[cfg(not(target_arch = \"wasm32\"))]")
            .expect("native leg must exist");
        let native = &dispatcher[native_idx..];
        assert!(
            native.contains("event_signature(secret, timestamp, &body)"),
            "native leg must sign the exact body bytes:\n{native}"
        );
        assert!(
            native.contains(".header(\n                    \"X-Webhook-Signature\","),
            "native leg must set the signature header:\n{native}"
        );
        // wasm leg signs before the Fetch request.
        let wasm_idx = dispatcher
            .find("#[cfg(target_arch = \"wasm32\")]")
            .expect("wasm leg must exist");
        let wasm = &dispatcher[wasm_idx..];
        assert!(
            wasm.contains("event_signature(secret, timestamp, &body)"),
            "wasm leg must sign the exact body bytes:\n{wasm}"
        );
        assert!(
            wasm.contains("headers.set(\n                    \"X-Webhook-Signature\","),
            "wasm leg must set the signature header:\n{wasm}"
        );
    }
}

// ── HMAC known-answer vector (issue #518, the #511 mandate) ──

/// Extract the source span from `start_marker` through the closing brace
/// of the enclosing function (brace-counted, provider-independent).
fn extract_fn_span(source: &str, start_marker: &str) -> String {
    let start = source
        .find(start_marker)
        .unwrap_or_else(|| panic!("dispatcher must carry `{start_marker}`"));
    let open = start
        + source[start..]
            .find('{')
            .unwrap_or_else(|| panic!("`{start_marker}` must open a body"));
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    for (i, b) in bytes[open..].iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return source[start..=open + i].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("`{start_marker}` body never closes");
}

/// Issue #518 (the #511 mandate): the signing wire construction is a
/// CONTRACT — `"{timestamp}.{body}"` with the literal dot, the
/// `sha256=` hex prefix, the ±replay window — so the rendered
/// `event_signature` / `verify_event_signature` functions are extracted
/// VERBATIM from the dispatcher output, compiled into a scratch crate,
/// and executed against a known-answer vector pinned from an independent
/// HMAC-SHA256 implementation (python stdlib), plus the verify round-trip
/// and the replay-window rejections. The existing containment pin above
/// asserts the calls exist; THIS pin asserts they compute the right bytes.
#[test]
fn event_signature_computed_vector_and_verify_round_trip() {
    let dispatcher = render_dispatcher("sea_orm");

    // The replay-window const rides the contract (301s stale ⇒ rejected).
    let const_start = dispatcher
        .find("pub const EVENT_SIGNATURE_MAX_AGE_SECS")
        .expect("dispatcher must declare EVENT_SIGNATURE_MAX_AGE_SECS");
    let const_end = const_start
        + dispatcher[const_start..]
            .find(';')
            .expect("const declaration must terminate");
    let window_const = dispatcher[const_start..=const_end].to_string();
    let signature_fn = extract_fn_span(&dispatcher, "fn event_signature(");
    let verify_fn = extract_fn_span(&dispatcher, "pub fn verify_event_signature(");

    let scratch = tempfile::tempdir().unwrap();
    fs::write(
        scratch.path().join("Cargo.toml"),
        // Same crate pins the generated app declares (cargo_toml.tera).
        r#"[package]
name = "event-signature-kat"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
path = "kat.rs"

[dependencies]
hmac = "0.12"
sha2 = "0.10"
hex = "0.4"
"#,
    )
    .unwrap();

    let kat_source = r##"//! Known-answer vector for the dispatcher signing contract (issue #518).
//! The signing functions below are extracted VERBATIM from the rendered
//! `integration/dispatcher.tera` output.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

@WINDOW_CONST@

@SIGNATURE_FN@

@VERIFY_FN@

#[cfg(test)]
mod known_answer {
    use super::*;

    const SECRET: &str = "test-secret";
    const TS: i64 = 1_717_171_717;
    const BODY: &[u8] = br#"{"event":"recruiting.candidate.created","entity_id":"01933e2c-0000-7000-8000-000000000001"}"#;
    /// Pinned from an independent HMAC-SHA256 implementation:
    /// python3 hmac.new(b"test-secret", b"1717171717." + BODY, hashlib.sha256).
    const EXPECTED: &str = "sha256=225f3684afa5f13827f89e01ac4efcd299a6b4252c55e7d1bd28e6784acd045b";

    #[test]
    fn known_answer_vector() {
        assert_eq!(event_signature(SECRET, TS, BODY), EXPECTED);
    }

    #[test]
    fn verify_round_trip_accepts_fresh_and_boundary_ages() {
        let sig = event_signature(SECRET, TS, BODY);
        assert_eq!(verify_event_signature(SECRET, TS, BODY, &sig, TS), Ok(()));
        assert_eq!(
            verify_event_signature(SECRET, TS, BODY, &sig, TS + EVENT_SIGNATURE_MAX_AGE_SECS),
            Ok(())
        );
        assert_eq!(verify_event_signature(SECRET, TS, BODY, &sig, TS - 60), Ok(()));
    }

    #[test]
    fn verify_rejects_stale_future_tampered_and_malformed() {
        let sig = event_signature(SECRET, TS, BODY);
        // Stale beyond the 5-minute window.
        assert!(verify_event_signature(
            SECRET, TS, BODY, &sig, TS + EVENT_SIGNATURE_MAX_AGE_SECS + 1
        )
        .is_err());
        // More than 60s in the future.
        assert!(verify_event_signature(SECRET, TS, BODY, &sig, TS - 61).is_err());
        // Tampered body (the MAC covers the exact bytes).
        assert!(verify_event_signature(SECRET, TS, b"tampered", &sig, TS).is_err());
        // Wrong secret.
        assert!(verify_event_signature("other-secret", TS, BODY, &sig, TS).is_err());
        // Wrong timestamp (the timestamp is INSIDE the MAC).
        assert!(verify_event_signature(SECRET, TS + 1, BODY, &sig, TS + 1).is_err());
        // Missing the sha256= prefix.
        let bare = sig.trim_start_matches("sha256=");
        assert!(verify_event_signature(SECRET, TS, BODY, bare, TS).is_err());
        // Malformed hex.
        assert!(verify_event_signature(SECRET, TS, BODY, "sha256=zz", TS).is_err());
    }
}
"##
    .replace("@WINDOW_CONST@", &window_const)
    .replace("@SIGNATURE_FN@", &signature_fn)
    .replace("@VERIFY_FN@", &verify_fn);
    fs::write(scratch.path().join("kat.rs"), kat_source).unwrap();

    let out = Command::new("cargo")
        .arg("test")
        .arg("--manifest-path")
        .arg(scratch.path().join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", scratch.path().join("target"))
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("cargo test must run for the signing KAT scratch crate");
    assert!(
        out.status.success(),
        "the extracted signing functions must satisfy the known-answer vector:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// The installations table carries the nullable signing_secret column —
/// the documented back-compat shape (NULL ⇒ unsigned delivery).
#[test]
fn installation_table_carries_signing_secret() {
    let tera = codegraph::generate::template_engine::create_tera(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
    )
    .unwrap();
    let ctx: HashMap<String, String> = HashMap::new();
    let tables = codegraph::generate::render_template_with_project(
        &tera,
        "integration/tables.tera",
        &ctx,
        &test_project("sea_orm"),
    )
    .expect("tables.tera must render");
    assert!(
        tables.contains("signing_secret TEXT,"),
        "installations must carry the signing_secret column:\n{tables}"
    );
    assert!(
        tables.contains("CREATE TABLE platform_integrations.installations"),
        "tables must declare the installations table:\n{tables}"
    );
}

/// The POST payload is the documented contract: event namespaced by the
/// full `domain.table` entity_type, plus the four identity fields.
#[test]
fn dispatcher_payload_shape_is_the_documented_contract() {
    let dispatcher = render_dispatcher("sea_orm");
    for fragment in [
        "serde_json::json!({",
        "format!(\"{}.{}\", entity_type, event)",
        "\"entity_id\": entity_id.to_string()",
        "\"org_id\": org_id.to_string()",
        "\"installation_id\": installation_id.to_string()",
        "\"correlation_id\": correlation_id.to_string()",
    ] {
        assert!(
            dispatcher.contains(fragment),
            "dispatcher payload must carry `{fragment}`:\n{dispatcher}"
        );
    }
}

/// The dispatcher must be constructible in EVERY topology the call sites
/// fire in: cornucopia via the emitted `ClientSource`/`GenericClient`
/// handle (never a SeaORM handle there), and the wasm32 worker slice via
/// `worker::Fetch` (reqwest does not link wasm32).
#[test]
fn dispatcher_supports_cornucopia_and_wasm_legs() {
    let cornucopia = render_dispatcher("cornucopia");
    assert!(
        cornucopia.contains("ClientSource"),
        "cornucopia dispatcher must take a ClientSource handle:\n{cornucopia}"
    );
    assert!(
        !cornucopia.contains("DatabaseConnection"),
        "cornucopia dispatcher must not reference a SeaORM handle:\n{cornucopia}"
    );

    for dispatcher in [render_dispatcher("sea_orm"), cornucopia] {
        assert!(
            dispatcher.contains("#[cfg(not(target_arch = \"wasm32\"))]"),
            "native HTTP leg must be reqwest behind the native cfg:\n{dispatcher}"
        );
        assert!(
            dispatcher.contains("reqwest"),
            "native HTTP leg must use reqwest:\n{dispatcher}"
        );
        assert!(
            dispatcher.contains("#[cfg(target_arch = \"wasm32\")]"),
            "wasm HTTP leg must be cfg-gated:\n{dispatcher}"
        );
        assert!(
            dispatcher.contains("worker::Fetch"),
            "wasm HTTP leg must use worker::Fetch:\n{dispatcher}"
        );
    }
}
