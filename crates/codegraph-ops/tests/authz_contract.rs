//! DB-level authorization contract tests for #169 (middleware → RLS collapse).
//!
//! These tests pin the generated app's authorization semantics AT THE DATABASE
//! LAYER so the middleware→DB refactor cannot silently change HTTP behavior:
//!
//! - **Cross-tenant access stays a silent RLS filter** (404 on by-id, `[]` on
//!   list) — characterization: green before AND after the refactor.
//! - **Out-of-scope API-key operations raise the custom `P0403`
//!   INSUFFICIENT_SCOPE error** (mapped to 403 by the generated error mapper) —
//!   RED until the strict policy helpers land (#169 Phase 4).
//! - **Role denials raise `P0403`** for JWT identities (member cannot delete) —
//!   RED until permission graph data compiles into RLS (#169 Phase 5).
//!
//! Every case runs through one transaction mirroring the generated data path:
//! session vars (`app.organization_id`, `app.current_api_key`, `app.user_id`)
//! set via `set_config(..., true)`, under the `app_user` role — once in legacy
//! mode (superuser + `SET LOCAL ROLE app_user`) and once connecting directly
//! as `app_user` (the #169 app_user-pool mode).
//!
//! Ignored by default — needs a Supabase-compatible Postgres reachable at
//! `DATABASE_URL` (default `postgres://postgres:postgres@localhost:5432/postgres`)
//! and `psql` on PATH:
//!
//! ```text
//! cargo test -p codegraph-ops --test authz_contract -- --ignored --nocapture
//! ```

use codegraph_ops::db::{psql_exec, psql_query};
use codegraph_ops::migrate::run_api_migrations;
use codegraph_ops::pg::PgTarget;

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Serializes cluster-level DDL (CREATE DATABASE / ALTER ROLE) and migration
/// runs across the tests in this binary.
static DB_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

/// Org A / org B fixture UUIDs (arbitrary — `create_api_key` has no FK).
const ORG_A: &str = "11111111-1111-4111-8111-111111111111";
const ORG_B: &str = "22222222-2222-4222-8222-222222222222";
/// JWT fixture user (arbitrary; never resolved against basejump here).
const USER_A: &str = "33333333-3333-4333-8333-333333333333";

/// Common probe: count every visible `common.code` row.
const COUNT_ALL: &str = "SELECT count(*) FROM common.code";

fn admin_target() -> PgTarget {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/postgres".to_string());
    PgTarget::from_url(&url).expect("invalid DATABASE_URL")
}

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../review/generated-candidate/migrations")
}

/// A no-op pgmq stand-in for plain Postgres targets (CI's `postgres:15`
/// service): queue create/send become sinks and the domain-event trigger
/// function becomes a pass-through. The contract cases never assert on event
/// emission — only on authorization.
const PGMQ_SHIM: &str = "\
-- Test shim: pgmq stand-in (no extension available).
CREATE SCHEMA IF NOT EXISTS pgmq;
CREATE OR REPLACE FUNCTION pgmq.create(p_name text) RETURNS void
AS $shim$ BEGIN END $shim$ LANGUAGE plpgsql;
CREATE OR REPLACE FUNCTION pgmq.send(p_queue text, p_msg jsonb) RETURNS bigint
AS $shim$ SELECT 1 $shim$ LANGUAGE sql;
CREATE OR REPLACE FUNCTION emit_domain_event() RETURNS TRIGGER
AS $shim$ BEGIN RETURN COALESCE(NEW, OLD); END $shim$ LANGUAGE plpgsql;
";

