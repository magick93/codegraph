//! L3 conformance kit (issue #275): the committed case file pins the graph
//! artifact document format — the canonical serialization is a fixed point,
//! accepted variants normalize onto it, and rejected inputs fail with the
//! named diagnostic.

use codegraph::artifact::kit;

const CASES: &str = include_str!("fixtures/artifact_kit/graph_document_v1.md");

#[test]
fn kit_cases_pass() {
    kit::run_cases(CASES).unwrap();
}

#[test]
fn kit_runner_rejects_a_failing_case() {
    let md = "## broken\n\n```json rejected diagnostic=wrong_diagnostic_name\n{\"formatVersion\":2,\"nodes\":[],\"edges\":[]}\n```\n";
    let err =
        kit::run_cases(md).expect_err("a case whose diagnostic name does not match must fail");
    assert!(err.to_string().contains("wrong_diagnostic_name"));
}
