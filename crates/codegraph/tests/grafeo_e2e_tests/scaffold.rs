//! Scaffold + codelist-surface tests: `DomainTypesScaffoldGenerator` output
//! (lib.rs structured re-exports, Cargo.toml) and the Rust codelist enum files
//! that form the generated domain-types crate's public surface (stragglers
//! housed here, nearest cohesive).

use std::path::Path;

use codegraph::generate::codelist::rust_enum::RustCodelistGenerator;
use codegraph::generate::domain_types::scaffold::DomainTypesScaffoldGenerator;
use codegraph::generate::template_engine::create_tera;
use codegraph::generate::traits::GlobalGenerator;
use codegraph::generate::ProjectConfig;

use crate::setup::setup_grafeo;
#[tokio::test]
async fn grafeo_scaffold_lib_rs_includes_structured_re_exports() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let tmp = std::env::temp_dir().join("grafeo-test-scaffold-re-exports");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let gen = DomainTypesScaffoldGenerator::new_with_base(tmp.clone());
    let order = codegraph::generate::compute_generation_order(&engine, &config)
        .await
        .unwrap();
    let files = gen
        .generate(&engine, &config, &order, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("lib.rs"))
        .expect("should produce lib.rs");
    let content = &lib_rs.content;

    // Should re-export IdentifierType since CandidateType uses it
    assert!(
        content.contains("pub use codegraph_type_contracts::IdentifierType;"),
        "lib.rs should re-export IdentifierType"
    );
    assert!(
        content.contains("// --- STRUCTURED WRAPPER RE-EXPORTS ---"),
        "lib.rs should have structured wrapper section"
    );
}

#[tokio::test]
async fn grafeo_scaffold_domain_types_has_cargo_toml() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let tmp = std::env::temp_dir().join("grafeo-test-scaffold-cargo-toml");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let gen = DomainTypesScaffoldGenerator::new_with_base(tmp.clone());
    let order = codegraph::generate::compute_generation_order(&engine, &config)
        .await
        .unwrap();
    let files = gen
        .generate(&engine, &config, &order, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let has_cargo_toml = files
        .iter()
        .any(|f| f.path.to_string_lossy().ends_with("Cargo.toml"));
    assert!(
        has_cargo_toml,
        "DomainTypesScaffoldGenerator should produce a Cargo.toml"
    );

    let cargo_toml = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("Cargo.toml"))
        .unwrap();
    assert!(
        cargo_toml.content.contains("[package]"),
        "Cargo.toml should have [package] section"
    );
    assert!(
        cargo_toml.content.contains("name = \"domain-types\""),
        "Should use correct package name"
    );
}

#[tokio::test]
async fn grafeo_rust_codelist_generator_emits_enum() {
    let (engine, _config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let gen = RustCodelistGenerator::new(Path::new("/tmp/out"));
    let files = gen
        .generate_all(&engine, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    assert!(
        !files.is_empty(),
        "RustCodelistGenerator should produce files"
    );

    // Should have a mod.rs
    let mod_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("mod.rs"));
    assert!(mod_file.is_some(), "should generate codelist/mod.rs");
    let mod_content = &mod_file.unwrap().content;
    assert!(
        mod_content.contains("pub mod currency_code_list;"),
        "mod.rs should declare currency_code_list module"
    );
    assert!(
        mod_content.contains("pub use currency_code_list::CurrencyCodeList;"),
        "mod.rs should re-export CurrencyCodeList"
    );

    // Should have a currency_code_list.rs
    let currency_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("currency_code_list.rs"));
    assert!(
        currency_file.is_some(),
        "should generate currency_code_list.rs"
    );
    let content = &currency_file.unwrap().content;

    // Verify derives
    assert!(
        content.contains(
            "#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]"
        ),
        "enum should have correct derives"
    );
    // Verify enum name
    assert!(
        content.contains("pub enum CurrencyCodeList"),
        "should contain CurrencyCodeList enum"
    );
    // Verify serde rename on variants (USD is all-caps, PascalCase would be Usd)
    assert!(
        content.contains("#[serde(rename = \"USD\")]"),
        "USD variant should have serde rename"
    );
    assert!(
        content.contains("Usd"),
        "USD should be sanitized to Usd variant"
    );
    // Verify Display impl
    assert!(
        content.contains("impl std::fmt::Display for CurrencyCodeList"),
        "should implement Display"
    );
    assert!(
        content.contains("write!(f, \"USD\")"),
        "Display impl should write original code"
    );
}

#[tokio::test]
async fn grafeo_gender_codelist_variants_no_rename_when_pascal() {
    let (engine, _config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let gen = RustCodelistGenerator::new(Path::new("/tmp/out"));
    let files = gen
        .generate_all(&engine, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let gender_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("gender_code_list.rs"))
        .expect("should generate gender_code_list.rs");
    let content = &gender_file.content;

    // "Male" is already valid PascalCase — should NOT have serde rename
    assert!(
        content.contains("    Male,"),
        "Male variant should exist without rename"
    );
    // "NotSpecified" is already PascalCase — should NOT have serde rename
    assert!(
        content.contains("    NotSpecified,"),
        "NotSpecified variant should exist"
    );
}
