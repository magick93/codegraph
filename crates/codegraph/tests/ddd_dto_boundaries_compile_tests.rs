//! Compile pin for the modeled DTO boundary plane (issue #667, the #551
//! policy): the emitted boundary module must actually compile — serde +
//! utoipa derives over the resolved member types — on native AND
//! `wasm32-unknown-unknown` (the domain-types crate is shared with the
//! worker crates, so the plane must be wasm-clean).
//!
//! The scratch crate carries only the boundary module + the dependency set
//! the domain-types crate declares; no SeaORM, no generated app.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// End-to-end generation over the boundary fixture (mox + ddd).
async fn generate() -> tempfile::TempDir {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dto_boundaries");
    let temp = tempfile::tempdir().expect("tempdir");
    let output = temp.path().join("out");
    let config = temp.path().join("domains.toml");
    fs::write(
        &config,
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.library]
label = "Library"
schema_dir = "schemas/library"
postgres_schema = "library"
"#,
    )
    .unwrap();
    let mox_files = vec![fixture.join("dto_boundaries.mox")];
    let ddd_files = vec![fixture.join("dto_boundaries.ddd")];
    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &config,
        output: &output,
        extension_points_path: None,
        integration_manifests: &[],
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(PathBuf::from("definitely-absent-profiles.toml")),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &mox_files,
        rosetta_files: &[],
        ddd_files: &ddd_files,
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
    .expect("pipeline run");
    temp
}

/// Assemble the emitted boundary module into a scratch crate and
/// `cargo check` it.
fn cargo_check_boundary_plane(domain_types: &Path, target: Option<&str>) {
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path();
    let krate = root.join("boundary_plane");
    fs::create_dir_all(krate.join("src/dto")).unwrap();

    for entry in fs::read_dir(domain_types.join("src/dto")).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), krate.join("src/dto").join(entry.file_name())).unwrap();
    }
    // Issue #768: the projection constructors read the entity Response the
    // entity plane emits into the SAME crate — carry ONLY the
    // `{domain}/{module}/dto_response.rs` files (the create/update/query
    // modules need garde/async_trait the scratch deliberately omits), with
    // the minimal `mod` plumbing so `crate::{domain}::{module}::
    // dto_response::{Entity}Response` resolves exactly as in the real crate.
    let mut lib = String::from("pub mod dto;\n");
    let src_root = domain_types.join("src");
    for domain_entry in fs::read_dir(&src_root).unwrap() {
        let domain_entry = domain_entry.unwrap();
        let domain = domain_entry.file_name().to_string_lossy().to_string();
        if domain == "dto" || !domain_entry.path().is_dir() {
            continue;
        }
        let mut domain_mod = String::new();
        for module_entry in fs::read_dir(domain_entry.path()).unwrap() {
            let module_entry = module_entry.unwrap();
            let module = module_entry.file_name().to_string_lossy().to_string();
            let response = module_entry.path().join("dto_response.rs");
            if !response.is_file() {
                continue;
            }
            fs::create_dir_all(krate.join("src").join(&domain).join(&module)).unwrap();
            fs::copy(
                &response,
                krate
                    .join("src")
                    .join(&domain)
                    .join(&module)
                    .join("dto_response.rs"),
            )
            .unwrap();
            fs::write(
                krate.join("src").join(&domain).join(&module).join("mod.rs"),
                "pub mod dto_response;\n",
            )
            .unwrap();
            domain_mod.push_str(&format!("pub mod {module};\n"));
        }
        if !domain_mod.is_empty() {
            fs::write(krate.join("src").join(&domain).join("mod.rs"), domain_mod).unwrap();
            lib.push_str(&format!("pub mod {domain};\n"));
        }
    }
    fs::write(krate.join("src/lib.rs"), lib).unwrap();
    fs::write(
        krate.join("Cargo.toml"),
        "[package]\nname = \"boundary_plane\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [dependencies]\n\
         serde = { version = \"1\", features = [\"derive\"] }\n\
         serde_json = \"1\"\n\
         uuid = { version = \"1\", features = [\"v4\", \"serde\", \"js\"] }\n\
         chrono = { version = \"0.4\", features = [\"serde\"] }\n\
         rust_decimal = { version = \"1\", features = [\"serde-with-str\"] }\n\
         utoipa = { version = \"6\", features = [\"chrono\", \"uuid\", \"decimal\"] }\n",
    )
    .unwrap();

    let mut cmd = Command::new("cargo");
    cmd.arg("check")
        .arg("--manifest-path")
        .arg(krate.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", root.join("target"))
        .env("CARGO_TERM_COLOR", "never");
    if let Some(t) = target {
        cmd.arg("--target").arg(t);
    }
    let out = cmd.output().expect("cargo check must run");
    assert!(
        out.status.success(),
        "cargo check {} failed:\n{}\n{}",
        target.unwrap_or("(native)"),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

#[tokio::test]
async fn emitted_dto_boundary_plane_compiles_native_and_wasm() {
    let dir = generate().await;
    let domain_types = dir.path().join("out/crates/domain-types");
    let module = fs::read_to_string(domain_types.join("src/dto/catalog.rs")).unwrap();
    assert!(
        module.contains("pub struct NewBookInput") && module.contains("pub struct BookSummary"),
        "the emitted module must carry both boundaries:\n{module}"
    );
    assert!(
        !module.contains("sea_orm"),
        "the boundary plane must stay persistence-agnostic:\n{module}"
    );

    cargo_check_boundary_plane(&domain_types, None);
    cargo_check_boundary_plane(&domain_types, Some("wasm32-unknown-unknown"));
}
