//! WS2 workers live gate: a Supabase-ONLY workers-topology live gate.
//!
//! The product decision this gate pins: the workers topology is exercised
//! over a REAL local Supabase stack (plain-Postgres is out of scope for the
//! workers suite everywhere — no pgmq/basejump shims). The fixture
//! (`tests/fixtures/workers_gate/`) is a consumer-style workspace: 10 mox
//! domains → the `workers-cornucopia`-shaped `gate-workers` profile →
//! per-domain worker crates + a gateway, plus the generated `testkit`
//! harness crate.
//!
//! Two layers (the #463 testing discipline):
//!
//! 1. **Node-free PR-CI pins** — `workers_gate_fixture_manifest_is_complete`
//!    (and friends): the checked-in fixture must be complete and coherent —
//!    the `[workers]` plan names exactly the fixture's 10 domains (THE trap:
//!    an empty `[workers].domains` leaks the harness's historical hr-specs
//!    list into the run), the manifest targets the fixture's dedicated
//!    Supabase postgres, hurl skips are fixture-specific only, the profile
//!    resolves through `BuildPlan` (workers ⇒ cornucopia is a hard rule),
//!    and the hand-authored workflow contract really pins the 409 edges.
//! 2. **The live gate** (ignored; nightly lane): regenerate → migrate into
//!    a scratch database inside the fixture Supabase postgres → build the
//!    11-crate workers workspace → boot gateway + 10 workers → run the
//!    generated hurl contracts through the gateway, asserting a GENUINE mix
//!    of 200/201 (per-entity CRUD), 409 (workflow wrong-state), and 501
//!    (the `.ddd` service-op seam).
//!
//! ```text
//! cargo test -p codegraph --test workers_live_gate                          # node-free
//! cargo test -p codegraph --test workers_live_gate -- --ignored --nocapture # live
//! ```

use std::path::{Path, PathBuf};

use codegraph_ops::config::OpsConfig;
use codegraph_ops::pg::PgTarget;

/// The fixture workspace (consumer-style monorepo).
fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/workers_gate")
}

/// The fixture manifest (checked in; regeneration never wipes it).
fn fixture_manifest_path() -> PathBuf {
    fixture_dir().join("codegraph-ops.toml")
}

/// The 10 fixture domains — NO dashes (worker binaries/bindings assume it).
pub const EXPECTED_DOMAINS: [&str; 10] = [
    "billing",
    "common",
    "dispatch",
    "fleet",
    "inventory",
    "leave",
    "payroll2",
    "scheduling",
    "security",
    "vendor",
];

/// The fixture's dedicated Supabase ports (supabase/config.toml). The
/// foreign stacks at 54321-54334/54337 are never touched.
pub const SUPABASE_DB_PORT: u16 = 54340;
pub const SUPABASE_HEALTH_URL: &str = "http://localhost:54341/auth/v1/health";

/// hurl files the fixture manifest may skip — fixture-specific reasons
/// only. These two generated files reference keys the workers suite does
/// not provision (it passes exactly `base_url` + `api_key`; no
/// `api_key_limited` / `api_key_b`). The harness's DEFAULT_HURL_SKIPS
/// (08_rls_isolation, the hr-IRD/compliance set) must NOT be duplicated in
/// the fixture manifest — and no hr-specs file may leak in.
pub const FIXTURE_HURL_SKIPS: [&str; 2] = ["03_scope_denial_403.hurl", "04_cross_tenant_404.hurl"];

/// The harness's default workers hurl skips (workers.rs
/// `DEFAULT_HURL_SKIPS`) — none of these may appear in the fixture
/// manifest: they are harness-level, hr-specs-shaped, and duplicating them
/// in a fixture manifest is how consumer skips rot.
const HARNESS_DEFAULT_SKIPS: [&str; 10] = [
    "08_rls_isolation.hurl",
    "12_webhook_extensions.hurl",
    "10_compliance_rules.hurl",
    "11_compliance_check.hurl",
    "12b_ird_sync_review.hurl",
    "13_ird_rls_isolation.hurl",
    "14_ird_connection_lifecycle.hurl",
    "15_ird_employment_sync.hurl",
    "16_ird_ei_filing.hurl",
    "17_ird_events_inbox.hurl",
];