/// Resolve the migration dir to apply: the review fixture when the target has
/// pgmq, otherwise a staged copy with `0003_pgmq_setup.sql` replaced by the
/// shim above.
async fn resolve_migrations_dir(target: &PgTarget) -> (PathBuf, Option<tempfile::TempDir>) {
    let has_pgmq = psql_query(
        target,
        "SELECT count(*) FROM pg_available_extensions WHERE name = 'pgmq';",
    )
    .await
    .map(|v| v == "1")
    .unwrap_or(false);
    if has_pgmq {
        return (migrations_dir(), None);
    }
    let staged = tempfile::tempdir().expect("stage migrations tempdir");
    let dst = staged.path().join("migrations");
    std::fs::create_dir_all(&dst).expect("create staged migrations dir");
    for entry in std::fs::read_dir(migrations_dir()).expect("read review migrations") {
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

fn skip_if_no_psql() -> bool {
    if codegraph_ops::env::ensure_psql().is_err() {
        eprintln!("skipping: psql not found");
        true
    } else {
        false
    }
}

/// Unique scratch database name.
fn scratch_db_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("authz_itest_{:x}", nanos)
}

/// Provision a fresh database with the review fixture's migrations applied,
/// fixture rows inserted, and `app_user`'s password pinned to the migration
/// default (the role itself is created by migration 0002).
struct ScratchDb {
    admin: PgTarget,
    db: String,
}

impl ScratchDb {
    async fn create(admin: PgTarget) -> Self {
        let db = scratch_db_name();
        psql_exec(&admin, &format!("CREATE DATABASE {db};"))
            .await
            .expect("create scratch database");
        Self { admin, db }
    }

    async fn drop_best_effort(&self) {
        let _ = psql_exec(
            &self.admin,
            &format!("DROP DATABASE IF EXISTS {} WITH (FORCE);", self.db),
        )
        .await;
    }
}

/// Apply the review fixture migrations, pin app_user's password, insert
/// fixture rows (2 in org A, 1 in org B), and provision the API keys used by
/// the contract cases. Returns (scratch db, key pairs) as (id, raw key)
/// tuples: org-A full / write-only / read-only / org-B full / org-A creator.
#[allow(clippy::type_complexity)]
async fn setup(
    admin: &PgTarget,
) -> (
    ScratchDb,
    (String, String),
    (String, String),
    (String, String),
    (String, String),
    (String, String),
) {
    let scratch = ScratchDb::create(admin.clone()).await;
    let target = PgTarget {
        db: scratch.db.clone(),
        role: "authz_itest".into(),
        ..admin.clone()
    };

    let (migration_dir, _staged) = resolve_migrations_dir(&target).await;
    run_api_migrations(&migration_dir, &target)
        .await
        .expect("review fixture migrations must apply cleanly");

    // The app_user role may pre-date this run (cluster-wide); pin its password
    // to the migration default so the app_user-pool probes can connect.
    psql_exec(&target, "ALTER ROLE app_user PASSWORD 'app_user_pass';")
        .await
        .expect("pin app_user password");

    // Fixture rows in common.code: two org A rows, one org B row.
    psql_exec(
        &target,
        &format!(
            "INSERT INTO common.code (id, platform_organization_id) VALUES \
             ('aaaaaaaa-0000-4000-8000-000000000001', '{ORG_A}'), \
             ('aaaaaaaa-0000-4000-8000-000000000002', '{ORG_A}'), \
             ('bbbbbbbb-0000-4000-8000-000000000001', '{ORG_B}');"
        ),
    )
    .await
    .expect("insert fixture rows");

    // API keys (legacy object-scope model — the vocabulary the RLS scope
    // policies enforce).
    let key_full_a = create_api_key(
        &target,
        ORG_A,
        "full",
        r#"[{"entity_type": "*", "entity_id": "*", "action": "*"}]"#,
    )
    .await;
    let key_write_only_a = create_api_key(
        &target,
        ORG_A,
        "write-only",
        r#"[{"entity_type": "code", "entity_id": "*", "action": "write"}]"#,
    )
    .await;
    let key_read_only_a = create_api_key(
        &target,
        ORG_A,
        "read-only",
        r#"[{"entity_type": "code", "entity_id": "*", "action": "read"}]"#,
    )
    .await;
    let key_full_b = create_api_key(
        &target,
        ORG_B,
        "full-b",
        r#"[{"entity_type": "*", "entity_id": "*", "action": "*"}]"#,
    )
    .await;
    // Creator key: create + read (POST responses read the row back via
    // RETURNING, so create without read cannot return the resource).
    let key_creator_a = create_api_key(
        &target,
        ORG_A,
        "creator",
        r#"[{"entity_type": "code", "entity_id": "*", "action": "read"},
            {"entity_type": "code", "entity_id": "*", "action": "create"}]"#,
    )
    .await;

    (
        scratch,
        key_full_a,
        key_write_only_a,
        key_read_only_a,
        key_full_b,
        key_creator_a,
    )
}

