//! The generated error plane classifies database failures by SQLSTATE
//! (issue #828, the real fix behind codegraph#810).
//!
//! Why this binary exists: every generated classifier used to decide from
//! `Display` text, and on `tokio-postgres` >= 0.7.15 a server error renders
//! as the bare string `db error` — the SQLSTATE/message/detail/hint live on
//! the typed `DbError` in `source()`. On the workers slice that made every
//! classifier dead: a scope-denied create (P0403 `INSUFFICIENT_SCOPE`)
//! answered `500 "Failed to create …: db error"`, and a unique/FK refusal
//! was an opaque 500 instead of 409/422. codegraph#810 spent a multi-day
//! investigation on a machine/kernel path because of it.
//!
//! The pins:
//!
//! - **Node-free** (`generated_error_plane_is_assembly_ready`, PR CI): the
//!   driver emits a self-contained `src/domain/{domain}/errors.rs` carrying
//!   the one classifier, and the scratch assembly's inputs all exist.
//! - **Runtime** (ignored; the `test-ops-integration` lane): the EMITTED
//!   errors module is compiled verbatim into a scratch crate and driven
//!   against a real Postgres — P0403 and 42501 are 403, 23505 is 409,
//!   23503/23502/23514 are 422 (the FK names its constraint), an unknown
//!   SQLSTATE stays 500, and the SeaORM/sqlx `DbErr` path agrees with the
//!   tokio path. The boxed repository shape (`Box<dyn Error + Send + Sync>`
//!   — the issue #813 contract) classifies identically, because that is the
//!   shape the command plane actually passes.
//!
//! ```bash
//! cargo test -p codegraph --test error_mapping_runtime_tests
//! cargo test -p codegraph --test error_mapping_runtime_tests -- --ignored --nocapture
//! ```
//!
//! Mutation proof (teeth): on master's Display-based classifiers the runtime
//! legs fail with `INTERNAL_ERROR`/500 for P0403, 42505, 23503 and 23505 —
//! the `db error` Display carries none of the markers. Reverting any SQLSTATE
//! arm turns its leg red.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use codegraph_grafeo::GrafeoEngine;

/// Minimal single-domain config — the pin needs ONE generated domain error
/// module, not the full fixture graph.
const RUNTIME_DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.rsvp]
label = "Rsvp"
schema_dir = "rsvp"
postgres_schema = "rsvp"
entities = ["RsvpType"]
"#;

fn tree_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("error-mapping-runtime-tree-{label}"))
}

/// Render the emitted app tree with the cornucopia provider (the workers
/// slice's classifier: typed `tokio-postgres` errors, plus the SeaORM/sqlx
/// conversions that the same file carries on native).
async fn generate_error_plane(label: &str) -> (PathBuf, PathBuf) {
    let root = tree_root(label);
    let _ = fs::remove_dir_all(&root);
    let app_dir = root.join("app");
    let domain_types_dir = root.join("domain-types");
    fs::create_dir_all(&app_dir).unwrap();

    let engine = GrafeoEngine::in_memory().unwrap();
    let classifier = codegraph_classifier::config::parse_classifier_config(Path::new(
        "tests/fixtures/classifier.toml",
    ))
    .unwrap();
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = config_dir.path().join("domains.toml");
    fs::write(&config_path, RUNTIME_DOMAINS_TOML).unwrap();
    let config = codegraph_config::config::parse_domain_config(&config_path).unwrap();

    let entity_names: std::collections::HashSet<String> = config
        .domains
        .values()
        .flat_map(|d| d.entities.iter().cloned())
        .collect();
    codegraph::ingest::async_ingest::ingest_schemas(
        &engine,
        Path::new("tests/fixtures/schemas"),
        &classifier,
        &entity_names,
        &codegraph_config::UiOverrideConfig::default(),
        &config.defaults.type_suffix,
    )
    .await
    .unwrap();

    let tera = codegraph::generate::template_engine::create_tera(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
    )
    .unwrap();

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let type_contracts_path = workspace_root
        .join("crates")
        .join("codegraph-type-contracts");
    let workflow_path = workspace_root.join("crates").join("codegraph-workflow");
    let profiles = workspace_root.join("profiles.toml");

    let registry = codegraph::profile::CapabilityRegistry::new();
    let mut resolved =
        codegraph::profile::load_and_resolve_profile(&profiles, "default", None).unwrap();
    for section in resolved.sections.values_mut() {
        section.generators.retain(|g| !g.starts_with("grpc_"));
    }
    let plan = codegraph::profile::BuildPlan::from_profile(&resolved, &registry).unwrap();

    let project_config = codegraph::generate::ProjectConfig {
        identity: codegraph::generate::IdentityConfig {
            app_name: "error-mapping".into(),
            domain_types_crate: "domain_types".into(),
            generator_name: "error-mapping-pin".into(),
            ..Default::default()
        },
        paths: codegraph::generate::PathsConfig {
            type_contracts_base: type_contracts_path.to_string_lossy().to_string(),
            codegraph_workflow_base: workflow_path.to_string_lossy().to_string(),
            domain_types_base: "domain-types".into(),
            ..Default::default()
        },
        codegen: codegraph::generate::CodegenConfig {
            types_import_prefix: "codegraph_type_contracts".into(),
            ..Default::default()
        },
        database: codegraph::generate::DatabaseConfig {
            persistence_provider: codegraph::generate::profile::PersistenceProvider::Cornucopia,
            ..Default::default()
        },
        ..Default::default()
    };

    let report =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            integration_manifests: None,
            db: &engine,
            config: &config,
            output_dir: &app_dir,
            tera: &tera,
            ui_overrides: &Default::default(),
            ui_domains: &Default::default(),
            schema_base_dir: Path::new(""),
            seed_config: None,
            domain_types_base: Some(&domain_types_dir),
            hooks_base: None,
            ext_points: None,
            build_plan: Some(&plan),
            profile_name: "default",
            ifml_frameworks: vec![],
            ifml_components: None,
            ux_rules: None,
            project_config: Some(&project_config),
            emdash_plugins: None,
            domain_config_dir: None,
        })
        .await
        .unwrap();
    assert!(
        !report.has_errors(),
        "generation reported errors: {:#?}",
        report.errors
    );

    (
        app_dir.join("src/domain/rsvp/errors.rs"),
        app_dir.join("src/error.rs"),
    )
}

