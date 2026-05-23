use codegraph_core::error::GraphError;

#[test]
fn not_found_error_displays_title() {
    let err = GraphError::NotFound("PersonType".into());
    assert_eq!(err.to_string(), "Schema not found: PersonType");
}

#[test]
fn internal_error_wraps_boxed_error() {
    let inner: Box<dyn std::error::Error + Send + Sync> = "oops".into();
    let err = GraphError::Internal(inner);
    assert!(err.to_string().contains("oops"));
}
