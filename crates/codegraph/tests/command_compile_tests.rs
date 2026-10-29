//! Compile pin for the CQRS command/query plane with hooks and extension
//! points enabled (issue #551 — the compile-pin policy: every
//! Rust-emitting generator carries a test that COMPILES its output).
//!
//! Three planes ride one scratch crate here because they share the command
//! module's import surface:
//!
//! 1. **command + query modules** (sea_orm leg) — the hooks-gated
//!    (`hooks: Option<Arc<dyn …Lifecycle>>`, `TxContext`/`HookTxAdapter`)
//!    and ext-points-gated (`dispatcher: Option<Arc<IntegrationDispatcher>>`,
//!    `notify(…)` call sites) code paths compile NOWHERE else: the
//!    full-app compile gate runs with hooks/ext-points off, and the
//!    cornucopia runtime pin (`hooks_cornucopia_runtime_tests`) covers the
//!    cornucopia provider only.
//! 2. **the push dispatcher** (`integration/dispatcher.tera` →
//!    `src/integration_dispatch.rs`, the SeaORM monolith leg) — previously
//!    compiled by no gate at all (the workers leg is `#550`'s wasm gate).
//! 3. **the hooks types** (the generated lifecycle trait +
//!    `generated/types.rs` SPI) — previously compiled only by the
//!    `--ignored` Postgres runtime pin.
//!
//! Template shape: `mox_plane_compile_tests` (issue #517); materialization
//! uses a fixed root (the #550 convention) and a shared cached target dir.
//!
//! Teeth (validated à la #550): breaking a gated import in `command.tera`,
//! deleting a `notify` call site, or emitting an unresolvable hooks-crate
//! path fails `cargo check`; skipping any of the three planes' emission
//! fails the presence guards below.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use codegraph_ext_points::parse_extension_points_str;
use codegraph_grafeo::GrafeoEngine;

/// Extension point covering the fixture entity (CandidateType → table
/// `candidate` in domain `recruiting`), push direction — the shape the
/// command-layer call sites key on (same fixture as
/// `extension_points_tests`).
const EXT_POINTS_TOML: &str = r#"
[payroll-sync]
name = "Payroll Sync"
description = "Synchronise employee and pay data"
cardinality = "exclusive"
entities = ["recruiting.candidate"]
directions = ["push"]
"#;

/// The hooks crate name the emitted command/query paths reference
/// (`{hooks_api_crate}::generated::…`).
const HOOKS_CRATE: &str = "my_hooks";

/// Fixed materialization root (the #550 convention): survives a failed run
/// so the generated tree can be inspected.
fn tree_root() -> PathBuf {
    std::env::temp_dir().join("command-pin-tree")
}

/// Build cache shared across runs (and warmed by CI's rust-cache): a
/// subdir of the workspace `target/` so it never collides with the
/// workspace build's lock.
fn pin_target_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("COMMAND_PIN_TARGET_DIR") {
        return PathBuf::from(dir);
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/command-pin")
}

