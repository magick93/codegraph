//! Regression pins for issue #559 — IFML detail lowering + service-op
//! action controls must agree with the mounted HTTP routes.
//!
//! #559 was reported against crewbase's `ird` domain:
//!
//! 1. `view "TaxEventDetail"` with `type: detail` lowered to an EMPTY page
//!    (`type: detail` is the singular DSL spelling; every generator branch
//!    matches `details`).
//! 2. `view "FilingDetail"` declares TWO components but only the list
//!    lowered — the detail sibling was dropped with the same spelling bug.
//! 3. `on request -> action("RequestFiling")` produced no control and no
//!    HTTP route — the UI and API surface were both absent.
//!
//! The shipped fixes (ingest normalization in #564; action controls in
//! #564; service-op route mounting in #590) were only pinned piecemeal:
//! the page side in `codegraph-generate` unit tests, the route side in
//! `service_ops_tests`/`worker_wasm_compile_tests`. Nothing tied the two
//! ends of the convention together, so a dead button (a control posting a
//! route that was never mounted) could regress silently.
//!
//! These pins drive the real pipeline end to end:
//!
//! - a `type: detail` view renders its detail surface (not an empty page),
//! - a multi-component view renders BOTH declared components,
//! - the action control's `actions/{op}` URL matches a route the monolith
//!   router registers,
//! - the workers router registers the same service-op route.
//!
//! ```text
//! cargo test -p codegraph --test ifml_559_regression_tests
//! ```

use std::fs;
use std::path::{Path, PathBuf};

const CLAIMS_MOX: &str = r#"package claims

class TaxEventType {
    String subject
    String note
}

class FilingType {
    String period
    String category
}

class FilingLineType {
    String code
    String amount
}
"#;

const CLAIMS_DDD: &str = r#"import "claims.mox";

application Claims {
    base claims

    module claims {
        entity TaxEventType scaffold repository TaxEventRepository {
            findById;
            findAll;
            save;
            delete;
        }
        entity FilingType scaffold repository FilingRepository {
            findById;
            findAll;
            save;
            delete;
        }
        entity FilingLineType scaffold repository FilingLineRepository {
            findById;
            findAll;
            save;
            delete;
        }
        service ClaimsService {
            inject TaxEventRepository;
            inject FilingRepository;
            resolveTaxEvent => TaxEventRepository.delete;
            requestFiling => FilingRepository.save;
        }
    }
}
"#;

const CLAIMS_IFML: &str = r#"domain "claims" { schema "claims"; }

view "TaxEventDetail" {
    params { id: Uuid };
    label "Tax event";

    component "event" {
        type: detail;
        data: TaxEventType;
        fields: [subject, note];
    }
}

view "FilingDetail" {
    params { id: Uuid };
    label "Filing detail";

    component "filing" {
        type: detail;
        data: FilingType;
        fields: [period, category];

        on request -> action("RequestFiling");
    }

    component "lines" {
        type: list;
        data: FilingLineType;
        fields: [code, amount];
    }
}

action "RequestFiling" {
    target: FilingType;
}
"#;

const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]
api_version = "v1"

[domains.claims]
label = "Claims"
schema_dir = "claims"
postgres_schema = "claims"
entities = ["TaxEventType", "FilingType", "FilingLineType"]
"#;

struct Fixture {
    root: tempfile::TempDir,
    config: PathBuf,
    mox: PathBuf,
    ddd: PathBuf,
    ifml: PathBuf,
}

fn write_fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("domains.toml");
    let mox = root.path().join("claims.mox");
    let ddd = root.path().join("claims.ddd");
    let ifml = root.path().join("app.ifml");
    fs::write(&config, DOMAINS_TOML).unwrap();
    fs::write(&mox, CLAIMS_MOX).unwrap();
    fs::write(&ddd, CLAIMS_DDD).unwrap();
    fs::write(&ifml, CLAIMS_IFML).unwrap();
    Fixture {
        root,
        config,
        mox,
        ddd,
        ifml,
    }
}

fn workspace_profiles() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles.toml")
}

/// The no-plan (all generators) backward-compat run: mox + ddd + IFML with
/// the standalone `svelte/` IFML tree and the monolith `src/api/` surface.
async fn run_monolith(fixture: &Fixture, output: &Path) {
    let frameworks = vec!["svelte".to_string()];
    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &fixture.config,
        output,
        extension_points_path: None,
        integration_manifests: &[],
        profile_name: "default",
        variant: None,
        // Nonexistent path ⇒ the no-plan (all generators) backward-compat
        // path used by the mox/ddd pipeline pins.
        profiles_config_path: Some(PathBuf::from("definitely-absent-profiles.toml")),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: std::slice::from_ref(&fixture.ifml),
        openapi_files: &[],
        mox_files: std::slice::from_ref(&fixture.mox),
        rosetta_files: &[],
        ddd_files: std::slice::from_ref(&fixture.ddd),
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: &frameworks,
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await
    .expect("monolith generation over the #559 fixture must succeed");
}

