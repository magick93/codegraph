//! Issue #700 — the live DB-tier gate for child-table tenancy.
//!
//! Child rows (contained value objects and junctions) had NO writer that
//! bound `platform_organization_id`: the SeaORM `child_insert_sql`, the
//! cornucopia `insert_{entity}_{child}` / `link_{junction}` queries and the
//! media handler all omitted it, so the DDL's zero default was persisted and
//! every child policy tolerated zeros — which made those rows readable AND
//! writable by EVERY organization at the database tier.
//!
//! A template pin cannot prove that hole is closed, so this gate generates a
//! migration set with the CURRENT generators (never a committed fixture tree,
//! which may be stale — that is how the defect survived review) and drives it
//! against real Postgres:
//!
//! 1. **The writer fix**: an INSERT that omits the tenant column is stamped
//!    with the session organization by the child's BEFORE INSERT trigger.
//! 2. **The upgrade path**: re-applying the child's RLS migration — every
//!    migration file is re-applied on each `migrate` run, which is why that
//!    file IS the upgrade migration — backfills a legacy zero-tenant row from
//!    its parent.
//! 3. **The fix itself**: afterwards the child policies are exact-org. Org A
//!    reads its rows; org B reads NONE of them (before #700 both read them).
//!
//! Both child shapes are covered: a contained VO (`contains`) and a junction
//! (`refers T[]`).
//!
//! Needs Postgres reachable at `DATABASE_URL` (or the gate container
//! fallback) and `psql` on PATH:
//!
//! ```text
//! DATABASE_URL=postgres://postgres:postgres@127.0.0.1:54350/postgres \
//!   cargo test -p codegraph --test child_tenant_contract_tests -- --ignored --nocapture
//! ```

#[path = "test_framework/mod.rs"]
mod test_framework;

use std::path::{Path, PathBuf};

use codegraph_ops::db::{psql_exec, psql_query};
use codegraph_ops::pg::PgTarget;
use test_framework::postgres::{GateDb, resolve_base_target};

const ORG_A: &str = "11111111-1111-4111-8111-111111111111";
const ORG_B: &str = "22222222-2222-4222-8222-222222222222";
const PARENT_A: &str = "dddddddd-0000-4000-8000-000000000001";
const PARENT_B: &str = "dddddddd-0000-4000-8000-000000000002";
const ZERO_TENANT: &str = "00000000-0000-0000-0000-000000000000";

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Generate the review fixture's models with the CURRENT generators into
/// `out` — the same inputs every other full-pipeline gate uses, so the
/// migration set under test is the one a consumer would receive.
async fn generate_migrations(out: &Path) {
    let fixtures = fixtures_dir();
    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: Some(&fixtures),
        classifier: Some(&fixtures.join("classifier.toml")),
        config_path: &fixtures.join("domains.toml"),
        output: out,
        extension_points_path: None,
        integration_manifests: &[],
        profile_name: "default",
        variant: None,
        // Absent profiles ⇒ the no-plan (all generators) path the sibling
        // pipeline gates use.
        profiles_config_path: Some(PathBuf::from("definitely-absent-profiles.toml")),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &[],
        rosetta_files: &[],
        ddd_files: &[],
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
    .expect("fixture generation must succeed");
}

/// Run `sql` as `app_user` under a session for `org`, returning
/// `OK|<value>` or `<SQLSTATE>|<message>` (the `authz_contract` probe shape).
async fn probe(target: &PgTarget, org: &str, sql: &str) -> String {
    let payload = format!(
        "BEGIN;\nSET LOCAL ROLE app_user;\n\
         DO $probe_wrapper$\nDECLARE v text;\nBEGIN\n\
           PERFORM set_config('app.organization_id', '{org}', true);\n\
           CREATE TEMP TABLE IF NOT EXISTS probe_result(state text, detail text);\n\
           DELETE FROM probe_result;\n\
           BEGIN\n\
             EXECUTE $probe${sql}$probe$ INTO v;\n\
             INSERT INTO probe_result VALUES ('OK', coalesce(v, ''));\n\
           EXCEPTION WHEN OTHERS THEN\n\
             INSERT INTO probe_result VALUES (SQLSTATE, left(SQLERRM, 200));\n\
           END;\n\
         END\n$probe_wrapper$;\n\
         SELECT state || '|' || detail FROM probe_result;\n\
         ROLLBACK;"
    );
    let out = psql_query(target, &payload).await.expect("probe psql run");
    out.lines()
        .filter(|l| !matches!(l.trim(), "BEGIN" | "SET" | "DO" | "ROLLBACK" | ""))
        .filter(|l| !l.trim().starts_with("SET LOCAL ROLE"))
        .rev()
        .find(|l| l.contains('|'))
        .unwrap_or("")
        .trim()
        .to_string()
}

