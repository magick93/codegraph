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
//! presence guards below. The media variant's teeth (issue #719): removing
//! the `if child.is_media { continue; }` skip from the emitter's child
//! INSERT path re-binds the media child against the request DTO that omits
//! it — the pin's presence guard fails first naming the defect ("the
//! repository must never write the media child"), and the emitted binding
//! (`if let Some(ref item) = cmd.photo` on a `CreateAssetRequest` with no
//! `photo` member) is the E0609 the compile layer rejects if the guard text
//! is bypassed. Reverted. The DELETE guard's teeth (teeth run #2): narrowing
//! that skip to the INSERT half only — `crud.rs`'s replace loop then emits
//! the media child's delete-and-replace DELETE with no re-insert — fails the
//! DELETE guard first (the INSERT skip in `child.rs` keeps
//! `INSERT INTO assets.asset_photo` structurally unemittable), naming the
//! same #707 TRAP the shipped skip exists to prevent. Reverted. The media
//! leg's mox fixture root is pid-suffixed so concurrent checkouts building
//! the pin cannot cross-contaminate the fixture input.

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

/// Mixed-shape fixture (issue #459): an unformatted wraps-String datatype
/// (→ TEXT/String; the ingest WARN "declares no format — mapping to TEXT"
/// is expected) living BESIDE a scalar `refers` edge (uuid FK column) and a
/// `refers[]` junction, over the TRIO's recruiting package. The `id`
/// feature stays a separate uuid-formatted field so the sole-identity
/// override (#522) never touches the datatype field. The repository plane
/// for this shape must `cargo check` — the main row→Response hydrator and
/// the DTO type source agree on the FK's uuid even when TEXT-mapped
/// datatype fields surround it.
const MIXED_MOX: &str = r#"
package staff

/// A uuid-formatted surrogate identity.
type UUID wraps String {
    format "uuid"
}

/// A durable badge identifier (no format — maps to TEXT/String).
type Identifier wraps String

/// A duty roster.
class DutyType {
    UUID id
    String label
}

/// A crew member.
class CrewType {
    UUID id
    Identifier badge
    refers DutyType [0..1] duty
    refers DutyType[] duties
}
"#;

/// Config for the mixed fixture's `staff` package (exact domain-key match),
/// sharing the gate run with the TRIO's `recruiting` package.
const MIXED_DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"

[domains.staff]
label = "Staff"
schema_dir = "staff"
postgres_schema = "staff"
"#;

/// Media-plane fixture (issue #719, the #707 shape): one entity with ONE
/// contained media reference. Through-app is deliberate — it is the only
/// upload mode whose route OWNS the reference row
/// (`media_upload_owned_fields` returns the field only there), so the
/// request DTO pair must omit the member and the repository's child
/// insert/replace loops must skip the media child. `media_classes` defaults
/// to `["MediaReference"]`, so no explicit declaration is needed.
const MEDIA_MOX: &str = r#"
package assets

class MediaReference {
    String url
    String mimeType
}

class Asset {
    String name
    contains MediaReference[0..1] photo
}
"#;

/// Config for the media fixture's `assets` package. `r2` + `through-app`
/// (the `media_runtime` fixture's provider/mode pair — a combination the
/// capability matrix admits) with the default `{org}/…` key template.
const MEDIA_DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.assets]
label = "Assets"
schema_dir = "assets"
postgres_schema = "assets"

[domains.assets.media]
provider = "r2"
upload_mode = "through-app"
bucket = "repository-pin-media"
max_bytes = 1024
accept = ["image/png"]

[domains.assets.media.r2]
binding = "MEDIA_BUCKET"
"#;

/// Dot-include fixture (issue #789): the worker→deployment→position shape —
/// a scalar `refers` chain two hops deep (the explicit `[0..1]`
/// multiplicity keeps each hop a scalar FK column, not a junction). The
/// source's `allow_include` declares the dot path so the repository plane
/// emits the combined intermediate+leaf fetch (`emit_dot_fetch_method`),
/// the plane whose intermediate addressing the fix re-points through the
/// source's FK.
const DOT_MOX: &str = r#"
package staff

class WorkerType {
    String name
    refers DeploymentType [0..1] deployment
}

class DeploymentType {
    String reason
    refers PositionType [0..1] position
}

class PositionType {
    String title
}
"#;

/// Config for the dot-include fixture's `staff` package: the explicit
/// `allow_include` dot path on the source entity (the dot_notation mock's
/// shape — explicit paths are the documented way to opt into dot includes).
const DOT_DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.staff]
label = "Staff"
schema_dir = "staff"
postgres_schema = "staff"

