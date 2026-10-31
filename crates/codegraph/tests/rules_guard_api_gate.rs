//! Live eligibility-rule endpoint-guard gate (issue #338).
//!
//! Proves — over real HTTP against the regenerated review fixture — that a
//! modeled `eligibility rule` blocks the gated operation:
//!
//! 1. the regenerated fixture carries the compiled guard (`rules.rs` over
//!    the real record type), the handler guard blocks (load-or-404 →
//!    evaluate → 403 on create/get/update/delete), and the hand-authored
//!    hurl exercise;
//! 2. an ineligible create (amount = 0) answers **403** and is COMPENSATED
//!    — no amount-0 row survives;
//! 3. an eligible create answers **201** and reads back through the read
//!    guard (200);
//! 4. the generated per-entity hurl contract still passes (the rule's
//!    minimal-body semantics stay eligible), and anonymous requests stay
//!    401.
//!
//! Self-contained: Postgres (`DATABASE_URL`), `hurl`, and a cargo build of
//! the fixture app. Skips gracefully when tools are missing. Runs the SAME
//! fixture regeneration as `review_api_suite` (shared
//! `review_api_suite_regen` module) — one recipe, no drift.
//!
//! ```text
//! cargo test -p codegraph --test rules_guard_api_gate -- --ignored --nocapture
//! ```

#[path = "review_api_suite_regen.rs"]
mod review_api_suite_regen;

#[path = "test_framework/mod.rs"]
mod test_framework;

use std::path::{Path, PathBuf};
use std::process::Command;

use test_framework::axum_server;
use test_framework::postgres::{GateDb, base_target, database_reachable};

fn fixture_dir() -> PathBuf {
    review_api_suite_regen::fixture_dir()
}

fn logs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/rules-guard-gate/logs")
}

fn gate_target_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/rules-guard-gate/target")
}

fn skip(msg: &str) {
    println!("skipping: {msg}");
}

fn have_tool(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The eligibility-guard surface the regenerated fixture must carry
/// (generation-time pins, seconds after regen — before any build spend).
fn assert_guard_surface(fixture: &Path) {
    let rules_rs = std::fs::read_to_string(fixture.join("src/domain/recruiting/rules.rs"))
        .expect("the fixture must ship src/domain/recruiting/rules.rs (rosetta rules generator)");
    assert!(
        rules_rs.contains("pub fn positive_grant(input: &GrantApplicationResponse) -> bool {"),
        "the eligibility rule must compile to a record-typed guard fn:\n{rules_rs}"
    );
    assert!(
        rules_rs.contains(
            "use crate::domain::recruiting::grant_application::dto_response::GrantApplicationResponse;"
        ),
        "the guard's record type must be imported from the dto_response module:\n{rules_rs}"
    );

    let handler_rs = std::fs::read_to_string(
        fixture.join("src/api/recruiting/grant_application_handler.rs"),
    )
    .expect("the fixture must ship the guarded grant_application handler");
    let guard = "crate::domain::recruiting::rules::positive_grant";
    assert_eq!(
        handler_rs.matches(&format!("if !{guard}(&existing) {{")).count(),
        2,
        "update AND delete must load-or-404 then evaluate the guard:\n{handler_rs}"
    );
    assert_eq!(
        handler_rs.matches(&format!("if !{guard}(&response) {{")).count(),
        2,
        "get_by_id AND create must evaluate the guard over the loaded record:\n{handler_rs}"
    );
    assert!(
        handler_rs.matches("AppError::forbidden").count() >= 4,
        "the guard denial maps to 403:\n{handler_rs}"
    );
    assert!(
        handler_rs.contains("eligibility rule denied create — compensating"),
        "an ineligible create must be compensated (row removed):\n{handler_rs}"
    );

    assert!(
        fixture.join("hurl/90_eligibility_rule_403.hurl").is_file(),
        "the fixture must carry hurl/90_eligibility_rule_403.hurl"
    );
}

/// Run hurl over the given files against the booted server.
fn run_hurl(port: u16, api_key: &str, files: &[PathBuf]) -> Result<(), String> {
    let mut cmd = Command::new("hurl");
    cmd.arg("--test")
        .arg("--error-format")
        .arg("long")
        .arg("--variable")
        .arg(format!("base_url=http://127.0.0.1:{port}"))
        .arg("--variable")
        .arg(format!("api_key={api_key}"))
        .args(files);
    let out = cmd
        .output()
        .map_err(|e| format!("failed to spawn hurl: {e}"))?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if !out.status.success() {
        let tail: String = combined
            .lines()
            .rev()
            .take(60)
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!("hurl failed ({})\n{tail}", out.status));
    }
    println!("hurl: {}", combined.lines().last().unwrap_or(""));
    Ok(())
}

/// A no-op pgmq stand-in for plain Postgres targets (the
/// `authz_contract` precedent): queue create/send become sinks. Staged
/// over `0003_pgmq_setup.sql` when the cluster lacks the pgmq extension.
const PGMQ_SHIM: &str = "\
-- Test shim: pgmq stand-in (no extension available).
CREATE SCHEMA IF NOT EXISTS pgmq;
CREATE OR REPLACE FUNCTION pgmq.create(p_name text) RETURNS void
AS $shim$ BEGIN END $shim$ LANGUAGE plpgsql;
CREATE OR REPLACE FUNCTION pgmq.send(p_queue text, p_msg jsonb) RETURNS bigint
AS $shim$ SELECT 1 $shim$ LANGUAGE sql;
";