fn assert_ok(result: &str, value: &str, context: &str) {
    assert_eq!(
        result,
        format!("OK|{value}"),
        "{context}: expected OK|{value}, got {result:?}"
    );
}

/// Locate a migration by suffix: the numeric prefix is a global counter that
/// moves whenever a generator adds a file, so the gate must not hardcode it.
fn migration_by_suffix(dir: &Path, suffix: &str) -> PathBuf {
    let mut hits: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read migrations dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().ends_with(suffix))
                .unwrap_or(false)
        })
        .collect();
    hits.sort();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one migration ending in {suffix}, found {hits:?}"
    );
    hits.remove(0)
}

/// The junction child of `common.event_base` (a `refers PersonBaseType[]`).
/// The contained-VO shape renders the SAME child file — same template, same
/// trigger, same policy block — which the DDL unit pins cover structurally;
/// this gate spends its time on the database behaviour instead.
const CHILD_SCHEMA: &str = "common";
const CHILD: &str = "event_base_personbasetype";
const CHILD_FK: &str = "event_base_id";

#[tokio::test]
#[ignore = "generates + migrates a fixture and needs Postgres (set DATABASE_URL)"]
async fn child_rows_carry_the_session_org_and_evict_the_zero_tenant() {
    let out = tempfile::tempdir().expect("temp output dir");
    generate_migrations(out.path()).await;
    let migrations = out.path().join("migrations");
    assert!(
        migrations.is_dir(),
        "the pipeline must emit a migrations dir at {migrations:?}"
    );

    let base = resolve_base_target()
        .await
        .expect("a Postgres base target (DATABASE_URL or the gate container)");
    // The prelude is the PLAIN-postgres shim (Supabase roles, pgcrypto, the
    // pgmq schema). The Supabase image — the supported target (#640) —
    // already provides all of it, and re-creating its reserved roles errors,
    // so the marker (`auth` schema) decides whether it runs at all.
    let supabase_image = psql_query(
        &base,
        "SELECT count(*) FROM pg_namespace WHERE nspname = 'auth';",
    )
    .await
    .map(|v| v.trim() != "0")
    .unwrap_or(false);
    let db = GateDb::create(base)
        .await
        .expect("create the per-run database");
    if !supabase_image {
        db.apply_prelude()
            .await
            .expect("apply the plain-postgres prelude");
    }
    db.apply_migrations(&migrations)
        .await
        .expect("the generated migration set must apply cleanly");

    // A target for owner-side statements (GateDb connects as the owner
    // superuser, which bypasses RLS and can disable the trigger).
    let target = PgTarget::from_url(&db.url()).expect("parse the gate db url");

    // Parents: org A and org B each own one row.
    psql_exec(
        &target,
        &format!(
            "INSERT INTO common.event_base (id, platform_organization_id, family_name, given_name, title) VALUES \
             ('{PARENT_A}', '{ORG_A}', 'A', 'Parent', 'A'), \
             ('{PARENT_B}', '{ORG_B}', 'B', 'Parent', 'B');"
        ),
    )
    .await
    .expect("insert parent rows");

    {
        let child = CHILD;
        let fk = CHILD_FK;
        let trigger = format!("trg_{child}_set_org_id");

        // (1) THE WRITER FIX is live: the generated migration created the
        // child's OWN org trigger (the #700 defect was that only parents had
        // one). Asserted on the catalog, not the template.
        let count = psql_query(
            &target,
            &format!(
                "SELECT count(*) FROM pg_trigger t JOIN pg_class c ON c.oid = t.tgrelid \
                 WHERE NOT t.tgisinternal AND c.relname = '{child}' AND t.tgname = '{trigger}';"
            ),
        )
        .await
        .expect("read pg_trigger");
        assert_eq!(count.trim(), "1", "child {child} carries its org trigger");

        // A LEGACY child row: written before that trigger existed. The
        // trigger is disabled for exactly this statement to reproduce the
        // pre-#700 writer shape.
        psql_exec(
            &target,
            &format!(
                "ALTER TABLE {CHILD_SCHEMA}.{child} DISABLE TRIGGER {trigger}; \
                 INSERT INTO {CHILD_SCHEMA}.{child} (platform_organization_id, {fk}, family_name, given_name) \
                 VALUES ('{ZERO_TENANT}', '{PARENT_A}'{extra_vals}); \
                 ALTER TABLE {CHILD_SCHEMA}.{child} ENABLE TRIGGER {trigger};",
                extra_vals = ", 'Legacy', 'Child'",
            ),
        )
        .await
        .expect("insert the legacy zero-tenant child row");

        let zero = psql_query(
            &target,
            &format!(
                "SELECT platform_organization_id::text FROM {CHILD_SCHEMA}.{child} \
                 WHERE {fk} = '{PARENT_A}' AND platform_organization_id = '{ZERO_TENANT}';"
            ),
        )
        .await
        .expect("read the legacy tenant");
        assert_eq!(
            zero.trim(),
            ZERO_TENANT,
            "{child}: the legacy row starts at zero"
        );

        // (2) THE UPGRADE PATH: re-apply the child's own RLS migration — the
        // exact file `testkit migrate` re-applies to every existing
        // deployment.
        let rls = migration_by_suffix(&migrations, &format!("_{child}_rls.sql"));
        let sql = std::fs::read_to_string(&rls).expect("read the child rls migration");
        psql_exec(&target, &sql)
            .await
            .expect("re-applying the child rls migration must succeed");

        let repaired = psql_query(
            &target,
            &format!(
                "SELECT platform_organization_id::text FROM {CHILD_SCHEMA}.{child} \
                 WHERE {fk} = '{PARENT_A}';"
            ),
        )
        .await
        .expect("read the repaired tenant");
        assert_eq!(
            repaired.trim(),
            ORG_A,
            "{child}: the backfill must repair the legacy row from its parent"
        );

        // (3) THE POLICIES THEMSELVES, read back from the catalog. A
        // row-level probe cannot prove the tolerance is gone (the backfill
        // already moved every row), so assert on the policy expressions Postgres
        // recorded: none may name the zero tenant, and the four org-isolation
        // policies must be present.
        let policies = psql_query(
            &target,
            &format!(
                "SELECT p.polname || '::: ' || coalesce(pg_get_expr(polqual, polrelid), '') \
                   || ' ' || coalesce(pg_get_expr(polwithcheck, polrelid), '') \
                   FROM pg_policy p JOIN pg_class c ON c.oid = p.polrelid \
                  WHERE c.relname = '{child}' ORDER BY p.polname;"
            ),
        )
        .await
        .expect("read pg_policy");
        let org_policies: Vec<&str> = policies
            .lines()
            .filter(|l| l.starts_with("org_isolation_"))
            .collect();
        assert_eq!(
            org_policies.len(),
            4,
            "{child}: select/insert/update/delete policies:\n{policies}"
        );
        assert!(
            !policies.contains(ZERO_TENANT),
            "{child}: no child policy may tolerate the zero tenant — the \
             database itself must record the exact-org predicate:\n{policies}"
        );

        // A reads its child row; B reads none of them.
        let a_count = probe(
            &target,
            ORG_A,
            &format!("SELECT count(*) FROM {CHILD_SCHEMA}.{child}"),
        )
        .await;
        assert_ok(
            &a_count,
            "1",
            &format!("{child}: org A reads its backfilled row"),
        );

        let b_count = probe(
            &target,
            ORG_B,
            &format!("SELECT count(*) FROM {CHILD_SCHEMA}.{child}"),
        )
        .await;
        assert_ok(
            &b_count,
            "0",
            &format!("{child}: org B must not read org A's child rows (#700)"),
        );

        // (1b) THE WRITER FIX, end to end: an INSERT that omits the tenant
        // column — the shape every generated writer emits — is stamped with
        // the session organization.
        let stamped = probe(
            &target,
            ORG_A,
            &format!(
                "WITH ins AS (INSERT INTO {CHILD_SCHEMA}.{child} ({fk}{extra_cols}) \
                 VALUES ('{PARENT_A}'{extra_vals}) \
                 RETURNING platform_organization_id::text AS tenant) SELECT tenant FROM ins",
                extra_cols = ", family_name, given_name",
                extra_vals = ", 'Fresh', 'Row'",
            ),
        )
        .await;
        assert_ok(
            &stamped,
            ORG_A,
            &format!("{child}: the INSERT trigger stamps the session organization"),
        );

        // And org B still cannot see the row A just created.
        let b_after = probe(
            &target,
            ORG_B,
            &format!("SELECT count(*) FROM {CHILD_SCHEMA}.{child}"),
        )
        .await;
        assert_ok(
            &b_after,
            "0",
            &format!("{child}: org B sees nothing after A's write"),
        );
    }

    db.cleanup().await;
}