/// Node-free PR-CI pin: the driver emits the classifier module the scratch
/// assembly compiles, and it carries every SQLSTATE the runtime legs assert.
#[tokio::test]
async fn generated_error_plane_is_assembly_ready() {
    let (errors_path, app_error_path) = generate_error_plane("smoke").await;
    let errors = fs::read_to_string(&errors_path)
        .unwrap_or_else(|e| panic!("{} must exist: {e}", errors_path.display()));
    let app_error = fs::read_to_string(&app_error_path)
        .unwrap_or_else(|e| panic!("{} must exist: {e}", app_error_path.display()));

    // The domain enum delegates to the ONE app-level classifier.
    assert!(
        errors.contains("crate::error::classify_db_error(&e)"),
        "from_repo_err must delegate to the app error plane:\n{errors}"
    );
    assert!(
        !errors.contains("pub fn classify_db_error("),
        "the classifier must live in ONE place (src/error.rs):\n{errors}"
    );
    for needle in [
        "pub fn classify_db_error(",
        "downcast_ref::<tokio_postgres::Error>()",
        "as_db_error()",
        "RuntimeErr::SqlxError",
        "as_database_error()",
        "Some(\"P0403\") | Some(\"42501\") => DbErrorClass::Forbidden",
        "Some(\"23505\") => DbErrorClass::Conflict",
        "Some(\"23503\") =>",
        "Some(\"23502\") | Some(\"23514\") => DbErrorClass::Unprocessable",
    ] {
        assert!(
            app_error.contains(needle),
            "the emitted app error plane must carry `{needle}`:\n{app_error}"
        );
    }
    assert!(
        errors.contains("Self::Unprocessable(_) => StatusCode::UNPROCESSABLE_ENTITY"),
        "the domain enum must map the 422 class:\n{errors}"
    );
    assert!(
        !app_error.contains("contains(\"duplicate key\")"),
        "the Display-based unique classifier must be gone:\n{app_error}"
    );
}

// ── The scratch crate ────────────────────────────────────────────────────

const SCRATCH_CARGO_TOML: &str = r#"[package]
name = "error-mapping-scratch"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
name = "error_mapping_scratch"
path = "src/lib.rs"

[dependencies]
axum = "0.8"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tracing = "0.1"
utoipa = { version = "6", features = ["axum_extras", "uuid"] }
uuid = { version = "1", features = ["v4", "serde"] }
sea-orm = { version = "2", features = ["sqlx-postgres", "runtime-tokio-rustls"] }
tokio-postgres = { version = "0.7", default-features = false, features = ["with-chrono-0_4", "with-uuid-1", "with-serde_json-1"] }
deadpool-postgres = "0.14"

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
"#;

const SCRATCH_LIB_RS: &str = r#"pub mod error;
pub mod errors;
"#;

/// The classification probes. Each leg obtains a REAL Postgres error and
/// asserts the emitted classifier's HTTP contract.
const SCRATCH_CLASSIFY_RS: &str = r##"//! SQLSTATE classification probes over real Postgres (issue #828).

use axum::http::StatusCode;
use error_mapping_scratch::error::{classify_db_error, DbErrorClass};
use error_mapping_scratch::errors::RsvpError;
use tokio_postgres::NoTls;

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/postgres".into())
}

