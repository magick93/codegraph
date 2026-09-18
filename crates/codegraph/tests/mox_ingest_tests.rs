//! mox domain ingest tests (issue #218).
//!
//! Ingest layer: a `.mox` domain source (vocabulary with facets + entries, a
//! class operation with an `expr` body, derived features) is compiled
//! in-process via `rex_driver::compile_files` and lands in the graph as
//! Vocabulary/Operation/DerivedFeature nodes with BelongsToClass /
//! VocabularyInPackage edges. Class attachment matches by NAME against
//! schema-ingested entities; mismatches warn and count as skipped — they
//! never fail ingestion.
//!
//! The consuming-generator layer (DTO read-only/exclusion) lives in
//! `mox_dto_tests.rs`; full-tree byte-identity with no mox ingested is
//! pinned by `policy_rls_tests::flag_off_full_output_hashes`, whose
//! snapshot was captured from a pre-mox master.

use std::fs;
use std::path::{Path, PathBuf};

use codegraph_core::traits::GraphQuerier;
use codegraph_grafeo::GrafeoEngine;

const VOCAB_SNAPSHOT: &str = r#"{
  "vocabulary": "iso:4217",
  "version": "2024-01-01",
  "entries": [
    { "alpha3": "USD", "symbol": "$", "minorUnits": 2 },
    { "alpha3": "EUR", "symbol": "€", "minorUnits": 2 }
  ]
}"#;

/// The canonical ingest fixture: a vocabulary with typed facets and vendored
/// entries, a class with a stored features, an operation with an `expr` body,
/// and a derived feature.
const CANONICAL_MOX: &str = r#"
package recruiting

vocabulary Currency from "iso:4217" {
    version "2024-01-01"
    key alpha3
    facet int minorUnits
    facet String symbol
}

class CandidateType {
    String candidateId
    String status

    derived String upperId {
        expr { candidateId }
    }

    op int idLength(String prefix) {
        expr { 1 }
    }
}
"#;

/// Same, plus a class that matches no ingested schema entity — its derived
/// feature must still ingest but must warn and count as skipped.
const CANONICAL_MOX_WITH_GHOST: &str = r#"
package recruiting

vocabulary Currency from "iso:4217" {
    version "2024-01-01"
    key alpha3
    facet int minorUnits
    facet String symbol
}

class CandidateType {
    String candidateId
    String status

    derived String upperId {
        expr { candidateId }
    }

    op int idLength(String prefix) {
        expr { 1 }
    }
}

class GhostType {
    String shadow

    derived String boo {
        expr { shadow }
    }
}
"#;

/// Broken mox source: syntax error, no model lowered.
const BROKEN_MOX: &str = r#"
package recruiting

class Broken {
    String
}
"#;

/// Writes a `.mox` file plus its vendored vocabulary snapshot (the snapshot
/// must exist on disk next to the model or vocabulary lowering errors).
pub(crate) fn write_mox_fixture(dir: &Path, name: &str, source: &str) -> PathBuf {
    let vocab_dir = dir.join("vocab");
    fs::create_dir_all(&vocab_dir).unwrap();
    fs::write(vocab_dir.join("iso-4217@2024-01-01.json"), VOCAB_SNAPSHOT).unwrap();
    let path = dir.join(name);
    fs::write(&path, source).unwrap();
    path
}

/// Ingest the fixture JSON schemas (schema entities must exist for class
/// name matching).
pub(crate) async fn ingest_fixture_schemas(engine: &GrafeoEngine) {
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
        engine,
        Path::new("tests/fixtures/schemas"),
        &classifier,
        &entity_names,
        &codegraph_config::UiOverrideConfig::default(),
        &config.defaults.type_suffix,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn mox_vocabulary_ingests_with_facets_and_entries() {
    let engine = GrafeoEngine::in_memory().unwrap();
    ingest_fixture_schemas(&engine).await;

    let dir = tempfile::tempdir().unwrap();
    let mox = write_mox_fixture(dir.path(), "model.mox", CANONICAL_MOX);

    let stats = codegraph::ingest::mox_ingest::ingest_mox_files(&engine, &engine, &[mox])
        .await
        .unwrap();

    assert_eq!(stats.vocabularies, 1);
    assert_eq!(stats.operations, 1);
    assert_eq!(stats.derived_features, 1);
    assert_eq!(stats.skipped, 0);

    let vocabs = engine.get_mox_vocabularies().await.unwrap();
    assert_eq!(vocabs.len(), 1);
    let vocab = &vocabs[0];
    assert_eq!(vocab.name, "Currency");
    assert_eq!(vocab.package, "recruiting");
    assert_eq!(vocab.source, "iso:4217");
    assert_eq!(vocab.version.as_deref(), Some("2024-01-01"));
    assert_eq!(vocab.key_facet, "alpha3");
    assert_eq!(
        vocab.facets,
        vec![
            codegraph_core::types::MoxFacet {
                name: "minorUnits".into(),
                type_: "int".into(),
            },
            codegraph_core::types::MoxFacet {
                name: "symbol".into(),
                type_: "string".into(),
            },
        ]
    );
    let keys: Vec<&str> = vocab.entries.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys, vec!["USD", "EUR"]);
    assert_eq!(vocab.entries[0].facets["minorUnits"], serde_json::json!(2));
    assert_eq!(vocab.entries[0].facets["symbol"], serde_json::json!("$"));
}

