//! The COMBINED OpenAPI document (`src/api/openapi/all.rs`) must publish
//! every dynamic-plane registration the per-domain documents publish
//! (issue #737 — the #704 class, one document over): HTTP-exposed modeled
//! functions (issue #337 B1) and service-op action endpoints (issue #590)
//! were annotated at the handler but registered by NO combined document,
//! so the spec served at the API root silently omitted both planes.
//!
//! Pins (node-free PR-CI):
//! - `combined_openapi_tracks_the_emitted_function_and_service_op_modules`
//!   — the #704 DRIFT-pin shape (`media_pipeline_tests.rs`): the expected
//!   registrations derive from the EMITTED modules (`functions_api.rs` +
//!   every `*_handler.rs`), and the combined document must register
//!   exactly that surface. A handler or function rename desyncs and
//!   fails; a hand-list cannot.
//! - the combined and per-domain documents agree on both planes.
//! - `flag_off_document_is_byte_identical_to_the_prefix_template` — a
//!   model with NO functions and NO service ops renders byte-equal to the
//!   PRE-FIX template (`fixtures/openapi_all_prefix/api/openapi_all.tera`,
//!   the committed pre-#737 bytes).
//!
//! ```text
//! cargo test -p codegraph --test openapi_combined_registration_tests
//! ```

#[path = "service_ops_fixture.rs"]
mod fixture;

mod helpers;

use std::fs;
use std::path::{Path, PathBuf};

use codegraph::driver::{RunArgs, run};
use codegraph::generate::traits::GlobalGenerator;
use codegraph::generate::{GenerationEntry, ProjectConfig};
use codegraph_core::traits::GraphQuerier;

use fixture::{DOMAINS_TOML, PAY_RUN_MOX, PAYROLL_DDD};

/// The payroll fixture's domains.toml PLUS one HTTP-exposed modeled
/// function binding (issue #337 B1). The func is modeled in the sibling
/// rosetta file (sigil funcs are the function plane's model source).
fn domains_toml_with_functions() -> String {
    format!("{DOMAINS_TOML}\n[functions.\"FeeQuote\"]\nexposure = \"http\"\n")
}

/// One fully-transpilable func with primitive inputs, in the payroll
/// namespace (the last segment matches the fixture's `payroll` domain, so
/// the FunctionNode lands in that domain and the functions plane emits
/// `src/api/payroll/functions_api.rs`).
const FUNCS_ROSETTA: &str = r#"
namespace test.payroll

func FeeQuote: <"quote a fee for a run">
	inputs:
		note string (1..1)
		total number (1..1)
	output:
		result number (1..1)

	set result:
		total
"#;

/// The fixture profiles: the mox service-op planes (handler/router carry
/// the action endpoints) plus the functions plane (issue #337 B1) and the
/// openapi publisher — exactly the planes the combined document must
/// register.
const PROFILES_TOML: &str = r#"
[profiles.default.meta]
name = "openapi-all-registration"
version = "1.0.0"
description = "issue #737 combined-document registration fixture"

[profiles.default.features]
rosetta_backend = true

[profiles.default.api]
generators = [
    "ddl", "sea_orm_entity", "codelist", "dto", "repository", "command",
    "query", "handler", "router", "functions", "errors", "openapi",
]
"#;

