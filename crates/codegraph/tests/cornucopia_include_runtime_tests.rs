//! Runtime pin for the cornucopia `?include=` SQL (issue #781 P1).
//!
//! The TDD (`docs/tdd/781-cornucopia-includes.tdd.md` §6 item 2) asks for the
//! one gate the node-free pins cannot give: the generated include SQL has **no
//! tenant predicate by design** (§1.6) — it depends entirely on the query
//! handler's session bundle (`SET LOCAL ROLE app_user` + the five `app.*`
//! GUCs, §4). §9 ranks that as risk #1 and the soft-delete marker as risk #2,
//! "both silent". A test that bypasses the bundle proves nothing, so this
//! suite **executes the emitted SQL** as `app_user` under a session built
//! exactly like the generated wrapper's.
//!
//! # What runs
//!
//! 1. Generate the fixture through the shared
//!    `cornucopia_include_support` module with the preview flag ON
//!    (`cornucopia-includes` profile, §1).
//! 2. Provision a unique database through `test_framework::postgres::GateDb`
//!    (`DATABASE_URL` when it can host per-run databases, else the pinned
//!    docker fallback).
//! 3. Apply the **emitted** table DDL + the **emitted** RLS band (below).
//! 4. Seed two organizations with customers, their `organization` targets,
//!    invoices (required FK) and work orders (optional FK), plus a soft-deleted
//!    target, a soft-deleted source, and cross-tenant FKs.
//! 5. Extract every `--! include_*` statement out of `queries/shop/*.sql`,
//!    substitute `:source_id`/`:source_ids` for `$1` (keeping the `::uuid[]`
//!    cast), and run each one inside a transaction carrying the emitted
//!    session bundle.
//! 6. Assert hydration, batch keying, the dot hop, cross-tenant isolation and
//!    the soft-delete contract.
//!
//! # The migration band: what is applied, and what is skipped
//!
//! The fixture's profile is deliberately minimal (no monolith `scaffold`
//! generator), so its `migrations/` directory is the entity band only. This
//! gate applies the include-relevant part of it and nothing else:
//!
//! | Applied | Why |
//! |---|---|
//! | `00NNNN_shop_<table>.sql` (currency, organization, customer, invoice, work_order) | the emitted table DDL the include statements read — FK column types, nullability, tenant column, audit/`deleted_at` columns |
//! | `00NNNN_shop_<table>_rls.sql` (all five) | the emitted `ENABLE`/`FORCE ROW LEVEL SECURITY` + org-isolation + RESTRICTIVE scope policies; without them the cross-tenant legs prove nothing |
//!
//! plus a **test-owned bootstrap** mirroring the platform band this profile
//! does not emit (documented at [`PLATFORM_BOOTSTRAP`]): the `app_user` /
//! `api_key` roles, `public.get_current_org_id()` copied verbatim from
//! `templates/db/api_key_migration.tera`, and a tenancy-only stand-in for
//! `public.enforce_api_key_scope`.
//!
//! | Skipped | Why |
//! |---|---|
//! | `0000_extensions.sql` | installs `http` + `pg_tle` (Supabase-image extensions); no include statement touches them. `gen_random_uuid()` is core on PG13+ and the bootstrap installs `pgcrypto`, which is all the DDL needs |
//! | `0001_basejump_install.sql` | the basejump package (`pgtle`) + `auth` schema; the tenancy under test is the emitted RLS, not basejump |
//! | `0003_pgmq_setup.sql` | pgmq queues + the event trigger plumbing; include queries are reads and emit no events |
//! | `0004_rbac_roles.sql` | basejump enum + role-rank helpers behind `role_enforced_*` policies; this fixture configures no `permissions.scope` |
//! | `0005_platform_schema.sql` | workflow/webhook/platform tables — unrelated to the include plane |
//! | `*_trigger.sql` | write-side `updated_at` + `set_org_id_from_session` stamping. The include plane is read-only, and the seed rows are inserted by the owner with explicit tenants (the session-stamp trigger would rewrite them from an unset session var — the opposite of what the seed needs) |
//! | `*_event_trigger.sql` | calls `emit_domain_event()` (pgmq) — not applicable to a read plane |
//! | `*_fts.sql` | FTS column + tsvector maintenance triggers — unrelated |
//!
//! # The session bundle
//!
//! [`set_rls_session_vars`] is the emitted `query.tera` function verbatim: the
//! five `set_config(..., true)` GUCs in ONE statement
//! (`app.current_api_key`, `app.current_api_key_id`, `app.organization_id`,
//! `app.user_id`, `app.role`) followed by `SET LOCAL ROLE app_user`, inside the
//! transaction that runs the include statement. Nothing is stubbed on the
//! tenancy axis: `app_user` is `NOSUPERUSER`/`NOBYPASSRLS` and the tables are
//! `FORCE ROW LEVEL SECURITY`, so a missing bundle would flood every leg with
//! the other tenant's rows.
//!
//! # Soft delete — the honest shape of this fixture
//!
//! §1.2: the marker filter keys on the target `EntityTree`'s
//! `soft_delete_column`, which comes from a **`SoftDelete` policy node** — not
//! from auditable-ness. No current input produces one (`policy_ingest`'s
//! `ingest_policies_and_relationships` has no caller on the mox-first driver
//! path), so the generated include SQL for this fixture carries **no**
//! `deleted_at` predicate, and auditable tables all have the column. The suite
//! therefore asserts (a) the emitted SQL carries no marker, (b) a soft-deleted
//! source still hydrates and a soft-deleted target IS still returned (the
//! designed behaviour of a marker-less tree), and (c) the marker suffix the
//! emitter appends (`render_include_sql`, `db/cornucopia_queries.rs`) — applied
//! to the same emitted statement — really does exclude the seeded row on this
//! schema. (c) is the runtime half of the emitter KATs; the end-to-end marker
//! arm has no reachable input yet and is called out in the report.
//!
//! # Running
//!
//! ```bash
//! cargo test -p codegraph --test cornucopia_include_runtime_tests -- --ignored --nocapture
//! ```
//!
//! `emitted_include_queries_extract_bind_and_carry_no_marker` and the extractor
//! unit tests run in PR CI (node-free, DB-free).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use tokio_postgres::{Client, Row};
use uuid::Uuid;

