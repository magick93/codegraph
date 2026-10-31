//! Compile gate for the generated worker plane on `wasm32-unknown-unknown`
//! (issue #550) — the CI half of the "#536 class" contract.
//!
//! Twice now (issue #536's `!Send` dispatch leg, and the `worker_main`
//! module gap in the same round) a workers-topology consumer's wasm32
//! build failed while every native check passed: `cargo check` is
//! native-only and CI never built a worker wasm target, so the failures
//! only surfaced at `wrangler dev`'s `worker-build` on the consumer's
//! machine. This gate closes that window: it generates a workers-topology
//! fixture with extension points and compile-checks the emitted plane —
//! the domain worker crate (lib + bin) natively, then the worker lib AND
//! the gateway lib for `wasm32-unknown-unknown` with the exact feature
//! set `worker-build` uses (`--no-default-features --features
//! cloudflare-worker`).
//!
//! The generated `cornucopia-queries` crate is replaced by a check stub
//! (the shared `test_framework::cornucopia_queries_stub` plus query
//! modules for the fixture's one entity): its real build.rs runs
//! cornucopia codegen against a live migrated Postgres, which no CI lane
//! owes this test. The stub carries the `GenericClient` trait with the
//! exact signature set the generated `db_client.rs` implements, and
//! per-query stub functions matching the shapes the generated
//! `cornucopia_repository_impl.rs` calls. A tripwire parses the emitted
//! `queries/**/*.sql` `--!` names and pins the stub's function set to
//! them, so a fixture or emitter change fails loudly instead of checking
//! against a stale surface.
//!
//! Teeth (the issue's "wasm-broken mutation then rejected"): after the
//! green checks, the test surgically reverts the #537 `spawn_local` fix
//! in the emitted `integration_dispatch.rs` (the wasm fetch leg awaited
//! directly, poisoning `notify`'s `Send`) and asserts the wasm check
//! REJECTS the tree; then removes `mod integration_dispatch;` from the
//! emitted `main.rs` (the #537 module-gap class) and asserts the native
//! whole-package check REJECTS it. Both mutations are restored.
//!
//! Ignored by default: downloads crates.io deps and compiles the worker
//! workspace for two targets (~3–6 min cold, ~1–2 min warm). Wired into
//! the `test-ops-integration` CI lane (which installs the wasm32 target):
//!
//! ```bash
//! cargo test -p codegraph --test worker_wasm_compile_tests -- --ignored --nocapture
//! ```
//!
//! Known gaps (documented deferrals, not regressions this fixture
//! exercises): the events/webhook worker surfaces ride separate fixtures;
//! `api/media_route.tera`'s sea_orm usage is unguarded and only bites
//! worker models with media fields (the fixture has none — a template fix
//! would change emitted bytes and owes the byte-identity re-bless); the
//! worker bin is native-only by design (`worker-build` compiles the lib),
//! so the wasm check is `--lib` and the bin's module list is covered by
//! the native whole-package check instead.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use codegraph::generate::template_engine;
use codegraph::profile::BuildPlan;

#[path = "test_framework/mod.rs"]
mod test_framework;

use test_framework::cornucopia_queries_stub;

/// The fixture's entity: one covered root entity with a required and a
/// nullable text column. Deliberately minimal — the gate targets the
/// worker plane's module surface (integration_dispatch, app_state, the
/// command modules), not the full query-family matrix.
const ENTITY_TITLE: &str = "PayRunType";
const DOMAIN: &str = "compensation";
const TABLE: &str = "pay_run";
const APP_NAME: &str = "compilecheck";
const WORKER_CRATE: &str = "compilecheck-compensation";
const GATEWAY_CRATE: &str = "compilecheck-gateway";

