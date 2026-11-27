//! Real-Postgres integration tests for the ops harness db/migrate layer.
//!
//! All tests are `#[ignore]`d — they need a running Postgres reachable at
//! `DATABASE_URL` (default `postgres://postgres:postgres@localhost:5432/postgres`)
//! and `psql` on PATH (or `PSQL_PATH`). Run explicitly with:
//!
//! ```text
//! cargo test -p codegraph-ops --test db_integration -- --ignored --nocapture
//! ```

use codegraph_ops::db::{
    advisory_lock, advisory_unlock, missing_extensions, psql_exec, psql_exec_file, psql_query,
};
use codegraph_ops::env::ensure_psql;
use codegraph_ops::migrate::{clear_tracking, link_migrations_to_supabase, run_api_migrations};
use codegraph_ops::pg::PgTarget;

use std::fs;
use std::path::{Path, PathBuf};

/// Serializes tests that mutate the shared target database. The integration
/// tests create/drop schemas (and the basejump install + `run_api_migrations`
/// touch overlapping objects), so they must not run concurrently against the
/// same Postgres. (They still use the advisory lock inside `run_api_migrations`,
/// but raw `psql` phases in the parity test bypass it.)
static DB_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

fn target() -> PgTarget {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/postgres".to_string());
    PgTarget::from_url(&url).expect("invalid DATABASE_URL")
}

/// True when psql is unavailable — the test should return early instead of
/// failing so the suite passes trivially on machines without postgres tooling.
fn skip_if_no_psql() -> bool {
    if ensure_psql().is_err() {
        eprintln!("skipping: psql not found");
        true
    } else {
        false
    }
}

#[tokio::test]
#[ignore = "requires a running Postgres (set DATABASE_URL or default localhost:5432)"]
async fn psql_roundtrip() {
    if skip_if_no_psql() {
        return;
    }
    let _guard = DB_LOCK.lock().await;
    let t = target();

    let one = psql_query(&t, "SELECT 1;").await.expect("SELECT 1 failed");
    assert_eq!(one, "1");

    psql_exec(
        &t,
        "DROP SCHEMA IF EXISTS ops_itest_rt CASCADE; CREATE SCHEMA ops_itest_rt;",
    )
    .await
    .expect("schema setup failed");
    psql_exec(
        &t,
        "CREATE TABLE ops_itest_rt.t (id int); INSERT INTO ops_itest_rt.t VALUES (7);",
    )
    .await
    .expect("create/insert failed");
    assert_eq!(
        psql_query(&t, "SELECT count(*) FROM ops_itest_rt.t;")
            .await
            .unwrap(),
        "1"
    );
    assert_eq!(
        psql_query(&t, "SELECT id FROM ops_itest_rt.t;")
            .await
            .unwrap(),
        "7"
    );

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ops_itest_rt_insert.sql");
    std::fs::write(
        &file,
        "CREATE TABLE ops_itest_rt.from_file (v text);\nINSERT INTO ops_itest_rt.from_file VALUES ('hello');\n",
    )
    .unwrap();
    psql_exec_file(&t, &file).await.expect("psql -f failed");
    assert_eq!(
        psql_query(&t, "SELECT v FROM ops_itest_rt.from_file;")
            .await
            .unwrap(),
        "hello"
    );

    psql_exec(&t, "DROP SCHEMA ops_itest_rt CASCADE;")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires a running Postgres (set DATABASE_URL or default localhost:5432)"]
async fn migrations_apply_in_phases() {
    if skip_if_no_psql() {
        return;
    }
    let _guard = DB_LOCK.lock().await;
    let t = target();

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("0001_schema.sql"),
        "CREATE SCHEMA IF NOT EXISTS ops_itest;\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("0002_parent.sql"),
        "CREATE TABLE ops_itest.parent (id uuid PRIMARY KEY DEFAULT gen_random_uuid(), name TEXT);\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("0003_child.sql"),
        "CREATE TABLE ops_itest.child (id uuid PRIMARY KEY DEFAULT gen_random_uuid(), parent_id uuid, note TEXT);\nALTER TABLE ops_itest.child ADD CONSTRAINT fk_parent FOREIGN KEY (parent_id) REFERENCES ops_itest.parent(id);\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("0004_idx_rls.sql"),
        "ALTER TABLE ops_itest.parent ENABLE ROW LEVEL SECURITY;\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("0005_fk_trigger.sql"),
        "CREATE OR REPLACE FUNCTION ops_itest.child_default_note()\nRETURNS trigger AS $$\nBEGIN\n  NEW.note := COALESCE(NEW.note, 'auto');\n  RETURN NEW;\nEND;\n$$ LANGUAGE plpgsql;\n\nCREATE TRIGGER child_note_default\nBEFORE INSERT ON ops_itest.child\nFOR EACH ROW EXECUTE FUNCTION ops_itest.child_default_note();\n",
    )
    .unwrap();

    psql_exec(&t, "DROP SCHEMA IF EXISTS ops_itest CASCADE;")
        .await
        .expect("cleanup failed");

    // Issue #427: the tracking table would SKIP these fixed-name files on a
    // re-run against the shared target — clear it so the test always
    // exercises a fresh application (the reset semantic).
    clear_tracking(&t).await.expect("clear tracking failed");

    run_api_migrations(dir.path(), &t)
        .await
        .expect("migrations failed");

    let tables = psql_query(
        &t,
        "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'ops_itest' AND table_name IN ('parent', 'child');",
    )
    .await
    .unwrap();
    assert_eq!(tables, "2", "parent and child tables should exist");

    let fks = psql_query(
        &t,
        "SELECT count(*) FROM pg_constraint WHERE conname = 'fk_parent';",
    )
    .await
    .unwrap();
    assert_eq!(
        fks, "1",
        "FK constraint should have been applied in phase 2"
    );

    psql_exec(
        &t,
        "INSERT INTO ops_itest.parent (id, name) VALUES ('00000000-0000-0000-0000-000000000001', 'p1');",
    )
    .await
    .expect("parent insert failed");
    psql_exec(
        &t,
        "INSERT INTO ops_itest.child (id, parent_id, note) VALUES (gen_random_uuid(), '00000000-0000-0000-0000-000000000001', 'ok');",
    )
    .await
    .expect("child insert with valid FK failed");
    assert!(
        psql_exec(
            &t,
            "INSERT INTO ops_itest.child (id, parent_id, note) VALUES (gen_random_uuid(), '00000000-0000-0000-0000-00000000dead', 'bad');",
        )
        .await
        .is_err(),
        "child insert with dangling FK should fail"
    );

    let rls = psql_query(
        &t,
        "SELECT relrowsecurity FROM pg_class WHERE oid = 'ops_itest.parent'::regclass;",
    )
    .await
    .unwrap();
    assert_eq!(rls, "t", "RLS should be enabled on parent (phase 4)");

    let triggers = psql_query(
        &t,
        "SELECT count(*) FROM pg_trigger WHERE tgname = 'child_note_default' AND tgrelid = 'ops_itest.child'::regclass;",
    )
    .await
    .unwrap();
    assert_eq!(triggers, "1", "trigger should have been applied in phase 4");

    psql_exec(&t, "DROP SCHEMA ops_itest CASCADE;")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires a running Postgres (set DATABASE_URL or default localhost:5432)"]
async fn missing_extensions_reports_unavailable() {
    if skip_if_no_psql() {
        return;
    }
    let t = target();

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("0000_ext.sql"),
        "CREATE EXTENSION IF NOT EXISTS definitely_not_a_real_extension_xyz;\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("0001_real.sql"),
        "CREATE EXTENSION IF NOT EXISTS plpgsql;\n",
    )
    .unwrap();

    let missing = missing_extensions(dir.path(), &t)
        .await
        .expect("extension scan failed");
    assert!(
        missing
            .iter()
            .any(|name| name == "definitely_not_a_real_extension_xyz"),
        "expected unavailable extension to be reported, got: {missing:?}"
    );
    assert!(
        !missing.iter().any(|name| name == "plpgsql"),
        "installed extension should not be reported, got: {missing:?}"
    );
}

