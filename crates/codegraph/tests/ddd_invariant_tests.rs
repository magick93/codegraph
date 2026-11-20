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

#[path = "service_ops_fixture.rs"]
mod fixture;

use codegraph_generate::traits::{DomainGenerator, EntityGenerator};

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

// ── Slice 4d: handler enforcement blocks (issue #666) ──────────────────

/// Render one entity's handler over the invariant fixture, with or
/// without the `rules` plan membership (the flag the registry derives).
async fn render_invariant_handler(with_rules: bool, title: &str) -> String {
    let fx = fixture::write_invariant_fixture();
    let engine = fixture::ingest_invariant(&fx).await;
    let config = codegraph_config::config::parse_domain_config(&fx.config).unwrap();
    let tera = codegraph_generate::template_engine::create_tera(Path::new("")).unwrap();
    let project = codegraph_generate::ProjectConfig::default();
    codegraph::generate::type_registry::register_framework_types();
    let generator = codegraph_generate::api::handler::HandlerGenerator::new(Path::new(
        "/tmp/ddd-invariant-handler",
    ))
    .with_rules_guards(with_rules);
    let files = generator
        .generate(&engine, title, "payroll", &config, &tera, &project)
        .await
        .expect("handler generation");
    assert_eq!(files.len(), 1, "{:?}", files.len());
    files[0].content.clone()
}

#[tokio::test]
async fn create_handler_evaluates_the_created_record_compensates_and_conflicts_409() {
    let out = render_invariant_handler(true, "PayRunType").await;

    // The created record is evaluated through the slice-2 predicate.
    assert!(
        out.contains("if !crate::domain::payroll::rules::pay_run_type_invariant_0(&response) {"),
        "the create block evaluates the CREATED record:\n{out}"
    );
    // The violation is compensated (the auditable soft-delete shape)
    // before the 409.
    assert!(
        out.contains("design invariant violated on create — compensating"),
        "{out}"
    );
    assert!(out.contains("payroll_pay_run_commands.delete(id"), "{out}");
    // The 409 detail: INVARIANT_VIOLATION + class + invariantIndex.
    assert!(out.contains("code: \"INVARIANT_VIOLATION\""), "{out}");
    assert!(
        out.contains(
            "crate::error::FieldError { field: \"class\".to_string(), message: \"PayRunType\".to_string() }"
        ),
        "{out}"
    );
    assert!(
        out.contains(
            "crate::error::FieldError { field: \"invariantIndex\".to_string(), message: \"0\".to_string() }"
        ),
        "{out}"
    );
    // utoipa documents the denial on BOTH guarded routes.
    assert_eq!(
        out.matches(
            r#"(status = 409, description = "Denied by a design invariant (INVARIANT_VIOLATION)")"#
        )
        .count(),
        2,
        "create AND update document the 409:\n{out}"
    );
}

#[tokio::test]
async fn root_save_validates_member_invariants_with_the_aggregate_root_detail() {
    let out = render_invariant_handler(true, "PayRunType").await;

    // Member rows load through the scoped fetch the repository exposes.
    assert!(
        out.contains(
            "payroll_pay_run_line_queries.list_filtered(0, 1000, &member_filters_pay_run_line"
        ),
        "the root save loads member rows:\n{out}"
    );
    assert!(
        out.contains("member_filters_pay_run_line.insert(\"pay_run_id\""),
        "the member fetch filters on the child→root FK column:\n{out}"
    );
    // The member predicate evaluates in declaration order and names the
    // member class, its index, and the aggregate root.
    assert!(
        out.contains(
            "if !crate::domain::payroll::rules::pay_run_line_type_invariant_0(&member_row) {"
        ),
        "{out}"
    );
    assert!(
        out.contains(
            "crate::error::FieldError { field: \"aggregateRoot\".to_string(), message: \"PayRunType\".to_string() }"
        ),
        "a member violation names the aggregate root:\n{out}"
    );
    assert!(
        out.contains(
            "crate::error::FieldError { field: \"class\".to_string(), message: \"PayRunLineType\".to_string() }"
        ),
        "a member violation names the member class:\n{out}"
    );
    // Member validation rides BOTH the create and the update blocks.
    assert!(
        out.matches("let (member_rows_pay_run_line, _)").count() == 2,
        "create AND update validate the members:\n{out}"
    );
}

#[tokio::test]
async fn update_handler_builds_the_candidate_before_the_write() {
    let out = render_invariant_handler(true, "PayRunType").await;

    assert!(
        out.contains("let candidate: PayRunResponse = serde_json::from_value(candidate_json)"),
        "the candidate decodes back into the record type:\n{out}"
    );
    assert!(
        out.contains("if !crate::domain::payroll::rules::pay_run_type_invariant_0(&candidate) {"),
        "{out}"
    );
    // Ordering: the candidate is built (and evaluated) BEFORE the write.
    let candidate_pos = out
        .find("let candidate: PayRunResponse")
        .expect("candidate");
    let write_pos = out
        .find("commands.update(id, body")
        .expect("update command");
    assert!(
        candidate_pos < write_pos,
        "the invariant evaluates BEFORE the write: {candidate_pos} < {write_pos}"
    );
    // The root update arm carries its own load-or-404 (no eligibility
    // guards bound `existing` first).
    assert!(
        out.contains("let existing = state.payroll_pay_run_queries.find_by_id(id, "),
        "{out}"
    );
}

