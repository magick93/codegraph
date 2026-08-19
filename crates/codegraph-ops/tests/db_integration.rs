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
use codegraph_ops::migrate::{link_migrations_to_supabase, run_api_migrations};
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

/// True when the target looks like a Supabase-compatible Postgres: the `auth`
/// schema is present and the extensions basejump/pgmq depend on are available.
/// The `migrate` integration tests that exercise the full generated platform
/// band skip otherwise (e.g. a vanilla `postgres:15` container in CI).
async fn supabase_compatible(t: &PgTarget) -> bool {
    let out = psql_query(
        t,
        "SELECT \
         EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = 'auth') AND \
         EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'pgmq') AND \
         EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'pg_tle') AND \
         EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'http') AND \
         EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'uuid-ossp');",
    )
    .await
    .unwrap_or_default();
    out.trim() == "t"
}

/// Write a representative full generated migration set (the same band the
/// `ddl`/`basejump_setup`/`pgmq_setup`/`scaffold` generators emit) into `dir`:
/// extensions, basejump install, api-key/org resolution, pgmq setup, rbac
/// roles, platform schema, plus one entity with an FK, an RLS policy and a
/// domain-event trigger.
fn write_full_generated_set(dir: &Path) {
    let basejump = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../codegraph/static/basejump_core_2.0.1_install.sql");
    let basejump_sql = fs::read_to_string(&basejump)
        .unwrap_or_else(|_| panic!("cannot read basejump install at {}", basejump.display()));

    fs::write(
        dir.join("0000_extensions.sql"),
        "CREATE EXTENSION IF NOT EXISTS http WITH SCHEMA extensions;\nCREATE EXTENSION IF NOT EXISTS pg_tle;\nCREATE EXTENSION IF NOT EXISTS pgcrypto;\nCREATE EXTENSION IF NOT EXISTS \"uuid-ossp\";\n",
    )
    .unwrap();
    fs::write(dir.join("0001_basejump_install.sql"), basejump_sql).unwrap();
    fs::write(
        dir.join("0002_api_key_migration.sql"),
        "-- API keys + unified org resolution\nCREATE SCHEMA IF NOT EXISTS api_keys_private;\nCREATE TABLE IF NOT EXISTS api_keys_private.api_keys (\n  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),\n  organization_id uuid NOT NULL,\n  created_at timestamptz NOT NULL DEFAULT now()\n);\nALTER TABLE api_keys_private.api_keys ENABLE ROW LEVEL SECURITY;\nDO $$ BEGIN\n  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'app_user') THEN\n    CREATE ROLE app_user LOGIN PASSWORD 'x' NOBYPASSRLS;\n  END IF;\nEND $$;\nCREATE OR REPLACE FUNCTION public.get_current_org_id() RETURNS uuid AS $$\nBEGIN\n  RETURN nullif(current_setting('app.organization_id', true), '')::uuid;\nEND;\n$$ LANGUAGE plpgsql STABLE;\nGRANT USAGE ON SCHEMA basejump TO app_user;\nGRANT SELECT ON basejump.account_user TO app_user;\nGRANT SELECT ON basejump.accounts TO app_user;\nCREATE OR REPLACE FUNCTION public.resolve_user_org(p_user_id uuid)\nRETURNS uuid AS $$\n  SELECT au.account_id FROM basejump.account_user au WHERE au.user_id = p_user_id LIMIT 1;\n$$ LANGUAGE sql STABLE SECURITY DEFINER;\n",
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
        "CREATE SCHEMA IF NOT EXISTS platform;\nGRANT USAGE ON SCHEMA pgmq TO app_user;\n",
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
         DROP SCHEMA IF EXISTS platform CASCADE;",
    )
    .await;
}

#[tokio::test]
#[ignore = "requires a Supabase-compatible Postgres (set DATABASE_URL or default localhost:5432)"]
async fn migrations_apply_full_generated_set() {
    if skip_if_no_psql() {
        return;
    }
    let _guard = DB_LOCK.lock().await;
    let t = target();
    if !supabase_compatible(&t).await {
        eprintln!("skipping: target lacks Supabase-compatible auth schema / pgmq / pg_tle / http");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    write_full_generated_set(dir.path());
    cleanup_full_set(&t).await;

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

    cleanup_full_set(&t).await;
}

#[tokio::test]
#[ignore = "requires a Supabase-compatible Postgres (set DATABASE_URL or default localhost:5432)"]
async fn phased_and_file_order_application_match() {
    if skip_if_no_psql() {
        return;
    }
    let _guard = DB_LOCK.lock().await;
    let t = target();
    if !supabase_compatible(&t).await {
        eprintln!("skipping: target lacks Supabase-compatible auth schema / pgmq / pg_tle / http");
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

/// A deterministic, sorted inventory of the objects the migration set creates
/// (tables, functions, triggers, constraints, policies, enum values). Two runs
/// that produce the same inventory are considered schema-equivalent. Query
/// failures are fatal — a silent empty result would make the parity comparison
/// vacuous.
async fn object_inventory(t: &PgTarget) -> String {
    let schemas = "('basejump','platform','api_keys_private','demo')";
    let mut parts = Vec::new();
    let tables = psql_query(
        t,
        &format!(
            "SELECT n.nspname, c.relname FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname IN {schemas} AND c.relkind IN ('r','p') ORDER BY 1,2;"
        ),
    )
    .await
    .expect("inventory query (tables) failed");
    parts.push(tables);

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

    let triggers = psql_query(
        t,
        "SELECT t.tgname, t.tgrelid::regclass::text FROM pg_trigger t \
         WHERE NOT t.tgisinternal ORDER BY 1,2;",
    )
    .await
    .expect("inventory query (triggers) failed");
    parts.push(triggers);

    let constraints = psql_query(
        t,
        "SELECT c.conname, c.conrelid::regclass::text, pg_get_constraintdef(c.oid) \
         FROM pg_constraint c ORDER BY 1,2,3;",
    )
    .await
    .expect("inventory query (constraints) failed");
    parts.push(constraints);

    let policies = psql_query(
        t,
        "SELECT pol.polname, c.relname FROM pg_policy pol \
         JOIN pg_class c ON c.oid = pol.polrelid ORDER BY 1,2;",
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