/// The RED/green boundary for the live gate: the node-free pins must all
/// hold before the slow path is worth running.
#[test]
fn workers_gate_fixture_manifest_is_complete() {
    let manifest = OpsConfig::load(&fixture_manifest_path())
        .unwrap_or_else(|e| {
            panic!("the fixture manifest must load (missing fixture = RED by design): {e}")
        })
        .manifest;

    // ── [workers]: the hr-domains leak trap ────────────────────────────
    let workers = manifest.workers.as_ref().expect(
        "the fixture manifest must carry a [workers] section — an absent section \
                 leaks the harness's 11 hr-specs domains into the run",
    );
    let mut domains = workers.domains.clone();
    domains.sort();
    let mut expected = EXPECTED_DOMAINS
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(
        domains, expected,
        "[workers].domains must name exactly the fixture's 10 domains \
         (non-empty — empty leaks the hr-specs domain list)"
    );
    for d in &workers.domains {
        assert!(!d.contains('-'), "worker bindings assume no dashes: {d}");
    }
    assert_eq!(
        workers.profile.as_deref(),
        Some("gate-workers"),
        "the workers plan must regenerate with the fixture's own profile (the \
         harness default names the repo's workers-cornucopia profile, which \
         the fixture profiles.toml does not define)"
    );
    assert_eq!(
        workers.binary_prefix.as_deref(),
        Some("gate"),
        "the workers binaries are gate-<domain> / gate-gateway"
    );
    assert_eq!(workers.gateway_port, Some(8787));
    assert_eq!(workers.worker_base_port, Some(3001));

    // ── [supabase]: the stack the suite boots (stage 0) ───────────────
    let supabase = manifest
        .supabase
        .as_ref()
        .expect("the workers suite requires [supabase].dir (supabase_ensure_up)");
    assert_eq!(
        supabase.dir,
        Path::new("supabase"),
        "[supabase].dir must name the fixture's own stack dir"
    );
    assert_eq!(
        supabase.health_url.as_deref(),
        Some(SUPABASE_HEALTH_URL),
        "the health probe must ride the fixture's dedicated ports, not the \
         default 54321 Kong (foreign stacks own those)"
    );

    // ── [database.api]: the fixture's dedicated Supabase postgres ─────
    assert_eq!(
        manifest.database.api.port, SUPABASE_DB_PORT,
        "[database.api].port must be the fixture supabase db port \
         (supabase/config.toml [db].port) — the migrations, the cornucopia \
         build and the worker boot all target it"
    );
    assert_eq!(manifest.database.api.host, "localhost");
    assert_eq!(manifest.database.api.database, "postgres");

    // ── [hurl]: fixture-specific skips only ───────────────────────────
    let hurl = manifest
        .hurl
        .as_ref()
        .expect("the fixture must configure [hurl] (the generated contract dir)");
    assert_eq!(
        hurl.dir,
        Path::new("generated-workers/hurl"),
        "the generated per-entity contracts live under generated-workers/hurl \
         and are what actually runs through the gateway"
    );
    let mut skips = hurl.skip.clone();
    skips.sort();
    let mut expected_skips = FIXTURE_HURL_SKIPS
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    expected_skips.sort();
    assert_eq!(
        skips, expected_skips,
        "the fixture skip list must be exactly the fixture-specific set"
    );
    for harness_skip in HARNESS_DEFAULT_SKIPS {
        assert!(
            !hurl.skip.iter().any(|s| s == harness_skip),
            "{harness_skip} is a harness-default skip (hr-specs-shaped) — the \
             fixture manifest must not duplicate it"
        );
    }

    // ── [smoke]: the curl smoke has its entity ────────────────────────
    let smoke = manifest
        .smoke
        .as_ref()
        .expect("the workers suite's gateway smoke needs [smoke].entity");
    assert_eq!(smoke.entity, "leave/leave-request");
    assert_eq!(
        smoke.route.as_deref(),
        Some("leave/leave-request"),
        "mox classes generate singular kebab routes"
    );
    assert!(
        smoke.create_body.contains("\"title\""),
        "the smoke create body must satisfy the LeaveRequestType create DTO \
         (the machine-managed status is DTO-excluded, so the body carries \
         the required scalar only)"
    );

    // ── model ↔ [workers] ↔ mox_files coherence ───────────────────────
    assert_eq!(
        manifest.mox_files.len(),
        EXPECTED_DOMAINS.len(),
        "one mox package per domain"
    );
    for domain in &workers.domains {
        let model = PathBuf::from(format!("model/{domain}.mox"));
        assert!(
            manifest.mox_files.iter().any(|m| m == &model),
            "mox_files must cover the {domain} model"
        );
        assert!(
            fixture_dir().join(&model).is_file(),
            "the {} model file must exist",
            model.display()
        );
    }

    // ── the hand-authored 409 contract pins a genuine mix ─────────────
    let workflow_hurl =
        std::fs::read_to_string(fixture_dir().join("hurl/90_workflow_transition_409.hurl"))
            .expect("the hand-authored workflow-409 contract must exist");
    assert!(
        workflow_hurl.contains("HTTP 201"),
        "the workflow contract starts from a CREATE (201)"
    );
    assert_eq!(
        workflow_hurl.match_indices("HTTP 200").count(),
        2,
        "exactly the two valid transitions pin 200"
    );
    assert!(
        workflow_hurl.match_indices("HTTP 409").count() >= 2,
        "at least the back-transition and the terminal-state replay pin 409 — \
         a wrong-state write must never be a silent 200"
    );
    assert!(
        workflow_hurl.contains("/actions/transition"),
        "the contract drives the generated workflow action endpoint"
    );

    // ── the .ddd design carries the 501 seam ──────────────────────────
    let design = std::fs::read_to_string(fixture_dir().join("design/leave.ddd"))
        .expect("the leave design must exist");
    assert!(
        design.contains("repository LeaveRequestRepository"),
        "the design binds the repository (builtins lower to the CRUD op set)"
    );
    for builtin in ["findById", "findAll", "save", "delete"] {
        assert!(
            design.contains(builtin),
            "repository builtin {builtin} present"
        );
    }
    assert!(
        design.contains("notify => LeaveRequestRepository.findAll;"),
        "the service must carry `notify` delegating to the declared `findAll` \
         builtin — the binding resolves through the model's repository while \
         `findAll` has no executable lowering, so the op lowers to the 501 \
         seam the generated actions contract pins (capability-class binding \
         needs an ingested `.actor` policy, which the workers run path does \
         not ingest; class-typed signatures don't bind — bind_entity reads a \
         top-level `name` the rex type JSON nests under `value`)"
    );
    let design_without_comments: String = design
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !design_without_comments.contains("machine "),
        "the design must not declare a state machine (bare-String status = \
         upstream compile error, rexlang Rule 16) — it lives in domains.toml"
    );
    let domains_toml =
        std::fs::read_to_string(fixture_dir().join("domains.toml")).expect("domains.toml");
    assert!(
        domains_toml.contains("generate_action_endpoints = true"),
        "the domains.toml workflow must emit the transition endpoints the 409 \
         contract drives"
    );
    assert!(
        domains_toml.contains("status_field = \"status\""),
        "the workflow binds the codelist-typed status field \
         (check_status_bypass stays quiet)"
    );

    // ── the fixture workspace detaches the generated output ───────────
    let workspace_cargo =
        std::fs::read_to_string(fixture_dir().join("Cargo.toml")).expect("fixture Cargo.toml");
    assert!(
        workspace_cargo.contains("exclude = [\"generated-workers\"]"),
        "generated-workers must be excluded so the generated testkit crate \
         builds standalone"
    );
}