#[tokio::test]
async fn mox_operations_and_derived_features_ingest_with_class_links() {
    let engine = GrafeoEngine::in_memory().unwrap();
    ingest_fixture_schemas(&engine).await;

    let dir = tempfile::tempdir().unwrap();
    let mox = write_mox_fixture(dir.path(), "model.mox", CANONICAL_MOX);

    let stats = codegraph::ingest::mox_ingest::ingest_mox_files(&engine, &engine, &[mox])
        .await
        .unwrap();
    assert_eq!(stats.skipped, 0);

    // Operation: name, class, return type, params, and the verbatim body text.
    let ops = engine.get_mox_operations().await.unwrap();
    assert_eq!(ops.len(), 1);
    let op = &ops[0];
    assert_eq!(op.name, "idLength");
    assert_eq!(op.class, "CandidateType");
    assert_eq!(op.package, "recruiting");
    assert_eq!(op.return_type, "int");
    assert_eq!(op.params.len(), 1);
    assert_eq!(op.params[0].name, "prefix");
    assert_eq!(op.params[0].type_, "string");
    assert_eq!(op.bodies.get("expr").map(String::as_str), Some(" 1 "));

    // Derived feature: the "expr" body text is carried verbatim.
    let derived = engine.get_mox_derived_features().await.unwrap();
    assert_eq!(derived.len(), 1);
    let d = &derived[0];
    assert_eq!(d.name, "upperId");
    assert_eq!(d.class, "CandidateType");
    assert_eq!(d.package, "recruiting");
    assert_eq!(d.type_ref, "string");
    assert_eq!(d.expr.as_deref(), Some(" candidateId "));

    // BelongsToClass edge: the derived feature is linked to the
    // schema-ingested CandidateType entity.
    let linked = engine
        .get_mox_derived_features_for_schema("CandidateType")
        .await
        .unwrap();
    assert_eq!(linked.len(), 1);
    assert_eq!(linked[0].name, "upperId");
}

#[tokio::test]
async fn mox_class_mismatch_warns_counts_skipped_and_never_fails() {
    let engine = GrafeoEngine::in_memory().unwrap();
    ingest_fixture_schemas(&engine).await;

    let dir = tempfile::tempdir().unwrap();
    let mox = write_mox_fixture(dir.path(), "model.mox", CANONICAL_MOX_WITH_GHOST);

    let stats = codegraph::ingest::mox_ingest::ingest_mox_files(&engine, &engine, &[mox])
        .await
        .unwrap();

    // GhostType has one derived feature and matches no schema entity: warned
    // and counted as skipped, but the ingest itself succeeds and the feature
    // is still persisted (keyed by class name) for later runs.
    assert_eq!(stats.skipped, 1);
    assert_eq!(stats.vocabularies, 1);
    assert_eq!(stats.operations, 1);
    assert_eq!(stats.derived_features, 2);

    let derived = engine.get_mox_derived_features().await.unwrap();
    assert_eq!(derived.len(), 2);
    let linked = engine
        .get_mox_derived_features_for_schema("CandidateType")
        .await
        .unwrap();
    assert_eq!(linked.len(), 1);
    assert_eq!(linked[0].name, "upperId");
}

#[tokio::test]
async fn mox_compile_failure_warns_and_skips_without_ingesting() {
    let engine = GrafeoEngine::in_memory().unwrap();
    ingest_fixture_schemas(&engine).await;

    let dir = tempfile::tempdir().unwrap();
    let mox = write_mox_fixture(dir.path(), "broken.mox", BROKEN_MOX);

    let stats = codegraph::ingest::mox_ingest::ingest_mox_files(&engine, &engine, &[mox])
        .await
        .unwrap();

    assert_eq!(stats.skipped, 1);
    assert_eq!(stats.vocabularies, 0);
    assert_eq!(stats.operations, 0);
    assert_eq!(stats.derived_features, 0);
    assert!(engine.get_mox_derived_features().await.unwrap().is_empty());
    assert!(engine.get_mox_operations().await.unwrap().is_empty());
}
