//! Runtime pins for the lifecycle-hooks plane under the cornucopia
//! provider (issue #518, the #509 R5 mandate).
//!
//! What landed with #509 were template-shape pins
//! (`template_harness::cornucopia_command_keeps_all_hooks_without_wasm_gates`).
//! This suite closes the gap: a registered lifecycle impl must actually
//! RUN on the generated cornucopia create path — `before_create` observes
//! the payload on the OPEN transaction (through the real `HookTxAdapter`),
//! `after_create_commit` fires post-commit with a working connection
//! source (through the real `HookConnSourceAdapter`), and
//! `HookControl::Abort` rolls the transaction back (the row never lands).
//!
//! Harness: the fixture's `RsvpType` is generated with
//! `persistence_provider = "cornucopia"` + `hooks_api_crate = "my_hooks"`;
//! a scratch cargo workspace assembles the generated command path VERBATIM
//! (`command.rs`, `hook_adapters.rs`, `db_client.rs`, repository trait,
//! DTOs, domain errors, the hooks crate's `generated` tree) plus the
//! runtime-capable `cornucopia-queries` stub from
//! `workers_gateway_compile_tests` (a `GenericClient` whose deadpool
//! impls genuinely delegate to tokio-postgres). The repository
//! implementation is the one hand-written stand-in: same type/method
//! shape as the generated `cornucopia_repository_impl.rs`, with a real
//! INSERT through the `GenericClient` — the repo plane has its own pins;
//! the mandate targets the hook plumbing.
//!
//! The runtime tests need Postgres (`DATABASE_URL`, default
//! `postgres://postgres:postgres@localhost:5432/postgres`) and are
//! `#[ignore]`d — they run in the `test-ops-integration` lane:
//!
//! ```bash
//! cargo test -p codegraph --test hooks_cornucopia_runtime_tests -- --ignored --nocapture
//! ```
//!
//! `generated_cornucopia_hooks_tree_is_assembly_ready` is the node-free
//! PR-CI pin: it runs the generation and asserts every artifact the
//! scratch assembly consumes is emitted where the assembly looks for it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use codegraph_grafeo::GrafeoEngine;

#[path = "test_framework/mod.rs"]
mod test_framework;

use test_framework::cornucopia_queries_stub;

/// Minimal single-domain config — the runtime harness needs ONE creatable
/// entity, not the full fixture graph (every other fixture entity is
/// FK-entangled; RsvpType is the smallest root entity).
const RUNTIME_DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.rsvp]
label = "Rsvp"
schema_dir = "rsvp"
postgres_schema = "rsvp"
entities = ["RsvpType"]
"#;

/// Where the generated cornucopia tree is materialized for inspection and
/// assembly (same fixed-path convention as the template harness). The
/// label keeps the (parallel) tests in this binary from racing on one dir.
fn tree_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("hooks-cornucopia-runtime-tree-{label}"))
}

struct GeneratedTree {
    app_dir: PathBuf,
    domain_types_dir: PathBuf,
    hooks_dir: PathBuf,
}

async fn generate_tree(label: &str) -> GeneratedTree {
    let root = tree_root(label);
    let _ = fs::remove_dir_all(&root);
    let app_dir = root.join("app");
    let domain_types_dir = root.join("domain-types");
    let hooks_dir = root.join("my-hooks");
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
            app_name: "hooks-runtime".into(),
            domain_types_crate: "domain_types".into(),
            generator_name: "hooks-runtime-pin".into(),
            hooks_api_crate: "my_hooks".into(),
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
            db: &engine,
            config: &config,
            output_dir: &app_dir,
            tera: &tera,
            ui_overrides: &Default::default(),
            ui_domains: &Default::default(),
            schema_base_dir: Path::new(""),
            seed_config: None,
            domain_types_base: Some(&domain_types_dir),
            hooks_base: Some(&hooks_dir),
            ext_points: None,
            build_plan: Some(&plan),
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

    GeneratedTree {
        app_dir,
        domain_types_dir,
        hooks_dir,
    }
}