/// The profile resolves through the same path the driver takes
/// (`BuildPlan::from_profile`) — the workers ⇒ cornucopia rule is a HARD
/// error, so a mis-configured fixture fails here in seconds instead of
/// minutes into the live run.
#[test]
fn workers_gate_profile_resolves_into_a_valid_build_plan() {
    let profiles = fixture_dir().join("profiles.toml");
    let registry = codegraph::profile::CapabilityRegistry::new();
    let resolved = codegraph::profile::load_and_resolve_profile(&profiles, "gate-workers", None)
        .expect("the gate-workers profile must resolve");
    let plan = codegraph::profile::BuildPlan::from_profile(&resolved, &registry)
        .expect("the gate-workers profile must build a valid plan (workers ⇒ cornucopia)");
    let generators = plan
        .entity_generators
        .iter()
        .chain(&plan.domain_generators)
        .chain(&plan.global_generators)
        .cloned()
        .collect::<Vec<String>>();
    for required in [
        "worker_scaffold",
        "cornucopia_queries",
        "hurl_contract",
        "ops",
        "workflow_action",
        "mox_ops",
    ] {
        assert!(
            generators.iter().any(|g| g == required),
            "the gate-workers plan must carry `{required}` (got {generators:?})"
        );
    }
}

// ── the live gate ─────────────────────────────────────────────────────