/// Stub surface for the fixture's query module (appended to the shared
/// stub's `lib.rs` as `pub mod queries;`). Shapes mirror cornucopia's
/// generated per-query modules: a `Stmt` holder + constructor, a sync
/// `bind` returning a row-query builder (`one`/`all`/`opt`) for queries
/// with a return annotation, and an async `bind` returning rows-affected
/// for exec queries. Bind arity/order/optionality match the shapes the
/// generated `cornucopia_repository_impl.rs` calls (which follow the
/// emitted SQL's first-occurrence order, not the header order — e.g.
/// `update` binds the SET params before the trailing `&id`). Bodies never
/// execute — the gate is compile-only.
const QUERIES_RS: &str = r#"//! Check-time stub (test scaffold): the fixture's query modules.
//! (This file IS the crate's `queries` module.)

pub mod compensation {
    pub mod pay_run {
        use crate::client::GenericClient;
        use std::marker::PhantomData;

        type Error = tokio_postgres::Error;

        /// Row shape: field-for-field what the generated
        /// `cornucopia_repository_impl.rs` reads off `.opt()`/`.all()`
        /// rows — the direct-copy fields must equal the DTO field types.
        #[derive(Debug)]
        pub struct GetPayRun {
            pub id: uuid::Uuid,
            pub name: String,
            pub notes: Option<String>,
            pub created_at: chrono::DateTime<chrono::Utc>,
            pub updated_at: chrono::DateTime<chrono::Utc>,
        }

        pub type ListPayRun = GetPayRun;

        // --! create_pay_run (name, notes?) : (id)
        pub struct CreatePayRunStmt;

        pub fn create_pay_run() -> CreatePayRunStmt {
            CreatePayRunStmt
        }

        pub struct CreatePayRunQuery<'c, 'a, 's, C: GenericClient> {
            _marker: PhantomData<(&'c C, &'a (), &'s ())>,
        }