#[tokio::test]
async fn transition_arm_evaluates_the_post_patch_candidate_pre_write() {
    let out = render_invariant_handler(true, "PayRunType").await;

    let arm_pos = out
        .find("pub async fn service_op_submit")
        .expect("the transition handler must emit");
    let arm = &out[arm_pos..];
    assert!(
        arm.contains("if !crate::domain::payroll::rules::pay_run_type_invariant_0(&candidate) {"),
        "the transition arm evaluates the candidate:\n{arm}"
    );
    // The machine state patch applies BEFORE the candidate is evaluated.
    let patch_pos = arm
        .find("value[\"pay_run_status\"]")
        .expect("the state patch");
    let cand_pos = arm.find("let mut candidate_json").expect("the candidate");
    assert!(
        patch_pos < cand_pos,
        "the state patch rides the candidate before evaluation"
    );
    // The write sits AFTER the evaluation in the arm.
    let write_pos = arm
        .find("dto_update::UpdatePayRunRequest")
        .expect("the cmd");
    assert!(cand_pos < write_pos, "evaluation precedes the write");
    // The action route documents the invariant denial (the utoipa block
    // precedes the handler fn — assert on the whole output).
    assert!(
        out.contains("The transition is not allowed from the current state, or a design invariant denied the write"),
        "the utoipa 409 names both denial classes:\n{out}"
    );
}

#[tokio::test]
async fn child_update_claims_the_ownership_prefix_for_the_candidate() {
    let out = render_invariant_handler(true, "PayRunLineType").await;

    // The child's existing ownership load gains the `let existing = `
    // binding (no eligibility guards claimed it first).
    assert!(
        out.contains(
            "let existing = state.payroll_pay_run_line_queries.find_by_id_scoped(id, parent_id"
        ),
        "{out}"
    );
    assert!(
        out.contains("let candidate: PayRunLineResponse = serde_json::from_value(candidate_json)"),
        "{out}"
    );
    assert!(
        out.contains(
            "if !crate::domain::payroll::rules::pay_run_line_type_invariant_0(&candidate) {"
        ),
        "{out}"
    );
}

#[tokio::test]
async fn invariant_enforcement_off_keeps_the_handler_free_of_the_surface() {
    let out = render_invariant_handler(false, "PayRunType").await;
    for needle in [
        "INVARIANT_VIOLATION",
        "candidate_json",
        "member_filters_pay_run_line",
        "pay_run_type_invariant_0",
        "Denied by a design invariant",
    ] {
        assert!(
            !out.contains(needle),
            "flag-off handler must not contain {needle:?}"
        );
    }
}

#[tokio::test]
async fn workers_topology_warns_that_invariants_are_not_enforced() {
    let fixture = write_fixture(GRANTS_DDD);
    let engine = ingest(&fixture).await;
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();

    // Under workers: the Warning names the counts and the monolith-only
    // rules plane.
    let issues =
        codegraph::validate::ValidationPass::run_with_topology(&engine, &config, true).await;
    let warn = issues
        .iter()
        .find(|i| {
            i.check == "invariant" && matches!(i.severity, codegraph::validate::Severity::Warning)
        })
        .unwrap_or_else(|| panic!("expected the workers-topology invariant WARN, got: {issues:?}"));
    assert!(
        warn.message
            .contains("not enforced under the workers topology"),
        "{warn:?}"
    );
    assert!(
        warn.message
            .contains("1 design invariant(s) on 1 entity/entities"),
        "the WARN names the counts: {warn:?}"
    );
    assert!(warn.message.contains("monolith-only"), "{warn:?}");

    // Monolith: no Warning-severity invariant finding (the lowering
    // refusals stay Error-severity; this fixture lowers cleanly).
    let issues = codegraph::validate::ValidationPass::run(&engine, &config).await;
    assert!(
        !issues.iter().any(|i| i.check == "invariant"
            && matches!(i.severity, codegraph::validate::Severity::Warning)),
        "monolith runs stay quiet: {issues:?}"
    );
}

// ── Slice 4e: the hurl contract's three-verdict invariant legs ─────────

