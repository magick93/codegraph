//! End-to-end api-suite gate for the review fixture (#169 DB-level authz,
//! #463 generated hurl contracts).
//!
//! Proves — over real HTTP, through the ops harness (`testkit api`) — that
//! the generated server:
//!
//! 1. boots in **app_user pool mode** (`APP_DATABASE_URL`, the NOBYPASSRLS
//!    `app_user` role created by migration 0002) — asserted via the app log's
//!    `mode=AppUser` pool-mode line;
//! 2. ships its OWN hurl contract suite: the regenerated fixture must carry
//!    the `hurl_contract` generator's output (`hurl/01_auth.hurl`,
//!    `03_scope_denial_403.hurl`, `04_cross_tenant_404.hurl`,
//!    `08_rls_isolation.hurl` plus the per-entity contracts) and a
//!    `[hurl]` manifest section with `limited_key = true` — the gate is RED
//!    until that generator lands (Phase 1 of #463);
//! 3. enforces the `scope_enforced_*` RLS policies over HTTP (read-only key
//!    write → 403 INSUFFICIENT_SCOPE, `hurl/03_scope_denial_403.hurl`);
//! 4. keeps cross-tenant access a silent RLS filter (org B GET of org A's row
//!    → 404, `hurl/04_cross_tenant_404.hurl`);
//! 5. still serves plain CRUD for a full-wildcard key and rejects anonymous
//!    requests with 401 (`hurl/01`, `10+`).
//!
//! The test runs the FULL `api` suite against the fixture
//! `crates/review/generated-candidate/` on the resolved base target's DEFAULT
//! database — the supabase container is used precisely because its default
//! database carries the `auth` bootstrap; we never create databases:
//! migration → server boot → hurl contracts → curl smoke → RLS checks →
//! graceful shutdown. Repeat runs apply ~0 files (`platform._migrations`,
//! #756) and the target accumulates demo rows; `supabase db reset` restores
//! it when wanted. The suite OWNS the migration stage now:
//! `stage_database` applies the band and writes `platform._migrations`, and
//! the app's boot migrator must apply NOTHING over it (issue #756) — asserted
//! from the app log. There is no test-side bookkeeping shim anymore.
//!
//! The base Postgres target is SHAPE B (issue #709/#764): the shared
//! `test_framework::postgres::resolve_base_target()` (DATABASE_URL when it is
//! reachable and can host per-run databases, else the pinned Supabase-image
//! container on 127.0.0.1:15432) must pass
//! `codegraph_ops::supabase::compatibility_gate` — `Run`. A plain Postgres
//! target ABORTS loudly: the RLS legs need the Supabase band (the `auth`
//! schema and the image's reserved roles), and a silent skip would make the
//! gate vacuous. The fixture's `[supabase]` manifest section + `supabase/`
//! project are the MANUAL full-stack fallback, not a gate dependency.
//!
//! Before the suite runs, the fixture is regenerated FROM CLEAN (issue #694:
//! the previous generation's files are swept, then the same pipeline as the
//! `grafeo_generated_code_compiles` gate runs, minus the gRPC and ops
//! generators so the checked-in manifest and hurl dir survive), under the
//! cross-process fixture lock shared with `entity_codegen_gate` and
//! `rules_guard_api_gate` — so the hurl contracts the suite runs are always
//! this run's, never an older generation's renumbered leftovers. The suite
//! runs against the COMMITTED manifest with the resolved base injected
//! through `CODEGRAPH_API_DATABASE_URL` (the #709 whole-block override) — no
//! temp manifest, no scratch database.
//!
//! Self-contained but slow: the first run builds the fixture app from cold
//! (~10-20 minutes); warm runs are a few minutes. Needs a Supabase-compatible
//! Postgres (or docker for the pinned fallback container), `psql` and `hurl`
//! on PATH. Skips gracefully when the TOOLS are missing; never when the
//! database cannot serve the band.
//!
//! ```text
//! cargo test -p codegraph --test review_api_suite -- --ignored --nocapture
//! ```

use std::path::Path;
use std::process::Command;

use codegraph_ops::db::psql_query;
use codegraph_ops::pg::PgTarget;
use codegraph_ops::supabase::{CompatibilityGate, compatibility_gate};
use test_framework::postgres::resolve_base_target;

#[path = "test_framework/mod.rs"]
mod test_framework;

/// Which environment variable the caller may use to point at the cluster.
const DATABASE_URL_ENV: &str = "DATABASE_URL";

#[path = "review_api_suite_regen.rs"]
mod review_api_suite_regen;

use review_api_suite_regen::fixture_dir;