        impl CreatePayRunStmt {
            pub fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
                _name: &'a String,
                _notes: &'a Option<String>,
            ) -> CreatePayRunQuery<'c, 'a, 's, C> {
                CreatePayRunQuery {
                    _marker: PhantomData,
                }
            }
        }

        impl<'c, 'a, 's, C: GenericClient> CreatePayRunQuery<'c, 'a, 's, C> {
            pub async fn one(&self) -> Result<uuid::Uuid, Error> {
                unreachable!("check stub never executes")
            }
        }

        // --! get_pay_run (id) : (id, name, notes?, created_at, updated_at)
        pub struct GetPayRunStmt;

        pub fn get_pay_run() -> GetPayRunStmt {
            GetPayRunStmt
        }

        pub fn get_pay_run_including_deleted() -> GetPayRunStmt {
            GetPayRunStmt
        }

        pub struct GetPayRunQuery<'c, 'a, 's, C: GenericClient> {
            _marker: PhantomData<(&'c C, &'a uuid::Uuid, &'s ())>,
        }

        impl GetPayRunStmt {
            pub fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
                _id: &'a uuid::Uuid,
            ) -> GetPayRunQuery<'c, 'a, 's, C> {
                GetPayRunQuery {
                    _marker: PhantomData,
                }
            }
        }

        impl<'c, 'a, 's, C: GenericClient> GetPayRunQuery<'c, 'a, 's, C> {
            pub async fn opt(&self) -> Result<Option<GetPayRun>, Error> {
                unreachable!("check stub never executes")
            }
        }

        // --! list_pay_run (offset, page_size) : (id, name, notes?, ...)
        pub struct ListPayRunStmt;

        pub fn list_pay_run() -> ListPayRunStmt {
            ListPayRunStmt
        }

        pub fn list_pay_run_including_deleted() -> ListPayRunStmt {
            ListPayRunStmt
        }

        pub struct ListPayRunQuery<'c, 'a, 's, C: GenericClient> {
            _marker: PhantomData<(&'c C, &'a i64, &'s ())>,
        }

        impl ListPayRunStmt {
            pub fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
                _page_size: &'a i64,
                _offset: &'a i64,
            ) -> ListPayRunQuery<'c, 'a, 's, C> {
                ListPayRunQuery {
                    _marker: PhantomData,
                }
            }
        }

        impl<'c, 'a, 's, C: GenericClient> ListPayRunQuery<'c, 'a, 's, C> {
            pub async fn all(&self) -> Result<Vec<ListPayRun>, Error> {
                unreachable!("check stub never executes")
            }
        }

        // --! count_pay_run : (count)
        pub struct CountPayRunStmt;

        pub fn count_pay_run() -> CountPayRunStmt {
            CountPayRunStmt
        }

        pub fn count_pay_run_including_deleted() -> CountPayRunStmt {
            CountPayRunStmt
        }

        pub struct CountPayRunQuery<'c, 'a, 's, C: GenericClient> {
            _marker: PhantomData<(&'c C, &'a (), &'s ())>,
        }

        impl CountPayRunStmt {
            pub fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
            ) -> CountPayRunQuery<'c, 'a, 's, C> {
                CountPayRunQuery {
                    _marker: PhantomData,
                }
            }
        }

        impl<'c, 'a, 's, C: GenericClient> CountPayRunQuery<'c, 'a, 's, C> {
            pub async fn one(&self) -> Result<i64, Error> {
                unreachable!("check stub never executes")
            }
        }

        // --! update_pay_run (id, name?, notes?) — exec: SET params bind
        // in SQL first-occurrence order, the trailing `&id` last.
        pub struct UpdatePayRunStmt;

        pub fn update_pay_run() -> UpdatePayRunStmt {
            UpdatePayRunStmt
        }

        impl UpdatePayRunStmt {
            pub async fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
                _name: &'a Option<String>,
                _notes: &'a Option<String>,
                _id: &'a uuid::Uuid,
            ) -> Result<u64, Error> {
                unreachable!("check stub never executes")
            }
        }

        // --! delete_pay_run (id) — exec
        pub struct DeletePayRunStmt;

        pub fn delete_pay_run() -> DeletePayRunStmt {
            DeletePayRunStmt
        }

        impl DeletePayRunStmt {
            pub async fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
                _id: &'a uuid::Uuid,
            ) -> Result<u64, Error> {
                unreachable!("check stub never executes")
            }
        }
    }
}
"#;

/// Fixed materialization root (same convention as the hooks runtime
/// harness): survives a failed run so the generated tree can be inspected.
fn tree_root() -> PathBuf {
    std::env::temp_dir().join("worker-wasm-gate-tree")
}

/// Build cache shared across runs (and warmed by CI's rust-cache): a
/// subdir of the workspace `target/` so it never collides with the
/// workspace build's lock.
fn gate_target_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("WORKER_WASM_GATE_TARGET_DIR") {
        return PathBuf::from(dir);
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/worker-wasm-gate")
}

struct CheckOutcome {
    success: bool,
    stdout: String,
    stderr: String,
}

fn cargo_check(args: &[&str]) -> CheckOutcome {
    let output = Command::new("cargo")
        .args(args)
        .arg("--manifest-path")
        .arg(tree_root().join("app").join("workers").join("Cargo.toml"))
        .env("CARGO_TERM_COLOR", "never")
        .env("CARGO_TARGET_DIR", gate_target_dir())
        .output()
        .expect("failed to spawn cargo check");
    CheckOutcome {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn assert_check_green(label: &str, outcome: &CheckOutcome) {
    assert!(
        outcome.success,
        "cargo check ({label}) was expected to pass:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        outcome.stdout, outcome.stderr
    );
}

fn assert_check_red(label: &str, expected_marker: &str, outcome: &CheckOutcome) {
    assert!(
        !outcome.success,
        "cargo check ({label}) was expected to FAIL — the mutation must be rejected:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        outcome.stdout, outcome.stderr
    );
    assert!(
        outcome.stderr.contains(expected_marker) || outcome.stdout.contains(expected_marker),
        "cargo check ({label}) failed, but not with the expected `{expected_marker}` failure class:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        outcome.stdout,
        outcome.stderr
    );
}

/// The annotation names in the emitted SQL — the stub function set is
/// pinned to exactly this list (both directions).
fn emitted_query_fns(output: &Path) -> Vec<String> {
    let sql_path = output
        .join("queries")
        .join(DOMAIN)
        .join(format!("{TABLE}.sql"));
    let sql = fs::read_to_string(&sql_path).unwrap_or_else(|e| {
        panic!(
            "emitted query SQL must exist at {}: {e}",
            sql_path.display()
        )
    });
    let mut names: Vec<String> = sql
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            trimmed
                .strip_prefix("--! ")?
                .split_whitespace()
                .next()
                .map(|name| name.trim_end_matches('(').to_string())
        })
        .collect();
    names.sort();
    assert!(
        !names.is_empty(),
        "emitted query SQL must carry `--!` annotations: {}",
        sql_path.display()
    );
    names
}