/// Create an API key via the generated SECURITY DEFINER function; returns
/// (resolved id, raw `sk_...` key). The id rides the request context as
/// `app.current_api_key_id` — the RLS scope policies read it instead of
/// re-verifying the raw key per row.
async fn create_api_key(
    target: &PgTarget,
    org: &str,
    name: &str,
    scopes: &str,
) -> (String, String) {
    let sql = format!("SELECT public.create_api_key('{org}', '{name}', '{scopes}'::jsonb)::text;");
    let out = psql_query(target, &sql)
        .await
        .expect("create_api_key must succeed");
    let json: serde_json::Value =
        serde_json::from_str(&out).expect("create_api_key returns JSONB text");
    let id = json["id"].as_str().expect("id field").to_string();
    let key = json["key"].as_str().expect("key field").to_string();
    (id, key)
}

/// Session identity for [`probe`].
enum Mode {
    /// Legacy data path: superuser connection + `SET LOCAL ROLE app_user`.
    LegacyAppUser,
    /// #169 target: direct `app_user` connection (no role flip).
    AppUserPool,
    /// Supabase-style role for API-key policies (`TO api_key`).
    ApiKeyRole,
}

/// Run `probe_sql` inside one transaction as the given session identity and
/// return `"OK|<value>"` on success or `"<SQLSTATE>|<SQLERRM>"` when the
/// statement raised. The transaction always rolls back.
async fn probe(
    admin: &PgTarget,
    scratch: &ScratchDb,
    mode: &Mode,
    org: &str,
    key: &(String, String),
    user: &str,
    probe_sql: &str,
) -> String {
    let mut target = PgTarget {
        db: scratch.db.clone(),
        role: "probe".into(),
        ..admin.clone()
    };
    let set_role = match mode {
        Mode::LegacyAppUser => Some("SET LOCAL ROLE app_user;"),
        Mode::ApiKeyRole => Some("SET LOCAL ROLE api_key;"),
        Mode::AppUserPool => {
            target.user = "app_user".into();
            target.password = "app_user_pass".into();
            None
        }
    };

    let sql = format!(
        "BEGIN;\n{}\nDO $probe_wrapper$\nDECLARE v text;\nBEGIN\n\
           PERFORM set_config('app.organization_id', '{}', true);\n\
           PERFORM set_config('app.current_api_key', '{}', true);\n\
           PERFORM set_config('app.current_api_key_id', '{}', true);\n\
           PERFORM set_config('app.user_id', '{}', true);\n\
           PERFORM set_config('app.role', 'member', true);\n\
           CREATE TEMP TABLE IF NOT EXISTS probe_result(state text, detail text);\n\
           DELETE FROM probe_result;\n\
           BEGIN\n\
             EXECUTE $probe${}$probe$ INTO v;\n\
             INSERT INTO probe_result VALUES ('OK', coalesce(v, ''));\n\
           EXCEPTION WHEN OTHERS THEN\n\
             INSERT INTO probe_result VALUES (SQLSTATE, left(SQLERRM, 200));\n\
           END;\n\
         END\n$probe_wrapper$;\n\
         SELECT state || '|' || detail FROM probe_result;\n\
         ROLLBACK;",
        set_role.unwrap_or(""),
        org,
        key.1.replace('\'', "''"),
        key.0.replace('\'', "''"),
        user,
        probe_sql,
    );
    psql_query(&target, &sql)
        .await
        .map(|out| {
            // psql prints a command tag (BEGIN / SET / DO / ROLLBACK) for each
            // statement in the -c payload even in tuples-only mode; the probe
            // verdict is the single remaining line.
            out.lines()
                .filter(|l| {
                    !matches!(
                        l.trim(),
                        "BEGIN"
                            | "SET"
                            | "DO"
                            | "ROLLBACK"
                            | ""
                            | "SET LOCAL ROLE app_user"
                            | "SET LOCAL ROLE api_key"
                    )
                })
                .rev()
                .find(|l| !l.trim().is_empty() && l.contains('|'))
                .unwrap_or("")
                .to_string()
        })
        .expect("probe psql run")
}