/// Resolve the base Postgres target and HARD-assert it serves the Supabase
/// band (issue #708's shared predicate, shape B from issue #709). The RLS
/// legs depend on the image's `auth` schema and reserved roles, so a plain
/// `postgres:<n>` target is a lane misconfiguration — fail in seconds,
/// naming the pinned image, never skip.
async fn supabase_compatible_base() -> PgTarget {
    let base = resolve_base_target().await.unwrap_or_else(|e| {
        panic!(
            "review_api_suite could not resolve a Postgres base target: {e} \
             (issue #709: point DATABASE_URL at the supabase image container \
             whose default database carries the bootstrap).",
        )
    });
    match compatibility_gate(&base).await {
        CompatibilityGate::Run => {}
        CompatibilityGate::Skip { reason } | CompatibilityGate::Fail { reason } => {
            panic!(
                "review_api_suite requires a Supabase-compatible Postgres base target \
                 (shape B, issue #709/#764) at {}:{} — the RLS legs need the image's \
                 `auth` schema and reserved roles. {reason}",
                base.host, base.port
            );
        }
    }
    base
}

fn skip_if_no_tooling() -> bool {
    let psql = codegraph_ops::env::ensure_psql().is_ok();
    let hurl = Command::new("hurl")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !psql {
        eprintln!("skipping: psql not found");
    }
    if !hurl {
        eprintln!("skipping: hurl not found");
    }
    !(psql && hurl)
}

/// Regenerate the committed fixture under the shared cross-process gate
/// (issue #694) — the regeneration half of the gate.
async fn provision_fixture(gate: &review_api_suite_regen::FixtureGate) -> Result<(), String> {
    gate.provision().await
}

fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

/// The gate: regenerate the fixture, run the ops api suite against the
/// resolved base target's DEFAULT database, and assert the pool-mode + authz
/// outcomes.
#[tokio::test]
#[ignore = "requires Postgres + hurl + a (cold) cargo build of the fixture app"]
async fn review_fixture_api_suite_passes_in_app_pool_mode() {
    if skip_if_no_tooling() {
        return;
    }
    let base = supabase_compatible_base().await;
    // Fail in seconds with a clear message when the cluster is unreachable.
    if let Err(e) = psql_query(&base, "SELECT 1").await {
        panic!("Postgres not reachable at {}: {e}", base.url());
    }

    println!("regenerating fixture from current templates…");
    // Held for the whole test: the regeneration AND the `testkit api` build
    // read the same committed tree the other review-fixture gates rewrite.
    let gate = review_api_suite_regen::FixtureGate::acquire().expect("acquire fixture gate");
    provision_fixture(&gate)
        .await
        .expect("fixture regeneration must succeed");

    // ── generated hurl contract suite (#463 Phase 1) ──────────────────
    // The gate is RED until the `hurl_contract` generator lands: the
    // regenerated fixture must carry its output and the checked-in
    // manifest must provision the keys those files reference. Failing
    // here (seconds after regeneration) avoids wasting the slow fixture
    // build on a run that could never exercise the contracts.
    assert_generated_hurl_contracts(&fixture_dir(), &fixture_manifest());

    run_suite_on_base(&base).await;
}

/// The checked-in fixture manifest (the regeneration deliberately strips the
/// ops generator so this hand-committed file survives).
fn fixture_manifest() -> String {
    std::fs::read_to_string(fixture_dir().join("codegraph-ops.toml"))
        .expect("read fixture manifest")
}

/// The generated hurl contract inventory the regenerated fixture must carry
/// (#463): the four authn/authz convention files plus at least one per-entity
/// contract. Byte-set equality lives in `hurl_contract_tests`; this gate
/// asserts the fixture actually ships the suite (and would run it).
fn assert_generated_hurl_contracts(fixture: &Path, manifest: &str) {
    let hurl_dir = fixture.join("hurl");
    let mut files: Vec<String> = std::fs::read_dir(&hurl_dir)
        .unwrap_or_else(|e| {
            panic!(
                "the regenerated fixture must ship the generated hurl contract dir at {}: {e} \
                 (the `hurl_contract` generator has not landed — #463 Phase 1)",
                hurl_dir.display()
            )
        })
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "hurl"))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    files.sort();

    for required in [
        "01_auth.hurl",
        "03_scope_denial_403.hurl",
        "04_cross_tenant_404.hurl",
        "08_rls_isolation.hurl",
    ] {
        assert!(
            files.iter().any(|f| f == required),
            "the generated hurl suite must include {required}; got {files:?}"
        );
    }
    assert!(
        files.iter().any(|f| f.starts_with("10_")),
        "the generated hurl suite must include per-entity contracts (numbered from 10); \
         got {files:?}"
    );

    assert!(
        manifest.contains("[hurl]"),
        "the fixture manifest must carry the [hurl] section (dir/skip/org ids/limited_key) — \
         the ops-generator coupling of #463"
    );
    assert!(
        manifest.contains("limited_key = true"),
        "the fixture manifest must opt into limited-key provisioning: \
         03_scope_denial_403.hurl asserts with {{{{api_key_limited}}}}"
    );
}