// The shared fixture also carries the flag-OFF profile constant, which this
// binary (flag-ON only) never reads.
#[allow(dead_code)]
#[path = "cornucopia_include_support/mod.rs"]
mod support;

#[path = "test_framework/mod.rs"]
mod test_framework;

use support::{FLAG_ON_PROFILE, generate, read};
use test_framework::postgres::{GateDb, resolve_base_target};

/// The fixture's include-emitting tables, in the order the assertions walk
/// them.
const INCLUDE_SQL_TABLES: [&str; 3] = ["work_order", "customer", "invoice"];

/// The fixture's tables that carry an RLS band (generation order).
const SHOP_TABLES: [&str; 5] = [
    "currency",
    "organization",
    "customer",
    "invoice",
    "work_order",
];

// ── The emitted annotation contract ──

/// One `--! name (params) : (types)` block from a cornucopia query file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EmittedQuery {
    /// The annotation's parameter list (`source_id` or `source_ids`).
    params: String,
    /// The statement body: comment lines and the terminating `;` stripped,
    /// `:param` placeholders NOT yet substituted.
    body: String,
}

/// Parse every annotated query in a generated `queries/{domain}/{table}.sql`
/// file.
///
/// The cornucopia contract is one `--!` header per statement, an optional
/// `---` doc line per parameter-ish description, then the SQL terminated by
/// `;`. The body is a single statement (asserted here), so "collect until the
/// first line whose trimmed form ends with `;`" is exact.
fn parse_emitted_queries(sql: &str) -> BTreeMap<String, EmittedQuery> {
    let mut out = BTreeMap::new();
    let mut lines = sql.lines();
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix("--! ") else {
            continue;
        };
        let (name, signature) = rest
            .split_once(' ')
            .unwrap_or_else(|| panic!("annotation has no signature: {line}"));
        let signature = signature.trim_start();
        // A parameterless query annotates only the row shape (`--! name :
        // (count)`); everything else lists `(params)` first.
        let params = if let Some(rest) = signature.strip_prefix('(') {
            rest.split_once(')')
                .map(|(params, _)| params.trim().to_string())
                .unwrap_or_else(|| panic!("annotation has no parameter list: {line}"))
        } else {
            String::new()
        };
        let mut body: Vec<String> = Vec::new();
        let mut terminated = false;
        for raw in lines.by_ref() {
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with("---") {
                continue;
            }
            body.push(trimmed.to_string());
            if trimmed.ends_with(';') {
                terminated = true;
                break;
            }
        }
        assert!(terminated, "query {name} is not terminated by `;`");
        let body = body.join("\n");
        let body = body
            .strip_suffix(';')
            .unwrap_or_else(|| panic!("query {name} lost its `;`"))
            .to_string();
        let previous = out.insert(name.to_string(), EmittedQuery { params, body });
        assert!(
            previous.is_none(),
            "query {name} is declared twice in one file"
        );
    }
    out
}

/// Substitute the emitted `:source_id` / `:source_ids` placeholder for the
/// single positional bind the statement declares.
///
/// `:source_ids` is replaced FIRST: `str::replace` on `:source_id` would
/// otherwise eat the prefix of the batch placeholder. The cast survives
/// (`ANY($1::uuid[])`), which is what cornucopia's inference and Postgres both
/// need.
fn bind_source_param(query: &EmittedQuery) -> String {
    let sql = query
        .body
        .replace(":source_ids", "$1")
        .replace(":source_id", "$1");
    let mut chars = sql.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if ch != ':' {
            continue;
        }
        match chars.peek().map(|(_, next)| *next) {
            // `::` opens a cast — consume the second colon so it is not
            // re-examined as a placeholder opener.
            Some(':') => {
                chars.next();
            }
            _ => {
                panic!("unsubstituted placeholder in the emitted statement at byte {index}: {sql}")
            }
        }
    }
    sql
}

