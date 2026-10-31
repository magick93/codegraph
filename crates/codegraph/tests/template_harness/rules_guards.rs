//! Eligibility-rule handler guards (issue #338).
//!
//! A modeled `eligibility rule` must gate the entity's API surface at the
//! handler site: load-or-404, evaluate the compiled boolean over the
//! record, deny with 403. Guards ride the `rules` domain generator's plan
//! membership — flag-off (no rules generator, or no eligibility rule in
//! the graph) keeps the handler byte-identical.
//!
//! [issue #338] B2 slice: the emitted `rules.rs` functions become real
//! endpoint guards instead of doc-comment-only wiring.

use crate::harness::{candidate_properties, candidate_schema, test_project_config, test_tera};
use codegraph::generate;
use codegraph::generate::traits::EntityGenerator;
use codegraph_config::DomainConfig;
use codegraph_core::mock::MockEngine;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{RuleKind, RuleNode};

/// `eligibility rule EligibleCandidate from CandidateType: familyName exists`
/// — the body transpiles to `input.family_name.is_some()` (bool).
fn eligibility_rule() -> RuleNode {
    RuleNode {
        name: "EligibleCandidate".to_string(),
        domain: Some("recruiting".to_string()),
        definition: Some("candidates with a family name may proceed".to_string()),
        kind: RuleKind::Eligibility,
        input_type: Some("CandidateType".to_string()),
        expr_json: r#"{"kind":"Exists","modifier":"none","argument":{"kind":"SymbolReference","symbol":"familyName","explicit":false,"args":[]}}"#.to_string(),
        properties: serde_json::json!({ "origin": "rosetta" }),
    }
}

/// `reporting rule NamedCandidate from CandidateType: familyName exists` —
/// reporting rules are computed fields, never endpoint guards.
fn reporting_rule() -> RuleNode {
    RuleNode {
        name: "NamedCandidate".to_string(),
        domain: Some("recruiting".to_string()),
        definition: None,
        kind: RuleKind::Reporting,
        input_type: Some("CandidateType".to_string()),
        expr_json: r#"{"kind":"Exists","modifier":"none","argument":{"kind":"SymbolReference","symbol":"familyName","explicit":false,"args":[]}}"#.to_string(),
        properties: serde_json::json!({ "origin": "rosetta" }),
    }
}

async fn guard_mock(rules: Vec<RuleNode>) -> MockEngine {
    let mock = MockEngine::builder()
        .with_schema(candidate_schema())
        .with_properties("CandidateType", candidate_properties())
        .build();
    for rule in rules {
        mock.ingest_rule(&rule).await.unwrap();
    }
    mock
}

/// All five operations resolved for CandidateType — every mutating + read
/// handler carries the guard.
fn guard_domain_config() -> DomainConfig {
    codegraph_config::config::parse_domain_config(std::path::Path::new(
        "tests/fixtures/rules_guards_domains.toml",
    ))
    .unwrap()
}

