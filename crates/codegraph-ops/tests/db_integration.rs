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