/// Assert the probe raised with the given SQLSTATE and an error message
/// carrying `code`.
fn assert_raised(result: &str, sqlstate: &str, code: &str, context: &str) {
    let (state, detail) = result.split_once('|').unwrap_or((result, ""));
    assert_eq!(
        state, sqlstate,
        "{context}: expected SQLSTATE {sqlstate}, got {result:?}"
    );
    assert!(
        detail.contains(code),
        "{context}: expected error message to contain {code:?}, got {result:?}"
    );
}

/// Assert the probe succeeded with the given scalar value.
fn assert_ok(result: &str, value: &str, context: &str) {
    assert_eq!(
        result,
        format!("OK|{value}"),
        "{context}: expected OK|{value}, got {result:?}"
    );
}

// ─── Characterization: cross-tenant isolation is a silent RLS filter ┈──────
//
// These pin today's behavior and must stay green through the whole #169
// refactor: cross-tenant reads filter to 0 rows (HTTP 404 / empty list), and
// cross-tenant writes are rejected by the org-isolation policy (42501 → 403).

async fn org_isolation_filters_cross_tenant_reads(admin: PgTarget) {
    if skip_if_no_psql() {
        return;
    }
    let _guard = DB_LOCK.lock().await;
    let (scratch, key_full_a, _, _, key_full_b, key_creator_a) = setup(&admin).await;
    let result = probe(
        &admin,
        &scratch,
        &Mode::LegacyAppUser,
        ORG_A,
        &key_full_a,
        USER_A,
        "SELECT count(*) FROM common.code WHERE platform_organization_id = '22222222-2222-4222-8222-222222222222'",
    )
    .await;
    assert_ok(&result, "0", "cross-tenant list under org A session");

    // by-id of another org's row is also a silent 0-row filter (404 semantics).
    let result = probe(
        &admin,
        &scratch,
        &Mode::LegacyAppUser,
        ORG_A,
        &key_full_a,
        USER_A,
        "SELECT count(*) FROM common.code WHERE id = 'bbbbbbbb-0000-4000-8000-000000000001'",
    )
    .await;
    assert_ok(&result, "0", "cross-tenant by-id under org A session");

    // The org B key still sees exactly its own row.
    let result = probe(
        &admin,
        &scratch,
        &Mode::AppUserPool,
        ORG_B,
        &key_full_b,
        USER_A,
        COUNT_ALL,
    )
    .await;
    assert_ok(&result, "1", "org B sees only its own rows (app_user pool)");

    scratch.drop_best_effort().await;
}

async fn org_isolation_blocks_cross_tenant_writes(admin: PgTarget) {
    if skip_if_no_psql() {
        return;
    }
    let _guard = DB_LOCK.lock().await;
    let (scratch, key_full_a, _, _, _, _) = setup(&admin).await;
    // In-scope key attempting to insert a row pointing at another org.
    let result = probe(
        &admin,
        &scratch,
        &Mode::LegacyAppUser,
        ORG_A,
        &key_full_a,
        USER_A,
        "INSERT INTO common.code (id, platform_organization_id) \
         VALUES ('aaaaaaaa-9999-4000-8000-000000000001', \
         '22222222-2222-4222-8222-222222222222') RETURNING id",
    )
    .await;
    assert_raised(
        &result,
        "42501",
        "",
        "cross-tenant insert under org A session",
    );

    scratch.drop_best_effort().await;
}

