//! Golden-conformance gate for the rexlang DDD design artifact (issue #449
//! Phase 4): the committed `library.ddd` — verbatim from the rexlang
//! conformance suite at the pinned rev — compiles with zero error
//! diagnostics and serializes byte-identically to the committed
//! `library.ddd.json` wire artifact. This is the in-process equivalent of
//! the rexlang CLI gates (`rexlang check` + `rexlang artifact check`) —
//! node-free and CI-safe. Also pins the hard-error contract for a design
//! violating the aggregate boundary: `Error::DddModel` with rexlang's
//! wording, and NOTHING ingested.

use std::path::PathBuf;

use codegraph_core::mock::MockEngine;
use codegraph_core::traits::GraphQuerier as _;
use rex_ir::ddd::DddModel;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ddd")
        .join(name)
}

const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.library]
label = "Library"
schema_dir = "library"
postgres_schema = "library"
"#;

/// The golden gate: `library.ddd` compiles cleanly (the fixture's own
/// `import "library.mox"` resolves beside it) and the produced
/// [`DddModel`] byte-equals the committed `library.ddd.json`, which then
/// round-trips back through [`DddModel::from_json`] to an equal model.
///
/// Byte-equality is direct: rex's `to_json_pretty` (serde_json pretty,
/// 2-space) emits NO trailing newline and the committed artifact has none
/// — no normalization applied.
#[test]
fn library_ddd_compiles_clean_and_byte_matches_the_committed_artifact() {
    let compiled = codegraph::ingest::ddd_ingest::read_and_compile_ddd(&fixture("library.ddd"))
        .expect("library.ddd reads and compiles");

    let errors: Vec<String> = compiled
        .compilation
        .diagnostics
        .iter()
        .filter(|(_, d)| d.is_error())
        .map(|(file, d)| format!("{file}: {}", d.message))
        .collect();
    assert!(
        errors.is_empty(),
        "golden fixture must compile with zero error diagnostics: {errors:?}"
    );

    let model = compiled
        .compilation
        .model
        .as_ref()
        .expect("a clean compile produces the design artifact");

    let committed = std::fs::read_to_string(fixture("library.ddd.json")).unwrap();
    assert_eq!(
        model.to_json_pretty().unwrap(),
        committed,
        "serialized artifact must byte-equal the committed library.ddd.json"
    );

    let round_tripped = DddModel::from_json(&committed).expect("artifact deserializes");
    assert_eq!(
        &round_tripped, model,
        "from_json(to_json_pretty(model)) must round-trip equal"
    );
}

/// The aggregate-boundary violation: `invalid.ddd` designs `Chapter` (a
/// non-root — it sits inside Book inside Library) with a repository. The
/// ingest hard-errors with [`codegraph::error::Error::DddModel`], the
/// reason carries rexlang's aggregate-boundary wording, and NOTHING was
/// ingested (`get_ddd_models` empty — a broken design never half-ingests).
#[tokio::test]
async fn repository_on_non_root_aggregate_hard_errors_and_ingests_nothing() {
    let domain_config: codegraph_config::config::DomainConfig =
        codegraph_config::config::parse_domain_config_str(DOMAINS_TOML).unwrap();

    let mock = MockEngine::new();
    let err = codegraph::ingest::ddd_ingest::ingest_ddd_files(
        &mock,
        &mock,
        &[fixture("invalid.ddd")],
        &domain_config,
    )
    .await
    .unwrap_err();

    match &err {
        codegraph::error::Error::DddModel { file, reason } => {
            assert!(file.ends_with("invalid.ddd"), "{file}");
            assert!(
                reason.contains("entity 'Chapter' is contained by 'Library'"),
                "the rendered diagnostic names the containment: {reason}"
            );
            assert!(
                reason.contains("only aggregate roots"),
                "the rendered diagnostic carries the aggregate-boundary wording: {reason}"
            );
        }
        other => panic!("expected Error::DddModel, got {other:?}"),
    }

    let models = mock.get_ddd_models().await.unwrap();
    assert!(
        models.is_empty(),
        "a broken design never half-ingests: {} models landed",
        models.len()
    );
}
