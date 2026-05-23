use codegraph_grafeo::GrafeoEngine;

#[test]
fn test_in_memory_creation() {
    let engine = GrafeoEngine::in_memory();
    assert!(
        engine.is_ok(),
        "Failed to create in-memory engine: {:?}",
        engine.err()
    );
}

#[test]
fn test_in_memory_is_idempotent() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let result = engine.reinit_schema();
    assert!(result.is_ok(), "reinit_schema failed: {:?}", result.err());
}

#[test]
fn test_basic_gql_insert_and_query() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let session = engine.db().session();

    // Insert a schema node using GQL
    session
        .execute(
            "INSERT (:Schema {
            schema_id: 'test/Foo',
            title: 'Foo',
            description: null,
            schema_type: 'object',
            classification: 'entity',
            pg_type: 'TABLE',
            rust_type: 'Foo',
            sea_orm_type: 'Entity',
            domain: 'test',
            rel_path: 'test/Foo.json',
            rust_type_name: 'Foo',
            pg_table_name: 'foo',
            api_path_segment: 'foo',
            parent_schema: null,
            is_entity: true,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false
        })",
        )
        .expect("INSERT should succeed");

    // Query it back
    let result = session
        .execute("MATCH (s:Schema {title: 'Foo'}) RETURN s.schema_id")
        .expect("MATCH should succeed");

    assert_eq!(result.rows.len(), 1, "Should find exactly one schema node");
    let schema_id = result.rows[0][0]
        .as_str()
        .expect("schema_id should be a string");
    assert_eq!(schema_id, "test/Foo");
}