/// Every `include_*` query the fixture emits, keyed by name.
fn include_queries(output: &Path) -> BTreeMap<String, EmittedQuery> {
    let mut out = BTreeMap::new();
    for table in INCLUDE_SQL_TABLES {
        let file = output.join(format!("queries/shop/{table}.sql"));
        for (name, query) in parse_emitted_queries(&read(&file)) {
            if name.starts_with("include_") {
                out.insert(name, query);
            }
        }
    }
    out
}

// ── PR-CI pins (node-free, DB-free) ──

#[test]
fn extractor_reads_the_annotation_contract() {
    let sql = "\
--! include_thing_for_owner (source_id) : (id, name?, created_at)
--- Eager-load `thing`.
SELECT \"id\", \"name\"
FROM \"shop\".\"thing\"
WHERE \"id\" = (SELECT \"thing_id\" FROM \"shop\".\"owner\" WHERE \"id\" = :source_id)
LIMIT 1;

--! include_thing_batch_for_owner (source_ids) : (source_id, id, name?)
--- Eager-load `thing` for many owners.
SELECT \"src\".\"source_id\" AS \"source_id\", \"t\".\"id\"
FROM (
    SELECT \"id\" AS \"source_id\", \"thing_id\" AS \"target_id\"
    FROM \"shop\".\"owner\"
    WHERE \"id\" = ANY(:source_ids::uuid[])
) AS \"src\"
JOIN \"shop\".\"thing\" AS \"t\" ON \"t\".\"id\" = \"src\".\"target_id\";
";
    let parsed = parse_emitted_queries(sql);
    assert_eq!(parsed.len(), 2, "both statements must be found");
    let single = &parsed["include_thing_for_owner"];
    assert_eq!(single.params, "source_id");
    assert!(!single.body.ends_with(';'), "the terminator is stripped");
    assert!(!single.body.contains("---"));
    assert!(single.body.contains("WHERE \"id\" = :source_id)"));
    let batch = &parsed["include_thing_batch_for_owner"];
    assert_eq!(batch.params, "source_ids");
    // The longer placeholder is substituted first and the cast survives.
    let bound = bind_source_param(batch);
    assert!(bound.contains("= ANY($1::uuid[])"), "{bound}");
    assert!(!bound.contains(":source"), "{bound}");
}

#[test]
#[should_panic(expected = "unsubstituted placeholder")]
fn binding_rejects_a_placeholder_it_did_not_map() {
    let query = EmittedQuery {
        params: "source_id".into(),
        body: "SELECT 1 FROM t WHERE id = :source_id AND other = :other".into(),
    };
    let _ = bind_source_param(&query);
}

/// The generation-time half of the runtime pin: every include statement the
/// fixture emits is extractable, binds to exactly one positional parameter, and
/// carries no soft-delete marker (this fixture's graph holds no `SoftDelete`
/// policy — see the module docs).
#[tokio::test]
async fn emitted_include_queries_extract_bind_and_carry_no_marker() {
    let (_fixture, output) = generate(FLAG_ON_PROFILE).await;
    let queries = include_queries(&output);
    let names: BTreeSet<&str> = queries.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        BTreeSet::from([
            "include_customer_batch_for_invoice",
            "include_customer_batch_for_work_order",
            "include_customer_for_invoice",
            "include_customer_for_work_order",
            "include_customer_org_batch_for_invoice",
            "include_customer_org_for_invoice",
            "include_invoice_batch_for_customer",
            "include_invoice_for_customer",
            "include_organization_batch_for_customer",
            "include_organization_for_customer",
            "include_work_order_batch_for_customer",
            "include_work_order_for_customer",
        ]),
        "the fixture emits one GET + one LIST statement per implemented path"
    );
    for (name, query) in &queries {
        assert!(
            query.params == "source_id" || query.params == "source_ids",
            "{name} declares an unexpected parameter list {:?}",
            query.params
        );
        let bound = bind_source_param(query);
        assert_eq!(
            bound.matches("$1").count(),
            1,
            "{name} must bind exactly one positional parameter:\n{bound}"
        );
        assert!(
            !bound.contains("deleted_at"),
            "{name} must invent no soft-delete marker on a marker-less tree:\n{bound}"
        );
    }
}

// ── Platform bootstrap (mirrors the band this profile does not emit) ──