/// The fixture's migrations, with the pgmq setup replaced by the shim when
/// the target cluster has no pgmq extension.
async fn resolve_migrations_dir(target: &codegraph_ops::pg::PgTarget) -> (PathBuf, Option<tempfile::TempDir>) {
    let migrations = fixture_dir().join("migrations");
    let has_pgmq = codegraph_ops::db::psql_query(
        target,
        "SELECT count(*) FROM pg_available_extensions WHERE name = 'pgmq';",
    )
    .await
    .map(|v| v == "1")
    .unwrap_or(false);
    if has_pgmq {
        return (migrations, None);
    }
    let staged = tempfile::tempdir().expect("stage migrations tempdir");
    let dst = staged.path().join("migrations");
    std::fs::create_dir_all(&dst).expect("create staged migrations dir");
    for entry in std::fs::read_dir(&migrations).expect("read review migrations") {
        let entry = entry.expect("read migration entry");
        let name = entry.file_name();
        let dst_path = dst.join(&name);
        if name.to_string_lossy() == "0003_pgmq_setup.sql" {
            std::fs::write(&dst_path, PGMQ_SHIM).expect("write pgmq shim");
        } else {
            std::fs::copy(entry.path(), &dst_path).expect("copy migration file");
        }
    }
    (dst, Some(staged))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live gate: needs Postgres, hurl, and a cargo build of the fixture app"]
async fn eligibility_rule_blocks_the_gated_operation_over_http() {
    if !have_tool("hurl") {
        skip("hurl not found");
        return;
    }
    let Some(base) = base_target() else {
        skip("no usable postgres (DATABASE_URL + docker fallback failed)");
        return;
    };
    if !database_reachable(&base).await {
        skip("postgres not reachable");
        return;
    }

    println!("regenerating fixture from current templates…");
    review_api_suite_regen::regenerate_fixture()
        .await
        .expect("fixture regeneration must succeed");
    assert_guard_surface(&fixture_dir());

    let db = GateDb::create(base).await.expect("scratch database");
    db.apply_prelude().await.expect("prelude");
    let db_target = codegraph_ops::pg::PgTarget::from_url(&db.url()).expect("scratch target");
    let (migrations_dir, _staged) = resolve_migrations_dir(&db_target).await;
    db.apply_migrations(&migrations_dir)
        .await
        .expect("migrations");
    let api_key = db.provision_api_key().await.expect("api key");

    let result = async {
        let bin = axum_server::build_app(
            &fixture_dir().join("Cargo.toml"),
            &fixture_dir(),
            &gate_target_dir(),
            &logs_dir(),
            "app",
        )?;
        let database_url = db.url();
        let server = axum_server::boot_server(
            &bin,
            &[
                ("DATABASE_URL", database_url.as_str()),
                ("SUPABASE_JWT_SECRET", "rules-gate-test-secret"),
                ("APP_NAME", "rules-guard-gate-app"),
            ],
            "rules-guard-gate-app",
            &logs_dir(),
        )
        .await?;
        println!("gate: server booted on 127.0.0.1:{}", server.port);

        // The hand-authored eligibility exercise: ineligible create → 403,
        // eligible create → 201, read guard lets the eligible record
        // through, and the guarded anonymous surface stays 401.
        let mut files = vec![fixture_dir().join("hurl/90_eligibility_rule_403.hurl")];
        files.push(fixture_dir().join("hurl/01_auth.hurl"));
        // The generated per-entity contract for the guarded entity must
        // still pass end-to-end (its minimal bodies stay eligible).
        let mut generated: Vec<PathBuf> = std::fs::read_dir(fixture_dir().join("hurl"))
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.ends_with("_recruiting_grant_application.hurl"))
                    .unwrap_or(false)
            })
            .collect();
        generated.sort();
        assert_eq!(
            generated.len(),
            1,
            "the guarded entity's generated contract must exist: {generated:?}"
        );
        files.push(generated.remove(0));

        run_hurl(server.port, &api_key, &files)?;

        // Compensation proof: the denied create left nothing SERVED behind
        // (the entity is auditable — delete is a soft delete, so the proof
        // counts live rows; the soft-deleted corpse is invisible to the
        // API) and the eligible create landed.
        let leftovers = codegraph_ops::db::psql_query(
            &db_url_target(&database_url),
            "SELECT count(*) FROM recruiting.grant_application \
             WHERE amount = 0 AND deleted_at IS NULL",
        )
        .await
        .map_err(|e| format!("compensation proof query failed: {e}"))?;
        assert_eq!(
            leftovers, "0",
            "an eligibility-denied create must be compensated (row removed), \
             found {leftovers} live amount-0 rows"
        );
        let eligible = codegraph_ops::db::psql_query(
            &db_url_target(&database_url),
            "SELECT count(*) FROM recruiting.grant_application \
             WHERE amount > 0 AND deleted_at IS NULL",
        )
        .await
        .map_err(|e| format!("eligible-rows query failed: {e}"))?;
        assert!(
            eligible != "0",
            "the eligible create must have landed (found {eligible} live amount>0 rows)"
        );
        Ok::<(), String>(())
    }
    .await;

    db.cleanup().await;
    result.unwrap();
}

/// A `PgTarget` for the scratch database parsed back out of its URL (the
/// psql helpers take a target, the GateDb owns the URL).
fn db_url_target(url: &str) -> codegraph_ops::pg::PgTarget {
    codegraph_ops::pg::PgTarget::from_url(url).expect("scratch url")
}