#[tokio::test]
#[ignore = "requires a running Postgres (set DATABASE_URL or default localhost:5432)"]
async fn advisory_locks_serialize() {
    if skip_if_no_psql() {
        return;
    }
    let t = target();

    advisory_lock(&t, 987654321)
        .await
        .expect("advisory lock failed");
    advisory_unlock(&t, 987654321)
        .await
        .expect("advisory unlock failed");
}

#[cfg(unix)]
#[test]
#[ignore = "requires a running Postgres (set DATABASE_URL or default localhost:5432)"]
fn link_migrations_creates_symlinks() {
    let mig = tempfile::tempdir().unwrap();
    std::fs::write(mig.path().join("a.sql"), "CREATE TABLE a ();").unwrap();
    std::fs::write(mig.path().join("b.sql"), "CREATE TABLE b ();").unwrap();

    let sup = tempfile::tempdir()
        .unwrap()
        .path()
        .join("supabase")
        .join("migrations");
    let count = link_migrations_to_supabase(mig.path(), &sup).unwrap();
    assert_eq!(count, 2);
    assert!(sup.join("a.sql").is_symlink());
    assert!(sup.join("b.sql").is_symlink());

    std::fs::remove_file(mig.path().join("b.sql")).unwrap();
    let count = link_migrations_to_supabase(mig.path(), &sup).unwrap();
    assert_eq!(count, 1, "stale link should be dropped after re-link");
    assert!(sup.join("a.sql").is_symlink());
    assert!(!sup.join("b.sql").exists());
}

/// Probe the target through the SHARED fail-closed gate
/// (`codegraph_ops::supabase`, issue #708): under
/// `CODEGRAPH_REQUIRE_SUPABASE=1` a missing capability is a FAIL naming
/// what is missing and the image; unset, the caller skips as before. One
/// predicate, three consumers — no private copy that could drift.
/// Returns false when the caller should skip (the reason is printed).
async fn band_gate(t: &PgTarget) -> bool {
    use codegraph_ops::supabase::{CompatibilityGate, compatibility_gate};
    match compatibility_gate(t).await {
        CompatibilityGate::Run => true,
        CompatibilityGate::Skip { reason } => {
            eprintln!("skipping: {reason}");
            false
        }
        CompatibilityGate::Fail { reason } => panic!("{reason}"),
    }
}