/// The session-role band the minimal fixture profile omits, written once here:
///
/// * `app_user` / `api_key` — the roles the emitted policies name and the
///   generated bundle flips to. The check-then-create rides a transaction
///   advisory lock because roles are CLUSTER-wide while each gate's database is
///   not (the `hooks_cornucopia_runtime_tests` precedent, separate key).
/// * `public.get_current_org_id()` — **verbatim** from
///   `templates/db/api_key_migration.tera` (the org-isolation predicates under
///   test call it).
/// * `public.enforce_api_key_scope()` — a tenancy-only STAND-IN. Its real body
///   belongs to the #169 API-key band (needs `api_keys_private`), which this
///   profile does not emit. The policies that call it are RESTRICTIVE on
///   `app_user`/`api_key`; the stand-in answers TRUE so the SCOPE axis is inert
///   and the ORG-isolation predicates under test are the emitted ones.
fn platform_bootstrap(base_user: &str) -> String {
    format!(
        r#"
CREATE EXTENSION IF NOT EXISTS pgcrypto;

DO $$
BEGIN
    PERFORM pg_advisory_xact_lock(781100001);
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'app_user') THEN
        CREATE ROLE app_user NOLOGIN;
    END IF;
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'api_key') THEN
        CREATE ROLE api_key NOLOGIN;
    END IF;
END $$;

GRANT app_user TO "{base_user}";

CREATE OR REPLACE FUNCTION public.get_current_org_id()
RETURNS uuid AS $$
BEGIN
  RETURN nullif(current_setting('app.organization_id', true), '')::uuid;
END;
$$ LANGUAGE plpgsql STABLE;

CREATE OR REPLACE FUNCTION public.enforce_api_key_scope(
    p_entity_type text, p_entity_id text, p_action text
) RETURNS boolean LANGUAGE plpgsql STABLE AS $fn$
BEGIN
    RETURN TRUE;
END;
$fn$;
"#
    )
}

/// Which emitted migration files this gate consumes, and which it
/// deliberately does not (see the module docs for the reasons).
#[derive(Debug)]
struct MigrationBand {
    applied: Vec<PathBuf>,
    skipped: Vec<String>,
}

fn is_table_ddl(name: &str) -> bool {
    SHOP_TABLES
        .iter()
        .any(|table| name.ends_with(&format!("_shop_{table}.sql")))
}

