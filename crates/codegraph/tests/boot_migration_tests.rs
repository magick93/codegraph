//! One migration tracking table — boot + ops band hand-off (issue #756).
//!
//! The generated app's CLI-less boot runner (`has_admin_cli = false`) used to
//! apply the always-generated SeaORM `migration` crate on every boot, tracking
//! in `seaql_migrations`. The ops harness band recorded NOTHING, so a boot
//! over an already-migrated database re-applied every file and the second pass
//! of a codelist seed died on the #700 `set_org_id_from_session` trigger (no
//! session organization at boot ⇒ the zero tenant rewrites to NULL ⇒ NOT NULL
//! violation). This file pins the fix:
//!
//! * ONE tracking table — `platform._migrations (name, checksum, applied_at)`;
//!   checksum = FNV-1a 64-bit hex over the ORIGINAL file bytes;
//! * the ops band records the band after every phase succeeds;
//! * boot skips tracked files, refuses a changed file by NAME (checksum drift
//!   is a hard error, never a silent re-apply), and ADOPTS a legacy band:
//!   `seaql_migrations` (SeaORM / sea-orm-cli), `schema_migrations` (the old
//!   admin-CLI engine), or — for a raw old harness band — an object-existence
//!   probe per file;
//! * a second boot over the same band applies nothing.
//!
//! # Node-free pins (PR CI)
//!
//! The first five tests render the scaffold templates directly — they are the
//! fast contract pins (sqlite branch, the named drift error, the CLI engine's
//! shared table, the boot wiring). They need no database.
//!
//! # The live gate (ignored)
//!
//! `cargo test -p codegraph --test boot_migration_tests -- --ignored --nocapture`
//!
//! Scaffolds the tiny init project, swaps in a model with a codelist (the
//! issue's repro needs a seed), flips `has_admin_cli` to false, generates and
//! builds the app once, then drives the five legs against a fresh scratch
//! database per run:
//!
//! (a) ops band → boot: succeeds, every band file tracked, NOTHING re-applied;
//! (b) fresh empty DB: first boot records every file, second boot applies 0;
//! (c) a `seaql_migrations` band (the old SeaORM tracking) is adopted;
//! (d) a raw band (no tracking tables) is adopted by the object probe;
//! (e) a tampered tracked file fails with the NAMED drift error and the
//!     tracking table is untouched.
//!
//! Requires Postgres. The band needs the Supabase image's `auth` schema
//! (`basejump` install), so the gate uses
//! `test_framework::postgres::resolve_base_target()` — which falls back to the
//! pinned Supabase-image container on 127.0.0.1:15432.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use codegraph_ops::db::{psql_exec, psql_query};
use codegraph_ops::migrate::{GrantOptions, run_api_migrations_with_options};
use codegraph_ops::pg::PgTarget;

use codegraph::generate::db::dialect::DatabaseTarget;
use codegraph::generate::template_engine::create_tera;
use codegraph::generate::{DatabaseConfig, ProjectConfig, render_template_with_project};
use codegraph_generate::scaffold::generator::ScaffoldContext;

#[path = "test_framework/mod.rs"]
mod test_framework;

use test_framework::axum_server::{BootedServer, boot_server, build_app};
use test_framework::postgres::{GateDb, resolve_base_target};

// ── helpers shared by the pins + the live gate ──────────────────────────────

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    manifest_dir()
        .parent()
        .and_then(Path::parent)
        .expect("codegraph crate lives at <repo>/crates/codegraph")
        .to_path_buf()
}