/// Write a representative full generated migration set (the same band the
/// `ddl`/`basejump_setup`/`pgmq_setup`/`scaffold` generators emit) into `dir`:
/// extensions, basejump install, api-key/org resolution, pgmq setup, rbac
/// roles, platform schema, plus one entity with an FK, an RLS policy and a
/// domain-event trigger.
fn write_full_generated_set(dir: &Path) {
    let basejump = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../codegraph-generate/static/basejump_core_2.0.1_install.sql");
    let basejump_sql = fs::read_to_string(&basejump)
        .unwrap_or_else(|_| panic!("cannot read basejump install at {}", basejump.display()));

    // Mirror the generator's `EXTENSIONS_SQL` (basejump_setup.rs): the
    // `extensions` schema comes first, so `CREATE EXTENSION … WITH SCHEMA
    // extensions` cannot fail silently (psql -f exits 0 on a statement error
    // unless ON_ERROR_STOP is set) and make the two application paths see a
    // different extension set.
    fs::write(
        dir.join("0000_extensions.sql"),
        "CREATE SCHEMA IF NOT EXISTS extensions;\nCREATE EXTENSION IF NOT EXISTS http WITH SCHEMA extensions;\nCREATE EXTENSION IF NOT EXISTS pg_tle;\nCREATE EXTENSION IF NOT EXISTS pgcrypto;\nCREATE EXTENSION IF NOT EXISTS \"uuid-ossp\";\n",
    )
    .unwrap();
    fs::write(dir.join("0001_basejump_install.sql"), basejump_sql).unwrap();
    fs::write(
        dir.join("0002_api_key_migration.sql"),
        "-- API keys + unified org resolution\nCREATE SCHEMA IF NOT EXISTS api_keys_private;\nCREATE TABLE IF NOT EXISTS api_keys_private.api_keys (\n  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),\n  organization_id uuid NOT NULL,\n  created_at timestamptz NOT NULL DEFAULT now()\n);\nALTER TABLE api_keys_private.api_keys ENABLE ROW LEVEL SECURITY;\nDO $$ BEGIN\n  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'app_user') THEN\n    CREATE ROLE app_user LOGIN PASSWORD 'x' NOBYPASSRLS;\n  END IF;\nEND $$;\nCREATE OR REPLACE FUNCTION public.get_current_org_id() RETURNS uuid AS $$\nBEGIN\n  RETURN nullif(current_setting('app.organization_id', true), '')::uuid;\nEND;\n$$ LANGUAGE plpgsql STABLE;\nGRANT USAGE ON SCHEMA basejump TO app_user;\nGRANT SELECT ON basejump.account_user TO app_user;\nGRANT SELECT ON basejump.accounts TO app_user;\nCREATE OR REPLACE FUNCTION public.resolve_user_org(p_user_id uuid)\nRETURNS uuid AS $$\n  SELECT au.account_id FROM basejump.account_user au WHERE au.user_id = p_user_id LIMIT 1;\n$$ LANGUAGE sql STABLE SECURITY DEFINER;\n-- Grant app_user DML on domain/platform schemas up front (CREATE SCHEMA +\n-- ALTER DEFAULT PRIVILEGES) so it applies regardless of application order.\nDO $$ DECLARE s TEXT; BEGIN\n  FOREACH s IN ARRAY ARRAY['demo','platform','platform_integrations'] LOOP\n    EXECUTE format('CREATE SCHEMA IF NOT EXISTS %I', s);\n    EXECUTE format('GRANT USAGE ON SCHEMA %I TO app_user', s);\n    EXECUTE format('GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA %I TO app_user', s);\n    EXECUTE format('ALTER DEFAULT PRIVILEGES IN SCHEMA %I GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO app_user', s);\n  END LOOP;\nEND $$;\n",
    )
    .unwrap();
    fs::write(
        dir.join("0003_pgmq_setup.sql"),
        "-- pgmq domain event infrastructure\nCREATE EXTENSION IF NOT EXISTS pgmq;\nSELECT pgmq.create('events_demo');\nCREATE OR REPLACE FUNCTION emit_domain_event() RETURNS TRIGGER AS $fn$\nBEGIN\n  RETURN COALESCE(NEW, OLD);\nEND;\n$fn$ LANGUAGE plpgsql;\n",
    )
    .unwrap();
    fs::write(
        dir.join("0004_rbac_roles.sql"),
        "DO $$\nBEGIN\n  IF NOT EXISTS (SELECT 1 FROM pg_enum WHERE enumtypid = 'basejump.account_role'::regtype AND enumlabel = 'manager') THEN\n    ALTER TYPE basejump.account_role ADD VALUE 'manager';\n  END IF;\n  IF NOT EXISTS (SELECT 1 FROM pg_enum WHERE enumtypid = 'basejump.account_role'::regtype AND enumlabel = 'employee') THEN\n    ALTER TYPE basejump.account_role ADD VALUE 'employee';\n  END IF;\nEND $$;\n",
    )
    .unwrap();
    fs::write(
        dir.join("0005_platform_schema.sql"),
        "CREATE SCHEMA IF NOT EXISTS platform;\nCREATE TABLE platform.webhook_endpoint (\n  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),\n  platform_organization_id uuid,\n  url text NOT NULL\n);\nGRANT USAGE ON SCHEMA pgmq TO app_user;\n",
    )
    .unwrap();
    fs::write(
        dir.join("0007_integration_tables.sql"),
        "CREATE SCHEMA IF NOT EXISTS platform_integrations;\nCREATE TABLE platform_integrations.extension_points (\n  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),\n  platform_organization_id uuid,\n  name text NOT NULL\n);\n",
    )
    .unwrap();
    fs::write(
        dir.join("0010_demo_widget.sql"),
        "CREATE SCHEMA IF NOT EXISTS demo;\nCREATE TABLE demo.widget (\n  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),\n  name text NOT NULL,\n  platform_organization_id uuid\n);\nALTER TABLE demo.widget ADD CONSTRAINT fk_widget_org FOREIGN KEY (platform_organization_id) REFERENCES basejump.accounts(id);\n",
    )
    .unwrap();
    fs::write(
        dir.join("0011_demo_widget_rls.sql"),
        "ALTER TABLE demo.widget ENABLE ROW LEVEL SECURITY;\nCREATE POLICY tenant_isolation ON demo.widget USING (platform_organization_id = public.get_current_org_id());\n",
    )
    .unwrap();
    fs::write(
        dir.join("0012_demo_widget_event_trigger.sql"),
        "CREATE TRIGGER demo_widget_event AFTER INSERT ON demo.widget FOR EACH ROW EXECUTE FUNCTION emit_domain_event('demo', 'widget');\n",
    )
    .unwrap();
    fs::write(dir.join("README.txt"), "not a migration\n").unwrap();
}