fn select_band(migrations: &Path) -> MigrationBand {
    let mut files: Vec<PathBuf> = std::fs::read_dir(migrations)
        .unwrap_or_else(|e| panic!("read {}: {e}", migrations.display()))
        .map(|entry| entry.expect("migration entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
        .collect();
    files.sort();
    let mut applied = Vec::new();
    let mut skipped = Vec::new();
    for path in files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("utf-8 migration name")
            .to_string();
        let include_rls = SHOP_TABLES
            .iter()
            .any(|table| name.ends_with(&format!("_shop_{table}_rls.sql")));
        if is_table_ddl(&name) || include_rls {
            applied.push(path);
        } else {
            skipped.push(name);
        }
    }
    // Fail loudly if the emitted band stops carrying a piece this gate needs
    // (a renamed migration would otherwise be skipped silently and the seed
    // would fail with an opaque SQL error).
    let applied_names: Vec<String> = applied
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    for table in SHOP_TABLES {
        for suffix in [
            format!("_shop_{table}.sql"),
            format!("_shop_{table}_rls.sql"),
        ] {
            assert!(
                applied_names.iter().any(|name| name.ends_with(&suffix)),
                "the emitted migration band has no `{suffix}`:\n{applied_names:#?}"
            );
        }
    }
    MigrationBand { applied, skipped }
}

// ── Seeded fixture ──

/// Two tenants' worth of rows. Every id is explicit so assertions name the row
/// they mean rather than counting.
#[derive(Debug, Clone)]
struct Seed {
    org_a: Uuid,
    org_b: Uuid,
    organization_a: Uuid,
    organization_b: Uuid,
    customer_a: Uuid,
    customer_a_deleted: Uuid,
    customer_b: Uuid,
    invoice_a1: Uuid,
    invoice_a2: Uuid,
    invoice_b: Uuid,
    invoice_deleted_target: Uuid,
    invoice_cross: Uuid,
    work_order_a1: Uuid,
    work_order_a2: Uuid,
    work_order_b: Uuid,
    work_order_soft_deleted: Uuid,
    work_order_cross: Uuid,
}

impl Seed {
    fn new() -> Self {
        Self {
            org_a: Uuid::new_v4(),
            org_b: Uuid::new_v4(),
            organization_a: Uuid::new_v4(),
            organization_b: Uuid::new_v4(),
            customer_a: Uuid::new_v4(),
            customer_a_deleted: Uuid::new_v4(),
            customer_b: Uuid::new_v4(),
            invoice_a1: Uuid::new_v4(),
            invoice_a2: Uuid::new_v4(),
            invoice_b: Uuid::new_v4(),
            invoice_deleted_target: Uuid::new_v4(),
            invoice_cross: Uuid::new_v4(),
            work_order_a1: Uuid::new_v4(),
            work_order_a2: Uuid::new_v4(),
            work_order_b: Uuid::new_v4(),
            work_order_soft_deleted: Uuid::new_v4(),
            work_order_cross: Uuid::new_v4(),
        }
    }
}

/// Insert as the migration OWNER (the base role is `BYPASSRLS`, exactly the
/// role the generated migrations run as). No trigger band is applied, so the
/// explicit `platform_organization_id` is what lands.
async fn seed_rows(client: &Client, seed: &Seed) {
    for (id, org, name) in [
        (seed.organization_a, seed.org_a, "Org A"),
        (seed.organization_b, seed.org_b, "Org B"),
    ] {
        client
            .execute(
                "INSERT INTO shop.organization (id, platform_organization_id, \"name\") \
                 VALUES ($1, $2, $3)",
                &[&id, &org, &name],
            )
            .await
            .expect("seed organization");
    }

    for (id, org, org_ref, name, deleted) in [
        (
            seed.customer_a,
            seed.org_a,
            seed.organization_a,
            "Customer A",
            false,
        ),
        (
            seed.customer_a_deleted,
            seed.org_a,
            seed.organization_a,
            "Customer A Deleted",
            true,
        ),
        (
            seed.customer_b,
            seed.org_b,
            seed.organization_b,
            "Customer B",
            false,
        ),
    ] {
        client
            .execute(
                "INSERT INTO shop.customer \
                 (id, platform_organization_id, org_id, \"name\", deleted_at) \
                 VALUES ($1, $2, $3, $4, CASE WHEN $5 THEN now() ELSE NULL END)",
                &[&id, &org, &org_ref, &name, &deleted],
            )
            .await
            .expect("seed customer");
    }

    for (id, org, customer, number) in [
        (seed.invoice_a1, seed.org_a, seed.customer_a, "INV-A1"),
        (seed.invoice_a2, seed.org_a, seed.customer_a, "INV-A2"),
        (seed.invoice_b, seed.org_b, seed.customer_b, "INV-B"),
        (
            seed.invoice_deleted_target,
            seed.org_a,
            seed.customer_a_deleted,
            "INV-A-DELETED-TARGET",
        ),
        // Org A's invoice whose REQUIRED FK points at an org-B customer.
        (seed.invoice_cross, seed.org_a, seed.customer_b, "INV-CROSS"),
    ] {
        client
            .execute(
                "INSERT INTO shop.invoice (id, platform_organization_id, customer_id, number, total) \
                 VALUES ($1, $2, $3, $4, 100)",
                &[&id, &org, &customer, &number],
            )
            .await
            .expect("seed invoice");
    }

    for (id, org, customer, reference, deleted) in [
        (
            seed.work_order_a1,
            seed.org_a,
            seed.customer_a,
            "WO-A1",
            false,
        ),
        (
            seed.work_order_a2,
            seed.org_a,
            seed.customer_a,
            "WO-A2",
            false,
        ),
        (
            seed.work_order_b,
            seed.org_b,
            seed.customer_b,
            "WO-B",
            false,
        ),
        (
            seed.work_order_soft_deleted,
            seed.org_a,
            seed.customer_a,
            "WO-A-SOFT-DELETED",
            true,
        ),
        // Org A's work order whose optional FK points at an org-B customer.
        // It shares no source with `customer_a`, so the reverse collection
        // assertions below stay exact.
        (
            seed.work_order_cross,
            seed.org_a,
            seed.customer_b,
            "WO-CROSS",
            false,
        ),
    ] {
        client
            .execute(
                "INSERT INTO shop.work_order \
                 (id, platform_organization_id, customer_id, reference, total, deleted_at) \
                 VALUES ($1, $2, $3, $4, 42, CASE WHEN $5 THEN now() ELSE NULL END)",
                &[&id, &org, &customer, &reference, &deleted],
            )
            .await
            .expect("seed work_order");
    }
}

// ── Session-scoped execution ──

enum Bind<'a> {
    Source(Uuid),
    Sources(&'a [Uuid]),
}

/// The emitted `query.tera` `set_rls_session_vars`, verbatim in shape: the five
/// GUCs in ONE statement, then the role flip. Both are transaction-local.
async fn set_rls_session_vars(tx: &tokio_postgres::Transaction<'_>, org: Uuid) {
    let api_key_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    tx.execute(
        "SELECT set_config($1, $2, true), set_config($3, $4, true), \
                set_config($5, $6, true), set_config($7, $8, true), \
                set_config($9, $10, true)",
        &[
            &"app.current_api_key",
            &api_key_id.to_string(),
            &"app.current_api_key_id",
            &api_key_id.to_string(),
            &"app.organization_id",
            &org.to_string(),
            &"app.user_id",
            &user_id.to_string(),
            &"app.role",
            &"owner",
        ],
    )
    .await
    .expect("set the five app.* GUCs (the emitted bundle)");
    tx.execute("SET LOCAL ROLE app_user", &[])
        .await
        .expect("SET LOCAL ROLE app_user (the emitted bundle)");
}

/// Run one generated include statement under an `app_user` session scoped to
/// `org`. One transaction per call, exactly like the generated wrapper.
async fn query_scoped(client: &mut Client, org: Uuid, sql: &str, bind: Bind<'_>) -> Vec<Row> {
    let tx = client.transaction().await.expect("begin scoped tx");
    set_rls_session_vars(&tx, org).await;
    let rows = match bind {
        Bind::Source(id) => tx.query(sql, &[&id]).await,
        Bind::Sources(ids) => tx.query(sql, &[&ids]).await,
    }
    .unwrap_or_else(|e| panic!("include statement failed under app_user:\n{sql}\n{e}"));
    tx.commit().await.expect("commit scoped tx");
    rows
}

fn uuid_col(row: &Row, name: &str) -> Uuid {
    row.try_get(name)
        .unwrap_or_else(|e| panic!("column {name} as uuid: {e}"))
}

fn opt_uuid_col(row: &Row, name: &str) -> Option<Uuid> {
    row.try_get(name)
        .unwrap_or_else(|e| panic!("column {name} as optional uuid: {e}"))
}

fn text_col(row: &Row, name: &str) -> String {
    row.try_get(name)
        .unwrap_or_else(|e| panic!("column {name} as text: {e}"))
}

fn ids(rows: &[Row]) -> BTreeSet<Uuid> {
    rows.iter().map(|row| uuid_col(row, "id")).collect()
}

/// The target marker suffix `render_include_sql` appends for the reverse-FK
/// (`SingleArray`) shape — `db/cornucopia_queries.rs`.
fn array_marker_suffix() -> &'static str {
    "\n  AND \"deleted_at\" IS NULL"
}

