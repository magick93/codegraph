//! Compile pin for the SeaORM repository plane (issue #551 — the
//! compile-pin policy: every Rust-emitting generator carries a test that
//! COMPILES its output; string-assert suites passed through both #517 and
//! #536).
//!
//! The emitted repository plane — the trait (`repository.tera`), the
//! SeaORM impl (`repository_emitter`, including child-table, junction,
//! include-path and design-finder emission) and the DTO re-export shims it
//! resolves through — must compile in a scratch crate carrying ONLY the
//! plane's dependency set. The full-app compile gate
//! (`grafeo_generated_code_compiles`) already exercises this code, but a
//! failure there is neither attributable to the plane nor cheap to run;
//! this pin makes repository-emitter breakages fail naming the plane.
//!
//! Template shape: `mox_plane_compile_tests` (issue #517). Materialization
//! uses a fixed root (the #550 convention) so a failed run's tree can be
//! inspected; `cargo check` shares a cached target dir across runs.
//!
//! Teeth (validated à la #550 — break the emitter, the pin must fail):
//! emitting an unresolvable import or a broken fetch method fails
//! `cargo check`; skipping the emission of `repository_impl.rs` fails the
//! presence guards below.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use codegraph_grafeo::GrafeoEngine;

/// Junction/refs fixture (the `mox_plane_compile_tests` TRIO): a
/// refers-array materializes as a junction table, exercising the
/// junction-table branches of the repository emitter.
const TRIO_MOX: &str = r#"
package recruiting

enum ShelfKind {
    Fiction as "fiction" = 0
    Reference as "reference" = 1
}

class ShelfType {
    String label
}

class ReviewType {
    String comment
    ShelfKind kind
    refers ShelfType shelf
    refers ShelfType[] shelves

    op string labelOf(ShelfType s) {
        expr { comment }
    }
}
"#;

/// The ddd golden pipeline's library config (the `ddd_pipeline_tests`
/// recipe): design-driven operations, no entity config.
const LIBRARY_DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.library]
label = "Library"
schema_dir = "library"
postgres_schema = "library"
"#;

/// Fixed materialization root (the #550 convention): survives a failed run
/// so the generated tree can be inspected.
fn tree_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("repository-pin-tree-{label}"))
}