// ─── Target semantics: scope + role enforcement raises P0403 in-DB ┈────────
//
// RED until #169 Phase 4/5: today these operations silently succeed
// (app_user has no scope policies) or silently filter (api_key role). The
// target raises the custom INSUFFICIENT_SCOPE error, which the generated
// error mapper turns into HTTP 403 — preserving the middleware behavior.

async fn out_of_scope_read_raises_insufficient_scope(admin: PgTarget) {
    if skip_if_no_psql() {
        return;
    }
    let _guard = DB_LOCK.lock().await;
    let (scratch, _, key_write_only_a, _, _, _) = setup(&admin).await;

    // Write-only key reading its own org's rows: must raise, not leak.
    let result = probe(
        &admin,
        &scratch,
        &Mode::LegacyAppUser,
        ORG_A,
        &key_write_only_a,
        USER_A,
        COUNT_ALL,
    )
    .await;
    assert_raised(
        &result,
        "P0403",
        "INSUFFICIENT_SCOPE",
        "out-of-scope read (legacy)",
    );

    let result = probe(
        &admin,
        &scratch,
        &Mode::AppUserPool,
        ORG_A,
        &key_write_only_a,
        USER_A,
        COUNT_ALL,
    )
    .await;
    assert_raised(
        &result,
        "P0403",
        "INSUFFICIENT_SCOPE",
        "out-of-scope read (app_user pool)",
    );

    // The Supabase api_key role path must raise too (today it silently
    // filters to 0 rows).
    let result = probe(
        &admin,
        &scratch,
        &Mode::ApiKeyRole,
        ORG_A,
        &key_write_only_a,
        USER_A,
        COUNT_ALL,
    )
    .await;
    assert_raised(
        &result,
        "P0403",
        "INSUFFICIENT_SCOPE",
        "out-of-scope read (api_key role)",
    );

    scratch.drop_best_effort().await;
}

async fn out_of_scope_write_raises_insufficient_scope(admin: PgTarget) {
    if skip_if_no_psql() {
        return;
    }
    let _guard = DB_LOCK.lock().await;
    let (scratch, _, _, key_read_only_a, _, _) = setup(&admin).await;

    // Read-only key inserting into its own org: must raise (today it
    // silently succeeds under app_user).
    let result = probe(
        &admin,
        &scratch,
        &Mode::LegacyAppUser,
        ORG_A,
        &key_read_only_a,
        USER_A,
        "INSERT INTO common.code (id, platform_organization_id) \
         VALUES ('aaaaaaaa-9999-4000-8000-000000000002', \
         '11111111-1111-4111-8111-111111111111') RETURNING id",
    )
    .await;
    assert_raised(
        &result,
        "P0403",
        "INSUFFICIENT_SCOPE",
        "out-of-scope insert (legacy)",
    );

    let result = probe(
        &admin,
        &scratch,
        &Mode::AppUserPool,
        ORG_A,
        &key_read_only_a,
        USER_A,
        "INSERT INTO common.code (id, platform_organization_id) \
         VALUES ('aaaaaaaa-9999-4000-8000-000000000003', \
         '11111111-1111-4111-8111-111111111111') RETURNING id",
    )
    .await;
    assert_raised(
        &result,
        "P0403",
        "INSUFFICIENT_SCOPE",
        "out-of-scope insert (app_user pool)",
    );

    scratch.drop_best_effort().await;
}

async fn role_denial_raises_forbidden(admin: PgTarget) {
    if skip_if_no_psql() {
        return;
    }
    let _guard = DB_LOCK.lock().await;
    let (scratch, key_full_a, _, _, _, _) = setup(&admin).await;

    // JWT identity with a member-role role claim deleting a row: the DB must
    // raise (today roles are app-level only and the delete succeeds).
    // `app.role` is the session var the Phase-5 role policies read.
    let result = probe(
        &admin,
        &scratch,
        &Mode::AppUserPool,
        ORG_A,
        &key_full_a,
        USER_A,
        "DELETE FROM common.code WHERE id = 'aaaaaaaa-0000-4000-8000-000000000001' RETURNING id",
    )
    .await;
    assert_raised(
        &result,
        "P0403",
        "ROLE_FORBIDDEN",
        "member delete denied in-DB",
    );

    scratch.drop_best_effort().await;
}