/// Drop the objects `write_full_generated_set` + the basejump install create,
/// restoring the target to its pre-test state (best-effort).
async fn cleanup_full_set(t: &PgTarget) {
    let _ = psql_exec(
        t,
        "DROP TRIGGER IF EXISTS on_auth_user_created ON auth.users;",
    )
    .await;
    let _ = psql_exec(
        t,
        "DROP SCHEMA IF EXISTS basejump CASCADE; \
         DROP SCHEMA IF EXISTS demo CASCADE; \
         DROP SCHEMA IF EXISTS api_keys_private CASCADE; \
         DROP SCHEMA IF EXISTS platform CASCADE; \
         DROP SCHEMA IF EXISTS platform_integrations CASCADE;",
    )
    .await;
    // On the Supabase postgres image (the lane's image since #708) the
    // basejump install lands as an EXTENSION (`basejump@basejump_core`,
    // pg_tle-powered) — DROP SCHEMA basejump CASCADE refuses with
    // "extension basejump@basejump_core requires it", leaving objects
    // behind and making the parity test's outcome depend on leftover
    // symmetry. Drop the extension first (a no-op where basejump installed
    // as plain SQL, e.g. postgres:15), then the schema.
    let _ = psql_exec(
        t,
        "DROP EXTENSION IF EXISTS \"basejump@basejump_core\" CASCADE;",
    )
    .await;
    let _ = psql_exec(t, "DROP SCHEMA IF EXISTS basejump CASCADE;").await;
}