fn stub_query_fns() -> Vec<String> {
    let mut names: Vec<String> = QUERIES_RS
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let rest = trimmed.strip_prefix("pub fn ")?;
            // Constructor signature only (`pub fn name() -> XStmt`) — skips
            // the `impl` blocks' `bind` methods (generic or `&self`-led).
            let name_end = rest.find('(')?;
            let after = &rest[name_end..];
            if !after.starts_with("() ->") {
                return None;
            }
            Some(rest[..name_end].to_string())
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

#[tokio::test]
#[ignore = "compiles a generated workers workspace for native + wasm32: downloads crates.io deps, ~3-6 min cold (needs the wasm32-unknown-unknown target)"]
async fn generated_worker_wasm_check_is_in_ci() {
    // ── Fixture: workers topology + extension points, cornucopia provider ──
    let pay_run = codegraph_core::types::SchemaNode {
        namespace: None,
        schema_id: "compensation/json/PayRunType.json".to_string(),
        title: ENTITY_TITLE.to_string(),
        description: Some("A pay run".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some(DOMAIN.to_string()),
        rel_path: "compensation/json/PayRunType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "PayRun".to_string(),
        pg_table_name: TABLE.to_string(),
        api_path_segment: "pay-runs".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: true,
        has_one_of: false,
        has_any_of: false,
        has_definitions: true,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    };
    let text_prop = |name: &str, nullable: bool| codegraph_core::types::PropertyNode {
        is_id: false,
        name: name.to_string(),
        prop_type: "string".to_string(),
        description: Some(name.to_string()),
        format: None,
        is_required: !nullable,
        is_nullable: nullable,
        is_array: false,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: name.to_string(),
        pg_column_type: "TEXT".to_string(),
        rust_field_name: name.to_string(),
        rust_field_type: "String".to_string(),
        sea_orm_type: "Text".to_string(),
        render_strategy: "direct_column".to_string(),
        ref_target: None,
        classification: Some("primitive_wrapper".to_string()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    };
    let props = vec![text_prop("name", false), text_prop("notes", true)];
    let mock = codegraph_core::mock::MockEngine::builder()
        .with_schema(pay_run)
        .with_properties(ENTITY_TITLE, props)
        .build();
    let config = codegraph_config::config::parse_domain_config_str(
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.compensation]
label = "Compensation"
schema_dir = "compensation"
postgres_schema = "compensation"
entities = ["PayRunType"]
"#,
    )
    .unwrap();
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = template_engine::create_tera(&template_dir).unwrap();

    let root = tree_root();
    let _ = fs::remove_dir_all(&root);
    let output_dir = root.join("app");
    let domain_types_base = output_dir.join("domain-types");
    let hooks_tmp = root.join("hooks");
    fs::create_dir_all(&hooks_tmp).unwrap();

    let ext = codegraph_ext_points::parse_extension_points_str(
        r#"
[payroll-sync]
name = "Payroll Sync"
description = "Synchronise pay data"
cardinality = "exclusive"
entities = ["compensation.pay_run"]
directions = ["push"]
"#,
    )
    .unwrap();

    // The workers workspace manifest path-depends on the repo's
    // codegraph-workflow / codegraph-type-contracts crates; Cargo resolves
    // the whole workspace when writing its lockfile, so point them at the
    // real checkout (absolute paths pass through `resolve_path` unchanged).
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let project = codegraph::generate::ProjectConfig {
        identity: codegraph::generate::IdentityConfig {
            app_name: APP_NAME.to_string(),
            domain_types_crate: "domain_types".to_string(),
            ..Default::default()
        },
        database: codegraph::generate::DatabaseConfig {
            database_target: codegraph::generate::db::dialect::DatabaseTarget::Postgres,
            persistence_provider: codegraph::profile::PersistenceProvider::Cornucopia,
        },
        deployment: codegraph::generate::DeploymentConfig {
            deployment_topology: codegraph::profile::DeploymentTopology::Workers,
        },
        integration: codegraph::generate::IntegrationFlags {
            has_extension_points: true,
            ..Default::default()
        },
        paths: codegraph::generate::PathsConfig {
            // Output-anchored: the domain-types crate is scaffolded INSIDE
            // the generated tree and the worker manifests path-depend on it,
            // so the gate's wasm check compiles it for wasm32 as a dep.
            domain_types_base: "domain-types".to_string(),
            codegraph_workflow_base: repo
                .join("crates/codegraph-workflow")
                .to_string_lossy()
                .into_owned(),
            type_contracts_base: repo
                .join("crates/codegraph-type-contracts")
                .to_string_lossy()
                .into_owned(),
            ..Default::default()
        },
        ..Default::default()
    };

    let plan = BuildPlan {
        entity_generators: vec![
            "ddl".to_string(),
            "sea_orm_entity".to_string(),
            "cornucopia_queries".to_string(),
            "cornucopia_repo".to_string(),
            "handler".to_string(),
            "dto".to_string(),
            "repository".to_string(),
            "command".to_string(),
            "query".to_string(),
            // ENTITY generators: the DTO definitions + query service live
            // in the domain-types crate (the worker's dto files re-export
            // from it — the crate must exist for the worker to compile).
            "domain_types_dto".to_string(),
            "domain_types_query_service".to_string(),
        ],
        domain_generators: vec![
            "errors".to_string(),
            "router".to_string(),
            "links".to_string(),
        ],
        global_generators: vec![
            "worker_scaffold".to_string(),
            // Emits the cornucopia-queries crate skeleton at the output root
            // (the worker/gateway Cargo manifests path-depend on it) and the
            // annotated SQL the stub tripwire pins against.
            "cornucopia_config".to_string(),
            "integration_tables".to_string(),
            "integration_config".to_string(),
            "integration_dispatch".to_string(),
            "integration_catalog".to_string(),
            // GLOBAL: the domain-types crate scaffold (lib.rs / mod.rs /
            // Cargo.toml) — output-anchored via `paths.domain_types_base`.
            "domain_types_scaffold".to_string(),
        ],
        post_gen_scripts: vec![],
        ifml_frameworks: vec![],
        template_pack_path: None,
        database_target: codegraph::generate::db::dialect::DatabaseTarget::Postgres,
        has_atproto: false,
        atproto_tenancy: "shared_pds".to_string(),
        has_fern: false,
        has_emdash: false,
        fern_sdk_languages: vec!["typescript".to_string()],
        persistence_provider: codegraph::profile::PersistenceProvider::Cornucopia,
        dto_key_casing: "snake".to_string(),
        deployment_topology: codegraph::profile::DeploymentTopology::Workers,
        dependency_strategy: codegraph::profile::DependencyStrategy::Rev,
        namespace_layout: false,
        ux_rules: false,
        expr_ir: false,
        ifml_e2e_auth: false,
        ifml_remote_functions: false,
        entity_remote_functions: false,
        testid_module: false,
        structural_gates: false,
        public_operations_rls: false,
        features: toml::Table::new(),
    };

    let report =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            integration_manifests: None,
            db: &mock,
            config: &config,
            output_dir: &output_dir,
            tera: &tera,
            ui_overrides: &Default::default(),
            ui_domains: &Default::default(),
            schema_base_dir: Path::new(""),
            domain_types_base: Some(&domain_types_base),
            hooks_base: Some(&hooks_tmp),
            ext_points: Some(&ext),
            seed_config: None,
            build_plan: Some(&plan),
            ifml_frameworks: vec![],
            ifml_components: None,
            ux_rules: None,
            project_config: Some(&project),
            emdash_plugins: None,
            domain_config_dir: None,
        })
        .await
        .unwrap();
    assert!(
        !report.has_errors(),
        "generation should succeed: {}",
        report.summary()
    );

    // The issue's minimum surface must be on disk: the per-worker module
    // tree (integration_dispatch, app_state, the command modules) plus the
    // gateway catalog surface.
    let worker_src = output_dir.join("workers").join(DOMAIN).join("src");
    for required in [
        "integration_dispatch.rs",
        "app_state.rs",
        "main.rs",
        "lib.rs",
        "worker.rs",
        "db_client.rs",
        "domain/compensation/pay_run/command.rs",
        "domain/compensation/pay_run/cornucopia_repository_impl.rs",
        "domain/compensation/pay_run/dto_response.rs",
    ] {
        let path = worker_src.join(required);
        assert!(
            path.exists(),
            "missing generated worker file: {}",
            path.display()
        );
    }
    for required in [
        "workers/gateway/src/integration_catalog.rs",
        "workers/gateway/src/integration_router.rs",
        "workers/gateway/src/app_state.rs",
        "workers/Cargo.toml",
        "cornucopia-queries/Cargo.toml",
        "domain-types/Cargo.toml",
        "queries/compensation/pay_run.sql",
    ] {
        let path = output_dir.join(required);
        assert!(path.exists(), "missing generated file: {}", path.display());
    }

    // ── Replace the generated cornucopia-queries skeleton with the stub ──
    // (see module doc): no live-Postgres codegen, same trait surface plus
    // the fixture's query modules.
    let queries_dir = output_dir.join("cornucopia-queries");
    fs::write(
        queries_dir.join("Cargo.toml"),
        cornucopia_queries_stub::CARGO_TOML,
    )
    .unwrap();
    fs::write(
        queries_dir.join("build.rs"),
        cornucopia_queries_stub::BUILD_RS,
    )
    .unwrap();
    fs::write(
        queries_dir.join("src").join("lib.rs"),
        format!("{}\npub mod queries;\n", cornucopia_queries_stub::LIB_RS),
    )
    .unwrap();
    fs::write(queries_dir.join("src").join("queries.rs"), QUERIES_RS).unwrap();

    // ── Tripwire: the stub's function set is pinned to the emitted SQL ──
    let sql_fns = emitted_query_fns(&output_dir);
    let stub_fns = stub_query_fns();
    assert_eq!(
        sql_fns, stub_fns,
        "the stub query surface must cover EXACTLY the emitted `--!` annotations \
         (update QUERIES_RS when the fixture or the SQL emitter changes)"
    );

    // ── Phase 1: native-green first (whole package: lib + bin) ──────────
    // The bin target is where the #537 module-gap class (worker_main's
    // `mod` list drifting from worker_lib's) manifests as E0433.
    let native = cargo_check(&["check", "-p", WORKER_CRATE]);
    assert_check_green("native whole-package worker", &native);

    // ── Phase 2: wasm-green, domain worker lib ──────────────────────────
    // Mirrors `worker-build --no-default-features --features
    // cloudflare-worker` (the lib is what worker-build compiles). The
    // domain-types / codegraph-workflow / codegraph-type-contracts deps
    // compile for wasm32 as part of this check.
    let wasm_worker = cargo_check(&[
        "check",
        "--lib",
        "-p",
        WORKER_CRATE,
        "--no-default-features",
        "--features",
        "cloudflare-worker",
        "--target",
        "wasm32-unknown-unknown",
    ]);
    assert_check_green("wasm32 domain worker lib", &wasm_worker);

    // ── Phase 3: wasm-green, gateway lib ────────────────────────────────
    let wasm_gateway = cargo_check(&[
        "check",
        "--lib",
        "-p",
        GATEWAY_CRATE,
        "--no-default-features",
        "--features",
        "cloudflare-worker",
        "--target",
        "wasm32-unknown-unknown",
    ]);
    assert_check_green("wasm32 gateway lib", &wasm_gateway);

    // ── Phase 4: mutation A — revert the #537 spawn_local fix ───────────
    // Awaiting the wasm fetch leg directly poisons `notify`'s `Send`, so
    // every covered handler future is `!Send` and axum's `Handler` bound
    // refuses the router — the exact consumer failure this gate exists
    // for. The check must REJECT the mutated tree.
    const SPAWN_BLOCK: &str = "wasm_bindgen_futures::spawn_local(async move {\n                    this.post_event(&worker_url, &payload, &installation_id, signing_secret.as_deref())\n                        .await;\n                });";
    let dispatch_path = worker_src.join("integration_dispatch.rs");
    let dispatch_original = fs::read_to_string(&dispatch_path).unwrap();
    assert!(
        dispatch_original.contains(SPAWN_BLOCK),
        "mutation A prerequisite: the #537 spawn_local fix must be present \
         in the emitted integration_dispatch.rs (the dispatcher.tera shape \
         moved — update SPAWN_BLOCK)"
    );
    let dispatch_mutated = dispatch_original.replacen(
        SPAWN_BLOCK,
        "(async move {\n                    this.post_event(&worker_url, &payload, &installation_id, signing_secret.as_deref())\n                        .await;\n                })\n                .await;",
        1,
    );
    assert_ne!(
        dispatch_mutated, dispatch_original,
        "mutation A surgery must change the emitted dispatch"
    );
    fs::write(&dispatch_path, &dispatch_mutated).unwrap();
    let wasm_mutated = cargo_check(&[
        "check",
        "--lib",
        "-p",
        WORKER_CRATE,
        "--no-default-features",
        "--features",
        "cloudflare-worker",
        "--target",
        "wasm32-unknown-unknown",
    ]);
    assert_check_red(
        "wasm32 with the #536 break re-introduced",
        "Handler",
        &wasm_mutated,
    );
    fs::write(&dispatch_path, &dispatch_original).unwrap();

    // ── Phase 5: mutation B — the #537 module-gap class ─────────────────
    // Removing `mod integration_dispatch;` from the emitted main.rs (the
    // bin re-declares the domain modules) leaves the bin's command module
    // referencing a crate path that no longer resolves: E0433 on the
    // native whole-package check.
    let main_path = worker_src.join("main.rs");
    let main_original = fs::read_to_string(&main_path).unwrap();
    assert!(
        main_original.contains("mod integration_dispatch;"),
        "mutation B prerequisite: the emitted main.rs must declare \
         `mod integration_dispatch;` (the #537 fix)"
    );
    let main_mutated = main_original.replace("mod integration_dispatch;\n", "");
    assert_ne!(
        main_mutated, main_original,
        "mutation B surgery must change the emitted main.rs"
    );
    fs::write(&main_path, &main_mutated).unwrap();
    let native_mutated = cargo_check(&["check", "-p", WORKER_CRATE]);
    assert_check_red(
        "native whole-package with the #537 module gap re-introduced",
        "E0433",
        &native_mutated,
    );
    fs::write(&main_path, &main_original).unwrap();
}