/// Node-free assembly-readiness pin (PR CI): the cornucopia + hooks
/// generation emits every artifact the scratch runtime assembly consumes,
/// and the command path carries the hook call sites the runtime tests
/// exercise.
#[tokio::test]
async fn generated_cornucopia_hooks_tree_is_assembly_ready() {
    let tree = generate_tree("smoke").await;

    for required in [
        "src/db_client.rs",
        "src/hook_adapters.rs",
        "src/domain/rsvp/mod.rs",
        "src/domain/rsvp/errors.rs",
        "src/domain/rsvp/rsvp/command.rs",
        "src/domain/rsvp/rsvp/repository.rs",
        "src/domain/rsvp/rsvp/dto_create.rs",
        "src/domain/rsvp/rsvp/dto_response.rs",
    ] {
        assert!(
            tree.app_dir.join(required).exists(),
            "the generated app tree must carry {required}:\n{:?}",
            tree.app_dir
        );
    }
    for required in [
        "src/generated/mod.rs",
        "src/generated/types.rs",
        "src/generated/rsvp/mod.rs",
    ] {
        assert!(
            tree.hooks_dir.join(required).exists(),
            "the hooks crate tree must carry {required}:\n{:?}",
            tree.hooks_dir
        );
    }
    assert!(
        tree.domain_types_dir.join("src").exists(),
        "the domain-types tree must carry src/"
    );

    // The generated create path is the code under runtime test: it must
    // wire the hook SPI on the cornucopia handles (shape re-pin, the
    // runtime tests below execute it).
    let command = fs::read_to_string(tree.app_dir.join("src/domain/rsvp/rsvp/command.rs")).unwrap();
    for fragment in [
        "hooks.before_create(",
        "crate::hook_adapters::HookTxAdapter(&tx)",
        "after_create_tx(",
        "tx.commit()",
        "after_create_commit(",
        "crate::hook_adapters::HookConnSourceAdapter(",
    ] {
        assert!(
            command.contains(fragment),
            "the cornucopia create path must carry `{fragment}`:\n{command}"
        );
    }
}

// ── Scratch workspace assembly ──