async fn render_handler(mock: &MockEngine, rules_guards: bool) -> String {
    generate::type_registry::register_framework_types();
    let config = guard_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-rules-guards");

    let mut generator = generate::api::handler::HandlerGenerator::new(&output_dir);
    if rules_guards {
        generator = generator.with_rules_guards(true);
    }
    let files = generator
        .generate(
            mock,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    files[0].content.clone()
}

const GUARD_CALL_RECORD: &str = "crate::domain::recruiting::rules::eligible_candidate";

// === Guards on (the issue #338 surface) ===

#[tokio::test]
async fn update_handler_loads_evaluates_and_denies_403() {
    let mock = guard_mock(vec![eligibility_rule()]).await;
    let out = render_handler(&mock, true).await;

    // Load-or-404 before the operation: the root update handler had no
    // load before guards — the guard plane adds the find_by_id + 404.
    assert!(
        out.contains(&format!("if !{GUARD_CALL_RECORD}(&existing) {{")),
        "update handler must evaluate the rule over the loaded record:\n{out}"
    );
    assert!(
        out.contains("Eligibility rule 'EligibleCandidate'"),
        "the 403 denial names the rule:\n{out}"
    );
    assert!(out.contains("AppError::forbidden"), "{out}");
}

#[tokio::test]
async fn delete_handler_loads_evaluates_and_denies_403() {
    let mock = guard_mock(vec![eligibility_rule()]).await;
    let out = render_handler(&mock, true).await;

    assert!(
        out.contains(&format!("if !{GUARD_CALL_RECORD}(&existing) {{")),
        "delete handler must evaluate the rule over the loaded record:\n{out}"
    );
    // Two occurrences: update AND delete both load into `existing`.
    assert!(
        out.matches(&format!("if !{GUARD_CALL_RECORD}(&existing) {{"))
            .count()
            >= 2,
        "update and delete both carry the guard:\n{out}"
    );
}

#[tokio::test]
async fn get_by_id_evaluates_and_denies_403() {
    let mock = guard_mock(vec![eligibility_rule()]).await;
    let out = render_handler(&mock, true).await;

    assert!(
        out.contains(&format!("if !{GUARD_CALL_RECORD}(&response) {{")),
        "get_by_id must evaluate the rule over the loaded record:\n{out}"
    );
    assert!(out.contains("AppError::forbidden"), "{out}");
}

#[tokio::test]
async fn create_evaluates_created_record_compensates_and_denies_403() {
    let mock = guard_mock(vec![eligibility_rule()]).await;
    let out = render_handler(&mock, true).await;

    // The create guard evaluates over the created record (loaded for the
    // response), compensates the ineligible write, and denies with 403.
    assert!(
        out.contains(&format!("if !{GUARD_CALL_RECORD}(&response) {{")),
        "create handler must evaluate the rule over the created record:\n{out}"
    );
    assert!(
        out.contains("recruiting_candidate_commands.delete(id"),
        "an ineligible create is compensated (row removed):\n{out}"
    );
    assert!(out.contains("AppError::forbidden"), "{out}");
}

#[tokio::test]
async fn guarded_handlers_document_the_403_contract() {
    let mock = guard_mock(vec![eligibility_rule()]).await;
    let out = render_handler(&mock, true).await;

    assert!(
        out.matches("(status = 403, description = \"Denied by an eligibility rule\")")
            .count()
            >= 4,
        "create/get/update/delete utoipa responses document the denial:\n{out}"
    );
}

// === Flag-off / presence gating (byte-identity) ===

#[tokio::test]
async fn rules_guards_off_keeps_handler_free_of_guard_surface() {
    // Rule in the graph, but the plan does not carry the rules generator —
    // the flag the registry derives from BuildPlan.
    let mock = guard_mock(vec![eligibility_rule()]).await;
    let out = render_handler(&mock, false).await;

    for needle in [
        GUARD_CALL_RECORD,
        "Eligibility rule",
        "Denied by an eligibility rule",
    ] {
        assert!(
            !out.contains(needle),
            "flag-off handler must not contain {needle:?}:\n{out}"
        );
    }
}

#[tokio::test]
async fn reporting_rules_never_become_guards() {
    let mock = guard_mock(vec![reporting_rule()]).await;
    let out = render_handler(&mock, true).await;

    assert!(
        !out.contains(GUARD_CALL_RECORD),
        "reporting rules are computed fields, not endpoint guards:\n{out}"
    );
}

#[tokio::test]
async fn ineligible_bodies_are_not_rejected_by_validation_only_by_the_guard() {
    // The 403 path must be reachable with a garde-valid body (an optional
    // field's absence), so the guard — not 422 — is what blocks the caller.
    // Pins the fixture shape: family_name is optional, the rule demands it.
    let props = candidate_properties();
    let family = props
        .iter()
        .find(|p| p.rust_field_name == "family_name")
        .unwrap();
    assert!(family.is_nullable, "fixture: family_name is optional");
}