[domains.staff.entity_config.WorkerType]
allow_include = ["deployment.position"]
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
    mox: Option<&[std::path::PathBuf]>,
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

    if let Some(mox_paths) = mox {
        codegraph::ingest::mox_ingest::ingest_mox_files(
            &engine,
            &engine,
            mox_paths,
            &config,
            &config.defaults.type_suffix,
        )
        .await
        .unwrap();
    }

    if let Some(ddd) = ddd_fixture {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        // A bare name resolves under `tests/fixtures/ddd/` (the golden
        // fixture shape); a path with a directory component is taken
        // verbatim (the dto-boundaries projection fixture lives in its own
        // directory).
        let ddd_path = if ddd.contains('/') {
            base.join(ddd)
        } else {
            base.join("ddd").join(ddd)
        };
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
            integration_manifests: None,
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
        Some(&[mox_root.join("model.mox")]),
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

/// Mixed-shape variant (issue #459): the TRIO's recruiting plane AND the
/// unformatted-datatype `staff` plane (TEXT datatype field beside a scalar
/// refers FK and a refers[] junction) materialize together and the
/// assembled workspace `cargo check`s green — no E0308 in the mixed shape.
#[tokio::test]
async fn mixed_datatype_repository_plane_compiles() {
    let trio_root = std::env::temp_dir().join("repository-pin-mixed-trio");
    write_file(&trio_root.join("model.mox"), TRIO_MOX);
    let mixed_root = std::env::temp_dir().join("repository-pin-mixed");
    write_file(&mixed_root.join("model.mox"), MIXED_MOX);
    let (app_dir, domain_types_dir) = generate_tree(
        "mixed",
        Some(MIXED_DOMAINS_TOML),
        false,
        Some(&[trio_root.join("model.mox"), mixed_root.join("model.mox")]),
        None,
    )
    .await;

    let impl_path = app_dir.join("src/domain/staff/crew/repository_impl.rs");
    assert!(
        impl_path.exists(),
        "the mixed fixture must emit the crew repository impl:\n{:?}",
        app_dir.join("src/domain")
    );
    let impl_src = fs::read_to_string(&impl_path).unwrap();
    // The scalar refers FK hydrates as a uuid column …
    assert!(
        impl_src.contains("duty_id: row.duty_id,"),
        "the scalar refers FK must keep the plain uuid assignment:\n{impl_src}"
    );
    // … while the junction branch stays present beside it.
    assert!(
        impl_src.contains("duties"),
        "the junction branch must appear in the emitted impl:\n{impl_src}"
    );

    let ws = tree_root("mixed").join("ws");
    let impls = assemble_workspace(&app_dir, &domain_types_dir, &ws);
    assert!(
        impls >= 4,
        "both fixtures must emit their repository impls, got {impls}"
    );
    cargo_check(&ws);
}

/// Design-finders variant (issue #449): the `.ddd` design's finders emit
/// trait methods + SeaORM impl blocks (EXISTS subqueries, `eq_arg`
/// binding) — compile them too. Since DD9 (issue #659) the same fixture
/// carries `query`-carrying operations, so the parameterized query plane
/// (Condition builder, ORDER BY, page window) is under the same pin.
#[tokio::test]
async fn design_finder_emission_compiles() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ddd");
    let (app_dir, domain_types_dir) = generate_tree(
        "finders",
        Some(LIBRARY_DOMAINS_TOML),
        false,
        Some(&[fixture_dir.join("library.mox")]),
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

    // DD9: the library design's query operations emit onto the same impl —
    // a parameterized Condition, the sort keys, and the bounded page.
    let movie_impl =
        fs::read_to_string(app_dir.join("src/domain/library/movie/repository_impl.rs"))
            .expect("movie's repository impl");
    assert!(
        movie_impl.contains("async fn recent_by_genre("),
        "the declared query lowers onto the impl:\n{movie_impl}"
    );
    assert!(
        movie_impl.contains("let page_size = limit.or(Some(20u64))"),
        "the declared page becomes the bounded window:\n{movie_impl}"
    );
    assert!(
        movie_impl.contains("order_by_asc(crate::entity::library_movie::Column::Id)"),
        "the identity tiebreaker is emitted:\n{movie_impl}"
    );
    assert!(
        movie_impl.contains("async fn find_by_title("),
        "the equality finder path is retained for query-less ops:\n{movie_impl}"
    );

    let ws = tree_root("finders").join("ws");
    assemble_workspace(&app_dir, &domain_types_dir, &ws);
    cargo_check(&ws);
}

/// Media-plane variant (issue #719, the #707 shape): a `[media]` through-app
/// domain whose entity carries a contained media reference. The repository
/// plane must compile WITH the #707 exclusions in place — the media child
/// struct never exists on the request DTO pair (the through-app upload
/// handler is the reference's only writer), the response DTO keeps the
/// member, and the repository's child insert/replace loops skip the media
/// child table (`assets.asset_photo`) while the read path still hydrates it.
///
/// Native only, deliberately: the repository plane is SeaORM and does not
/// link wasm32 (the cornucopia/wasm coverage rides
/// `worker_wasm_compile_tests`), and the route module the upload handler
/// writes through is `media_compile_tests`' pin — this pin covers the
/// planes #707 actually rewrote: the DTO pair and the repository impl.
#[tokio::test]
async fn media_repository_plane_compiles() {
    // The mox fixture lives OUTSIDE the materialization root —
    // `generate_tree` wipes that root at the start of every run. The root is
    // pid-suffixed (unlike the pre-existing legs' shared roots): two
    // checkouts building this pin concurrently must not cross-contaminate
    // the fixture input, which `write_file` overwrites in place.
    let mox_root =
        std::env::temp_dir().join(format!("repository-pin-media-{}", std::process::id()));
    write_file(&mox_root.join("model.mox"), MEDIA_MOX);
    let (app_dir, domain_types_dir) = generate_tree(
        "media",
        Some(MEDIA_DOMAINS_TOML),
        false,
        Some(&[mox_root.join("model.mox")]),
        None,
    )
    .await;

    // ── Presence guards (fail BEFORE cargo check — template drift must be
    //    named, not buried in a compile error). ──
    let impl_path = app_dir.join("src/domain/assets/asset/repository_impl.rs");
    assert!(
        impl_path.exists(),
        "the media fixture must emit the asset repository impl:\n{:?}",
        app_dir.join("src/domain")
    );
    let impl_src = fs::read_to_string(&impl_path).unwrap();
    // Issue #707: the media child is WRITE-excluded — no child INSERT and
    // no delete-and-replace may reference the media child table.
    assert!(
        !impl_src.contains("INSERT INTO assets.asset_photo"),
        "the repository must never write the media child (the upload route \
         owns the row):\n{impl_src}"
    );
    assert!(
        !impl_src.contains("DELETE FROM assets.asset_photo"),
        "the update path must not delete-and-replace the media child:\n{impl_src}"
    );
    // … but READ-included: the fetch still hydrates the reference.
    assert!(
        impl_src.contains("asset_photo"),
        "the read path must keep hydrating the media child:\n{impl_src}"
    );

    // The DTO pair lives in the domain-types crate (single source of truth;
    // the app-side files are re-export shims). Request DTOs omit the
    // reference entirely; the response DTO keeps the member + struct.
    let dt_root = domain_types_dir.join("src").join("assets").join("asset");
    let create =
        fs::read_to_string(dt_root.join("dto_create.rs")).expect("domain-types dto_create");
    let update =
        fs::read_to_string(dt_root.join("dto_update.rs")).expect("domain-types dto_update");
    let response =
        fs::read_to_string(dt_root.join("dto_response.rs")).expect("domain-types dto_response");
    for (name, src) in [("create", &create), ("update", &update)] {
        assert!(
            !src.contains("AssetMediaReference"),
            "the {name} request DTO must omit the media reference (neither the \
             member nor the child struct may exist):\n{src}"
        );
    }
    assert!(
        response.contains("pub photo: Option<AssetMediaReferenceResponse>"),
        "the response DTO must keep the media reference member:\n{response}"
    );

    let ws = tree_root("media").join("ws");
    let impls = assemble_workspace(&app_dir, &domain_types_dir, &ws);
    assert!(
        impls >= 1,
        "the media fixture must emit at least one repository impl, got {impls}"
    );
    cargo_check(&ws);
}

/// Projection-finder variant (issue #768): a declared finder whose return
/// slot is a projection boundary emits trait + impl methods returning the
/// read model, constructed through the boundary's `from_row`. The scratch
/// workspace carries the whole domain-types crate (the boundary module
/// lives there) and the app response module re-exports the type — the pin
/// proves the whole chain compiles on BOTH the single and `Vec` shapes.
#[tokio::test]
async fn projection_finder_emission_compiles() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dto_boundaries");
    let (app_dir, _domain_types) = generate_tree(
        "projection-finders",
        Some(LIBRARY_DOMAINS_TOML),
        false,
        Some(&[fixture.join("dto_boundaries.mox")]),
        Some("dto_boundaries/dto_boundaries.ddd"),
    )
    .await;

    let book_impl = fs::read_to_string(app_dir.join("src/domain/library/book/repository_impl.rs"))
        .expect("book's repository impl");
    assert!(
        book_impl.contains("-> Result<Option<BookSummary>,"),
        "the single projection finder must return the read model:\n{book_impl}"
    );
    assert!(
        book_impl.contains("-> Result<Vec<BookSummary>,"),
        "the many projection finder must return the read model:\n{book_impl}"
    );
    assert!(
        book_impl.contains("BookSummary::from_row"),
        "the impl must construct through the boundary's from_row:\n{book_impl}"
    );
    let book_response = fs::read_to_string(app_dir.join("src/domain/library/book/dto_response.rs"))
        .expect("book's app response module");
    assert!(
        book_response.contains("pub use domain_types::dto::catalog::BookSummary;"),
        "the response module must re-export the projection type:\n{book_response}"
    );

    let ws = tree_root("projection-finders").join("ws");
    assemble_workspace(&app_dir, &_domain_types, &ws);
    cargo_check(&ws);
}