async fn client() -> tokio_postgres::Client {
    let (client, conn) = tokio_postgres::connect(&db_url(), NoTls)
        .await
        .expect("connect to Postgres (DATABASE_URL or the local default)");
    tokio::spawn(async move { conn.await.expect("connection task") });
    client
}

fn assert_forbidden(e: &RsvpError) {
    assert_eq!(
        e.http_status(),
        StatusCode::FORBIDDEN,
        "a scope/role denial must be 403, not an opaque 500: {e}"
    );
    assert_eq!(e.code(), "FORBIDDEN");
}

fn assert_conflict(e: &RsvpError) {
    assert_eq!(e.http_status(), StatusCode::CONFLICT, "unique violation: {e}");
    assert_eq!(e.code(), "CONFLICT");
}

fn assert_unprocessable(e: &RsvpError) {
    assert_eq!(
        e.http_status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "integrity violation: {e}"
    );
    assert_eq!(e.code(), "UNPROCESSABLE_ENTITY");
}

fn assert_internal(e: &RsvpError) {
    assert_eq!(
        e.http_status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "an unknown SQLSTATE must stay internal: {e}"
    );
    assert_eq!(e.code(), "INTERNAL_ERROR");
}

/// P0403 is the code the generated RESTRICTIVE `scope_enforced_*` policies
/// raise (`INSUFFICIENT_SCOPE` JSON payload) — the exact shape codegraph#810
/// mis-reported as a machine defect.
#[tokio::test]
async fn scope_denial_p0403_is_forbidden() {
    let client = client().await;
    let err = client
        .batch_execute(
            r#"DO $$ BEGIN RAISE EXCEPTION USING ERRCODE = 'P0403',
               MESSAGE = '{"code":"INSUFFICIENT_SCOPE","scope":"pay-run.create"}'; END $$;"#,
        )
        .await
        .expect_err("the RAISE must fail");
    assert_forbidden(&RsvpError::from_repo_err(err));
}

/// The repository plane's error type is `Box<dyn Error + Send + Sync>`
/// (issue #813) — the classifier must see through the box.
#[tokio::test]
async fn scope_denial_survives_the_boxed_repository_shape() {
    let client = client().await;
    let err = client
        .batch_execute(
            r#"DO $$ BEGIN RAISE EXCEPTION USING ERRCODE = 'P0403',
               MESSAGE = '{"code":"INSUFFICIENT_SCOPE"}'; END $$;"#,
        )
        .await
        .expect_err("the RAISE must fail");
    let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(err);
    assert_forbidden(&RsvpError::from_repo_err(boxed));
}

/// 42501 is the standard privilege class (a raise that does not carry the
/// custom code, and the class Postgres itself uses for RLS).
#[tokio::test]
async fn insufficient_privilege_42501_is_forbidden() {
    let client = client().await;
    let err = client
        .batch_execute(
            "DO $$ BEGIN RAISE EXCEPTION USING ERRCODE = '42501', MESSAGE = 'permission denied'; END $$;",
        )
        .await
        .expect_err("the RAISE must fail");
    assert_forbidden(&RsvpError::from_repo_err(err));
}

async fn integrity_client() -> tokio_postgres::Client {
    let client = client().await;
    client
        .batch_execute(
            "CREATE TEMP TABLE parent (id int PRIMARY KEY); \
             CREATE TEMP TABLE child ( \
                 id int PRIMARY KEY, \
                 parent_id int CONSTRAINT child_parent_fk REFERENCES parent(id), \
                 label text NOT NULL, \
                 amount int CHECK (amount > 0) \
             );",
        )
        .await
        .expect("temp tables");
    client
}

#[tokio::test]
async fn unique_violation_is_conflict() {
    let client = integrity_client().await;
    client
        .batch_execute("INSERT INTO parent (id) VALUES (1)")
        .await
        .unwrap();
    let err = client
        .batch_execute("INSERT INTO parent (id) VALUES (1)")
        .await
        .expect_err("duplicate key must fail");
    assert_conflict(&RsvpError::from_repo_err(err));
}

#[tokio::test]
async fn foreign_key_violation_is_unprocessable_and_names_the_constraint() {
    let client = integrity_client().await;
    let err = client
        .batch_execute("INSERT INTO child (id, parent_id, label, amount) VALUES (1, 999, 'x', 5)")
        .await
        .expect_err("orphan FK must fail");
    let classified = RsvpError::from_repo_err(err);
    assert_unprocessable(&classified);
    assert!(
        classified.to_string().contains("child_parent_fk"),
        "the 422 must name the violated constraint: {classified}"
    );
}

