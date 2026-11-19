//! Design invariants lower to typed predicate fns (DD5, issue #666
//! slice 2).
//!
//! Node-free PR-CI pins over the production ingest front doors
//! (`ingest_mox_files` + `ingest_ddd_files` — the same path `driver::run`
//! drives):
//!
//! - the domain `rules.rs` plane carries one
//!   `pub fn {class_snake}_invariant_{index}(input: &{Entity}Response)
//!   -> bool` per invariant, with the declaration-order index documented
//!   as the `invariantIndex` contract (slice 4's 409 detail);
//! - a mox+ddd model with NO invariants emits no `rules.rs` at all (the
//!   pre-#666 shape — the flag-off byte-identity twin);
//! - an invariant outside the lowering subset is a HARD `ValidationPass`
//!   error (check `invariant`) naming the domain, the class, the index,
//!   and the construct — and emits NOTHING (the `query_refusal.ddd`
//!   posture: valid pherolang, refused at the codegraph lowering).

use std::fs;
use std::path::{Path, PathBuf};

use codegraph_core::traits::GraphQuerier;
use tempfile::TempDir;

const GRANTS_MOX: &str = r#"
package grants

class GrantApplication {
    String applicant
    int amount
}
"#;

/// One lowerable invariant: a numeric comparison over an own-record
/// scalar feature. Grammar order: the `invariant` clause precedes the
/// repository block.
const GRANTS_DDD: &str = r#"
import "grants.mox";

application Grants {
    module grants {
        entity GrantApplication invariant amount > 0; repository GrantApplicationRepository {
            findById;
            findAll;
            save;
            delete;
        }
    }
}
"#;

/// The invariant-less twin: identical model, no `invariant` clause — the
/// byte-identity arm.
const GRANTS_DDD_PLAIN: &str = r#"
import "grants.mox";

application Grants {
    module grants {
        entity GrantApplication repository GrantApplicationRepository {
            findById;
            findAll;
            save;
            delete;
        }
    }
}
"#;

/// `int + int` is VALID pherolang — typed, boolean-resulting — but outside
/// codegraph's own-record lowering subset. The invariant must REFUSE with
/// a named hard error, never emit a guessed predicate.
const GRANTS_DDD_REFUSED: &str = r#"
import "grants.mox";

application Grants {
    module grants {
        entity GrantApplication invariant amount + 1 > 0; repository GrantApplicationRepository {
            findById;
            findAll;
            save;
            delete;
        }
    }
}
"#;

const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.grants]
label = "Grants"
schema_dir = "grants"
postgres_schema = "grants"
"#;

struct Fixture {
    mox: PathBuf,
    ddd: PathBuf,
    config: PathBuf,
    _dir: TempDir,
}

fn write_fixture(ddd_source: &str) -> Fixture {
    let dir = TempDir::new().unwrap();
    let mox = dir.path().join("grants.mox");
    fs::write(&mox, GRANTS_MOX).unwrap();
    let ddd = dir.path().join("grants.ddd");
    fs::write(&ddd, ddd_source).unwrap();
    let config = dir.path().join("domains.toml");
    fs::write(&config, DOMAINS_TOML).unwrap();
    Fixture {
        mox,
        ddd,
        config,
        _dir: dir,
    }
}

/// The production ingest front doors: the mox bridge, then the ddd
/// compile. An upstream compile failure is a hard error here — codegraph
/// refuses only its OWN lowering subset, never valid pherolang.
async fn ingest(fixture: &Fixture) -> codegraph_core::mock::MockEngine {
    let engine = codegraph_core::mock::MockEngine::builder().build();
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let stats = codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        std::slice::from_ref(&fixture.mox),
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap()
    .stats;
    assert_eq!(stats.skipped, 0, "the mox class must bridge: {stats}");
    codegraph::ingest::ddd_ingest::ingest_ddd_files(
        &engine,
        &engine,
        std::slice::from_ref(&fixture.ddd),
        &config,
    )
    .await
    .expect("the design must compile (invalid pherolang is an upstream compile error, never a codegraph refusal)");
    engine
}

/// The `rules` domain generator over the fixture's single entity, in
/// generation order.
async fn render_rules(
    engine: &codegraph_core::mock::MockEngine,
    fixture: &Fixture,
) -> Vec<codegraph_generate::traits::GeneratedFile> {
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let tera = codegraph_generate::template_engine::create_tera(Path::new("")).unwrap();
    let project = codegraph_generate::ProjectConfig::default();
    let generator =
        codegraph_generate::ddd::rules::RulesGenerator::new(Path::new("/tmp/ddd-invariant-tests"));
    codegraph_generate::traits::DomainGenerator::generate(
        &generator,
        engine,
        "grants",
        &["GrantApplication".to_string()],
        &config,
        &tera,
        &project,
    )
    .await
    .expect("rules generation")
}