/// The workers topology: the service-op routes must mount in the owning
/// worker crate exactly like the monolith router (issue #590 rides both
/// topologies).
async fn run_workers(fixture: &Fixture, output: &Path) {
    let profiles = workspace_profiles();
    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &fixture.config,
        output,
        extension_points_path: None,
        integration_manifests: &[],
        profile_name: "workers-cornucopia",
        variant: None,
        profiles_config_path: Some(profiles),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: std::slice::from_ref(&fixture.mox),
        rosetta_files: &[],
        ddd_files: std::slice::from_ref(&fixture.ddd),
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
    .expect("workers generation over the #559 fixture must succeed");
}

fn read(base: &Path, rel: &str) -> String {
    fs::read_to_string(base.join(rel))
        .unwrap_or_else(|e| panic!("expected {rel} to be emitted: {e}"))
}

#[tokio::test]
async fn detail_views_lower_to_a_rendered_surface_and_keep_sibling_components() {
    let fixture = write_fixture();
    let out = fixture.root.path().join("out");
    run_monolith(&fixture, &out).await;

    // 1. `type: detail` (singular) must render the detail surface — the
    //    pre-fix output was an empty page (49 lines of helpers, nothing
    //    rendered).
    let tax_page = read(&out, "svelte/src/routes/taxeventdetail/+page.svelte");
    assert!(
        tax_page.contains("data-testid=\"event-details\""),
        "the singular `type: detail` component must lower to the details \
         surface:\n{tax_page}"
    );
    assert!(
        tax_page.contains("data.item.subject") && tax_page.contains("data.item.note"),
        "the declared detail fields must bind to the loaded record:\n{tax_page}"
    );

    // 2. A view declaring TWO components must render BOTH — the detail
    //    sibling used to be dropped alongside the empty page.
    let filing_page = read(&out, "svelte/src/routes/filingdetail/+page.svelte");
    assert!(
        filing_page.contains("data-testid=\"filing-details\""),
        "the `filing` details component must render:\n{filing_page}"
    );
    assert!(
        filing_page.contains("data.item.period") && filing_page.contains("data.item.category"),
        "the filing detail fields must bind:\n{filing_page}"
    );
    assert!(
        filing_page.contains("data-testid=\"lines-table\""),
        "the sibling `lines` list component must render too:\n{filing_page}"
    );
}

#[tokio::test]
async fn action_control_url_matches_the_mounted_service_op_route() {
    let fixture = write_fixture();
    let out = fixture.root.path().join("out");
    run_monolith(&fixture, &out).await;

    let filing_page = read(&out, "svelte/src/routes/filingdetail/+page.svelte");
    assert!(
        filing_page.contains("data-testid=\"filing-action-request-filing\""),
        "the action event must render a named control:\n{filing_page}"
    );
    assert!(
        filing_page.contains("call_request_filing"),
        "the control must drive a generated handler:\n{filing_page}"
    );
    assert!(
        filing_page.contains("actions/request_filing"),
        "the handler must POST the conventional op route:\n{filing_page}"
    );

    // The monolith router must register exactly those actions — the two
    // ends of the #559 convention (a control posting an unmounted route is
    // a dead button).
    let router = read(&out, "src/api/claims/router.rs");
    for op in ["request_filing", "resolve_tax_event"] {
        assert!(
            router.contains(&format!("/actions/{op}\"")),
            "the router must mount the `{op}` action the UI posts:\n{router}"
        );
        assert!(
            router.contains(&format!("service_op_{op}")),
            "the route must resolve into the generated handler:\n{router}"
        );
    }
}

#[tokio::test]
async fn workers_router_mounts_the_same_service_op_routes() {
    let fixture = write_fixture();
    let out = fixture.root.path().join("workers-out");
    run_workers(&fixture, &out).await;

    let router = read(&out, "workers/claims/src/api/claims/router.rs");
    for op in ["request_filing", "resolve_tax_event"] {
        assert!(
            router.contains(&format!("/actions/{op}\"")),
            "the worker router must register the `{op}` action route:\n{router}"
        );
        assert!(
            router.contains(&format!("service_op_{op}")),
            "the worker route must resolve into the generated handler:\n{router}"
        );
    }
}