#[tokio::test]
#[ignore = "requires a Supabase-compatible Postgres (set DATABASE_URL or default localhost:5432)"]
async fn migrations_apply_full_generated_set() {
    if skip_if_no_psql() {
        return;
    }
    let _guard = DB_LOCK.lock().await;
    let t = target();
    if !band_gate(&t).await {
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    write_full_generated_set(dir.path());
    cleanup_full_set(&t).await;
    // Issue #427: fixed-name files skip on a re-run against the shared
    // target — clear tracking so the test always applies fresh.
    clear_tracking(&t).await.expect("clear tracking failed");

    // The plain-Postgres api path must apply the SAME platform band as
    // `supabase db reset` — basejump (tenancy) + pgmq (events) included.
    run_api_migrations(dir.path(), &t)
        .await
        .expect("migrations failed");

    // Tenancy layer: basejump accounts + account_user.
    let tables = psql_query(
        &t,
        "SELECT count(*) FROM information_schema.tables \
         WHERE table_schema = 'basejump' AND table_name IN ('accounts', 'account_user');",
    )
    .await
    .unwrap();
    assert_eq!(
        tables, "2",
        "basejump accounts/account_user must exist (tenancy layer)"
    );

    // RBAC roles must extend basejump.account_role (previously half-applied).
    let roles = psql_query(
        &t,
        "SELECT count(*) FROM pg_enum \
         WHERE enumtypid = 'basejump.account_role'::regtype \
           AND enumlabel IN ('manager', 'employee');",
    )
    .await
    .unwrap();
    assert_eq!(roles, "2", "rbac_roles must extend basejump.account_role");

    // Event infrastructure: pgmq queue + emit_domain_event.
    let queues = psql_query(
        &t,
        "SELECT count(*) FROM pgmq.list_queues() WHERE queue_name = 'events_demo';",
    )
    .await
    .unwrap();
    assert_eq!(queues, "1", "pgmq queue events_demo must exist");

    let fnc = psql_query(
        &t,
        "SELECT count(*) FROM pg_proc WHERE proname = 'emit_domain_event';",
    )
    .await
    .unwrap();
    assert_eq!(fnc, "1", "emit_domain_event must exist");

    // Domain entity + FK + RLS policy + event trigger.
    let widget = psql_query(
        &t,
        "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'demo' AND table_name = 'widget';",
    )
    .await
    .unwrap();
    assert_eq!(widget, "1", "demo.widget must exist");

    let fk = psql_query(
        &t,
        "SELECT count(*) FROM pg_constraint WHERE conname = 'fk_widget_org';",
    )
    .await
    .unwrap();
    assert_eq!(fk, "1", "FK to basejump.accounts must be applied");

    let pol = psql_query(
        &t,
        "SELECT count(*) FROM pg_policies WHERE policyname = 'tenant_isolation';",
    )
    .await
    .unwrap();
    assert_eq!(pol, "1", "RLS policy must be applied");

    let trig = psql_query(
        &t,
        "SELECT count(*) FROM pg_trigger WHERE tgname = 'demo_widget_event';",
    )
    .await
    .unwrap();
    assert_eq!(trig, "1", "event trigger must be applied");

    // Org resolution must not dangle on the (now existing) basejump tables.
    let org = psql_query(
        &t,
        "SELECT public.resolve_user_org('00000000-0000-0000-0000-000000000000');",
    )
    .await;
    assert!(org.is_ok(), "resolve_user_org must execute without error");

    // The app role must have DML on domain + platform + integration tables.
    // This is the regression guard: the grants in 0002 used to run before the
    // schemas/tables existed, leaving app_user with zero access.
    assert_app_user_dml(&t, "demo", "widget").await;
    assert_app_user_dml(&t, "platform", "webhook_endpoint").await;
    assert_app_user_dml(&t, "platform_integrations", "extension_points").await;

    // Strict verification must also pass (the sweep is idempotent).
    let strict = codegraph_ops::migrate::GrantOptions {
        role: "app_user".to_string(),
        strict: true,
    };
    // Clear tracking so this second call RE-APPLIES (a skipped band would
    // make the strict verification vacuous).
    clear_tracking(&t).await.expect("clear tracking failed");
    codegraph_ops::migrate::run_api_migrations_with_options(dir.path(), &t, &strict)
        .await
        .expect("strict grant verification should pass after migration");

    cleanup_full_set(&t).await;
}

/// Assert the `app_user` role holds SELECT/INSERT/UPDATE/DELETE on
/// `schema.table` (the RLS-aware app role needs these to query at all).
async fn assert_app_user_dml(t: &PgTarget, schema: &str, table: &str) {
    let grants = psql_query(
        t,
        &format!(
            "SELECT string_agg(privilege_type, ',' ORDER BY privilege_type) \
             FROM information_schema.role_table_grants \
             WHERE grantee = 'app_user' AND table_schema = '{schema}' AND table_name = '{table}' \
               AND privilege_type IN ('SELECT','INSERT','UPDATE','DELETE');"
        ),
    )
    .await
    .unwrap_or_default();
    let got: Vec<&str> = grants.split(',').filter(|s| !s.is_empty()).collect();
    assert!(
        got.contains(&"SELECT")
            && got.contains(&"INSERT")
            && got.contains(&"UPDATE")
            && got.contains(&"DELETE"),
        "app_user must have DML on {schema}.{table}, got: {grants:?}"
    );
}

#[tokio::test]
#[ignore = "requires a Supabase-compatible Postgres (set DATABASE_URL or default localhost:5432)"]
async fn phased_and_file_order_application_match() {
    if skip_if_no_psql() {
        return;
    }
    let _guard = DB_LOCK.lock().await;
    let t = target();
    if !band_gate(&t).await {
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    write_full_generated_set(dir.path());

    // Run 1: per-file application in sorted order — what `supabase db reset`
    // effectively does once link_migrations_to_supabase symlinks the files in.
    cleanup_full_set(&t).await;
    for file in sorted_sql_files(dir.path()) {
        psql_exec_file(&t, &file)
            .await
            .unwrap_or_else(|e| panic!("file-order apply failed for {}: {e}", file.display()));
    }
    let inv_file_order = object_inventory(&t).await;

    // Run 2: the harness's phased application (run_api_migrations).
    cleanup_full_set(&t).await;
    // Issue #427: fixed-name files skip on a re-run against the shared
    // target — clear tracking so the phased inventory is a REAL application
    // (a skip would compare an empty inventory against the file-order one).
    clear_tracking(&t).await.expect("clear tracking failed");
    run_api_migrations(dir.path(), &t)
        .await
        .expect("phased migrations failed");
    let inv_phased = object_inventory(&t).await;

    assert_eq!(
        inv_phased, inv_file_order,
        "phased application must produce the same schema as per-file \
         (supabase db reset) application"
    );

    cleanup_full_set(&t).await;
}

fn sorted_sql_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "sql"))
        .collect();
    files.sort();
    files
}