async fn render_invariant_contracts(
    with_rules: bool,
) -> Vec<codegraph_generate::traits::GeneratedFile> {
    let fx = fixture::write_invariant_fixture();
    let engine = fixture::ingest_invariant(&fx).await;
    let config = codegraph_config::config::parse_domain_config(&fx.config).unwrap();
    let tera = codegraph_generate::template_engine::create_tera(Path::new("")).unwrap();
    let project = codegraph_generate::ProjectConfig::default();
    let generator = codegraph_generate::api::hurl_contract::HurlContractGenerator::new(Path::new(
        "/tmp/ddd-invariant-hurl",
    ))
    .with_rules_guards(with_rules);
    DomainGenerator::generate(
        &generator,
        &engine,
        "payroll",
        &["PayRunType".to_string(), "PayRunLineType".to_string()],
        &config,
        &tera,
        &project,
    )
    .await
    .expect("hurl contract generation")
}

#[tokio::test]
async fn hurl_contract_emits_the_provable_violating_create_leg() {
    let files = render_invariant_contracts(true).await;
    let pay_run = files
        .iter()
        .find(|f| {
            f.path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with("payroll_pay_run.hurl"))
        })
        .expect("the pay_run contract must emit");

    // The create body seeds total = 42; the violating leg flips ONE
    // satisfied leaf to its provably-rejected seed (the literal 0 stepped
    // below: -1) and pins the 409.
    assert!(
        pay_run.content.contains("\"total\": -1"),
        "the violating leg carries the flipped seed:\n{}",
        pay_run.content
    );
    assert!(pay_run.content.contains("HTTP 409"), "{}", pay_run.content);
    assert!(
        pay_run
            .content
            .contains("jsonpath \"$.error.code\" == \"INVARIANT_VIOLATION\""),
        "{}",
        pay_run.content
    );
    assert!(
        pay_run
            .content
            .contains("# Design invariant denial (DD5, issue #666 slice 4e)"),
        "{}",
        pay_run.content
    );
    // The leg posts the contract's OWN create URL (hurl == router).
    assert!(
        pay_run
            .content
            .contains("POST {{base_url}}/api/v1/payroll/pay-run\n"),
        "{}",
        pay_run.content
    );
    // The MAIN create leg stays 201 — the denial leg posts the flipped
    // body, not the contract's fixture row.
    assert!(pay_run.content.contains("HTTP 201"), "{}", pay_run.content);

    // The child contract carries the same leg shape for its invariant.
    let line = files
        .iter()
        .find(|f| {
            f.path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with("payroll_pay_run_line.hurl"))
        })
        .expect("the child contract must emit");
    assert!(
        line.content.contains("\"quantity\": -1"),
        "{}",
        line.content
    );
    assert!(line.content.contains("HTTP 409"), "{}", line.content);
}

#[tokio::test]
async fn hurl_contract_update_roundtrip_pins_409_when_provably_violating() {
    // `total > 100`: the update seed (43 over the create's 42) provably
    // violates, so the PUT roundtrip pins the pre-write 409 and drops the
    // update-landed echo.
    let fx = fixture::write_invariant_fixture();
    let ddd = fixture::INVARIANT_DDD.replace("invariant total > 0;", "invariant total > 100;");
    fs::write(&fx.ddd, ddd).unwrap();
    let engine = fixture::ingest_invariant(&fx).await;
    let config = codegraph_config::config::parse_domain_config(&fx.config).unwrap();
    let tera = codegraph_generate::template_engine::create_tera(Path::new("")).unwrap();
    let project = codegraph_generate::ProjectConfig::default();
    let generator = codegraph_generate::api::hurl_contract::HurlContractGenerator::new(Path::new(
        "/tmp/ddd-invariant-hurl-2",
    ))
    .with_rules_guards(true);
    let files = DomainGenerator::generate(
        &generator,
        &engine,
        "payroll",
        &["PayRunType".to_string()],
        &config,
        &tera,
        &project,
    )
    .await
    .expect("generation");
    let pay_run = files
        .iter()
        .find(|f| {
            f.path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with("payroll_pay_run.hurl"))
        })
        .expect("pay_run contract");
    assert!(
        pay_run
            .content
            .contains("PUT below pins the 409 the handler answers BEFORE the write"),
        "{}",
        pay_run.content
    );
    // The denied roundtrip: PUT 409 + the invariant assert + the read-back
    // that proves the write never started (no update echo asserts).
    assert!(
        pay_run
            .content
            .contains("HTTP 409\n[Asserts]\njsonpath \"$.error.code\" == \"INVARIANT_VIOLATION\""),
        "{}",
        pay_run.content
    );
    assert!(
        pay_run.content.contains("# GET — the write never started"),
        "{}",
        pay_run.content
    );
}

#[tokio::test]
async fn hurl_contract_flag_off_keeps_the_contract_free_of_invariant_legs() {
    let files = render_invariant_contracts(false).await;
    let pay_run = files
        .iter()
        .find(|f| {
            f.path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with("payroll_pay_run.hurl"))
        })
        .expect("pay_run contract");
    for needle in [
        "INVARIANT_VIOLATION",
        "Design invariant denial",
        "Design invariant omission",
        "\"total\": -1",
    ] {
        assert!(
            !pay_run.content.contains(needle),
            "flag-off contract must not contain {needle:?}"
        );
    }
}