fn project_config(database_target: DatabaseTarget) -> ProjectConfig {
    ProjectConfig {
        identity: codegraph::generate::IdentityConfig {
            app_name: "boot-app".into(),
            generator_name: "codegraph-boot-gate".into(),
            ..Default::default()
        },
        database: DatabaseConfig {
            database_target,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn render<C: serde::Serialize>(template: &str, context: &C, project: &ProjectConfig) -> String {
    let tera = create_tera(Path::new("unused")).expect("embedded templates must load");
    render_template_with_project(&tera, template, context, project)
        .unwrap_or_else(|e| panic!("render {template}: {e}"))
}

/// The scaffold context the boot template needs (it only reads `project`), so
/// an empty per-run context is enough.
fn empty_scaffold_context(has_admin_cli: bool) -> ScaffoldContext {
    ScaffoldContext {
        app_name: "boot-app".into(),
        domains: vec![],
        grant_schemas: vec!["common".into(), "platform".into()],
        codegraph_workflow_path: String::new(),
        type_contracts_path: String::new(),
        domain_types_path: "crates/domain-types".into(),
        hooks_api_path: String::new(),
        extensions_path: String::new(),
        app_config_path: String::new(),
        decision_engine_path: String::new(),
        has_webhooks: false,
        has_reports: false,
        has_grpc: false,
        has_atproto: false,
        has_fern: false,
        has_cli: false,
        has_test_gen: false,
        has_auth_rate_limit: false,
        has_admin_cli,
        has_labels: false,
        has_seed: false,
        has_events: false,
        has_function_events: false,
        has_media: false,
        has_machine_guards: false,
        seed_crate_path: String::new(),
        migration_strategy: "sea-orm".into(),
    }
}

fn fnv1a_64_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// Every `.sql` migration: `(filename, checksum)` in filename order.
fn migration_checksums(dir: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read migrations dir {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sql"))
        .collect();
    entries.sort();
    for path in entries {
        let name = path
            .file_name()
            .expect("migration file has a name")
            .to_string_lossy()
            .into_owned();
        let bytes = std::fs::read(&path).expect("read migration file");
        out.insert(name, fnv1a_64_hex(&bytes));
    }
    out
}

// ── node-free pins ──────────────────────────────────────────────────────────

/// The emitted boot runner must carry BOTH dialect branches: Postgres reaches
/// `platform._migrations` + the shared advisory lock; the sqlite target uses a
/// top-level `_migrations` table and no schema/advisory machinery.
#[test]
fn boot_runner_carries_the_sqlite_branch() {
    let postgres = render(
        "scaffold/boot_migrate.tera",
        &serde_json::json!({}),
        &project_config(DatabaseTarget::Postgres),
    );
    assert!(
        postgres.contains("const TRACKING_TABLE: &str = \"platform._migrations\";"),
        "the Postgres boot runner must use platform._migrations"
    );
    assert!(
        postgres.contains("CREATE SCHEMA IF NOT EXISTS platform"),
        "the Postgres boot runner must create the platform schema"
    );
    assert!(
        postgres.contains("pg_advisory_lock"),
        "the Postgres boot runner must take the harness advisory lock"
    );
    assert!(
        postgres.contains("pg_advisory_unlock"),
        "the Postgres boot runner must release the harness advisory lock"
    );
    assert!(
        postgres.contains("to_regclass"),
        "the Postgres boot runner must probe object existence for raw bands"
    );

    let sqlite = render(
        "scaffold/boot_migrate.tera",
        &serde_json::json!({}),
        &project_config(DatabaseTarget::Sqlite),
    );
    assert!(
        sqlite.contains("const BACKEND: DatabaseBackend = DatabaseBackend::Sqlite;"),
        "the sqlite boot runner must use the Sqlite backend"
    );
    assert!(
        sqlite.contains("CREATE TABLE IF NOT EXISTS _migrations"),
        "the sqlite boot runner must create a top-level _migrations table"
    );
    assert!(
        sqlite.contains("applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP"),
        "the sqlite tracking table must use sqlite types"
    );
    assert!(
        !sqlite.contains("CREATE SCHEMA IF NOT EXISTS platform"),
        "sqlite has no schemas"
    );
    assert!(
        !sqlite.contains("pg_advisory_lock"),
        "sqlite has no advisory locks"
    );
}

/// Checksum drift is a HARD, NAMED error — the message must name the file and
/// both checksums so a boot failure is actionable (never a silent re-apply).
#[test]
fn boot_runner_emits_the_named_drift_error() {
    for target in [DatabaseTarget::Postgres, DatabaseTarget::Sqlite] {
        let content = render(
            "scaffold/boot_migrate.tera",
            &serde_json::json!({}),
            &project_config(target),
        );
        assert!(
            content.contains("checksum mismatch for {name}: recorded {recorded}, current"),
            "the drift error must name the file and both checksums"
        );
        assert!(
            content.contains("refusing to re-apply it over"),
            "the drift error must state the refusal"
        );
        assert!(
            content.contains("fnv1a_64_hex"),
            "checksums must be FNV-1a 64-bit hex, computed inline"
        );
    }
}

/// The admin-CLI engine shares the ONE tracking table. It must no longer OWN
/// a `schema_migrations` table (no CREATE/INSERT/SELECT-as-tracking) — but it
/// still READS a legacy `schema_migrations` band to adopt it.
#[test]
fn cli_engine_uses_the_shared_tracking_table_and_adopts_the_legacy_band() {
    let content = render(
        "scaffold/migration.tera",
        &serde_json::json!({ "migration_strategy": "sea-orm" }),
        &project_config(DatabaseTarget::Postgres),
    );
    assert!(
        content.contains("platform._migrations"),
        "the CLI engine must record into the shared tracking table"
    );
    assert!(
        content.contains("fnv1a_64_hex"),
        "the CLI engine must record the same FNV-1a checksums"
    );
    assert!(
        content.contains("checksum mismatch for {fname}: recorded {recorded}, current"),
        "the CLI engine must reject a changed file by name"
    );
    assert!(
        !content.contains("CREATE TABLE IF NOT EXISTS schema_migrations"),
        "the CLI engine must no longer create its own tracking table"
    );
    assert!(
        !content.contains("INSERT INTO schema_migrations"),
        "the CLI engine must no longer record into schema_migrations"
    );
    assert!(
        !content.contains("SELECT 1 FROM schema_migrations WHERE version"),
        "the CLI engine must no longer use schema_migrations as its tracker"
    );
    assert!(
        content.contains("to_regclass('schema_migrations')"),
        "the CLI engine must ADOPT a legacy schema_migrations band"
    );
}

/// server.rs wires the CLI-less boot to the new runner, and the admin-CLI
/// branch keeps its own engine. The SeaORM `MigratorTrait` import is gone from
/// the CLI-less branch.
#[test]
fn server_boot_branch_calls_the_boot_runner() {
    let cli_less = render(
        "scaffold/server.tera",
        &empty_scaffold_context(false),
        &project_config(DatabaseTarget::Postgres),
    );
    assert!(
        cli_less.contains("crate::boot_migrate::run_boot_migrations(&db).await"),
        "the CLI-less boot branch must call the boot runner"
    );
    assert!(
        !cli_less.contains("Migrator::up"),
        "the CLI-less boot branch must not run the SeaORM Migrator"
    );
    assert!(
        !cli_less.contains("MigratorTrait"),
        "the CLI-less boot branch must not import the SeaORM migrator"
    );

    let with_cli = render(
        "scaffold/server.tera",
        &empty_scaffold_context(true),
        &project_config(DatabaseTarget::Postgres),
    );
    assert!(
        with_cli.contains("crate::migration::run_migrations()"),
        "the admin-CLI boot branch keeps its own engine"
    );
    assert!(
        !with_cli.contains("crate::boot_migrate::run_boot_migrations"),
        "the admin-CLI tree must not call the CLI-less runner"
    );
}

// ── the live gate ───────────────────────────────────────────────────────────

/// Any value satisfies the auth middleware at boot (the gate sends no JWT).
const GATE_JWT_SECRET: &str = "super-secret-jwt-token-with-at-least-32-characters-long";

/// The generated app + its built binary, produced once per test process.
struct PreparedApp {
    output: PathBuf,
    bin: PathBuf,
    logs: PathBuf,
}

static PREPARED: tokio::sync::OnceCell<PreparedApp> = tokio::sync::OnceCell::const_new();

fn gate_root() -> PathBuf {
    workspace_root().join("target").join("boot-migration-gate")
}

/// The tiny model: one class + one enum. The enum is load-bearing — its
/// codelist seed migration is what the issue's repro re-applied, and the
/// #700 trigger is what made the second pass fail.
const MODEL: &str = r#"package common

enum TodoPriority {
    Low as "L" = 0
    High as "H" = 1
}

/// A single todo item.
class TodoItemType {
    /// Short title of the item.
    String title

    /// How urgent the item is.
    TodoPriority priority
}
"#;

fn binary_name(cargo_toml: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(cargo_toml)
        .map_err(|e| format!("read {}: {e}", cargo_toml.display()))?;
    let mut in_package = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if in_package
            && let Some(rest) = trimmed.strip_prefix("name")
            && let Some(value) = rest.trim_start().strip_prefix('=')
        {
            return Ok(value.trim().trim_matches('"').to_string());
        }
    }
    Err(format!("no [package] name in {}", cargo_toml.display()))
}

async fn prepared_app() -> Result<&'static PreparedApp, String> {
    PREPARED
        .get_or_try_init(|| async {
            let root = gate_root();
            let project = root.join("boot-app");
            // Sweep the previous generation's source; keep `cargo-target` so a
            // warm rerun does not rebuild the whole app (the #694 discipline).
            if project.exists() {
                std::fs::remove_dir_all(&project)
                    .map_err(|e| format!("sweep {}: {e}", project.display()))?;
            }
            std::fs::create_dir_all(&root).map_err(|e| format!("mkdir gate root: {e}"))?;

            codegraph::init::commands::cmd_init(&codegraph::init::commands::InitArgs {
                name: Some("boot-app".to_string()),
                output_dir: root.clone(),
                domains: vec!["common".to_string()],
                database_target: "postgres".to_string(),
                persistence_provider: "sea_orm".to_string(),
                deployment_topology: "monolith".to_string(),
                grpc: false,
                ifml: false,
                ops: false,
                rosetta: false,
                rev: Some("abc123".to_string()),
                codegraph_path: Some(workspace_root()),
                force: true,
                template_dirs: vec![],
            })
            .map_err(|e| format!("init scaffold: {e}"))?;

            // A model with a codelist (the repro's seed).
            std::fs::write(project.join("model/common.mox"), MODEL)
                .map_err(|e| format!("write model: {e}"))?;

            // CLI-less: the old fallback boot path (server.tera's else branch).
            let profiles = project.join("profiles.toml");
            let text = std::fs::read_to_string(&profiles)
                .map_err(|e| format!("read profiles.toml: {e}"))?;
            let patched = text.replace("has_admin_cli = true", "has_admin_cli = false");
            if patched.contains("has_admin_cli = true")
                || !patched.contains("has_admin_cli = false")
            {
                return Err(
                    "the init scaffold no longer writes `has_admin_cli = true` — update the gate's \
                     profile patch"
                        .to_string(),
                );
            }
            std::fs::write(&profiles, patched).map_err(|e| format!("write profiles.toml: {e}"))?;

            let output = project.join("generated");
            let mox_files = vec![project.join("model/common.mox")];
            codegraph::driver::run(codegraph::driver::RunArgs {
                integration_manifests: &[],
                schemas: None,
                classifier: None,
                config_path: &project.join("domains.toml"),
                output: &output,
                extension_points_path: None,
                profile_name: "default",
                variant: None,
                profiles_config_path: Some(project.join("profiles.toml")),
                no_post_gen: true,
                template_dir: &[],
                ifml_files: &[],
                openapi_files: &[],
                mox_files: &mox_files,
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
            .map_err(|e| format!("generate app: {e}"))?;

            if !output.join("src/boot_migrate.rs").is_file() {
                return Err("the CLI-less app must emit src/boot_migrate.rs".to_string());
            }
            if output.join("src/migration.rs").is_file() {
                return Err("the CLI-less app must not emit src/migration.rs".to_string());
            }
            if !output.join("migrations").is_dir() {
                return Err("the generated app must carry migrations/".to_string());
            }

            let logs = root.join("logs");
            std::fs::create_dir_all(&logs).map_err(|e| format!("mkdir logs: {e}"))?;
            let cargo_toml = output.join("Cargo.toml");
            let bin_name = binary_name(&cargo_toml)?;
            let bin = build_app(
                &cargo_toml,
                &output,
                &root.join("cargo-target"),
                &logs,
                &bin_name,
            )?;
            // The admin-CLI engine (`src/migration.rs`, `has_admin_cli = true`)
            // is the OTHER half of the unification; it must compile too (the
            // #551 policy). Deps are cached by now, so this is one app-crate
            // check.
            assert_cli_variant_compiles(&root, &logs).await?;
            Ok(PreparedApp { output, bin, logs })
        })
        .await
}

/// Generate a second, ADMIN-CLI scaffold (the init default) and `cargo check`
/// it — the emitted `src/migration.rs` must compile with the shared tracking
/// table.
async fn assert_cli_variant_compiles(root: &Path, logs: &Path) -> Result<(), String> {
    let project = root.join("boot-app-cli");
    if project.exists() {
        std::fs::remove_dir_all(&project)
            .map_err(|e| format!("sweep {}: {e}", project.display()))?;
    }
    codegraph::init::commands::cmd_init(&codegraph::init::commands::InitArgs {
        name: Some("boot-app-cli".to_string()),
        output_dir: root.to_path_buf(),
        domains: vec!["common".to_string()],
        database_target: "postgres".to_string(),
        persistence_provider: "sea_orm".to_string(),
        deployment_topology: "monolith".to_string(),
        grpc: false,
        ifml: false,
        ops: false,
        rosetta: false,
        rev: Some("abc123".to_string()),
        codegraph_path: Some(workspace_root()),
        force: true,
        template_dirs: vec![],
    })
    .map_err(|e| format!("init CLI scaffold: {e}"))?;
    // `cmd_init --force` re-creates both projects under the same root; only the
    // CLI variant is generated here, so restore the CLI-less project's model.
    std::fs::write(project.join("model/common.mox"), MODEL)
        .map_err(|e| format!("write CLI model: {e}"))?;
    let output = project.join("generated");
    let mox_files = vec![project.join("model/common.mox")];
    codegraph::driver::run(codegraph::driver::RunArgs {
        integration_manifests: &[],
        schemas: None,
        classifier: None,
        config_path: &project.join("domains.toml"),
        output: &output,
        extension_points_path: None,
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(project.join("profiles.toml")),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &mox_files,
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
    .map_err(|e| format!("generate CLI app: {e}"))?;
    if !output.join("src/migration.rs").is_file() {
        return Err("the admin-CLI app must emit src/migration.rs".to_string());
    }
    if output.join("src/boot_migrate.rs").is_file() {
        return Err("the admin-CLI app must not emit src/boot_migrate.rs".to_string());
    }
    let target_dir = root.join("cargo-target").to_string_lossy().to_string();
    let manifest = output.join("Cargo.toml").to_string_lossy().to_string();
    let ok = test_framework::process::run(
        &output,
        "cargo",
        &["check", "--manifest-path", &manifest],
        &[("CARGO_TARGET_DIR", target_dir.as_str())],
        std::time::Duration::from_secs(1200),
        logs,
    )?;
    if !ok {
        return Err(format!(
            "cargo check of the admin-CLI app failed (see {}/cargo-check.log)",
            logs.display()
        ));
    }
    Ok(())
}

/// One gate scratch database with the plain-Postgres bootstrap applied.
struct Scratch {
    db: GateDb,
    target: PgTarget,
}

/// The Supabase image's `auth` shim, in the per-run database. `CREATE
/// DATABASE` clones `template1`, which does NOT carry the image's `auth`
/// schema (it is installed in the `postgres` database), but the generated
/// `0001_basejump_install.sql` references `auth.users` / `auth.uid()` and the
/// app's boot applies whole files strictly. This is faithful test scaffolding,
/// not a production path.
const AUTH_SHIM_SQL: &str = r#"
CREATE EXTENSION IF NOT EXISTS pgmq;
CREATE SCHEMA IF NOT EXISTS auth;
CREATE TABLE IF NOT EXISTS auth.users (id uuid PRIMARY KEY, email text);
CREATE OR REPLACE FUNCTION auth.uid() RETURNS uuid
    LANGUAGE sql STABLE AS $$
    SELECT nullif(current_setting('request.jwt.claim.sub', true), '')::uuid
$$;
"#;

/// `GateDb::create` names the scratch database from a millisecond clock, so
/// concurrent test threads collide; the prelude also creates cluster-wide
/// roles, which two preludes must not race. Serialize both, then let the legs
/// run concurrently.
static SCRATCH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn scratch(base: PgTarget) -> Result<Scratch, String> {
    let _guard = SCRATCH_LOCK.lock().await;
    let mut last = String::new();
    for _ in 0..20 {
        let db = match GateDb::create(base.clone()).await {
            Ok(db) => db,
            Err(e) if e.contains("already exists") => {
                last = e;
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                continue;
            }
            Err(e) => return Err(e),
        };
        let target = match PgTarget::from_url(&db.url()) {
            Ok(target) => target,
            Err(e) => {
                db.cleanup().await;
                return Err(e.to_string());
            }
        };
        // Install pgmq BEFORE the prelude's `CREATE SCHEMA pgmq`: the prelude
        // creates that schema as the ADMIN role, and the app role could then
        // not call `pgmq.create` the generated band issues. Owning the
        // extension here is the faithful Supabase-image shape (pgmq ships
        // installed on the image).
        if let Err(e) = psql_exec(&target, AUTH_SHIM_SQL).await {
            db.cleanup().await;
            return Err(format!("apply the gate auth/pgmq shim: {e}"));
        }
        if let Err(e) = db.apply_prelude().await {
            db.cleanup().await;
            return Err(format!("apply gate prelude: {e}"));
        }
        // The prelude's no-op `public.emit_domain_event()` stub would make the
        // adoption probe believe `0003_pgmq_setup.sql` already ran. A real
        // Supabase database has no such stub (the generated migration creates
        // it), so drop it for a faithful fresh state.
        if let Err(e) = psql_exec(
            &target,
            "DROP FUNCTION IF EXISTS public.emit_domain_event();",
        )
        .await
        {
            db.cleanup().await;
            return Err(format!("drop the prelude event stub: {e}"));
        }
        return Ok(Scratch { db, target });
    }
    Err(format!(
        "could not create a unique scratch database: {last}"
    ))
}

/// Boot the app over `migrations_dir`; returns the booted server (dropped at
/// the end of the leg).
async fn boot(
    app: &PreparedApp,
    scratch: &Scratch,
    migrations_dir: &Path,
    label: &str,
) -> Result<BootedServer, String> {
    let url = scratch.db.url();
    let migrations = migrations_dir.to_string_lossy().to_string();
    boot_server(
        &app.bin,
        &[
            ("DATABASE_URL", url.as_str()),
            ("MIGRATIONS_DIR", migrations.as_str()),
            ("APP_NAME", "boot-app"),
            // The auth middleware refuses to boot without a JWT secret; the
            // gate never presents a JWT, so any value is fine.
            ("SUPABASE_JWT_SECRET", GATE_JWT_SECRET),
            ("SUPABASE_URL", "http://127.0.0.1:54321"),
        ],
        label,
        &app.logs,
    )
    .await
}

fn app_log(app: &PreparedApp, label: &str) -> String {
    std::fs::read_to_string(app.logs.join(format!("{label}.log"))).unwrap_or_default()
}

/// `platform._migrations` as `name -> (checksum, applied_at::text)`.
async fn tracking_rows(target: &PgTarget) -> Result<BTreeMap<String, (String, String)>, String> {
    let out = psql_query(
        target,
        "SELECT name || E'\\t' || checksum || E'\\t' || applied_at::text \
         FROM platform._migrations ORDER BY name",
    )
    .await
    .map_err(|e| format!("read platform._migrations: {e}"))?;
    let mut rows = BTreeMap::new();
    for line in out.lines() {
        let mut parts = line.splitn(3, '\t');
        let name = parts.next().unwrap_or_default().to_string();
        if name.is_empty() {
            continue;
        }
        let checksum = parts.next().unwrap_or_default().to_string();
        let applied_at = parts.next().unwrap_or_default().to_string();
        rows.insert(name, (checksum, applied_at));
    }
    Ok(rows)
}

fn assert_tracks_every_file(
    tracked: &BTreeMap<String, (String, String)>,
    checksums: &BTreeMap<String, String>,
) {
    let tracked_names: BTreeSet<&String> = tracked.keys().collect();
    let file_names: BTreeSet<&String> = checksums.keys().collect();
    assert_eq!(
        tracked_names, file_names,
        "platform._migrations must track exactly the migration files (missing/extra above)"
    );
    for (name, checksum) in checksums {
        let (recorded, _) = tracked
            .get(name)
            .unwrap_or_else(|| panic!("{name} missing from platform._migrations"));
        assert_eq!(
            recorded, checksum,
            "{name}: recorded checksum must be the FNV-1a of the original file bytes"
        );
    }
}

#[tokio::test]
#[ignore = "requires Postgres (Supabase image) + a cargo build of the generated app"]
async fn harness_band_then_boot_applies_nothing() {
    let Some(base) = resolve_base_target().await.ok() else {
        eprintln!("skipping: no reachable Postgres base target");
        return;
    };
    let app = prepared_app().await.expect("prepare generated app");
    let migrations = app.output.join("migrations");
    let checksums = migration_checksums(&migrations);

    let scratch = scratch(base).await.expect("scratch database");
    let result = async {
        // (a) the ops harness band (records the ONE tracking table).
        run_api_migrations_with_options(&migrations, &scratch.target, &GrantOptions::default())
            .await
            .map_err(|e| format!("ops band failed: {e}"))?;

        let before = tracking_rows(&scratch.target).await?;
        assert_tracks_every_file(&before, &checksums);

        // Boot over the banded database — the #756 repro.
        let server = boot(app, &scratch, &migrations, "band-then-boot")
            .await
            .map_err(|e| format!("boot over an already-migrated database failed: {e}"))?;
        let log = app_log(app, "band-then-boot");
        assert!(
            log.contains("boot-migrations: 0 applied"),
            "the boot must apply NOTHING over the band; log tail:\n{log}"
        );
        assert!(
            !log.contains("Applied: "),
            "no file may be re-applied; log tail:\n{log}"
        );

        let after = tracking_rows(&scratch.target).await?;
        assert_eq!(
            before, after,
            "the tracking table (checksums AND applied_at) must be untouched by a no-op boot"
        );
        drop(server);
        Ok::<(), String>(())
    }
    .await;

    scratch.db.cleanup().await;
    result.expect("harness-band boot leg");
}

#[tokio::test]
#[ignore = "requires Postgres (Supabase image) + a cargo build of the generated app"]
async fn fresh_boot_records_and_second_boot_is_a_noop() {
    let Some(base) = resolve_base_target().await.ok() else {
        eprintln!("skipping: no reachable Postgres base target");
        return;
    };
    let app = prepared_app().await.expect("prepare generated app");
    let migrations = app.output.join("migrations");
    let checksums = migration_checksums(&migrations);

    let scratch = scratch(base).await.expect("scratch database");
    let result = async {
        // First boot on a fresh (prelude-only) database: applies everything.
        let server = boot(app, &scratch, &migrations, "fresh-first-boot")
            .await
            .map_err(|e| format!("first boot on a fresh database failed: {e}"))?;
        drop(server);
        let first_log = app_log(app, "fresh-first-boot");
        assert!(
            first_log.contains(&format!("boot-migrations: {} applied", checksums.len())),
            "the first boot on a fresh database must apply the whole band; log tail:\n{first_log}"
        );
        let first = tracking_rows(&scratch.target).await?;
        assert_tracks_every_file(&first, &checksums);

        // The seeded codelist row count must survive the second boot untouched.
        let codelist_table: String = psql_query(
            &scratch.target,
            "SELECT table_name FROM information_schema.columns \
             WHERE table_schema = 'common' AND column_name = 'code' \
             ORDER BY table_name LIMIT 1",
        )
        .await
        .map_err(|e| format!("find the codelist table: {e}"))?;
        assert!(
            !codelist_table.trim().is_empty(),
            "the fixture model must generate a codelist (the issue's repro needs a seed)"
        );
        let codelist_table = codelist_table.trim();
        let codelist_sql = format!("SELECT count(*) FROM common.\"{codelist_table}\"");
        let rows_before: String = psql_query(&scratch.target, &codelist_sql)
            .await
            .map_err(|e| format!("count codelist rows: {e}"))?;

        // Second boot: nothing to apply.
        let server = boot(app, &scratch, &migrations, "fresh-second-boot")
            .await
            .map_err(|e| format!("second boot failed: {e}"))?;
        let second_log = app_log(app, "fresh-second-boot");
        assert!(
            second_log.contains("boot-migrations: 0 applied"),
            "the second boot must apply nothing; log tail:\n{second_log}"
        );
        assert!(
            !second_log.contains("Applied: "),
            "no file may be re-applied on the second boot; log tail:\n{second_log}"
        );
        drop(server);

        let second = tracking_rows(&scratch.target).await?;
        assert_eq!(
            first, second,
            "the second boot must not touch tracking state"
        );
        let rows_after: String = psql_query(&scratch.target, &codelist_sql)
            .await
            .map_err(|e| format!("count codelist rows after boot 2: {e}"))?;
        assert_eq!(
            rows_before, rows_after,
            "a no-op boot must not duplicate seed rows in common.{codelist_table}"
        );
        Ok::<(), String>(())
    }
    .await;

    scratch.db.cleanup().await;
    result.expect("fresh-boot leg");
}

#[tokio::test]
#[ignore = "requires Postgres (Supabase image) + a cargo build of the generated app"]
async fn seaql_migrations_band_is_adopted() {
    let Some(base) = resolve_base_target().await.ok() else {
        eprintln!("skipping: no reachable Postgres base target");
        return;
    };
    let app = prepared_app().await.expect("prepare generated app");
    let migrations = app.output.join("migrations");
    let checksums = migration_checksums(&migrations);

    let scratch = scratch(base).await.expect("scratch database");
    let result = async {
        run_api_migrations_with_options(&migrations, &scratch.target, &GrantOptions::default())
            .await
            .map_err(|e| format!("ops band failed: {e}"))?;

        // Turn the band into an OLD SeaORM band: no shared tracking, and a
        // `seaql_migrations` table naming every file (the version is the full
        // filename — RawSqlMigration::name).
        psql_exec(&scratch.target, "DROP TABLE platform._migrations;")
            .await
            .map_err(|e| format!("drop shared tracking table: {e}"))?;
        psql_exec(
            &scratch.target,
            "CREATE TABLE seaql_migrations (\
             version VARCHAR(255) PRIMARY KEY, applied_at BIGINT NOT NULL DEFAULT 0);",
        )
        .await
        .map_err(|e| format!("create seaql_migrations: {e}"))?;
        let values: Vec<String> = checksums
            .keys()
            .map(|name| format!("('{}')", name.replace('\'', "''")))
            .collect();
        psql_exec(
            &scratch.target,
            &format!(
                "INSERT INTO seaql_migrations (version) VALUES {};",
                values.join(", ")
            ),
        )
        .await
        .map_err(|e| format!("populate seaql_migrations: {e}"))?;

        let server = boot(app, &scratch, &migrations, "seaql-adopt")
            .await
            .map_err(|e| format!("boot over a seaql_migrations band failed: {e}"))?;
        let log = app_log(app, "seaql-adopt");
        assert!(
            log.contains("boot-migrations: 0 applied"),
            "the SeaORM band must be adopted, not re-applied; log tail:\n{log}"
        );
        assert!(
            !log.contains("Applied: "),
            "no file may be re-applied after adoption; log tail:\n{log}"
        );
        drop(server);

        let tracked = tracking_rows(&scratch.target).await?;
        assert_tracks_every_file(&tracked, &checksums);
        Ok::<(), String>(())
    }
    .await;

    scratch.db.cleanup().await;
    result.expect("seaql-adoption leg");
}

#[tokio::test]
#[ignore = "requires Postgres (Supabase image) + a cargo build of the generated app"]
async fn raw_band_is_adopted_by_the_object_probe() {
    let Some(base) = resolve_base_target().await.ok() else {
        eprintln!("skipping: no reachable Postgres base target");
        return;
    };
    let app = prepared_app().await.expect("prepare generated app");
    let migrations = app.output.join("migrations");
    let checksums = migration_checksums(&migrations);

    let scratch = scratch(base).await.expect("scratch database");
    let result = async {
        // A raw old harness band: applied objects, NO tracking table at all.
        run_api_migrations_with_options(&migrations, &scratch.target, &GrantOptions::default())
            .await
            .map_err(|e| format!("ops band failed: {e}"))?;
        psql_exec(&scratch.target, "DROP TABLE platform._migrations;")
            .await
            .map_err(|e| format!("drop shared tracking table: {e}"))?;

        let server = boot(app, &scratch, &migrations, "raw-adopt")
            .await
            .map_err(|e| format!("boot over a raw band failed: {e}"))?;
        let log = app_log(app, "raw-adopt");
        assert!(
            log.contains("boot-migrations: 0 applied"),
            "the raw band must be adopted by the object probe; log tail:\n{log}"
        );
        assert!(
            !log.contains("Applied: "),
            "no file may be re-applied after raw-band adoption; log tail:\n{log}"
        );
        drop(server);

        let tracked = tracking_rows(&scratch.target).await?;
        assert_tracks_every_file(&tracked, &checksums);
        Ok::<(), String>(())
    }
    .await;

    scratch.db.cleanup().await;
    result.expect("raw-adoption leg");
}

#[tokio::test]
#[ignore = "requires Postgres (Supabase image) + a cargo build of the generated app"]
async fn checksum_drift_is_a_named_hard_error_and_leaves_the_db_untouched() {
    let Some(base) = resolve_base_target().await.ok() else {
        eprintln!("skipping: no reachable Postgres base target");
        return;
    };
    let app = prepared_app().await.expect("prepare generated app");
    let migrations = app.output.join("migrations");
    let checksums = migration_checksums(&migrations);
    let tampered_name = checksums
        .keys()
        .next()
        .expect("the fixture band has migration files")
        .clone();

    // Tamper a COPY of the band so the other legs' migrations dir is untouched.
    let tampered = tempfile::TempDir::new().expect("temp dir for the tampered band");
    for entry in std::fs::read_dir(&migrations).expect("read migrations") {
        let path = entry.expect("migration entry").path();
        let dest = tampered.path().join(path.file_name().expect("file name"));
        std::fs::copy(&path, &dest).expect("copy migration file");
    }
    let tampered_file = tampered.path().join(&tampered_name);
    let mut bytes = std::fs::read(&tampered_file).expect("read tampered file");
    bytes.extend_from_slice(b"\n-- tampered by the boot-migration gate\n");
    std::fs::write(&tampered_file, bytes).expect("write tampered file");

    let scratch = scratch(base).await.expect("scratch database");
    let result = async {
        run_api_migrations_with_options(&migrations, &scratch.target, &GrantOptions::default())
            .await
            .map_err(|e| format!("ops band failed: {e}"))?;
        let before = tracking_rows(&scratch.target).await?;
        assert_tracks_every_file(&before, &checksums);

        let err = match boot(app, &scratch, tampered.path(), "drift").await {
            Ok(_) => panic!("a tampered tracked file must fail the boot"),
            Err(e) => e,
        };
        assert!(
            err.contains(&tampered_name),
            "the drift error must NAME the file ({tampered_name}); got:\n{err}"
        );
        assert!(
            err.contains("checksum mismatch"),
            "the drift error must say the checksum drifted; got:\n{err}"
        );
        assert!(
            err.contains("refusing to re-apply"),
            "the drift error must state the refusal; got:\n{err}"
        );

        let after = tracking_rows(&scratch.target).await?;
        assert_eq!(
            before, after,
            "a refused boot must leave the tracking table untouched"
        );
        // The band itself must still be live (the tampered file was never run).
        let table_count: String = psql_query(
            &scratch.target,
            "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'common'",
        )
        .await
        .map_err(|e| format!("count common tables: {e}"))?;
        assert_ne!(
            table_count.trim(),
            "0",
            "the band's tables must still exist after the refusal"
        );
        Ok::<(), String>(())
    }
    .await;

    scratch.db.cleanup().await;
    result.expect("checksum-drift leg");
}