fn write_file(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn copy_file(from: &Path, to: &Path) {
    fs::create_dir_all(to.parent().unwrap()).unwrap();
    fs::copy(from, to).unwrap();
}

/// The hand-written repository stand-in: same type/method shape as the
/// generated `cornucopia_repository_impl.rs` (the command handler
/// constructs `Arc<RsvpRepositoryImpl>` and drives the trait), with a REAL
/// INSERT through the erased `GenericClient` on the OPEN transaction.
const STANDIN_REPO_RS: &str = r#"//! Runtime-harness stand-in for the generated `cornucopia_repository_impl.rs`.
//! The real INSERT executes through the erased `GenericClient` on the OPEN
//! transaction. VO child rows are deliberately out of scope — the repo
//! plane has its own pins; this harness targets the hook plumbing (issue
//! #518, the #509 R5 mandate).

use std::collections::HashMap;

use async_trait::async_trait;
use uuid::Uuid;

use cornucopia_queries::client::GenericClient;

use super::dto_create::CreateRsvpRequest;
use super::dto_response::RsvpResponse;
use super::dto_update::UpdateRsvpRequest;
use super::repository::RsvpRepository;

#[derive(Clone, Default)]
pub struct RsvpRepositoryImpl;

#[async_trait]
impl<C> RsvpRepository<C> for RsvpRepositoryImpl
where
    C: GenericClient + Sync,
{
    async fn create(
        &self,
        tx: &C,
        cmd: CreateRsvpRequest,
    ) -> Result<Uuid, Box<dyn std::error::Error>> {
        let id = Uuid::new_v4();
        tx.execute(
            "INSERT INTO rsvp.rsvp (id, status, ts) VALUES ($1, $2, $3)",
            &[&id, &cmd.status.to_string(), &cmd.timestamp],
        )
        .await?;
        Ok(id)
    }

    async fn find_by_id(
        &self,
        _db: &C,
        _id: Uuid,
        _include_deleted: bool,
    ) -> Result<Option<RsvpResponse>, Box<dyn std::error::Error>> {
        Ok(None)
    }

    async fn update(
        &self,
        _tx: &C,
        _id: Uuid,
        _cmd: UpdateRsvpRequest,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    async fn delete(
        &self,
        _tx: &C,
        _id: Uuid,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    async fn list(
        &self,
        _db: &C,
        _page: u64,
        _page_size: u64,
        _filters: &HashMap<String, String>,
        _include_deleted: bool,
    ) -> Result<(Vec<RsvpResponse>, u64), Box<dyn std::error::Error>> {
        Ok((Vec::new(), 0))
    }
}
"#;

/// Minimal consumer-owned hooks-crate surface: `crate::error::HookError`
/// plus the bulk-create types the generated command layer references.
const MY_HOOKS_ERROR_RS: &str = r#"//! Consumer-owned hooks crate error surface (HookError) plus the bulk
//! result types the generated command layer references. The HookError
//! variants mirror the consumer-owned surface the generated app error
//! mapping (`hook_error`) matches on.

use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum HookError {
    #[error("{0}")]
    Validation(String),
    #[error("{adapter}: {message}")]
    ExternalService { adapter: String, message: String },
    #[error("{0}")]
    Internal(String),
}

#[derive(Debug)]
pub struct BulkItemError {
    pub index: usize,
    pub error: String,
}

#[derive(Debug)]
pub struct BulkCreateResults {
    pub succeeded: Vec<Uuid>,
    pub failed: Vec<BulkItemError>,
}
"#;

/// The scratch `my_hooks` crate: the generated lifecycle trait + context
/// types verbatim, the registry omitted (it pulls axum/QueryService —
/// nothing the command path needs).
const MY_HOOKS_LIB_RS: &str = r#"//! Runtime-harness hooks crate: generated `generated` tree + consumer
//! error/bulk types.

pub mod error;
pub mod generated;

pub use error::{BulkCreateResults, BulkItemError, HookError};
"#;

const APP_API_MOD_RS: &str = r#"pub mod links;
"#;

const MY_HOOKS_GENERATED_MOD_RS: &str = r#"pub mod types;

pub mod rsvp;
"#;

const APP_LIB_RS: &str = r#"pub mod api;
pub mod db_client;
pub mod domain;
pub mod error;
pub mod hook_adapters;
"#;

const APP_RSVP_MOD_RS: &str = r#"pub mod command;
pub mod cornucopia_repository_impl;
pub mod dto_create;
pub mod dto_response;
pub mod dto_update;
pub mod repository;
"#;

const MY_HOOKS_CARGO_TOML: &str = r#"[package]
name = "my-hooks"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
async-trait = "0.1"
thiserror = "2"
uuid = { version = "1", features = ["v4", "serde"] }
domain-types = { path = "../domain-types" }
"#;

const APP_CARGO_TOML: &str = r#"[package]
name = "hooks-runtime-app"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
name = "hooks_runtime_app"
path = "src/lib.rs"

[dependencies]
async-trait = "0.1"
axum = { version = "0.8", features = ["multipart"] }
chrono = { version = "0.4", features = ["serde"] }
cornucopia-queries = { path = "../cornucopia-queries", features = ["deadpool"] }
deadpool-postgres = "0.14"
domain-types = { path = "../domain-types" }
garde = { version = "0.22", features = ["derive", "regex"] }
my-hooks = { path = "../my-hooks" }
sea-orm = { version = "2", features = ["sqlx-postgres", "runtime-tokio-rustls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
tokio-postgres = { version = "0.7", default-features = false, features = ["with-chrono-0_4", "with-uuid-1", "with-serde_json-1"] }
tracing = "0.1"
utoipa = { version = "6", features = ["chrono", "uuid", "decimal"] }
uuid = { version = "1", features = ["v4", "serde"] }
"#;

/// Assemble the scratch cargo workspace from the generated cornucopia tree.
/// Everything under `app/src` is copied VERBATIM except the `rsvp` module
/// index (rewritten to drop the excluded `event`/`query` planes) and the
/// repository implementation (the documented stand-in).
fn assemble_workspace(tree: &GeneratedTree, ws: &Path) -> PathBuf {
    // cornucopia-queries: the shared runtime-capable stub.
    write_file(
        &ws.join("cornucopia-queries/Cargo.toml"),
        cornucopia_queries_stub::CARGO_TOML,
    );
    write_file(
        &ws.join("cornucopia-queries/build.rs"),
        cornucopia_queries_stub::BUILD_RS,
    );
    write_file(
        &ws.join("cornucopia-queries/src/lib.rs"),
        cornucopia_queries_stub::LIB_RS,
    );

    // domain-types: the emitted crate, whole (Cargo.toml + src).
    copy_dir_recursive(&tree.domain_types_dir, &ws.join("domain-types"));

    // my-hooks: generated lifecycle trait + context types verbatim.
    write_file(&ws.join("my-hooks/Cargo.toml"), MY_HOOKS_CARGO_TOML);
    write_file(&ws.join("my-hooks/src/lib.rs"), MY_HOOKS_LIB_RS);
    write_file(&ws.join("my-hooks/src/error.rs"), MY_HOOKS_ERROR_RS);
    write_file(
        &ws.join("my-hooks/src/generated/mod.rs"),
        MY_HOOKS_GENERATED_MOD_RS,
    );
    copy_file(
        &tree.hooks_dir.join("src/generated/types.rs"),
        &ws.join("my-hooks/src/generated/types.rs"),
    );
    copy_file(
        &tree.hooks_dir.join("src/generated/rsvp/mod.rs"),
        &ws.join("my-hooks/src/generated/rsvp/mod.rs"),
    );
    copy_file(
        &tree.hooks_dir.join("src/generated/rsvp/rsvp.rs"),
        &ws.join("my-hooks/src/generated/rsvp/rsvp.rs"),
    );

    // app crate.
    let app = ws.join("app");
    write_file(&app.join("Cargo.toml"), APP_CARGO_TOML);
    write_file(&app.join("src/lib.rs"), APP_LIB_RS);
    for src in [
        "src/error.rs",
        "src/db_client.rs",
        "src/hook_adapters.rs",
        "src/api/links.rs",
        "src/domain/mod.rs",
        "src/domain/rsvp/mod.rs",
        "src/domain/rsvp/errors.rs",
        "src/domain/rsvp/rsvp/command.rs",
        "src/domain/rsvp/rsvp/repository.rs",
        "src/domain/rsvp/rsvp/dto_create.rs",
        "src/domain/rsvp/rsvp/dto_response.rs",
        "src/domain/rsvp/rsvp/dto_update.rs",
    ] {
        copy_file(&tree.app_dir.join(src), &app.join(src));
    }
    write_file(&app.join("src/api/mod.rs"), APP_API_MOD_RS);
    write_file(&app.join("src/domain/rsvp/rsvp/mod.rs"), APP_RSVP_MOD_RS);
    write_file(
        &app.join("src/domain/rsvp/rsvp/cornucopia_repository_impl.rs"),
        STANDIN_REPO_RS,
    );

    app.join("Cargo.toml")
}

fn copy_dir_recursive(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir_recursive(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

// ── Runtime tests (Postgres; the ops-integration lane) ──

/// The scratch test binary: registers a lifecycle impl, runs the GENERATED
/// create command path against real Postgres through the generated cornucopia
/// client plumbing, and asserts the hook SPI behaves (fires + Abort rolls
/// back). Both scenarios ride one cargo invocation from the parent suite.
const SCRATCH_RUNTIME_RS: &str = r#"//! Runtime pins for the cornucopia hook plumbing (issue #518/#509 R5).

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uuid::Uuid;

use hooks_runtime_app::db_client::DeadpoolClientSource;
use hooks_runtime_app::domain::rsvp::errors::RsvpError;
use hooks_runtime_app::domain::rsvp::rsvp::command::RsvpCommandHandler;
use hooks_runtime_app::domain::rsvp::rsvp::cornucopia_repository_impl::RsvpRepositoryImpl;
use my_hooks::error::HookError;
use my_hooks::generated::rsvp::rsvp::RsvpLifecycle;
use my_hooks::generated::types::{HookControl, HookResult, PostCommitContext, TxContext};

#[derive(Default, Clone)]
struct Recorder {
    before_create: usize,
    before_create_tx_sql: bool,
    after_create_tx: usize,
    after_create_commit: usize,
}

struct RecordingLifecycle {
    rec: Arc<Mutex<Recorder>>,
    abort_before_create: bool,
}

impl RecordingLifecycle {
    fn new(abort_before_create: bool) -> (Arc<Self>, Arc<Mutex<Recorder>>) {
        let rec = Arc::new(Mutex::new(Recorder::default()));
        (
            Arc::new(Self { rec: rec.clone(), abort_before_create }),
            rec,
        )
    }
}

#[async_trait]
impl RsvpLifecycle for RecordingLifecycle {
    async fn before_create(
        &self,
        ctx: &mut TxContext<'_>,
        _payload: &domain_types::rsvp::rsvp::CreateRsvpRequest,
    ) -> HookResult {
        // Exercise the REAL HookTxAdapter: SQL on the OPEN transaction.
        let sql_ok = ctx.execute("SELECT 1").await.is_ok();
        let mut rec = self.rec.lock().unwrap();
        rec.before_create += 1;
        rec.before_create_tx_sql = sql_ok;
        if self.abort_before_create {
            return Ok(HookControl::Abort { reason: "hook says no".into() });
        }
        Ok(HookControl::Continue)
    }

    async fn after_create_tx(&self, _ctx: &mut TxContext<'_>, _entity_id: Uuid) -> HookResult {
        self.rec.lock().unwrap().after_create_tx += 1;
        Ok(HookControl::Continue)
    }

    async fn after_create_commit(&self, ctx: &PostCommitContext) -> Result<(), HookError> {
        // Exercise the REAL HookConnSourceAdapter: the marker INSERT runs on
        // a FRESH pooled connection, outside the (now-committed) command tx.
        let _ = ctx.execute(&format!(
            "INSERT INTO rsvp.hook_marker (entity_id) VALUES ('{}')",
            ctx.entity_id
        ))
        .await;
        self.rec.lock().unwrap().after_create_commit += 1;
        Ok(())
    }
}

/// Minimal harness DDL: the rsvp table (columns matching the stand-in
/// repo INSERT), a marker table for the post-commit hook, and the
/// `app_user` role the RLS context bundle flips to.
const HARNESS_DDL: &str = "
CREATE SCHEMA IF NOT EXISTS rsvp;
DROP TABLE IF EXISTS rsvp.hook_marker;
DROP TABLE IF EXISTS rsvp.rsvp;
CREATE TABLE rsvp.rsvp (
    id uuid PRIMARY KEY,
    status text NOT NULL,
    ts timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE rsvp.hook_marker (
    entity_id uuid PRIMARY KEY
);
DO $$ BEGIN
    -- The role is CLUSTER-wide while the two scratch tests run in parallel
    -- (separate databases) — serialize the check-then-create with a
    -- transaction advisory lock so concurrent runs cannot collide on
    -- pg_authid's rolname unique index.
    PERFORM pg_advisory_xact_lock(718509224);
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'app_user') THEN
        CREATE ROLE app_user NOLOGIN;
    END IF;
END $$;
GRANT USAGE ON SCHEMA rsvp TO app_user;
GRANT INSERT, SELECT, UPDATE, DELETE ON TABLE rsvp.rsvp TO app_user;
GRANT INSERT, SELECT ON TABLE rsvp.hook_marker TO app_user;
GRANT app_user TO CURRENT_USER;
";

async fn admin_client() -> tokio_postgres::Client {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/postgres".into());
    let (client, conn) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
        .await
        .expect("connect to Postgres (DATABASE_URL or the local default)");
    tokio::spawn(async move { conn.await.expect("admin connection") });
    client
}

/// Create a uniquely-named scratch database and apply the harness DDL.
/// Returns the database's connection URL.
async fn scratch_db(name: &str) -> String {
    let base = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/postgres".into());
    let admin = admin_client().await;
    admin
        .batch_execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE);"))
        .await
        .expect("drop scratch db");
    admin
        .batch_execute(&format!("CREATE DATABASE {name};"))
        .await
        .expect("create scratch db");
    let db_url = base
        .rsplit_once('/')
        .map(|(prefix, _)| format!("{prefix}/{name}"))
        .expect("DATABASE_URL must end in a database segment");
    let conn = admin_client_with(&db_url).await;
    conn.batch_execute(HARNESS_DDL)
        .await
        .expect("apply harness DDL");
    db_url
}

async fn admin_client_with(url: &str) -> tokio_postgres::Client {
    let (client, conn) = tokio_postgres::connect(url, tokio_postgres::NoTls)
        .await
        .expect("connect to the scratch database");
    tokio::spawn(async move { conn.await.expect("scratch connection") });
    client
}

async fn row_count(db_url: &str, table: &str) -> i64 {
    let conn = admin_client_with(db_url).await;
    let row = conn
        .query_one(&format!("SELECT count(*) FROM {table}"), &[])
        .await
        .expect("count query");
    row.get(0)
}

async fn handler_with_hooks(
    db_url: &str,
    hooks: Option<Arc<dyn RsvpLifecycle>>,
) -> RsvpCommandHandler {
    let pg_config: tokio_postgres::Config = db_url.parse().expect("parse db url");
    let mgr = deadpool_postgres::Manager::from_config(
        pg_config,
        tokio_postgres::NoTls,
        deadpool_postgres::ManagerConfig::default(),
    );
    let pool = deadpool_postgres::Pool::builder(mgr)
        .build()
        .expect("build pool");
    let source: Arc<dyn hooks_runtime_app::db_client::ClientSource> =
        Arc::new(DeadpoolClientSource::new(pool));
    let db = sea_orm::Database::connect(db_url)
        .await
        .expect("connect sea_orm (the handler's dead monolith field)");
    RsvpCommandHandler::new(Arc::new(RsvpRepositoryImpl), source, db, hooks)
}

fn request() -> domain_types::rsvp::rsvp::CreateRsvpRequest {
    domain_types::rsvp::rsvp::CreateRsvpRequest {
        status: domain_types::codelist::RsvpRsvpStatus::Confirmed,
        timestamp: chrono::Utc::now(),
        event: None,
    }
}

/// The #509 R5 mandate: a registered hook RUNS on the create path under
/// cornucopia — `before_create` on the open transaction (real
/// `HookTxAdapter`), `after_create_commit` post-commit on a fresh
/// connection (real `HookConnSourceAdapter`), and the row lands.
#[tokio::test]
async fn hook_fires_on_create_under_cornucopia() {
    let db_url = scratch_db("hooks_rt_fire").await;
    let org = Uuid::new_v4();
    let (lifecycle, rec) = RecordingLifecycle::new(false);
    let handler = handler_with_hooks(&db_url, Some(lifecycle)).await;

    let id = handler
        .create(
            request(),
            domain_types::SourceContext::api(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            org,
            Uuid::new_v4(),
            "owner".into(),
        )
        .await
        .expect("create must succeed with a Continue hook");

    let r = rec.lock().unwrap().clone();
    assert_eq!(r.before_create, 1, "before_create must fire exactly once");
    assert!(
        r.before_create_tx_sql,
        "before_create must execute SQL on the OPEN transaction (HookTxAdapter)"
    );
    assert_eq!(r.after_create_tx, 1, "after_create_tx must fire");
    assert_eq!(r.after_create_commit, 1, "after_create_commit must fire");
    drop(r);

    // The row LANDED — the transaction committed after the hooks allowed it.
    let conn = admin_client_with(&db_url).await;
    let row = conn
        .query_one("SELECT count(*) FROM rsvp.rsvp WHERE id = $1", &[&id])
        .await
        .expect("row lookup");
    assert_eq!(row.get::<_, i64>(0), 1, "the created row must be committed");

    // The post-commit hook's marker is visible on a separate connection —
    // the HookConnSourceAdapter ran OUTSIDE the command tx and committed.
    let marker = conn
        .query_one(
            "SELECT count(*) FROM rsvp.hook_marker WHERE entity_id = $1",
            &[&id],
        )
        .await
        .expect("marker lookup");
    assert_eq!(
        marker.get::<_, i64>(0),
        1,
        "after_create_commit must run on a fresh committed connection"
    );
}

/// The #509 TDD `abort_rolls_back_under_cornucopia`: `HookControl::Abort`
/// from `before_create` rolls the transaction back — the row never lands,
/// later hooks never fire, and the caller sees the conflict error.
#[tokio::test]
async fn abort_rolls_back_under_cornucopia() {
    let db_url = scratch_db("hooks_rt_abort").await;
    let (lifecycle, rec) = RecordingLifecycle::new(true);
    let handler = handler_with_hooks(&db_url, Some(lifecycle)).await;

    let outcome = handler
        .create(
            request(),
            domain_types::SourceContext::api(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "owner".into(),
        )
        .await;

    assert!(
        matches!(outcome, Err(RsvpError::Conflict)),
        "Abort must surface as the conflict error, got {outcome:?}"
    );

    let r = rec.lock().unwrap().clone();
    assert_eq!(r.before_create, 1, "the hook DID run");
    assert_eq!(r.after_create_tx, 0, "later hooks must not fire after Abort");
    assert_eq!(r.after_create_commit, 0, "post-commit must not fire after Abort");
    drop(r);

    // THE pin: the transaction ROLLED BACK — no row, no marker.
    assert_eq!(row_count(&db_url, "rsvp.rsvp").await, 0);
    assert_eq!(row_count(&db_url, "rsvp.hook_marker").await, 0);
}
"#;

/// Runtime pin (the ops-integration lane): assemble the scratch workspace
/// and execute the generated cornucopia create path against real Postgres
/// with a registered lifecycle impl. Needs `DATABASE_URL` (default
/// `postgres://postgres:postgres@localhost:5432/postgres`).
#[tokio::test]
#[ignore = "needs Postgres (DATABASE_URL) and compiles the scratch app: downloads crates.io deps, ~2 min"]
async fn hooks_fire_and_abort_rolls_back_under_cornucopia() {
    let tree = generate_tree("runtime").await;
    let ws = tree_root("runtime").join("ws");
    let _ = fs::remove_dir_all(&ws);
    let app_manifest = assemble_workspace(&tree, &ws);

    write_file(&ws.join("app/tests/runtime.rs"), SCRATCH_RUNTIME_RS);

    let out = Command::new("cargo")
        .arg("test")
        .arg("--manifest-path")
        .arg(&app_manifest)
        .arg("--test")
        .arg("runtime")
        .arg("--")
        .arg("--nocapture")
        .env("CARGO_TARGET_DIR", ws.join("target"))
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("cargo test for the scratch runtime workspace must run");
    assert!(
        out.status.success(),
        "the generated cornucopia create path must fire hooks and honor Abort:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}