/// Build cache shared across runs (and warmed by CI's rust-cache): a
/// subdir of the workspace `target/` so it never collides with the
/// workspace build's lock.
fn pin_target_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("REPOSITORY_PIN_TARGET_DIR") {
        return PathBuf::from(dir);
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/repository-pin")
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

/// Ingest fixture schemas (+ optional mox and `.ddd` design), then run the
/// default-plan generation into `root/app` with domain types beside it.
/// Returns `(app_dir, domain_types_dir)`.
async fn generate_tree(
    label: &str,
    config_toml: Option<&str>,
    ingest_schemas: bool,
    mox: Option<&Path>,
    ddd_fixture: Option<&str>,
) -> (PathBuf, PathBuf) {
    let root = tree_root(label);
    let _ = fs::remove_dir_all(&root);
    let app_dir = root.join("app");
    let domain_types_dir = root.join("domain-types");
    fs::create_dir_all(&app_dir).unwrap();

    let config_path = root.join("domains.toml");
    match config_toml {
        Some(toml) => write_file(&config_path, toml),
        None => copy_file(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/domains.toml"),
            &config_path,
        ),
    }
    let config = codegraph_config::config::parse_domain_config(&config_path).unwrap();

    let engine = GrafeoEngine::in_memory().unwrap();
    if ingest_schemas {
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
    }

    if let Some(mox_path) = mox {
        codegraph::ingest::mox_ingest::ingest_mox_files(
            &engine,
            &engine,
            &[mox_path.to_path_buf()],
            &config,
            &config.defaults.type_suffix,
        )
        .await
        .unwrap();
    }

    if let Some(ddd) = ddd_fixture {
        let ddd_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/ddd")
            .join(ddd);
        codegraph::ingest::ddd_ingest::ingest_ddd_files(&engine, &engine, &[ddd_path], &config)
            .await
            .unwrap();
    }

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
            app_name: "repo-pin".into(),
            domain_types_crate: "domain_types".into(),
            generator_name: "repository-compile-pin".into(),
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
            hooks_base: None,
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

    (app_dir, domain_types_dir)
}

/// The repository-plane files of one entity module: the trait, the SeaORM
/// impl, and every DTO shim the impl/trait reference (included DTOs when
/// include paths resolved).
const MODULE_FILES: &[&str] = &[
    "repository.rs",
    "repository_impl.rs",
    "dto_response.rs",
    "dto_create.rs",
    "dto_update.rs",
    "dto_included.rs",
];

/// Assemble the scratch cargo workspace: the emitted domain-types crate
/// (whole — its manifest path-deps codegraph-type-contracts) plus an app
/// crate carrying ONLY the repository plane (`src/entity/` for the SeaORM
/// models, the per-module repository/DTO files, `src/api/links.rs` and
/// `src/codelist/` for the enum column surface). Returns the number of
/// entity modules whose `repository_impl.rs` was assembled.
fn assemble_workspace(app_dir: &Path, domain_types_dir: &Path, ws: &Path) -> usize {
    // domain-types: the emitted crate, whole.
    copy_dir_recursive(domain_types_dir, &ws.join("domain-types"));

    // entity models: the SeaORM layer the repository impl drives.
    let entity_src = app_dir.join("src").join("entity");
    assert!(
        entity_src.exists(),
        "the generated tree must carry src/entity/: {:?}",
        app_dir.join("src")
    );
    copy_dir_recursive(&entity_src, &ws.join("app/src/entity"));

    // app-side codelist (enum column surface), when emitted.
    let codelist_src = app_dir.join("src").join("codelist");
    let has_codelist = codelist_src.exists();
    if has_codelist {
        copy_dir_recursive(&codelist_src, &ws.join("app/src/codelist"));
    }

    // api::links + api::meta — the DTO shim surface.
    let links = app_dir.join("src/api/links.rs");
    assert!(
        links.exists(),
        "the generated tree must carry src/api/links.rs"
    );
    copy_file(&links, &ws.join("app/src/api/links.rs"));
    let meta = app_dir.join("src/api/meta.rs");
    assert!(
        meta.exists(),
        "the generated tree must carry src/api/meta.rs"
    );
    copy_file(&meta, &ws.join("app/src/api/meta.rs"));

    // Per-entity module files, walked dynamically so the pin never
    // hardcodes fixture module names. Modules nest under `pub mod domain`
    // — the emitted imports use `crate::domain::...`.
    let mut impls = 0usize;
    let mut lib = String::from("pub mod api {\n    pub mod links;\n    pub mod meta;\n}\n");
    if has_codelist {
        lib.push_str("pub mod codelist;\n");
    }
    lib.push_str("pub mod entity;\npub mod domain {\n");
    let domain_root = app_dir.join("src").join("domain");
    for domain_entry in fs::read_dir(&domain_root).unwrap() {
        let domain_entry = domain_entry.unwrap();
        if !domain_entry.path().is_dir() {
            continue;
        }
        let domain = domain_entry.file_name().to_string_lossy().to_string();
        let mut domain_mod = format!("pub mod {domain} {{\n");
        let mut has_any = false;
        for module_entry in fs::read_dir(domain_entry.path()).unwrap() {
            let module_entry = module_entry.unwrap();
            if !module_entry.path().is_dir() {
                continue;
            }
            let module = module_entry.file_name().to_string_lossy().to_string();
            let mut module_mod = format!("    pub mod {module} {{\n");
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
                        "        pub mod {};\n",
                        file.strip_suffix(".rs").unwrap()
                    ));
                    copied += 1;
                }
            }
            if copied > 0 {
                module_mod.push_str("    }\n");
                domain_mod.push_str(&module_mod);
                has_any = true;
                if module_entry.path().join("repository_impl.rs").exists() {
                    impls += 1;
                }
            }
        }
        domain_mod.push_str("}\n");
        if has_any {
            lib.push_str(&domain_mod);
        }
    }
    lib.push_str("}\n");
    assert!(
        impls > 0,
        "the generated tree must carry at least one repository_impl.rs"
    );
    write_file(&ws.join("app/src/lib.rs"), &lib);

    write_file(
        &ws.join("app/Cargo.toml"),
        r#"[package]
name = "repository-pin-app"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
name = "repository_pin_app"
path = "src/lib.rs"

[dependencies]
async-trait = "0.1"
chrono = { version = "0.4", features = ["serde"] }
domain-types = { path = "../domain-types" }
rust_decimal = { version = "1", features = ["serde-with-str"] }
sea-orm = { version = "2", features = ["sqlx-postgres", "runtime-tokio-rustls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
utoipa = { version = "6", features = ["chrono", "uuid", "decimal"] }
uuid = { version = "1", features = ["v4", "serde"] }
"#,
    );

    impls
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
        "the repository plane must compile in isolation:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// Mandate (issue #551): the SeaORM repository impl compiles in isolation
/// over the JSON-schema fixture — structured fields (child tables),
/// config-declared children, and include paths included.
#[tokio::test]
async fn seaorm_repository_impl_compiles_in_isolation() {
    let (app_dir, domain_types_dir) = generate_tree("base", None, true, None, None).await;
    let ws = tree_root("base").join("ws");
    let impls = assemble_workspace(&app_dir, &domain_types_dir, &ws);
    assert!(
        impls >= 2,
        "the fixtures must emit at least two repository impls, got {impls}"
    );
    cargo_check(&ws);
}

/// Junction variant: a refers-array (junction table) exercises the
/// junction branches of the emitter.
#[tokio::test]
async fn seaorm_repository_impl_with_junction_compiles() {
    // The mox fixture lives OUTSIDE the materialization root —
    // `generate_tree` wipes that root at the start of every run.
    let mox_root = std::env::temp_dir().join("repository-pin-trio");
    write_file(&mox_root.join("model.mox"), TRIO_MOX);
    let (app_dir, domain_types_dir) = generate_tree(
        "junction",
        None,
        true,
        Some(&mox_root.join("model.mox")),
        None,
    )
    .await;

    let impl_path = app_dir.join("src/domain/recruiting/review/repository_impl.rs");
    assert!(
        impl_path.exists(),
        "the junction fixture must emit the review repository impl:\n{:?}",
        app_dir.join("src/domain")
    );
    let junction = fs::read_to_string(&impl_path).unwrap();
    assert!(
        junction.contains("shelves"),
        "the junction branch must appear in the emitted impl:\n{junction}"
    );

    let ws = tree_root("junction").join("ws");
    assemble_workspace(&app_dir, &domain_types_dir, &ws);
    cargo_check(&ws);
}

/// Design-finders variant (issue #449): the `.ddd` design's finders emit
/// trait methods + SeaORM impl blocks (EXISTS subqueries, `eq_arg`
/// binding) — compile them too.
#[tokio::test]
async fn design_finder_emission_compiles() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ddd");
    let (app_dir, domain_types_dir) = generate_tree(
        "finders",
        Some(LIBRARY_DOMAINS_TOML),
        false,
        Some(&fixture_dir.join("library.mox")),
        Some("library.ddd"),
    )
    .await;

    let mut impls = Vec::new();
    let domain_root = app_dir.join("src/domain/library");
    for entry in fs::read_dir(&domain_root).unwrap() {
        let entry = entry.unwrap();
        if entry.path().is_dir() {
            let candidate = entry.path().join("repository_impl.rs");
            if candidate.exists() {
                impls.push(candidate);
            }
        }
    }
    assert!(
        !impls.is_empty(),
        "the design fixture must emit a repository impl under src/domain/library/"
    );
    let impl_src = fs::read_to_string(&impls[0]).unwrap();
    assert!(
        impl_src.contains("fn find_by_"),
        "the design finders must lower into the SeaORM impl:\n{impl_src}"
    );

    let ws = tree_root("finders").join("ws");
    assemble_workspace(&app_dir, &domain_types_dir, &ws);
    cargo_check(&ws);
}
