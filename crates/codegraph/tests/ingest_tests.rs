use codegraph::ingest::schema_loader::SchemaLoader;
use std::path::Path;

#[test]
fn loads_fixture_schemas() {
    let loader = SchemaLoader::load(Path::new("tests/fixtures/schemas")).unwrap();
    // Should find at least 9 top-level JSON files + inline defs
    assert!(
        loader.schema_count() >= 9,
        "Expected at least 9 schemas, got {}",
        loader.schema_count()
    );
}

#[test]
fn resolves_relative_ref() {
    let loader = SchemaLoader::load(Path::new("tests/fixtures/schemas")).unwrap();
    let (_uri, entry) = loader
        .resolve_ref(
            "../../common/json/PersonBaseType.json",
            "recruiting/json/CandidateType.json",
        )
        .unwrap();
    assert!(entry
        .schema
        .get("title")
        .unwrap()
        .as_str()
        .unwrap()
        .contains("PersonBase"));
}

#[test]
fn resolves_inline_def_ref() {
    let loader = SchemaLoader::load(Path::new("tests/fixtures/schemas")).unwrap();
    let (_uri, entry) = loader
        .resolve_ref(
            "#/definitions/QualificationType",
            "recruiting/json/CandidateType.json",
        )
        .unwrap();
    assert_eq!(entry.stem, "QualificationType");
}

#[test]
fn iterates_top_level_schemas() {
    let loader = SchemaLoader::load(Path::new("tests/fixtures/schemas")).unwrap();
    let top_level: Vec<_> = loader.iter_top_level().collect();
    // Should have 9 top-level schemas
    assert!(
        top_level.len() >= 9,
        "Expected at least 9 top-level schemas, got {}",
        top_level.len()
    );
}

#[test]
fn resolves_codelist_ref() {
    let loader = SchemaLoader::load(Path::new("tests/fixtures/schemas")).unwrap();
    let (_uri, entry) = loader
        .resolve_ref(
            "../../common/json/codelist/GenderCodeList.json",
            "recruiting/json/CandidateType.json",
        )
        .unwrap();
    assert_eq!(entry.stem, "GenderCodeList");
}