#[tokio::test]
async fn invariant_predicate_fns_emit_into_rules_rs() {
    let fixture = write_fixture(GRANTS_DDD);
    let engine = ingest(&fixture).await;

    // The authored facts survive ingest: one invariant, declaration order.
    // Pure mox-first: the schema title is the class name verbatim.
    let models = engine.get_ddd_models().await.unwrap();
    let design = models
        .iter()
        .flat_map(|m| &m.designs)
        .find(|d| d.resolved_title.as_deref() == Some("GrantApplication"))
        .expect("the design resolves");
    assert_eq!(design.invariants.len(), 1);
    assert_eq!(design.invariants[0].expression, "amount > 0");

    let files = render_rules(&engine, &fixture).await;
    assert_eq!(files.len(), 1, "rules.rs emits: {:?}", files.len());
    assert!(
        files[0].path.ends_with("src/domain/grants/rules.rs"),
        "{:?}",
        files[0].path
    );
    let rules = &files[0].content;

    // The typed predicate fn: real record type (the #338 shape), `input`
    // receiver, boolean return.
    assert!(
        rules.contains(
            "pub fn grant_application_invariant_0(input: &GrantApplicationResponse) -> bool {"
        ),
        "the predicate fn signature:\n{rules}"
    );
    assert!(
        rules.contains("    (input.amount > 0)"),
        "the lowered body over `input`:\n{rules}"
    );
    assert!(
        rules.contains(
            "use crate::domain::grants::grant_application::dto_response::GrantApplicationResponse;"
        ),
        "the record-type import:\n{rules}"
    );
    // The index contract: declaration order IS the invariantIndex (slice
    // 4's 409 detail).
    assert!(
        rules.contains("/// Invariant 0 of `GrantApplication` (issue #666)."),
        "the doc header:\n{rules}"
    );
    assert!(
        rules.contains("`invariantIndex`"),
        "the index contract must be documented:\n{rules}"
    );
    assert!(
        rules.contains("/// Authored expression: `amount > 0`"),
        "the verbatim expression rides the doc:\n{rules}"
    );
    assert!(
        rules.contains("// ── Design invariants (DD5, issue #666) ───────────"),
        "the gated section header:\n{rules}"
    );
}

#[tokio::test]
async fn no_invariants_emits_no_rules_file() {
    let fixture = write_fixture(GRANTS_DDD_PLAIN);
    let engine = ingest(&fixture).await;
    let files = render_rules(&engine, &fixture).await;
    assert!(
        files.is_empty(),
        "no rules and no invariants ⇒ no rules.rs (the pre-#666 shape): {:?}",
        files.iter().map(|f| &f.path).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn unlowerable_invariant_fails_validation_and_emits_nothing() {
    let fixture = write_fixture(GRANTS_DDD_REFUSED);
    let engine = ingest(&fixture).await;
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();

    // The pre-generation pass: HARD error severity, check `invariant`,
    // naming the domain, the class, the index, and the construct.
    let issues = codegraph::validate::ValidationPass::run(&engine, &config).await;
    let refusal = issues
        .iter()
        .find(|i| {
            matches!(i.severity, codegraph::validate::Severity::Error) && i.check == "invariant"
        })
        .unwrap_or_else(|| panic!("expected a hard invariant refusal, got: {issues:?}"));
    assert!(
        refusal
            .message
            .contains("invariant 0 of `GrantApplication` in domain `grants`"),
        "the refusal names the domain, class and index: {refusal:?}"
    );
    assert!(
        refusal.message.contains("arithmetic (`+`)"),
        "the refusal names the construct: {refusal:?}"
    );
    assert!(
        refusal.message.contains("not emitted"),
        "the refusal states the fail-closed contract: {refusal:?}"
    );

    // Fail-closed emission: the refused invariant produces NO predicate
    // fn — and with no rules either, the generator emits nothing.
    let files = render_rules(&engine, &fixture).await;
    assert!(
        files.is_empty(),
        "a refused invariant emits nothing: {:?}",
        files.iter().map(|f| &f.path).collect::<Vec<_>>()
    );
}