/// The target marker suffix `render_include_sql` appends for the single-scalar
/// shape: `AND "deleted_at" IS NULL` between the FK predicate and `LIMIT 1`.
fn scalar_marker_sql(body: &str) -> String {
    assert!(
        body.contains("\nLIMIT 1"),
        "the scalar include must end in LIMIT 1:\n{body}"
    );
    body.replace("\nLIMIT 1", "\n  AND \"deleted_at\" IS NULL\nLIMIT 1")
}

// ── The runtime gate ──

async fn connect(url: &str) -> Client {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls)
        .await
        .unwrap_or_else(|e| panic!("connect to {url}: {e}"));
    tokio::spawn(async move {
        // The queries carry their own error surface; the driver task only
        // completes with an error once the client is dropped.
        let _ = connection.await;
    });
    client
}

/// Apply the bootstrap + the selected band, printing what was consumed and what
/// was skipped (this is the artifact the report cites).
async fn apply_band(client: &Client, migrations: &Path, base_user: &str) {
    client
        .batch_execute(&platform_bootstrap(base_user))
        .await
        .expect("apply the test-owned platform bootstrap");

    let band = select_band(migrations);
    for path in &band.applied {
        let sql = read(path);
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        client
            .batch_execute(&sql)
            .await
            .unwrap_or_else(|e| panic!("apply emitted migration {name}: {e}"));
        println!("[band] applied  {name}");
    }
    for name in &band.skipped {
        println!("[band] skipped  {name}");
    }
}