/// Dot-include variant (issue #789): the worker→deployment→position shape
/// emits the combined intermediate+leaf fetch, whose INTERMEDIATE must be
/// addressed through the SOURCE's FK (load the source row by PK, read its
/// FK, load the intermediate by that value) — never by the source's PK,
/// which answered empty on every dot include. The presence guards name the
/// addressing shape BEFORE cargo check (template drift must be named, not
/// buried in a compile error); the scratch crate then proves the whole
/// plane — trait, impl, DTO shims, combined response — compiles.
///
/// Native only, like every leg here: the repository plane is SeaORM.
#[tokio::test]
async fn dot_include_fetch_compiles() {
    // The mox fixture lives OUTSIDE the materialization root —
    // `generate_tree` wipes that root at the start of every run — and is
    // pid-suffixed so concurrent checkouts building the pin cannot
    // cross-contaminate the fixture input (the media leg's precedent).
    let mox_root = std::env::temp_dir().join(format!("repository-pin-dot-{}", std::process::id()));
    write_file(&mox_root.join("model.mox"), DOT_MOX);
    let (app_dir, domain_types_dir) = generate_tree(
        "dot-include",
        Some(DOT_DOMAINS_TOML),
        false,
        Some(&[mox_root.join("model.mox")]),
        None,
    )
    .await;

    // ── Presence guards (fail BEFORE cargo check) ──
    let impl_path = app_dir.join("src/domain/staff/worker/repository_impl.rs");
    assert!(
        impl_path.exists(),
        "the dot fixture must emit the worker repository impl:\n{:?}",
        app_dir.join("src/domain")
    );
    let impl_src = fs::read_to_string(&impl_path).unwrap();
    assert!(
        impl_src.contains("async fn fetch_deployment_position_for_worker("),
        "the dot fetch method must be emitted for the allowed path:\n{impl_src}"
    );
    // The #789 fix: the SOURCE row loads by its own PK first.
    assert!(
        impl_src.contains("let source = crate::entity::staff_worker::Entity::find()"),
        "the dot fetch must load the source row by its own PK:\n{impl_src}"
    );
    // …read the source's FK to the intermediate…
    assert!(
        impl_src.contains("source.deployment_id"),
        "the source's FK to the intermediate must be read:\n{impl_src}"
    );
    // …and address the intermediate by that FK value.
    assert!(
        impl_src.contains("crate::entity::staff_deployment::Column::Id.eq(fk_value)"),
        "the intermediate must load by the source's FK value:\n{impl_src}"
    );
    assert!(
        !impl_src.contains("staff_deployment::Column::Id.eq(source_id)"),
        "the intermediate must not be addressed by the source's PK (the \
         #789 defect — every dot include answered empty):\n{impl_src}"
    );

    let ws = tree_root("dot-include").join("ws");
    let impls = assemble_workspace(&app_dir, &domain_types_dir, &ws);
    assert!(
        impls >= 1,
        "the dot fixture must emit at least one repository impl, got {impls}"
    );
    cargo_check(&ws);
}