/// `platform._migrations` is the migration BOOKKEEPING table (issue #756),
/// not band schema: the harness's PHASED path intentionally creates and
/// records it, while the raw per-file path (simulating `supabase db reset`)
/// applies only the files and never creates it. Every compared inventory
/// section therefore excludes exactly this relation — nothing else is
/// relaxed, so the parity assertion still proves the band itself matches.
const TRACKING_RELATION: &str = "platform._migrations";

/// A deterministic, sorted inventory of the objects the migration set creates
/// (tables + ACLs, functions, triggers, constraints, policies, enum values).
/// Two runs that produce the same inventory are considered schema-equivalent.
/// Query failures are fatal — a silent empty result would make the parity
/// comparison vacuous.
async fn object_inventory(t: &PgTarget) -> String {
    let schemas = "('basejump','platform','platform_integrations','api_keys_private','demo')";
    let mut parts = Vec::new();
    let tables = psql_query(
        t,
        &format!(
            "SELECT n.nspname, c.relname FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname IN {schemas} AND c.relkind IN ('r','p') \
               AND c.oid <> COALESCE(to_regclass('{TRACKING_RELATION}'), 0) \
             ORDER BY 1,2;"
        ),
    )
    .await
    .expect("inventory query (tables) failed");
    parts.push(tables);

    // ACLs: the app-role DML grants are part of the schema contract — both
    // application methods must produce the same grant state. The tracking
    // table is excluded for the same reason: only the phased path creates it
    // (and a re-run's grant sweep then touches it).
    let acls = psql_query(
        t,
        &format!(
            "SELECT n.nspname, c.relname, c.relacl::text FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname IN {schemas} AND c.relkind IN ('r','p') \
               AND c.oid <> COALESCE(to_regclass('{TRACKING_RELATION}'), 0) \
             ORDER BY 1,2,3;"
        ),
    )
    .await
    .expect("inventory query (acls) failed");
    parts.push(acls);

    let funcs = psql_query(
        t,
        "SELECT p.proname, pg_get_function_identity_arguments(p.oid) \
         FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
         WHERE n.nspname IN ('basejump','platform','api_keys_private','demo','public') \
           AND p.prokind <> 'a' ORDER BY 1,2;",
    )
    .await
    .expect("inventory query (functions) failed");
    parts.push(funcs);

    // Triggers/constraints/policies are filtered by the tracking relation for
    // the same reason (the table's primary-key constraint would otherwise
    // appear only in the phased snapshot). Domain constraints
    // (`conrelid = 0`) belong to system/extension domains
    // (`information_schema.*`, `extensions.content_type`) — PostgreSQL
    // materializes those lazily depending on which catalogs/extensions a path
    // touches, and the generated band creates no domains, so they are not
    // part of the band contract.
    let triggers = psql_query(
        t,
        &format!(
            "SELECT t.tgname, t.tgrelid::regclass::text FROM pg_trigger t \
             WHERE NOT t.tgisinternal \
               AND t.tgrelid <> COALESCE(to_regclass('{TRACKING_RELATION}'), 0) \
             ORDER BY 1,2;"
        ),
    )
    .await
    .expect("inventory query (triggers) failed");
    parts.push(triggers);

    let constraints = psql_query(
        t,
        &format!(
            "SELECT c.conname, c.conrelid::regclass::text, pg_get_constraintdef(c.oid) \
             FROM pg_constraint c \
             WHERE c.conrelid <> 0 \
               AND c.conrelid <> COALESCE(to_regclass('{TRACKING_RELATION}'), 0) \
             ORDER BY 1,2,3;"
        ),
    )
    .await
    .expect("inventory query (constraints) failed");
    parts.push(constraints);

    let policies = psql_query(
        t,
        &format!(
            "SELECT pol.polname, c.relname FROM pg_policy pol \
             JOIN pg_class c ON c.oid = pol.polrelid \
             WHERE c.oid <> COALESCE(to_regclass('{TRACKING_RELATION}'), 0) \
             ORDER BY 1,2;"
        ),
    )
    .await
    .expect("inventory query (policies) failed");
    parts.push(policies);

    let enums = psql_query(
        t,
        "SELECT t.typname, e.enumlabel FROM pg_enum e JOIN pg_type t ON t.oid = e.enumtypid \
         ORDER BY 1,2;",
    )
    .await
    .expect("inventory query (enums) failed");
    parts.push(enums);

    parts.join("\n===\n")
}

// ── Issue #427: fail-on-error, tracking skip, reset semantics, replay-all ──