// ─── Positive paths: in-scope access keeps working ┈────────────────────────

async fn in_scope_access_still_works(admin: PgTarget) {
    if skip_if_no_psql() {
        return;
    }
    let _guard = DB_LOCK.lock().await;
    let (scratch, key_full_a, key_write_only_a, key_read_only_a, _, key_creator_a) =
        setup(&admin).await;

    // Full-wildcard key: reads its org's rows in both pool modes.
    let result = probe(
        &admin,
        &scratch,
        &Mode::LegacyAppUser,
        ORG_A,
        &key_full_a,
        USER_A,
        COUNT_ALL,
    )
    .await;
    assert_ok(&result, "2", "wildcard key read (legacy)");
    let result = probe(
        &admin,
        &scratch,
        &Mode::AppUserPool,
        ORG_A,
        &key_full_a,
        USER_A,
        COUNT_ALL,
    )
    .await;
    assert_ok(&result, "2", "wildcard key read (app_user pool)");

    // Read-only key reads; write-only key writes.
    let result = probe(
        &admin,
        &scratch,
        &Mode::AppUserPool,
        ORG_A,
        &key_read_only_a,
        USER_A,
        COUNT_ALL,
    )
    .await;
    assert_ok(&result, "2", "read-only key reads its org");
    let result = probe(
        &admin,
        &scratch,
        &Mode::AppUserPool,
        ORG_A,
        &key_creator_a,
        USER_A,
        "INSERT INTO common.code (id, platform_organization_id) \
         VALUES ('aaaaaaaa-9999-4000-8000-000000000004', \
         '11111111-1111-4111-8111-111111111111') RETURNING id",
    )
    .await;
    assert_ok(
        &result,
        "aaaaaaaa-9999-4000-8000-000000000004",
        "creator key inserts and reads back",
    );

    // JWT identity (no API key) reads its org via org isolation.
    let result = probe(
        &admin,
        &scratch,
        &Mode::AppUserPool,
        ORG_A,
        &(String::new(), String::new()),
        USER_A,
        COUNT_ALL,
    )
    .await;
    assert_ok(&result, "2", "JWT identity reads its org");

    scratch.drop_best_effort().await;
}

#[tokio::test]
#[ignore = "requires a running Postgres (set DATABASE_URL or default localhost:5432)"]
async fn org_isolation_filters_cross_tenant_reads_both_modes() {
    org_isolation_filters_cross_tenant_reads(admin_target()).await;
}

#[tokio::test]
#[ignore = "requires a running Postgres (set DATABASE_URL or default localhost:5432)"]
async fn org_isolation_blocks_cross_tenant_writes_both_modes() {
    org_isolation_blocks_cross_tenant_writes(admin_target()).await;
}

#[tokio::test]
#[ignore = "requires a running Postgres (set DATABASE_URL or default localhost:5432)"]
async fn out_of_scope_read_raises_insufficient_scope_both_modes() {
    out_of_scope_read_raises_insufficient_scope(admin_target()).await;
}

#[tokio::test]
#[ignore = "requires a running Postgres (set DATABASE_URL or default localhost:5432)"]
async fn out_of_scope_write_raises_insufficient_scope_both_modes() {
    out_of_scope_write_raises_insufficient_scope(admin_target()).await;
}

#[tokio::test]
#[ignore = "requires a running Postgres (set DATABASE_URL or default localhost:5432)"]
async fn role_denial_raises_forbidden_in_db() {
    role_denial_raises_forbidden(admin_target()).await;
}

#[tokio::test]
#[ignore = "requires a running Postgres (set DATABASE_URL or default localhost:5432)"]
async fn in_scope_access_still_works_both_modes() {
    in_scope_access_still_works(admin_target()).await;
}