/// Runtime pin (the `test-ops-integration` lane): execute every generated
/// include statement against real Postgres, as `app_user`, under the emitted
/// session bundle, and assert hydration + tenancy + the soft-delete contract.
#[tokio::test]
#[ignore = "needs Postgres (DATABASE_URL or the docker fallback); see the module docs"]
async fn cornucopia_includes_hydrate_under_rls() {
    // ── 1. Generate the fixture with the preview flag ON ──
    let (_fixture, output) = generate(FLAG_ON_PROFILE).await;
    let queries = include_queries(&output);
    for (name, query) in &queries {
        // The marker-free contract is asserted again at runtime so the
        // soft-delete legs below state its premise.
        assert!(
            !query.body.contains("deleted_at"),
            "{name} carries a soft-delete marker although the fixture's graph \
             has no SoftDelete policy:\n{}",
            query.body
        );
    }
    let bind = |name: &str| {
        queries
            .get(name)
            .map(bind_source_param)
            .unwrap_or_else(|| panic!("missing generated query {name}"))
    };

    // ── 2. Provision a per-run database ──
    let base = resolve_base_target()
        .await
        .expect("resolve a Postgres base target (DATABASE_URL or the docker fallback)");
    let base_user = base.user.clone();
    let db = GateDb::create(base)
        .await
        .expect("create the per-run database");
    let url = db.url();

    let mut client = connect(&url).await;

    // ── 3. Apply the emitted DDL + RLS band ──
    apply_band(&client, &output.join("migrations"), &base_user).await;

    // ── 4. Seed two tenants ──
    let seed = Seed::new();
    seed_rows(&client, &seed).await;

    // ── 5. Scalar FK (work_order.customer): GET + LIST ──
    let customer_for_work_order = bind("include_customer_for_work_order");
    let customer_batch_for_work_order = bind("include_customer_batch_for_work_order");

    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &customer_for_work_order,
        Bind::Source(seed.work_order_a1),
    )
    .await;
    assert_eq!(rows.len(), 1, "a scalar FK resolves to at most one row");
    assert_eq!(
        uuid_col(&rows[0], "id"),
        seed.customer_a,
        "the include must return the row the FK points at"
    );
    assert_eq!(text_col(&rows[0], "name"), "Customer A");
    assert_eq!(
        opt_uuid_col(&rows[0], "org_id"),
        Some(seed.organization_a),
        "the target's own FK column is projected unchanged"
    );

    // Batch: one row per source id, keyed by `source_id` (the adapter's map
    // key). The soft-deleted source hydrates too — see the soft-delete legs.
    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &customer_batch_for_work_order,
        Bind::Sources(&[
            seed.work_order_a1,
            seed.work_order_a2,
            seed.work_order_soft_deleted,
        ]),
    )
    .await;
    assert_eq!(rows.len(), 3, "one target row per requested source id");
    assert_eq!(
        rows.iter()
            .map(|row| uuid_col(row, "source_id"))
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            seed.work_order_a1,
            seed.work_order_a2,
            seed.work_order_soft_deleted,
        ]),
        "the batch rows are keyed by the SOURCE id, not the target id"
    );
    assert!(
        rows.iter()
            .all(|row| uuid_col(row, "id") == seed.customer_a),
        "every batch row carries the same target (the shared FK)"
    );

    // ── 6. Reverse FK / collection (customer.work_order): GET + LIST ──
    let work_order_for_customer = bind("include_work_order_for_customer");
    let work_order_batch_for_customer = bind("include_work_order_batch_for_customer");

    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &work_order_for_customer,
        Bind::Source(seed.customer_a),
    )
    .await;
    assert_eq!(
        ids(&rows),
        BTreeSet::from([
            seed.work_order_a1,
            seed.work_order_a2,
            seed.work_order_soft_deleted,
        ]),
        "the reverse include scans the target's FK and returns the whole \
         collection (the soft-deleted source has no marker filter)"
    );
    assert!(
        rows.iter()
            .all(|row| opt_uuid_col(row, "customer_id") == Some(seed.customer_a)),
        "the optional reverse FK is projected as the target's own column"
    );

    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &work_order_batch_for_customer,
        Bind::Sources(&[seed.customer_a]),
    )
    .await;
    assert_eq!(rows.len(), 3, "the batch groups the same three children");
    assert!(
        rows.iter()
            .all(|row| opt_uuid_col(row, "source_id") == Some(seed.customer_a)),
        "the optional reverse FK hints `source_id?` — the group key is the FK"
    );

    // The REQUIRED reverse FK (`invoice.customer`): the key hint is a plain
    // `source_id`, and the collection is every invoice pointing at the source.
    let invoice_for_customer = bind("include_invoice_for_customer");
    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &invoice_for_customer,
        Bind::Source(seed.customer_a),
    )
    .await;
    assert_eq!(
        ids(&rows),
        BTreeSet::from([seed.invoice_a1, seed.invoice_a2]),
        "the required reverse FK returns every invoice of the source"
    );

    // ── 7. Dot notation (invoice.customer.org): both hops ──
    let customer_for_invoice = bind("include_customer_for_invoice");
    let customer_org_for_invoice = bind("include_customer_org_for_invoice");
    let customer_org_batch_for_invoice = bind("include_customer_org_batch_for_invoice");

    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &customer_for_invoice,
        Bind::Source(seed.invoice_a1),
    )
    .await;
    assert_eq!(rows.len(), 1);
    assert_eq!(uuid_col(&rows[0], "id"), seed.customer_a);

    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &customer_org_for_invoice,
        Bind::Source(seed.invoice_a1),
    )
    .await;
    assert_eq!(
        rows.len(),
        1,
        "the leaf hop joins the intermediate's own FK"
    );
    assert_eq!(
        uuid_col(&rows[0], "id"),
        seed.organization_a,
        "the dot leaf is the intermediate's FK target"
    );
    assert_eq!(text_col(&rows[0], "name"), "Org A");

    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &customer_org_batch_for_invoice,
        Bind::Sources(&[seed.invoice_a1, seed.invoice_a2]),
    )
    .await;
    assert_eq!(
        rows.len(),
        2,
        "the dot batch keys one row per source invoice"
    );
    assert_eq!(
        rows.iter()
            .map(|row| uuid_col(row, "source_id"))
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([seed.invoice_a1, seed.invoice_a2]),
        "the dot batch is keyed by the SOURCE id"
    );
    assert!(
        rows.iter()
            .all(|row| uuid_col(row, "id") == seed.organization_a),
        "both source invoices resolve through the same customer to the same org"
    );

    // ── 8. Cross-tenant: an org-A source whose FK points at an org-B target ──
    //
    // The exclude statement carries NO tenant predicate: the only fence is the
    // target table's RLS. Drop `SET LOCAL ROLE app_user` / the org GUC and this
    // leg returns the org-B row — the §9 risk #1 failure mode.
    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &customer_for_work_order,
        Bind::Source(seed.work_order_cross),
    )
    .await;
    assert!(
        rows.is_empty(),
        "an org-A source whose FK points at an org-B target must hydrate nothing"
    );

    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &customer_batch_for_work_order,
        Bind::Sources(&[seed.work_order_cross, seed.work_order_a1]),
    )
    .await;
    assert_eq!(
        rows.len(),
        1,
        "the batch drops the cross-tenant pair and keeps the visible one"
    );
    assert_eq!(uuid_col(&rows[0], "source_id"), seed.work_order_a1);
    assert_eq!(uuid_col(&rows[0], "id"), seed.customer_a);

    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &customer_for_invoice,
        Bind::Source(seed.invoice_cross),
    )
    .await;
    assert!(
        rows.is_empty(),
        "the dot path's intermediate hop is equally fenced"
    );
    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &customer_org_for_invoice,
        Bind::Source(seed.invoice_cross),
    )
    .await;
    assert!(rows.is_empty(), "the dot leaf surfaces no foreign org");

    // The reverse shape's fence is per ROW, not per source: asked for an org-B
    // source id from an org-A session, the collection still contains only
    // org-A rows (the org-B invoice is invisible).
    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &invoice_for_customer,
        Bind::Source(seed.customer_b),
    )
    .await;
    assert_eq!(
        ids(&rows),
        BTreeSet::from([seed.invoice_cross]),
        "the reverse include filters its target rows by the session tenant"
    );

    // And the mirror image: the org-B session cannot see the org-A rows.
    let rows = query_scoped(
        &mut client,
        seed.org_b,
        &customer_batch_for_work_order,
        Bind::Sources(&[seed.work_order_a1, seed.work_order_cross]),
    )
    .await;
    assert!(
        rows.is_empty(),
        "org B sees none of org A's sources or targets"
    );
    let rows = query_scoped(
        &mut client,
        seed.org_b,
        &customer_for_work_order,
        Bind::Source(seed.work_order_b),
    )
    .await;
    assert_eq!(rows.len(), 1);
    assert_eq!(
        uuid_col(&rows[0], "id"),
        seed.customer_b,
        "org B's own pair still resolves"
    );

    // ── 9. Soft delete ──
    //
    // (a) The emitted SQL carries no marker (asserted at the top, on every
    //     statement): the tree has no `SoftDelete` policy, so a soft-deleted
    //     TARGET is still returned — the "no invented marker" half of the
    //     contract.
    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &customer_for_invoice,
        Bind::Source(seed.invoice_deleted_target),
    )
    .await;
    assert_eq!(
        rows.len(),
        1,
        "a marker-less tree filters no `deleted_at` on the target"
    );
    assert_eq!(uuid_col(&rows[0], "id"), seed.customer_a_deleted);

    // (b) A soft-deleted SOURCE still hydrates: the `include_deleted` decision
    //     belongs to the CRUD read that produced the source row, not to the
    //     include statement.
    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &customer_for_work_order,
        Bind::Source(seed.work_order_soft_deleted),
    )
    .await;
    assert_eq!(
        rows.len(),
        1,
        "a soft-deleted source's relation still hydrates"
    );
    assert_eq!(uuid_col(&rows[0], "id"), seed.customer_a);

    // (c) The marker suffix `render_include_sql` appends, applied to the SAME
    //     emitted statements, excludes the seeded row on this schema (the
    //     column exists on every auditable table; the predicate is
    //     `IS NULL`). This is the runtime half of the emitter KATs — the
    //     end-to-end marker arm has no reachable input today.
    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &scalar_marker_sql(&customer_for_invoice),
        Bind::Source(seed.invoice_deleted_target),
    )
    .await;
    assert!(
        rows.is_empty(),
        "the scalar marker arm must exclude a soft-deleted target"
    );

    let marked_array = format!("{work_order_for_customer}{}", array_marker_suffix());
    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &marked_array,
        Bind::Source(seed.customer_a),
    )
    .await;
    assert_eq!(
        ids(&rows),
        BTreeSet::from([seed.work_order_a1, seed.work_order_a2]),
        "the reverse-FK marker arm excludes the soft-deleted child"
    );

    let marked_scalar = scalar_marker_sql(&customer_for_work_order);
    let rows = query_scoped(
        &mut client,
        seed.org_a,
        &marked_scalar,
        Bind::Source(seed.work_order_soft_deleted),
    )
    .await;
    assert_eq!(
        rows.len(),
        1,
        "the scalar marker filters the TARGET, not the source: the soft-deleted \
         source's live customer is still returned"
    );

    db.cleanup().await;
}