/// Issue #427 acceptance: a migration that errors FAILS the run (STRICT is
/// the default) with a per-file summary naming every broken file/chunk and
/// stating the band is partial — and NOTHING is recorded in the tracking
/// table (a partial band records nothing, #756). The lenient opt-out
/// downgrades the same failures to a report with `failed` set, still
/// without recording.
#[tokio::test]
#[ignore = "requires a running Postgres (set DATABASE_URL or default localhost:5432)"]
async fn migration_errors_fail_the_run_with_a_per_file_summary() {
    if skip_if_no_psql() {
        return;
    }
    let _guard = DB_LOCK.lock().await;
    let t = target();

    let dir = tempfile::tempdir().unwrap();
    // Unique prefix per test — the tracking table is SHARED across tests on
    // this target, and names are its keys.
    const P: &str = "t427_err";
    std::fs::write(
        dir.path().join(format!("{P}_001_good.sql")),
        format!("CREATE SCHEMA IF NOT EXISTS {P};\nCREATE TABLE {P}.x (id int);\n"),
    )
    .unwrap();
    std::fs::write(
        dir.path().join(format!("{P}_002_bad_table.sql")),
        format!("CREATE TABLE {P}.broken (id int NONSENSE COLUMN HERE;\n"),
    )
    .unwrap();
    std::fs::write(
        dir.path().join(format!("{P}_003_bad_fk.sql")),
        format!(
            "ALTER TABLE {P}.x ADD CONSTRAINT {P}_fk FOREIGN KEY (id) REFERENCES {P}.missing(id);\n"
        ),
    )
    .unwrap();

    cleanup_tracking_rows(&t, P).await;
    psql_exec(&t, &format!("DROP SCHEMA IF EXISTS {P} CASCADE;"))
        .await
        .expect("cleanup failed");

    // STRICT (the default): the run fails, the summary names every file.
    let err = run_api_migrations(dir.path(), &t)
        .await
        .expect_err("a migration that errors must fail the run (issue #427 fail-on-error)");
    let text = err.to_string();
    assert!(text.contains("2 migration file"), "{text}");
    assert!(text.contains("PARTIAL"), "{text}");
    assert!(
        text.contains(&format!("{P}_002_bad_table.sql")),
        "the summary must name the broken table file: {text}"
    );
    assert!(
        text.contains("FK constraint batch"),
        "the summary must name the failing FK chunk: {text}"
    );

    // A partial band records NOTHING (the #756 contract holds under the new
    // failure semantics).
    let recorded = tracking_rows(&t, P).await;
    assert_eq!(
        recorded, 0,
        "a failed band must not record in platform._migrations"
    );

    // LENIENT (the explicit opt-out): the same failures return a report —
    // and still record nothing. From a CLEAN slate (the strict run above
    // applied file 001 before failing), the failure count is exactly the
    // two broken files.
    psql_exec(&t, &format!("DROP SCHEMA IF EXISTS {P} CASCADE;"))
        .await
        .expect("cleanup failed");
    let report = codegraph_ops::migrate::run_migrations(
        dir.path(),
        &t,
        &codegraph_ops::migrate::MigrateOptions {
            lenient: true,
            ..Default::default()
        },
    )
    .await
    .expect("lenient mode must tolerate the failures");
    assert_eq!(report.failed, 2, "{report:?}");
    assert_eq!(
        tracking_rows(&t, P).await,
        0,
        "a tolerated PARTIAL band must not record either"
    );

    psql_exec(&t, &format!("DROP SCHEMA IF EXISTS {P} CASCADE;"))
        .await
        .expect("cleanup failed");
    cleanup_tracking_rows(&t, P).await;
}

