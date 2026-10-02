//! Compile gates: the full generated app (`grafeo_generated_code_compiles`,
//! the cfg(feature = "e2e") `grafeo_generated_tests_pass`) and the generated
//! domain-types crate (`generated_app_compiles_cleanly`) must `cargo check`
//! cleanly.

use std::path::Path;

use codegraph::generate::ProjectConfig;
use codegraph::generate::codelist::rust_enum::RustCodelistGenerator;
use codegraph::generate::domain_types::dto::DomainTypesDtoGenerator;
use codegraph::generate::domain_types::query_service::QueryServiceGenerator;
use codegraph::generate::domain_types::scaffold::DomainTypesScaffoldGenerator;
use codegraph::generate::template_engine::create_tera;
use codegraph::generate::traits::{EntityGenerator, GlobalGenerator};

use crate::setup::{generate_full_app, setup_grafeo};
/// Compile gate for the full generated app, including the emitted
/// `repository_impl.rs` files (written by `generate_full_app` for every
/// entity). Runs in the default `cargo test --workspace` so that changes to
/// the repository emitter cannot merge without the generated code compiling.
#[tokio::test]
async fn grafeo_generated_code_compiles() {
    use std::time::Duration;

    let tmp = tempfile::tempdir().unwrap();
    let output_dir = tmp.path();

    generate_full_app(output_dir).await;

    // Run cargo check with 5-minute timeout
    let result = tokio::time::timeout(
        Duration::from_secs(300),
        tokio::process::Command::new("cargo")
            .args(["check"])
            .current_dir(output_dir)
            .output(),
    )
    .await;

    let output = match result {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => panic!("failed to spawn cargo check: {e}"),
        Err(_) => {
            let preserved = tmp.keep();
            panic!(
                "cargo check timed out after 5 minutes!\n\
                 Temp dir preserved at: {}",
                preserved.display()
            );
        }
    };

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let preserved = tmp.keep();
        panic!(
            "cargo check failed!\n\
             Temp dir preserved at: {}\n\
             --- stdout ---\n{}\n\
             --- stderr ---\n{}",
            preserved.display(),
            stdout,
            stderr
        );
    }
    // On success, tmp drops and cleans up automatically
}

#[cfg(feature = "e2e")]
#[tokio::test]
async fn grafeo_generated_tests_pass() {
    use std::time::Duration;

    let tmp = tempfile::tempdir().unwrap();
    let output_dir = tmp.path();

    generate_full_app(output_dir).await;

    // Run cargo test with 10-minute timeout
    let result = tokio::time::timeout(
        Duration::from_secs(600),
        tokio::process::Command::new("cargo")
            .args(["test"])
            .current_dir(output_dir)
            .output(),
    )
    .await;

    let output = match result {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => panic!("failed to spawn cargo test: {e}"),
        Err(_) => {
            let preserved = tmp.keep();
            panic!(
                "cargo test timed out after 10 minutes!\n\
                 Temp dir preserved at: {}",
                preserved.display()
            );
        }
    };

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let preserved = tmp.keep();
        panic!(
            "cargo test failed!\n\
             Temp dir preserved at: {}\n\
             --- stdout ---\n{}\n\
             --- stderr ---\n{}",
            preserved.display(),
            stdout,
            stderr
        );
    }
    // On success, tmp drops and cleans up automatically
}

// === Compile-gate test: generated domain-types crate compiles ===

#[tokio::test]
async fn generated_app_compiles_cleanly() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    let output_dir = tmp.path().to_path_buf();

    // Compute absolute path to codegraph-type-contracts crate
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let type_contracts_path = workspace_root
        .join("crates")
        .join("codegraph-type-contracts");

    let project_config = ProjectConfig {
        identity: codegraph::generate::IdentityConfig {
            app_name: "test-app".into(),
            domain_types_crate: "domain_types".into(),
            generator_name: "codegraph-test".into(),
            ..Default::default()
        },
        paths: codegraph::generate::PathsConfig {
            type_contracts_base: type_contracts_path.to_string_lossy().to_string(),
            ..Default::default()
        },
        codegen: codegraph::generate::CodegenConfig {
            types_import_prefix: "codegraph_type_contracts".into(),
            ..Default::default()
        },
        ..Default::default()
    };

    let order = codegraph::generate::compute_generation_order(&engine, &config)
        .await
        .unwrap();

    // 1. Generate scaffold: lib.rs, Cargo.toml, domain/entity mod.rs files
    let scaffold_gen = DomainTypesScaffoldGenerator::new_with_base(output_dir.clone());
    let mut all_files = scaffold_gen
        .generate(&engine, &config, &order, &tera, &project_config)
        .await
        .unwrap();

    // 2. Generate DTOs + query services for each entity in generation order
    let dto_gen = DomainTypesDtoGenerator::new_with_base(output_dir.clone());
    let qs_gen = QueryServiceGenerator::new_with_base(output_dir.clone());

    for entry in &order {
        let dto_files = dto_gen
            .generate(
                &engine,
                &entry.schema_title,
                &entry.domain,
                &config,
                &tera,
                &project_config,
            )
            .await
            .unwrap();
        all_files.extend(dto_files);

        let qs_files = qs_gen
            .generate(
                &engine,
                &entry.schema_title,
                &entry.domain,
                &config,
                &tera,
                &project_config,
            )
            .await
            .unwrap();
        all_files.extend(qs_files);
    }

    // 3. Generate codelist enum files
    let codelist_gen = RustCodelistGenerator::new(&output_dir);
    let cl_files = codelist_gen
        .generate_all(&engine, &tera, &project_config)
        .await
        .unwrap();
    all_files.extend(cl_files);

    // Write all generated files to disk
    for file in &all_files {
        if let Some(parent) = file.path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&file.path, &file.content).unwrap();
    }

    // Write stub files for context.rs and query.rs that lib.rs references
    // but which have no dedicated generator.
    let src_dir = output_dir.join("src");
    std::fs::write(
        src_dir.join("context.rs"),
        "pub struct SourceContext;\npub enum SourceOrigin {}\n",
    )
    .unwrap();
    std::fs::write(
        src_dir.join("query.rs"),
        "pub struct ListParams;\npub struct PagedResult<T>(pub Vec<T>);\npub struct QueryError;\npub enum SortOrder {}\n",
    )
    .unwrap();

    // Verify Cargo.toml was generated
    let cargo_toml_path = output_dir.join("Cargo.toml");
    assert!(
        cargo_toml_path.exists(),
        "Cargo.toml should be generated by DomainTypesScaffoldGenerator"
    );

    // Run cargo check with 5-minute timeout
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        tokio::process::Command::new("cargo")
            .args([
                "check",
                "--manifest-path",
                &cargo_toml_path.to_string_lossy(),
            ])
            .output(),
    )
    .await;

    let output = match result {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => panic!("failed to spawn cargo check: {e}"),
        Err(_) => {
            let preserved = tmp.keep();
            panic!(
                "cargo check timed out after 5 minutes!\n\
                 Temp dir preserved at: {}",
                preserved.display()
            );
        }
    };

    if !output.status.success() {
        let preserved = tmp.keep();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "cargo check failed!\n\
             Temp dir preserved at: {}\n\
             --- stdout ---\n{stdout}\n\
             --- stderr ---\n{stderr}",
            preserved.display(),
        );
    }
    // On success, tmp is dropped and directory is cleaned up
}