/// Which tools the live gate needs (all present in CI's nightly lane).
fn skip_if_no_tooling() -> bool {
    let mut missing = Vec::new();
    let have = |bin: &str, args: &[&str]| {
        std::process::Command::new(bin)
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    if !have("npx", &["supabase", "--version"]) {
        missing.push("npx supabase");
    }
    if !have("docker", &["info"]) {
        missing.push("docker");
    }
    if codegraph_ops::env::ensure_psql().is_err() {
        missing.push("psql");
    }
    if !have("hurl", &["--version"]) {
        missing.push("hurl");
    }
    if !missing.is_empty() {
        eprintln!("skipping: missing tools: {}", missing.join(", "));
        return true;
    }
    false
}

/// The fixture supabase admin target.
fn supabase_admin_target() -> PgTarget {
    PgTarget {
        host: "localhost".into(),
        port: SUPABASE_DB_PORT,
        user: "postgres".into(),
        password: "postgres".into(),
        db: "postgres".into(),
        role: "workers_gate".into(),
    }
}

/// Unique scratch database name for this run (inside the fixture's
/// Supabase postgres — the suite applies migrations into `[database.api]`
/// directly; there is no `supabase db reset` on this path).
fn scratch_db_name() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("workers_gate_{nanos:x}")
}