/// Issue #427 acceptance (speed): the SECOND consecutive application applies
/// ~0 files (checksum-matched files skip), a CHANGED file re-applies, and
/// `--replay-all` bypasses the skip entirely.
#[tokio::test]
#[ignore = "requires a running Postgres (set DATABASE_URL or default localhost:5432)"]
async fn consecutive_run_applies_zero_and_replay_all_reapplies() {
    if skip_if_no_psql() {
        return;
    }
    let _guard = DB_LOCK.lock().await;
    let t = target();

    const P: &str = "t427_skip";
    let dir = tempfile::tempdir().unwrap();
    // Idempotent on purpose: the replay_all leg re-applies EVERYTHING, and
    // non-idempotent files over an existing band fail loudly (the documented
    // replay semantics — issue #427).
    std::fs::write(
        dir.path().join(format!("{P}_001_a.sql")),
        format!("CREATE SCHEMA IF NOT EXISTS {P};\nCREATE TABLE IF NOT EXISTS {P}.a (id int);\n"),
    )
    .unwrap();
    std::fs::write(
        dir.path().join(format!("{P}_002_b.sql")),
        format!("CREATE TABLE IF NOT EXISTS {P}.b (id int);\n"),
    )
    .unwrap();
    std::fs::write(
        dir.path().join(format!("{P}_003_c.sql")),
        format!("CREATE INDEX IF NOT EXISTS {P}_b_idx ON {P}.b (id);\n"),
    )
    .unwrap();

    cleanup_tracking_rows(&t, P).await;
    psql_exec(&t, &format!("DROP SCHEMA IF EXISTS {P} CASCADE;"))
        .await
        .expect("cleanup failed");

    let first = run_api_migrations(dir.path(), &t)
        .await
        .expect("first apply failed");
    assert_eq!(first.applied, 3, "{first:?}");
    assert_eq!(first.skipped, 0, "{first:?}");

    // The consecutive pass: the speed acceptance — ~0 files applied.
    let second = run_api_migrations(dir.path(), &t)
        .await
        .expect("second apply failed");
    assert_eq!(
        second.applied, 0,
        "checksum-matched files must skip: {second:?}"
    );
    assert_eq!(second.skipped, 3, "{second:?}");

    // A CHANGED file (new bytes, same name) re-applies and re-records. The
    // changed content is IDEMPOTENT on purpose — a non-idempotent changed
    // file over an existing band fails loudly (the documented replay
    // semantics), so the test proves the re-apply with SQL that can land.
    std::fs::write(
        dir.path().join(format!("{P}_002_b.sql")),
        format!(
            "CREATE TABLE IF NOT EXISTS {P}.b (id int);\n\
             ALTER TABLE {P}.b ADD COLUMN IF NOT EXISTS note text;\n"
        ),
    )
    .unwrap();
    let third = run_api_migrations(dir.path(), &t)
        .await
        .expect("third apply failed");
    assert_eq!(
        third.applied, 1,
        "the changed file must re-apply: {third:?}"
    );
    assert_eq!(third.skipped, 2, "{third:?}");
    let column = psql_query(
        &t,
        &format!(
            "SELECT count(*) FROM information_schema.columns \
             WHERE table_schema = '{P}' AND table_name = 'b' AND column_name = 'note';"
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        column, "1",
        "the changed file's new statement must have landed"
    );

    // replay_all bypasses the skip entirely (apply everything).
    let replayed = codegraph_ops::migrate::run_migrations(
        dir.path(),
        &t,
        &codegraph_ops::migrate::MigrateOptions {
            replay_all: true,
            ..Default::default()
        },
    )
    .await
    .expect("replay_all run failed");
    assert_eq!(replayed.applied, 3, "{replayed:?}");
    assert_eq!(replayed.skipped, 0, "{replayed:?}");

    psql_exec(&t, &format!("DROP SCHEMA IF EXISTS {P} CASCADE;"))
        .await
        .expect("cleanup failed");
    cleanup_tracking_rows(&t, P).await;
}

/// Issue #427 acceptance (reset semantics): `clear_tracking` makes a reset
/// database re-band cleanly — without it, stale rows would skip every file
/// against schemas the reset wiped.
#[tokio::test]
#[ignore = "requires a running Postgres (set DATABASE_URL or default localhost:5432)"]
async fn reset_clears_tracking_and_rebands() {
    if skip_if_no_psql() {
        return;
    }
    let _guard = DB_LOCK.lock().await;
    let t = target();

    const P: &str = "t427_reset";
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(format!("{P}_001_a.sql")),
        format!("CREATE SCHEMA IF NOT EXISTS {P};\nCREATE TABLE {P}.a (id int);\n"),
    )
    .unwrap();

    cleanup_tracking_rows(&t, P).await;
    psql_exec(&t, &format!("DROP SCHEMA IF EXISTS {P} CASCADE;"))
        .await
        .expect("cleanup failed");

    run_api_migrations(dir.path(), &t)
        .await
        .expect("first apply failed");
    assert_eq!(tracking_rows(&t, P).await, 1);

    // Simulate the reset: wipe the domain schema (what reset_sql does) —
    // then clear_tracking. The next run must RE-BAND (apply again), not
    // skip against the wiped schema.
    psql_exec(&t, &format!("DROP SCHEMA IF EXISTS {P} CASCADE;"))
        .await
        .expect("reset dropped the schema");
    codegraph_ops::migrate::clear_tracking(&t)
        .await
        .expect("clear_tracking failed");
    assert_eq!(
        tracking_rows(&t, P).await,
        0,
        "clear_tracking must empty the tracking table"
    );

    let rebanded = run_api_migrations(dir.path(), &t)
        .await
        .expect("re-band failed");
    assert_eq!(rebanded.applied, 1, "a reset DB must re-band: {rebanded:?}");
    assert_eq!(rebanded.skipped, 0, "{rebanded:?}");

    // The table is recreated by the next record (clear dropped it).
    let exists = psql_query(
        &t,
        "SELECT to_regclass('platform._migrations') IS NOT NULL;",
    )
    .await
    .unwrap();
    assert!(
        exists.trim().ends_with('t'),
        "record_tracking must recreate the table"
    );

    psql_exec(&t, &format!("DROP SCHEMA IF EXISTS {P} CASCADE;"))
        .await
        .expect("cleanup failed");
    cleanup_tracking_rows(&t, P).await;
}

/// Tracking rows for this test's file prefix (the name column is the key).
async fn tracking_rows(t: &PgTarget, prefix: &str) -> usize {
    // An absent table (a fresh target, or a failed run — a partial band
    // never records, so record_tracking never created the table) counts
    // as zero.
    let exists = psql_query(t, "SELECT to_regclass('platform._migrations') IS NOT NULL;")
        .await
        .unwrap_or_default()
        .trim()
        .ends_with('t');
    if !exists {
        return 0;
    }
    psql_query(
        t,
        &format!("SELECT count(*) FROM platform._migrations WHERE name LIKE '{prefix}\\_%';"),
    )
    .await
    .unwrap_or_else(|e| panic!("tracking count query failed: {e}"))
    .trim()
    .parse()
    .unwrap_or(0)
}

/// Delete this test's tracking rows (names are the shared table's keys).
async fn cleanup_tracking_rows(t: &PgTarget, prefix: &str) {
    psql_exec(
        t,
        &format!("DELETE FROM platform._migrations WHERE name LIKE '{prefix}\\_%';"),
    )
    .await
    .ok();
}