/// Write the mox + `.ddd` + rosetta-funcs fixture into `dir` and run the
/// FULL driver over it (ingestion passes + validation + generation in the
/// production order), returning the generated root.
async fn run_fixture(dir: &Path) -> PathBuf {
    let schemas_dir = dir.join("model-schemas");
    fs::create_dir_all(&schemas_dir).expect("schemas dir");
    let mox = dir.join("pay_run.mox");
    fs::write(&mox, PAY_RUN_MOX).expect("mox");
    let ddd = dir.join("payroll.ddd");
    fs::write(&ddd, PAYROLL_DDD).expect("ddd");
    let rosetta = dir.join("payroll_funcs.rosetta");
    fs::write(&rosetta, FUNCS_ROSETTA).expect("rosetta funcs");
    let config = dir.join("domains.toml");
    fs::write(&config, domains_toml_with_functions()).expect("domains.toml");
    let profiles = dir.join("profiles.toml");
    fs::write(&profiles, PROFILES_TOML).expect("profiles.toml");
    let output = dir.join("generated");

    run(RunArgs {
        schemas: None,
        classifier: None,
        config_path: &config,
        output: &output,
        extension_points_path: None,
        integration_manifests: &[],
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(profiles),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &[mox],
        rosetta_files: &[rosetta],
        ddd_files: &[ddd],
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await
    .expect("the #737 fixture pipeline succeeds");
    output
}

// ── Registration parsing (the #704 drift-pin helpers) ──────────────────

/// The registrations inside a `paths(...)` / `components(schemas(...))`
/// block, read POSITIONALLY between the opener and the first closing-paren
/// line — so a pin asserts what the derive actually publishes instead of
/// grepping the whole file (where a comment or a DTO name would match).
fn registrations_after(source: &str, opener: &str) -> Vec<String> {
    let start = source
        .find(opener)
        .unwrap_or_else(|| panic!("`{opener}` missing from: {source}"))
        + opener.len();
    let rest = &source[start..];
    let mut registrations = Vec::new();
    for line in rest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(')') {
            return registrations;
        }
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        registrations.push(trimmed.trim_end_matches(',').to_string());
    }
    panic!("the `{opener}` block never closes: {rest}");
}

fn openapi_paths(source: &str) -> Vec<String> {
    registrations_after(source, "paths(")
}

fn openapi_schemas(source: &str) -> Vec<String> {
    registrations_after(source, "components(schemas(")
}

/// The `pub async fn` names a module ACTUALLY defines — the expectation
/// source for the drift pin, so neither list is an independent hardcode.
fn emitted_async_fns(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| line.trim().strip_prefix("pub async fn "))
        .map(|rest| {
            rest.split(|c: char| c == '(' || c == '<' || c.is_whitespace())
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .collect()
}

/// The `pub struct` names a module ACTUALLY defines.
fn emitted_structs(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| line.trim().strip_prefix("pub struct "))
        .map(|rest| {
            rest.split(|c: char| c == '{' || c.is_whitespace())
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .collect()
}

// ── Pins ───────────────────────────────────────────────────────────────

/// The combined document must register exactly the functions and
/// service-op endpoints the emitted modules expose — derived from those
/// modules, never a hand-list (#704 drift-pin shape).
#[tokio::test]
async fn combined_openapi_tracks_the_emitted_function_and_service_op_modules() {
    let dir = tempfile::tempdir().expect("fixture dir");
    let output = run_fixture(dir.path()).await;

    let domain_dir = output.join("src/api/payroll");
    let functions_api =
        fs::read_to_string(domain_dir.join("functions_api.rs")).expect("functions_api.rs");

    // Expected FUNCTION paths: every handler fn the emitted functions_api
    // module defines, addressed the way the document must address it.
    let mut expected_fn_paths: Vec<String> = emitted_async_fns(&functions_api)
        .into_iter()
        .map(|fn_name| format!("crate::api::payroll::functions_api::{fn_name}"))
        .collect();
    // Expected FUNCTION schemas: the {Fn}Request / {Fn}Response structs the
    // module derives ToSchema on.
    let mut expected_fn_schemas: Vec<String> = emitted_structs(&functions_api)
        .into_iter()
        .filter(|name| name.ends_with("Request") || name.ends_with("Response"))
        .map(|name| format!("crate::api::payroll::functions_api::{name}"))
        .collect();
    // Expected SERVICE-OP paths: every `pub async fn service_op_*` across
    // the domain's emitted handler modules.
    let mut expected_op_paths: Vec<String> = Vec::new();
    for entry in fs::read_dir(&domain_dir).expect("domain api dir") {
        let path = entry.expect("dir entry").path();
        let is_handler = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("_handler.rs"));
        if !is_handler {
            continue;
        }
        let content = fs::read_to_string(&path).expect("handler source");
        let stem = path
            .file_stem()
            .expect("handler file stem")
            .to_string_lossy()
            .into_owned();
        for fn_name in emitted_async_fns(&content) {
            if fn_name.starts_with("service_op_") {
                expected_op_paths.push(format!("crate::api::payroll::{stem}::{fn_name}"));
            }
        }
    }
    expected_fn_paths.sort();
    expected_fn_schemas.sort();
    expected_op_paths.sort();
    assert!(
        !expected_fn_paths.is_empty(),
        "the fixture must expose at least one HTTP function"
    );
    assert!(
        !expected_op_paths.is_empty(),
        "the fixture must expose at least one service op"
    );

    let all = fs::read_to_string(output.join("src/api/openapi/all.rs")).expect("all.rs");
    let per_domain =
        fs::read_to_string(output.join("src/api/openapi/payroll.rs")).expect("payroll.rs");

    let mut combined_fn_paths: Vec<String> = openapi_paths(&all)
        .into_iter()
        .filter(|p| p.contains("::functions_api::"))
        .collect();
    let mut combined_op_paths: Vec<String> = openapi_paths(&all)
        .into_iter()
        .filter(|p| p.contains("_handler::service_op_"))
        .collect();
    let mut combined_fn_schemas: Vec<String> = openapi_schemas(&all)
        .into_iter()
        .filter(|p| p.contains("::functions_api::"))
        .collect();
    combined_fn_paths.sort();
    combined_op_paths.sort();
    combined_fn_schemas.sort();

    assert_eq!(
        combined_fn_paths, expected_fn_paths,
        "all.rs must register exactly the function paths functions_api.rs exposes"
    );
    assert_eq!(
        combined_op_paths, expected_op_paths,
        "all.rs must register exactly the service-op paths the handlers expose"
    );
    assert_eq!(
        combined_fn_schemas, expected_fn_schemas,
        "all.rs must register exactly the function request/response schemas"
    );

    // The combined document agrees with the per-domain document on both
    // planes — the registration contexts share one source, so the two
    // documents cannot drift.
    let mut per_domain_fn_paths: Vec<String> = openapi_paths(&per_domain)
        .into_iter()
        .filter(|p| p.contains("::functions_api::"))
        .collect();
    let mut per_domain_op_paths: Vec<String> = openapi_paths(&per_domain)
        .into_iter()
        .filter(|p| p.contains("_handler::service_op_"))
        .collect();
    let mut per_domain_fn_schemas: Vec<String> = openapi_schemas(&per_domain)
        .into_iter()
        .filter(|p| p.contains("::functions_api::"))
        .collect();
    per_domain_fn_paths.sort();
    per_domain_op_paths.sort();
    per_domain_fn_schemas.sort();
    assert_eq!(
        combined_fn_paths, per_domain_fn_paths,
        "all.rs and payroll.rs must register the same function paths"
    );
    assert_eq!(
        combined_op_paths, per_domain_op_paths,
        "all.rs and payroll.rs must register the same service-op paths"
    );
    assert_eq!(
        combined_fn_schemas, per_domain_fn_schemas,
        "all.rs and payroll.rs must register the same function schemas"
    );
}

/// Flag-off byte identity: a model with NO functions and NO service ops
/// renders byte-equal to the committed PRE-FIX template. Any flag-off
/// movement (a stray newline included) fails this.
#[tokio::test]
async fn flag_off_document_is_byte_identical_to_the_prefix_template() {
    let engine = helpers::mock_engine_with_candidate();
    let config = helpers::domain_config();
    let generation_order = vec![GenerationEntry {
        schema_title: "CandidateType".to_string(),
        domain: "recruiting".to_string(),
        pg_schema: "recruiting".to_string(),
        is_cyclic: false,
    }];

    let prefix_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/openapi_all_prefix");
    let prefix_tera =
        codegraph_generate::template_engine::create_tera_with_overrides(&[prefix_dir.as_path()])
            .expect("prefix tera");
    let fixed_tera =
        codegraph_generate::template_engine::create_tera_with_overrides(&[]).expect("fixed tera");

    let with_prefix = render_all(&engine, &config, &generation_order, &prefix_tera).await;
    let with_fixed = render_all(&engine, &config, &generation_order, &fixed_tera).await;
    assert_eq!(
        with_prefix, with_fixed,
        "flag-off all.rs moved against the pre-#737 template — re-bless the \
         openapi_all_prefix fixture with the enumerated diff, or fix the \
         template so the empty loops render nothing"
    );
}

/// Render `src/api/openapi/all.rs` for one engine/config and return its
/// bytes.
async fn render_all(
    engine: &dyn GraphQuerier,
    config: &codegraph_config::DomainConfig,
    generation_order: &[GenerationEntry],
    tera: &tera::Tera,
) -> String {
    let out = tempfile::tempdir().expect("out dir");
    let generator = codegraph_generate::api::openapi::OpenApiGenerator::new(out.path());
    let files = generator
        .generate(
            engine,
            config,
            generation_order,
            tera,
            &ProjectConfig::default(),
        )
        .await
        .expect("openapi generation");
    files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("all.rs"))
        .map(|f| f.content.clone())
        .expect("all.rs in openapi output")
}