/// Body of the gate once the fixture is provisioned — split out so the flow
/// reads top to bottom. The suite runs against the COMMITTED manifest with
/// the resolved base injected through `CODEGRAPH_API_DATABASE_URL` (the #709
/// whole-block override): no temp manifest, no scratch database. The target
/// accumulates demo rows across runs; `supabase db reset` restores it.
async fn run_suite_on_base(base: &PgTarget) {
    let fixture = fixture_dir();
    let manifest_path = fixture.join("codegraph-ops.toml");

    // Validate the manifest through the harness's own config loader before
    // spending a build on it, and learn the app log path from OpsConfig.
    let config = codegraph_ops::OpsConfig::load(&manifest_path)
        .map_err(|e| format!("fixture manifest invalid: {e}"))
        .unwrap();
    assert!(
        config
            .manifest
            .hurl
            .as_ref()
            .map(|h| h.limited_key)
            .unwrap_or(false),
        "the gate requires hurl.limited_key = true (read-only key for 403 tests)"
    );
    let log_file = config.log_file.clone();

    // The suite truncates the log on server spawn; remove any stale copy
    // anyway so the pool-mode / boot-migration assertions below can never
    // read an old run.
    let _ = std::fs::remove_file(&log_file);

    // ── the HONEST consumer path (issue #764 / #756) ──────────────────
    // No test-side migration shim: the suite's own stage 2 applies the band
    // (writing `platform._migrations`) and the app boots on it. The boot
    // migrator must therefore apply ZERO files — asserted from the app log
    // below. This is exactly what a production first boot does. The base
    // target reaches the suite through the #709 whole-block override, so the
    // committed manifest is never edited.
    println!(
        "running `testkit api` against {} (first run builds the fixture app — be patient)…",
        base.url()
    );
    let mut cmd = Command::new("cargo");
    cmd.args(["run", "-p", "testkit", "--release", "--", "--config"])
        .arg(&manifest_path)
        .arg("api")
        .current_dir(&fixture)
        .env(DATABASE_URL_ENV, base.url())
        .env(codegraph_ops::config::API_DATABASE_URL_ENV, base.url());
    // The gate MUST exercise pool mode: never let an outer APP_DATABASE_URL
    // disable the app_user provisioning / pool-mode export.
    cmd.env_remove("APP_DATABASE_URL");

    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn cargo: {e}"));
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // ── pool-mode proof (independent of the suite's exit status) ──────
    // The server logs `tracing::info!(mode = ?pool_mode, "database pool mode")`
    // as JSON: `{"fields":{"mode":"AppUser",...,"message":"database pool mode"}}`.
    // A MISSING log means the server never spawned — print the suite's own
    // output tail so the cause is in the panic, not in a swallowed variable.
    let app_log = match std::fs::read_to_string(&log_file) {
        Ok(log) => log,
        Err(e) => panic!(
            "read app log {}: {e} (the server never spawned — suite output tail:\n{}",
            log_file.display(),
            tail(&combined, 120)
        ),
    };
    let pool_mode_line = app_log
        .lines()
        .find(|l| l.contains("database pool mode"))
        .map(|l| l.to_string())
        .unwrap_or_default();
    assert!(
        pool_mode_line.contains("AppUser"),
        "the app log must record the AppUser pool mode (APP_DATABASE_URL serving pool), \
         got:\n{}",
        tail(&app_log, 30)
    );
    assert!(
        !app_log.contains("\"mode\":\"Legacy\""),
        "the app must not fall back to legacy pool mode when APP_DATABASE_URL is exported"
    );
    println!("pool-mode proof: {}", tail(&pool_mode_line, 1));

    // ── boot-migration proof (the #756 tracking table) ────────────────
    // The suite applied the band; boot must see `platform._migrations`, skip
    // every tracked file and apply NOTHING. A nonzero count here is the
    // double-apply regression the shim used to mask.
    let boot_line = app_log
        .lines()
        .find(|l| l.contains("boot-migrations:"))
        .map(|l| l.to_string())
        .unwrap_or_default();
    assert!(
        boot_line.contains("boot-migrations: 0 applied"),
        "the app's boot migrator must apply nothing over the suite-applied band \
         (issue #756: `platform._migrations` is the ONE tracking table), got:\n{}",
        tail(&app_log, 40)
    );
    println!("boot-migration proof: {}", boot_line.trim());

    if !output.status.success() {
        // Full hurl output lands in {fixture}/test-results/hurl/*.log; the
        // suite output tail plus the app log tail usually name the culprit.
        // Any failure is hard: the Forbidden → 403 mapping has shipped
        // (#169), so a failing scope-denial contract is a real authz
        // regression, never a transitional state.
        panic!(
            "testkit api failed ({})\n--- suite output tail ---\n{}\n--- app log tail ---\n{}",
            output.status,
            tail(&combined, 120),
            tail(&app_log, 30)
        );
    }

    println!("suite output tail:\n{}", tail(&combined, 40));
    println!("ALL GATE ASSERTIONS PASSED (pool mode + authz over HTTP)");
}