/// `curl -sf` probe of the fixture stack's GoTrue health endpoint.
async fn supabase_healthy() -> bool {
    std::process::Command::new("curl")
        .args(["-sf", "--max-time", "5"])
        .arg(SUPABASE_HEALTH_URL)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Ensure the fixture Supabase stack is up (the suite's stage-0 contract,
/// mirrored test-side: health probe first, `npx supabase start` when down).
/// The stack is left running (suite behavior); the foreign stacks on other
/// ports are never touched — the fixture project id scopes every container.
async fn ensure_supabase_up(fixture: &Path) -> Result<(), String> {
    if supabase_healthy().await {
        println!("fixture Supabase already running ({SUPABASE_HEALTH_URL})");
        return Ok(());
    }
    println!("starting fixture Supabase (npx supabase start; first run pulls GB of images)…");
    let out = std::process::Command::new("npx")
        .args(["supabase", "start"])
        .current_dir(fixture.join("supabase"))
        .output()
        .map_err(|e| format!("spawn npx supabase start: {e}"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if !out.status.success() {
        return Err(format!("npx supabase start failed:\n{}", tail(&text, 40)));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while std::time::Instant::now() < deadline {
        if supabase_healthy().await {
            println!("fixture Supabase healthy");
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Err(format!(
        "fixture Supabase did not become healthy; startup output tail:\n{}",
        tail(&text, 30)
    ))
}

/// Rewrite the `[database.api]` block of the fixture manifest to point at
/// the scratch database. Host/port/user/password ride the fixture's
/// dedicated Supabase postgres; everything else (workers plan, supabase
/// dir, smoke, hurl skips, hooks) is copied verbatim. The temp manifest
/// MUST live in the fixture dir: OpsConfig resolves the workers output
/// dir, hurl dir and hooks relative to the manifest's parent.
fn retarget_manifest_at_scratch(fixture_manifest: &str, scratch_db: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_api_block = false;
    for line in fixture_manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_api_block = trimmed == "[database.api]";
            out.push(line.to_string());
            continue;
        }
        if in_api_block
            && trimmed.starts_with("database")
            && trimmed
                .trim_start_matches("database")
                .trim_start()
                .starts_with('=')
        {
            out.push(format!("database = \"{scratch_db}\""));
            continue;
        }
        out.push(line.to_string());
    }
    let mut text = out.join("\n");
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

/// The entries the workers suite wipes before regenerating (workers.rs
/// `REGEN_WIPE` — mirrored here because the gate drives the regeneration
/// itself; see the skip-generate note on the live test).
const REGEN_WIPE: [&str; 8] = [
    "src",
    "workers",
    "migrations",
    "queries",
    "cornucopia-queries",
    "cli",
    "ui",
    "Cargo.toml",
];

/// Wipe the generated workers output (stage-2 parity), then regenerate it
/// from the current templates through the fixture's own wrapper binary —
/// exactly the command the suite's regenerate stage would run
/// (`cargo run --release -p {graph} -- run …`), but with the cwd on the
/// FIXTURE workspace: the ops harness resolves `workspace_root` to the
/// outermost enclosing cargo workspace, which inside this repository is
/// the codegraph repo itself, so the suite-driven `cargo run -p gate-graph`
/// would resolve against the wrong workspace. `--skip-generate` is passed
/// to the suite instead; the regeneration still happens (here), and the
/// post_generate hook that seeds the hand-authored 409 contract still
/// fires — because the gate runs it explicitly (the hook list rides the
/// temp manifest and `codegraph_ops::ext::run_hooks` is crate-private).
async fn regenerate_workers_output(config: &OpsConfig) -> Result<String, String> {
    let workers_out = config.root_dir.join(
        config
            .manifest
            .workers
            .as_ref()
            .and_then(|w| w.output_dir.clone())
            .unwrap_or_else(|| "generated-workers".into()),
    );
    for rel in REGEN_WIPE {
        let path = workers_out.join(rel);
        if path.is_dir() {
            std::fs::remove_dir_all(&path).map_err(|e| format!("wipe {}: {e}", path.display()))?;
        } else if path.is_file() {
            std::fs::remove_file(&path).map_err(|e| format!("wipe {}: {e}", path.display()))?;
        }
    }
    let mut cmd = std::process::Command::new("cargo");
    let mut args = vec![
        "run".to_string(),
        "--release".to_string(),
        "-p".to_string(),
        "gate-graph".to_string(),
        "--".to_string(),
        "run".to_string(),
    ];
    for file in &config.manifest.mox_files {
        args.push("--mox-files".to_string());
        args.push(config.root_dir.join(file).to_string_lossy().into_owned());
    }
    args.push("--ddd-files".to_string());
    args.push(
        config
            .root_dir
            .join("design/leave.ddd")
            .to_string_lossy()
            .into_owned(),
    );
    args.push("--config".to_string());
    args.push(
        config
            .root_dir
            .join(
                config
                    .manifest
                    .domain_config
                    .clone()
                    .unwrap_or_else(|| "domains.toml".into()),
            )
            .to_string_lossy()
            .into_owned(),
    );
    args.push("--profile".to_string());
    args.push("gate-workers".to_string());
    args.push("--output".to_string());
    args.push(workers_out.to_string_lossy().into_owned());
    cmd.args(&args).current_dir(&config.root_dir);
    let out = cmd
        .output()
        .map_err(|e| format!("spawn cargo (generate): {e}"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if !out.status.success() {
        return Err(format!(
            "workers regeneration failed — errors:\n{}",
            text.lines()
                .filter(|l| l.contains("error") || l.contains("Error"))
                .take(25)
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    // The post_generate hooks ride the temp manifest (the fixture's
    // hand-authored 409 contract is seeded into the generated hurl dir).
    codegraph_ops::ext::run_hooks(
        config,
        "post_generate",
        codegraph_ops::ext::HookPolicy::PerHook,
    )
    .await
    .map_err(|e| format!("post_generate hooks failed: {e}"))?;
    Ok(format!("regenerated {} files-ish", text.lines().count()))
}

/// First-ever run bootstrap: the generated `testkit` harness crate only
/// exists after a generation. A cold wipe of generated-workers (fresh
/// checkout) regenerates once from here; later runs reuse it (the ops
/// generator rewrites the crate identically on every regeneration).
async fn ensure_testkit(config: &OpsConfig) -> Result<(), String> {
    let testkit = config.root_dir.join("generated-workers/testkit/Cargo.toml");
    if !testkit.is_file() {
        println!(
            "bootstrap: generated-workers/testkit missing — generating once (cold driver build)…"
        );
        regenerate_workers_output(config).await?;
    }
    assert!(
        testkit.is_file(),
        "generation must emit the testkit crate (the `ops` generator is in the plan)"
    );
    ensure_testkit_standalone(&testkit)
}

/// Detach the generated testkit crate from every enclosing cargo workspace
/// (the review-gate precedent of patching generated manifests): the fixture
/// workspace excludes `generated-workers/`, but cargo keeps walking up past
/// an exclusion while an outer workspace exists — and inside this
/// repository the codegraph workspace sits above the fixture, so the
/// un-owned crate errors with "believes it's in a workspace when it's
/// not". An empty `[workspace]` table makes the crate its own workspace
/// root. Idempotent; the ops generator rewrites the manifest (without the
/// table) on every regeneration, so this runs before every testkit build.
fn ensure_testkit_standalone(testkit_manifest: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(testkit_manifest)
        .map_err(|e| format!("read {}: {e}", testkit_manifest.display()))?;
    if text.contains("[workspace]") {
        return Ok(());
    }
    let patched = format!(
        "{text}\n# Patched by the workers live gate: detach from enclosing workspaces.\n\
         [workspace]\n"
    );
    std::fs::write(testkit_manifest, patched)
        .map_err(|e| format!("patch {}: {e}", testkit_manifest.display()))
}

/// Asserted `HTTP <code>` statuses of a hurl source, in file order.
fn asserted_statuses(source: &str) -> Vec<u16> {
    source
        .lines()
        .filter_map(|l| {
            l.trim()
                .strip_prefix("HTTP ")
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|c| c.parse::<u16>().ok())
        })
        .collect()
}

/// A hurl file passed iff its test log carries the `Success` line (the
/// suite writes one log per run file under test-results/hurl/).
fn hurl_log_passed(log: &str) -> bool {
    log.lines().any(|l| l.trim().starts_with("Success "))
}

/// THE genuine-mix assertion: a passing hurl file proves its server
/// responses matched every asserted status, so the mix over the PASSED
/// files' asserted inventories is the mix the topology actually served.
fn assert_genuine_status_mix(fixture: &Path) -> Result<Vec<String>, String> {
    let logs_dir = fixture.join("test-results/hurl");
    let sources_dir = fixture.join("generated-workers/hurl");
    let mut passed: Vec<(String, Vec<u16>)> = Vec::new();
    let mut seen_logs = 0usize;
    let entries =
        std::fs::read_dir(&logs_dir).map_err(|e| format!("read {}: {e}", logs_dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("log") {
            continue;
        }
        seen_logs += 1;
        let log =
            std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        if !hurl_log_passed(&log) {
            return Err(format!(
                "hurl file failed: {} — log tail:\n{}",
                path.display(),
                tail(&log, 30)
            ));
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .trim_end_matches(".log")
            .to_string();
        let source = std::fs::read_to_string(sources_dir.join(&name))
            .map_err(|e| format!("read generated hurl source {name}: {e}"))?;
        passed.push((name, asserted_statuses(&source)));
    }
    if seen_logs == 0 {
        return Err(format!(
            "no hurl logs under {} — the suite never ran the contracts",
            logs_dir.display()
        ));
    }

    let mut inventory: Vec<u16> = passed.iter().flat_map(|(_, s)| s.iter().copied()).collect();
    inventory.sort_unstable();
    inventory.dedup();

    // 200/201 CRUD legs: at least one per-entity contract.
    let crud = passed.iter().any(|(name, statuses)| {
        !name.contains("_actions")
            && !name.starts_with("90_")
            && (statuses.contains(&200) || statuses.contains(&201))
    });
    // 409: the hand-authored workflow contract, both wrong-state edges.
    let workflow = passed
        .iter()
        .filter(|(name, _)| name == "90_workflow_transition_409.hurl")
        .find(|(_, statuses)| {
            statuses.iter().filter(|s| **s == 409).count() >= 2 && statuses.contains(&200)
        });
    // 501: the service-op seam in a generated actions contract.
    let seam = passed
        .iter()
        .any(|(name, statuses)| name.ends_with("_actions.hurl") && statuses.contains(&501));

    let mut report: Vec<String> = passed
        .iter()
        .map(|(name, statuses)| format!("{name}: {:?}", statuses))
        .collect();
    report.sort();

    if !crud {
        return Err(format!(
            "no passed per-entity contract with a 200/201 leg — inventories:\n{}",
            report.join("\n")
        ));
    }
    if workflow.is_none() {
        return Err(format!(
            "90_workflow_transition_409.hurl must pass with both its 409 edges \
             (back-transition + terminal replay) and its 200s — inventories:\n{}",
            report.join("\n")
        ));
    }
    if !seam {
        return Err(format!(
            "no passed *_actions.hurl with a 501 seam — the `.ddd` notify op \
             must lower to the 501 contract — inventories:\n{}",
            report.join("\n")
        ));
    }
    for expected in [409u16, 501] {
        assert!(
            inventory.contains(&expected),
            "the served-status inventory must contain {expected}: {inventory:?}"
        );
    }
    assert!(
        inventory.contains(&200) || inventory.contains(&201),
        "the served-status inventory must contain a 2xx create/read: {inventory:?}"
    );
    println!("hurl files passed (statuses per file):");
    for line in &report {
        println!("  {line}");
    }
    println!("served-status inventory: {inventory:?}");
    Ok(report)
}

/// The live gate: ensure the fixture stack → scratch database →
/// regenerate → run the harness's workers suite through the generated
/// testkit → assert results + the genuine status mix → drop the scratch db.
async fn run_live_gate() -> Result<(), String> {
    let fixture = fixture_dir();
    ensure_supabase_up(&fixture).await?;

    let admin = supabase_admin_target();
    codegraph_ops::db::psql_query(&admin, "SELECT 1")
        .await
        .map_err(|e| {
            format!(
                "fixture Supabase postgres not reachable at {}: {e}",
                admin.url()
            )
        })?;

    let scratch = scratch_db_name();
    codegraph_ops::db::psql_exec(&admin, &format!("CREATE DATABASE {scratch};"))
        .await
        .map_err(|e| format!("create scratch database: {e}"))?;
    println!("scratch database: {scratch}");

    let result = run_suite_on_scratch(&admin, &scratch).await;

    // Teardown: drop the scratch db (best effort); the stack stays up.
    let _ = codegraph_ops::db::psql_exec(
        &admin,
        &format!("DROP DATABASE IF EXISTS {scratch} WITH (FORCE);"),
    )
    .await;

    result
}

/// The gate body after the scratch database exists — split out so the
/// caller can drop the database no matter how this finishes.
async fn run_suite_on_scratch(_admin: &PgTarget, scratch: &str) -> Result<(), String> {
    let fixture = fixture_dir();
    let manifest_path = fixture_manifest_path();
    let fixture_manifest = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("read fixture manifest: {e}"))?;
    let retargeted = retarget_manifest_at_scratch(&fixture_manifest, scratch);

    let temp =
        tempfile::NamedTempFile::new_in(&fixture).map_err(|e| format!("temp manifest: {e}"))?;
    std::fs::write(temp.path(), &retargeted).map_err(|e| format!("write temp manifest: {e}"))?;

    // Validate through the harness's own config loader before any build.
    let config = OpsConfig::load(temp.path()).map_err(|e| format!("temp manifest invalid: {e}"))?;
    if config.api_db.db != scratch {
        return Err("the api db must be the scratch database".to_string());
    }

    // Regenerate (stage-2 parity, wipe first), then bootstrap the testkit
    // crate when a fresh checkout hasn't generated it yet.
    println!("regenerating generated-workers from the current templates…");
    regenerate_workers_output(&config).await?;
    ensure_testkit(&config).await?;

    // Fast pre-flight: the contract files the mix assertion depends on
    // must have been emitted — failing here (seconds) avoids spending the
    // 11-crate worker build on a run that could never pass.
    let generated_hurl = config.root_dir.join("generated-workers/hurl");
    let entries: Vec<String> = std::fs::read_dir(&generated_hurl)
        .map_err(|e| format!("generated hurl dir missing: {e}"))?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    if !entries.iter().any(|f| f.ends_with("_actions.hurl")) {
        return Err(format!(
            "the regenerated fixture must carry a service-op actions contract \
             (the `notify` 501 seam) — got {entries:?}"
        ));
    }
    if !entries
        .iter()
        .any(|f| f == "90_workflow_transition_409.hurl")
    {
        return Err(format!(
            "the hand-authored 90_workflow_transition_409.hurl must be seeded \
             into the generated hurl dir by the post_generate hook — got {entries:?}"
        ));
    }
    let entity_contracts = entries
        .iter()
        .filter(|f| {
            let stem = f.trim_end_matches(".hurl");
            stem.len() >= 3
                && stem.as_bytes()[0].is_ascii_digit()
                && stem.as_bytes()[1].is_ascii_digit()
                && !f.ends_with("_actions.hurl")
                && !stem.starts_with('0')
                && !stem.starts_with("90_")
        })
        .count();
    if entity_contracts < 10 {
        return Err(format!(
            "each of the 10 entities must ship a per-entity contract (got \
             {entity_contracts} of {entries:?})"
        ));
    }

    let results_path = std::env::temp_dir().join("workers-gate-results.json");
    let suite_log = std::env::temp_dir().join("workers-gate-suite.log");
    let _ = std::fs::remove_file(&results_path);
    let _ = std::fs::remove_file(&suite_log);

    println!(
        "running the generated testkit (`api --workers --skip-generate`) against {scratch} — \
         first run cold-builds 11 worker crates, be patient…"
    );
    let mut cmd = std::process::Command::new("cargo");
    cmd.args([
        "run",
        "--release",
        "--manifest-path",
        &config
            .root_dir
            .join("generated-workers/testkit/Cargo.toml")
            .to_string_lossy(),
        "--",
        "--config",
        &temp.path().to_string_lossy(),
        "api",
        "--workers",
        // The gate drives the regeneration itself (see
        // `regenerate_workers_output` for why the suite's own stage can't
        // resolve the wrapper workspace from inside this repository).
        "--skip-generate",
        "--results",
        &results_path.to_string_lossy(),
    ])
    .current_dir(&fixture);
    let output = cmd
        .output()
        .map_err(|e| format!("spawn cargo (testkit): {e}"))?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::write(&suite_log, &combined);

    // ── migration contract: the suite tolerates failures — the gate
    //    does NOT (a failed migration means a half-migrated schema).
    let migrations_line = combined
        .lines()
        .find(|l| l.contains("Migrations:") && l.contains("applied,"))
        .map(|l| l.to_string());
    if let Some(line) = &migrations_line {
        // Parse AFTER the `Migrations:` marker — the counter line carries
        // ANSI color codes whose digits must not pollute the split.
        let rest = &line[line.find("Migrations:").unwrap()..];
        let numbers: Vec<u64> = rest
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        if let Some(applied) = numbers.first()
            && *applied == 0
        {
            return Err(format!("the suite applied zero migrations (line: {line})"));
        }
        if let Some(failed) = numbers.get(1)
            && *failed != 0
        {
            return Err(format!(
                "migration failures must be 0 (line: {line}) — the workers \
                 suite tolerates them, this gate does not"
            ));
        }
    }
    if combined.contains("migration file(s) failed") {
        return Err(format!(
            "the suite reported tolerated migration failures — full log: {}",
            suite_log.display()
        ));
    }

    if !output.status.success() {
        let early = std::fs::read_to_string(&results_path).ok();
        return Err(format!(
            "testkit api --workers failed ({})\n--- results ---\n{}\n--- suite output tail ---\n{}",
            output.status,
            early.unwrap_or_default(),
            tail(&combined, 120)
        ));
    }

    // ── results contract ──────────────────────────────────────────────
    let results: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&results_path)
            .map_err(|e| format!("read results {}: {e}", results_path.display()))?,
    )
    .map_err(|e| format!("parse results: {e}"))?;
    if results["suite"] != "workers" {
        return Err(format!(
            "the results report must be the workers suite's: {results}"
        ));
    }
    let failed = results["failed"].as_u64().unwrap_or(u64::MAX);
    if failed != 0 {
        return Err(format!(
            "the workers suite must report zero failures:\n{}",
            serde_json::to_string_pretty(&results).unwrap_or_default()
        ));
    }
    if results["exit"] != 0 {
        return Err(format!("results exit must be 0: {results}"));
    }

    // ── the genuine status mix ────────────────────────────────────────
    let report = assert_genuine_status_mix(&fixture)?;

    println!("suite output tail:\n{}", tail(&combined, 30));
    println!("ALL GATE ASSERTIONS PASSED (workers topology over Supabase)");
    println!("status inventory: {}", report.len());
    Ok(())
}

/// The live gate (ignored; the nightly lane).
#[tokio::test]
#[ignore = "requires npx supabase + docker + psql + hurl and a (cold) build of the fixture workers topology"]
async fn workers_live_gate_supabase_stack_runs_the_full_workers_suite() {
    if skip_if_no_tooling() {
        return;
    }
    run_live_gate().await.unwrap_or_else(|err| panic!("{err}"));
}
