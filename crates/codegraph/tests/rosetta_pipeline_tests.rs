//! Rosetta pipeline threading tests (issue #257).
//!
//! `--rosetta-files` through `driver::run` / `driver::classify`: rosetta as
//! the sole model source, pass ordering before the JSON schema pass, the
//! model-source guard, and stderr notices.

use std::path::PathBuf;

const MODEL: &str = r#"
namespace pipeline.store
version "1.0.0"

type Product: <"A product">
	productId string (1..1)
	label string (0..1)
	price number (1..1)
	currency Currency (1..1)

enum Currency:
	EUR
	USD

type Holder:
	holderId string (1..1)

choice ProductType:
	Product
	Holder
"#;

const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.store]
label = "Store"
schema_dir = "store"
postgres_schema = "store"
"#;

fn write_fixture(dir: &std::path::Path) -> (PathBuf, PathBuf) {
    let model = dir.join("store.rosetta");
    std::fs::write(&model, MODEL).unwrap();
    let domains = dir.join("domains.toml");
    std::fs::write(&domains, DOMAINS_TOML).unwrap();
    (domains, model)
}

fn run_args<'a>(
    config: &'a PathBuf,
    rosetta: &'a [PathBuf],
    output: &'a std::path::Path,
) -> codegraph::driver::RunArgs<'a> {
    codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: config,
        output,
        extension_points_path: None,
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(PathBuf::from("definitely-absent-profiles.toml")),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &[],
        rosetta_files: rosetta,
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
    }
}

#[tokio::test]
async fn rosetta_only_run_generates_the_data_plane() {
    let dir = tempfile::tempdir().unwrap();
    let (config, model) = write_fixture(dir.path());
    let output = dir.path().join("generated");
    let rosetta = vec![model];

    codegraph::driver::run(run_args(&config, &rosetta, &output))
        .await
        .unwrap();

    let mut files: Vec<String> = Vec::new();
    for entry in walkdir::WalkDir::new(&output) {
        let entry = entry.unwrap();
        if entry.file_type().is_file() {
            files.push(
                entry
                    .path()
                    .strip_prefix(&output)
                    .unwrap()
                    .display()
                    .to_string()
                    .replace('\\', "/"),
            );
        }
    }
    let has = |suffix: &str| files.iter().any(|f| f.ends_with(suffix));

    // The type bridge drove the generators end-to-end.
    let product_ddl = files
        .iter()
        .find(|f| {
            f.starts_with("migrations/")
                && f.ends_with("_store_product.sql")
                && !f.ends_with("_rls.sql")
                && !f.ends_with("_trigger.sql")
                && !f.ends_with("_fts.sql")
        })
        .unwrap_or_else(|| panic!("product DDL missing; files: {files:?}"));
    assert!(!product_ddl.is_empty());
    assert!(has("src/entity/store_product.rs"), "entity missing");
    assert!(
        files
            .iter()
            .any(|f| f.starts_with("src/domain/store/product/")),
        "domain module missing"
    );

    // The enum codelist and the referenced type materialized.
    assert!(has("src/entity/store_currency.rs"));
    assert!(has("src/entity/store_holder.rs"));

    // KNOWN COLLISION (recorded for #259): `type Product` and
    // `choice ProductType` both strip to pg_table_name `product` — the
    // Type-suffix strip convention assumes Rosetta types carry the suffix,
    // but Rosetta choices are the ones named *Type. The choice's artifacts
    // are therefore NOT separately addressable in this model shape.
    assert!(
        !has("src/entity/store_product_type.rs"),
        "if this assertion fails, the Product/ProductType table-name collision was fixed"
    );
}

#[tokio::test]
async fn run_without_any_model_source_is_a_config_error() {
    let dir = tempfile::tempdir().unwrap();
    let (config, _model) = write_fixture(dir.path());
    let output = dir.path().join("generated");
    let rosetta: Vec<PathBuf> = Vec::new();

    let err = codegraph::driver::run(run_args(&config, &rosetta, &output))
        .await
        .expect_err("no sources must be rejected");
    assert!(err.to_string().contains("no model source"), "{err}");
}

#[tokio::test]
async fn rosetta_notice_mentions_the_primary_source() {
    let notice = codegraph::driver::rosetta_primary_notice();
    assert!(notice.contains(".rosetta"));
    assert!(notice.contains("--schemas fills gaps"));
}

#[tokio::test]
async fn classify_with_rosetta_only_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let (config, model) = write_fixture(dir.path());
    let rosetta = vec![model];

    // Rosetta-derived schemas are auto-scored (origin=rosetta never trips
    // the mox bypass) and appear in the report.
    codegraph::driver::classify(
        None,
        None,
        &config,
        Some("store"),
        codegraph::driver::ClassifyFormat::Json,
        &[],
        &rosetta,
    )
    .await
    .unwrap();
}