fn write_file(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn copy_file(from: &Path, to: &Path) {
    fs::create_dir_all(to.parent().unwrap()).unwrap();
    fs::copy(from, to).unwrap();
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

/// Ingest the fixture schemas and run the default-plan generation with
/// hooks + extension points ON, into `tree_root()/app` (domain types
/// beside it, hooks crate under `tree_root()/my-hooks`). Returns
/// `(app_dir, domain_types_dir, hooks_dir)`.
async fn generate_tree() -> (PathBuf, PathBuf, PathBuf) {
    let root = tree_root();
    let _ = fs::remove_dir_all(&root);
    let app_dir = root.join("app");
    let domain_types_dir = root.join("domain-types");
    let hooks_dir = root.join("my-hooks");
    fs::create_dir_all(&app_dir).unwrap();

    let engine = GrafeoEngine::in_memory().unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let classifier = codegraph_classifier::config::parse_classifier_config(Path::new(
        "tests/fixtures/classifier.toml",
    ))
    .unwrap();
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

    let registry = codegraph::profile::CapabilityRegistry::new();
    let mut resolved = codegraph::profile::load_and_resolve_profile(
        &workspace_root.join("profiles.toml"),
        "default",
        None,
    )
    .unwrap();
    for section in resolved.sections.values_mut() {
        section.generators.retain(|g| !g.starts_with("grpc_"));
    }
    let plan = codegraph::profile::BuildPlan::from_profile(&resolved, &registry).unwrap();

    let project_config = codegraph::generate::ProjectConfig {
        identity: codegraph::generate::IdentityConfig {
            app_name: "command-pin".into(),
            domain_types_crate: "domain_types".into(),
            generator_name: "command-compile-pin".into(),
            hooks_api_crate: HOOKS_CRATE.into(),
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
        integration: codegraph::generate::IntegrationFlags {
            has_extension_points: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let ext = parse_extension_points_str(EXT_POINTS_TOML).unwrap();

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
            ext_points: Some(&ext),
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

    (app_dir, domain_types_dir, hooks_dir)
}

/// The command/query-plane files of one entity module.
const MODULE_FILES: &[&str] = &[
    "command.rs",
    "query.rs",
    "repository.rs",
    "dto_response.rs",
    "dto_create.rs",
    "dto_update.rs",
];

/// Assemble the scratch cargo workspace: the emitted domain-types crate
/// (whole), the `my_hooks` crate (generated lifecycle trait + SPI types
/// verbatim + the consumer-owned error surface), and an app crate carrying
/// the command/query planes plus their gated dependencies
/// (`hook_adapters.rs`, `integration_dispatch.rs`, `error.rs`,
/// `api::{links,meta}.rs`, per-domain `errors.rs`). Returns the number of
/// entity modules assembled.
fn assemble_workspace(
    app_dir: &Path,
    domain_types_dir: &Path,
    hooks_dir: &Path,
    ws: &Path,
) -> usize {
    // domain-types: the emitted crate, whole.
    copy_dir_recursive(domain_types_dir, &ws.join("domain-types"));

    // my-hooks: the generated `generated` tree verbatim + the
    // consumer-owned lib/error surface (the hooks_cornucopia_runtime_tests
    // pattern — the registry is omitted, it pulls axum/QueryService, and
    // `generated/mod.rs` is rewritten to match).
    let generated_src = hooks_dir.join("src").join("generated");
    assert!(
        generated_src.exists(),
        "the hooks tree must carry src/generated/: {:?}",
        hooks_dir
    );
    copy_dir_recursive(&generated_src, &ws.join("my-hooks/src/generated"));
    let _ = fs::remove_file(ws.join("my-hooks/src/generated/registry.rs"));
    let mut generated_mod = String::from("pub mod types;\n");
    for entry in fs::read_dir(ws.join("my-hooks/src/generated")).unwrap() {
        let entry = entry.unwrap();
        if entry.path().is_dir() {
            generated_mod.push_str(&format!(
                "pub mod {};\n",
                entry.file_name().to_string_lossy()
            ));
        }
    }
    write_file(&ws.join("my-hooks/src/generated/mod.rs"), &generated_mod);
    write_file(
        &ws.join("my-hooks/src/lib.rs"),
        r#"//! Compile-pin hooks crate: generated `generated` tree + consumer error/bulk types.

pub mod error;
pub mod generated;

pub use error::{BulkCreateResults, BulkItemError, HookError};
"#,
    );
    write_file(
        &ws.join("my-hooks/src/error.rs"),
        r#"//! Consumer-owned hooks crate error surface (HookError) plus the bulk
//! result types the generated command layer references.

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
"#,
    );
    write_file(
        &ws.join("my-hooks/Cargo.toml"),
        r#"[package]
name = "my-hooks"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
async-trait = "0.1"
domain-types = { path = "../domain-types" }
thiserror = "2"
uuid = { version = "1", features = ["v4", "serde"] }
"#,
    );

    // app crate: the command/query planes + their gated dependencies.
    for file in [
        "src/error.rs",
        "src/hook_adapters.rs",
        "src/integration_dispatch.rs",
    ] {
        let src = app_dir.join(file);
        assert!(
            src.exists(),
            "the generated tree must carry {file} (hooks + ext-points ON)"
        );
        copy_file(&src, &ws.join("app").join(file));
    }
    for file in ["src/api/links.rs", "src/api/meta.rs"] {
        copy_file(&app_dir.join(file), &ws.join("app").join(file));
    }

    let mut modules = 0usize;
    let mut lib = String::from(
        "pub mod api {\n    pub mod links;\n    pub mod meta;\n}\npub mod error;\npub mod hook_adapters;\npub mod integration_dispatch;\npub mod domain {\n",
    );
    let domain_root = app_dir.join("src").join("domain");
    for domain_entry in fs::read_dir(&domain_root).unwrap() {
        let domain_entry = domain_entry.unwrap();
        if !domain_entry.path().is_dir() {
            continue;
        }
        let domain = domain_entry.file_name().to_string_lossy().to_string();
        let mut domain_mod = format!("    pub mod {domain} {{\n");
        let errors = domain_entry.path().join("errors.rs");
        if errors.exists() {
            copy_file(
                &errors,
                &ws.join("app/src/domain").join(&domain).join("errors.rs"),
            );
            domain_mod.push_str("        pub mod errors;\n");
        }
        let mut has_any = false;
        for module_entry in fs::read_dir(domain_entry.path()).unwrap() {
            let module_entry = module_entry.unwrap();
            if !module_entry.path().is_dir() {
                continue;
            }
            let module = module_entry.file_name().to_string_lossy().to_string();
            let mut module_mod = format!("        pub mod {module} {{\n");
            let mut copied = 0usize;
            for file in MODULE_FILES {
                let src = module_entry.path().join(file);
                if src.exists() {
                    copy_file(
                        &src,
                        &ws.join("app/src/domain")
                            .join(&domain)
                            .join(&module)
                            .join(file),
                    );
                    module_mod.push_str(&format!(
                        "            pub mod {};\n",
                        file.strip_suffix(".rs").unwrap()
                    ));
                    copied += 1;
                }
            }
            if copied > 0 {
                module_mod.push_str("        }\n");
                domain_mod.push_str(&module_mod);
                has_any = true;
                modules += 1;
            }
        }
        domain_mod.push_str("    }\n");
        if has_any {
            lib.push_str(&domain_mod);
        }
    }
    lib.push_str("}\n");
    assert!(modules > 0, "no entity modules were assembled");
    write_file(&ws.join("app/src/lib.rs"), &lib);

    write_file(
        &ws.join("app/Cargo.toml"),
        r#"[package]
name = "command-pin-app"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
name = "command_pin_app"
path = "src/lib.rs"

[dependencies]
async-trait = "0.1"
axum = "0.8"
chrono = { version = "0.4", features = ["serde"] }
domain-types = { path = "../domain-types" }
garde = { version = "0.22", features = ["derive", "regex"] }
hex = "0.4"
hmac = "0.12"
my-hooks = { path = "../my-hooks" }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
sea-orm = { version = "2", features = ["sqlx-postgres", "runtime-tokio-rustls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
thiserror = "2"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "time"] }
tracing = "0.1"
utoipa = { version = "6", features = ["chrono", "uuid", "decimal"] }
uuid = { version = "1", features = ["v4", "serde"] }
"#,
    );

    modules
}

fn cargo_check(ws: &Path) {
    let out = Command::new("cargo")
        .arg("check")
        .arg("--manifest-path")
        .arg(ws.join("app/Cargo.toml"))
        .env("CARGO_TERM_COLOR", "never")
        .env("CARGO_TARGET_DIR", pin_target_dir())
        .output()
        .expect("cargo check must run");
    assert!(
        out.status.success(),
        "the command/query/dispatcher/hooks planes must compile in isolation:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// Mandate (issue #551): the hooks-gated + ext-points-gated command/query
/// paths, the monolith dispatcher, and the hooks types compile in
/// isolation.
#[tokio::test]
async fn command_query_planes_with_hooks_and_dispatch_compile_in_isolation() {
    let (app_dir, domain_types_dir, hooks_dir) = generate_tree().await;

    // Silent-emission guards: the gated paths must actually be present.
    let command = fs::read_to_string(app_dir.join("src/domain/recruiting/candidate/command.rs"))
        .expect("command.rs must be emitted");
    for fragment in [
        "hooks: Option<Arc<dyn",
        "dispatcher: Option<std::sync::Arc<crate::integration_dispatch::IntegrationDispatcher>>",
        "dispatcher.notify(",
        "crate::hook_adapters::HookTxAdapter(&tx)",
    ] {
        assert!(
            command.contains(fragment),
            "the hooks/ext-points command path must carry `{fragment}`:\n{command}"
        );
    }
    let dispatch = fs::read_to_string(app_dir.join("src/integration_dispatch.rs"))
        .expect("the monolith dispatcher leg must be emitted");
    assert!(
        dispatch.contains("pub struct IntegrationDispatcher"),
        "the dispatcher struct must be emitted:\n{dispatch}"
    );
    assert!(
        hooks_dir.join("src/generated/types.rs").exists(),
        "the hooks SPI types must be emitted"
    );

    let ws = tree_root().join("ws");
    let modules = assemble_workspace(&app_dir, &domain_types_dir, &hooks_dir, &ws);
    assert!(
        modules >= 2,
        "the fixtures must emit at least two command modules, got {modules}"
    );
    cargo_check(&ws);
}
