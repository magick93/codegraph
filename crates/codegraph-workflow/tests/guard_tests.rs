use codegraph_workflow::guard::GuardEvaluator;
use serde_json::json;

#[test]
fn numeric_greater_than() {
    let data = json!({"salary": 50000});
    assert!(GuardEvaluator::evaluate("salary > 0", &data).unwrap());
    assert!(!GuardEvaluator::evaluate("salary > 100000", &data).unwrap());
}

#[test]
fn is_not_null() {
    let data = json!({"name": "Alice", "middle": null});
    assert!(GuardEvaluator::evaluate("name IS NOT NULL", &data).unwrap());
    assert!(!GuardEvaluator::evaluate("middle IS NOT NULL", &data).unwrap());
    assert!(!GuardEvaluator::evaluate("missing_field IS NOT NULL", &data).unwrap());
}

#[test]
fn is_null() {
    let data = json!({"name": "Alice", "middle": null});
    assert!(!GuardEvaluator::evaluate("name IS NULL", &data).unwrap());
    assert!(GuardEvaluator::evaluate("middle IS NULL", &data).unwrap());
    assert!(GuardEvaluator::evaluate("missing_field IS NULL", &data).unwrap());
}

#[test]
fn is_not_empty() {
    let data = json!({"items": [1, 2], "empty_list": [], "name": "x"});
    assert!(GuardEvaluator::evaluate("items IS NOT EMPTY", &data).unwrap());
    assert!(!GuardEvaluator::evaluate("empty_list IS NOT EMPTY", &data).unwrap());
    assert!(!GuardEvaluator::evaluate("missing IS NOT EMPTY", &data).unwrap());
}

#[test]
fn boolean_and() {
    let data = json!({"a": 10, "b": 20});
    assert!(GuardEvaluator::evaluate("a > 0 AND b > 0", &data).unwrap());
    assert!(!GuardEvaluator::evaluate("a > 0 AND b > 100", &data).unwrap());
}

#[test]
fn boolean_or() {
    let data = json!({"a": 10, "b": 0});
    assert!(GuardEvaluator::evaluate("a > 5 OR b > 5", &data).unwrap());
    assert!(!GuardEvaluator::evaluate("a > 100 OR b > 100", &data).unwrap());
}

#[test]
fn equality() {
    let data = json!({"status": "active"});
    assert!(GuardEvaluator::evaluate("status == \"active\"", &data).unwrap());
    assert!(!GuardEvaluator::evaluate("status == \"draft\"", &data).unwrap());
}

#[test]
fn inequality() {
    let data = json!({"count": 5});
    assert!(GuardEvaluator::evaluate("count != 0", &data).unwrap());
    assert!(!GuardEvaluator::evaluate("count != 5", &data).unwrap());
}

#[test]
fn in_operator() {
    let data = json!({"status": "active"});
    assert!(GuardEvaluator::evaluate("status IN (\"active\", \"draft\")", &data).unwrap());
    assert!(!GuardEvaluator::evaluate("status IN (\"closed\", \"cancelled\")", &data).unwrap());
}

#[test]
fn nested_field_access() {
    let data = json!({"person": {"name": "Alice"}});
    assert!(GuardEvaluator::evaluate("person.name IS NOT NULL", &data).unwrap());
    assert!(GuardEvaluator::evaluate("person.name == \"Alice\"", &data).unwrap());
}

#[test]
fn invalid_expression_returns_error() {
    let data = json!({});
    assert!(GuardEvaluator::evaluate(">>>invalid<<<", &data).is_err());
}

#[test]
fn complex_expression() {
    let data = json!({
        "compensation_expectation": 150000,
        "position_opening_id": "abc-123",
        "qualifications": [{"name": "AWS"}]
    });
    assert!(GuardEvaluator::evaluate(
        "compensation_expectation > 0 AND position_opening_id IS NOT NULL",
        &data
    )
    .unwrap());
}