#[tokio::test]
async fn not_null_violation_is_unprocessable() {
    let client = integrity_client().await;
    client
        .batch_execute("INSERT INTO parent (id) VALUES (1)")
        .await
        .unwrap();
    let err = client
        .batch_execute("INSERT INTO child (id, parent_id, label, amount) VALUES (1, 1, NULL, 5)")
        .await
        .expect_err("NULL in a required column must fail");
    assert_unprocessable(&RsvpError::from_repo_err(err));
}

#[tokio::test]
async fn check_violation_is_unprocessable() {
    let client = integrity_client().await;
    client
        .batch_execute("INSERT INTO parent (id) VALUES (1)")
        .await
        .unwrap();
    let err = client
        .batch_execute("INSERT INTO child (id, parent_id, label, amount) VALUES (1, 1, 'x', -1)")
        .await
        .expect_err("CHECK violation must fail");
    assert_unprocessable(&RsvpError::from_repo_err(err));
}

/// An unknown SQLSTATE (42P01) must NOT be coerced into a denial or a
/// validation error — it is a genuine server fault.
#[tokio::test]
async fn unknown_sqlstate_stays_internal() {
    let client = client().await;
    let err = client
        .batch_execute("SELECT * FROM this_table_does_not_exist")
        .await
        .expect_err("missing relation must fail");
    assert_internal(&RsvpError::from_repo_err(err));
}

/// The SeaORM/sqlx conversion must agree with the tokio path (the monolith
/// slice). `sea_orm::DbErr` is constructed around the REAL sqlx error so
/// the downcast in `sea_orm_error_detail` is exercised.
#[tokio::test]
async fn sea_orm_db_err_classifies_by_sqlstate() {
    use sea_orm::sqlx::Executor;
    use std::sync::Arc;

    let pool = sea_orm::sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&db_url())
        .await
        .expect("sqlx pool");
    let mut conn = pool.acquire().await.expect("sqlx connection");
    conn.execute("CREATE TEMP TABLE dup (id int PRIMARY KEY)")
        .await
        .expect("temp table");
    conn.execute("INSERT INTO dup (id) VALUES (1)")
        .await
        .expect("first insert");

    let sqlx_err = conn
        .execute("INSERT INTO dup (id) VALUES (1)")
        .await
        .expect_err("duplicate key must fail");
    let db_err = sea_orm::DbErr::Query(sea_orm::RuntimeErr::SqlxError(Arc::new(sqlx_err)));
    assert_conflict(&RsvpError::from(db_err));

    // And through the shared classifier directly.
    let sqlx_err = conn
        .execute("INSERT INTO dup (id) VALUES (1)")
        .await
        .expect_err("duplicate key must fail");
    let db_err = sea_orm::DbErr::Query(sea_orm::RuntimeErr::SqlxError(Arc::new(sqlx_err)));
    assert!(matches!(
        classify_db_error(&db_err),
        DbErrorClass::Conflict(_)
    ));
}
"##;

/// The scratch crate's build cache. Keep it on the disk-backed workspace
/// target (never a tmpfs `/tmp` — the #734 EDQUOT class), but isolate the
/// binary/dependency graph from the main workspace build.
fn scratch_target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))
        .join("error-mapping-scratch")
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

/// Assemble the scratch crate around the EMITTED errors module (verbatim).
fn assemble_scratch(errors_path: &Path, app_error_path: &Path, ws: &Path) -> PathBuf {
    write_file(&ws.join("Cargo.toml"), SCRATCH_CARGO_TOML);
    write_file(&ws.join("src/lib.rs"), SCRATCH_LIB_RS);
    fs::create_dir_all(ws.join("src")).unwrap();
    fs::copy(errors_path, ws.join("src/errors.rs")).expect("copy the emitted errors module");
    fs::copy(app_error_path, ws.join("src/error.rs")).expect("copy the emitted app error plane");
    write_file(&ws.join("tests/classify.rs"), SCRATCH_CLASSIFY_RS);
    ws.join("Cargo.toml")
}

/// Runtime pin (the ops-integration lane): compile the emitted errors module
/// verbatim and drive every SQLSTATE leg against real Postgres.
#[tokio::test]
#[ignore = "needs Postgres (DATABASE_URL) and compiles the scratch crate: downloads crates.io deps, ~2 min"]
async fn sqlstate_classification_matches_the_wire_contract() {
    let (errors_path, app_error_path) = generate_error_plane("runtime").await;
    let ws = tree_root("runtime").join("ws");
    let _ = fs::remove_dir_all(&ws);
    let manifest = assemble_scratch(&errors_path, &app_error_path, &ws);

    let out = Command::new("cargo")
        .arg("test")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--test")
        .arg("classify")
        .arg("--")
        .arg("--nocapture")
        .env("CARGO_TARGET_DIR", scratch_target_dir())
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("cargo test for the scratch error-mapping crate must run");
    assert!(
        out.status.success(),
        "the emitted error plane must classify by SQLSTATE:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}
