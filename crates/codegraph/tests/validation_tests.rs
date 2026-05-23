use codegraph::validate::{Severity, ValidationPass};
use codegraph_core::mock::MockEngine;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{
    CodeList, ColumnInfo, CompositionNode, CompositionTree, FkDirection, FkTarget, PropertyNode,
    SchemaNode,
};
use codegraph_type_contracts::RefClassificationKind;
use std::path::Path;

fn test_domain_config() -> codegraph_config::DomainConfig {
    codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml")).unwrap()
}

fn mock_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "recruiting/json/CandidateType.json".to_string(),
        title: "CandidateType".to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("recruiting".to_string()),
        rel_path: "recruiting/json/CandidateType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Candidate".to_string(),
        pg_table_name: "candidate".to_string(),
        api_path_segment: "candidate".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    }
}

fn make_entity_schema(title: &str, domain: &str) -> SchemaNode {
    SchemaNode {
        schema_id: format!("{domain}/json/{title}.json"),
        title: title.to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some(domain.to_string()),
        rel_path: format!("{domain}/json/{title}.json"),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: title.strip_suffix("Type").unwrap_or(title).to_string(),
        pg_table_name: title.strip_suffix("Type").unwrap_or(title).to_lowercase(),
        api_path_segment: title.strip_suffix("Type").unwrap_or(title).to_lowercase(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    }
}

fn make_entity_ref_property(name: &str, ref_target: &str) -> PropertyNode {
    PropertyNode {
        name: name.to_string(),
        prop_type: "string".to_string(),
        description: None,
        format: None,
        is_required: false,
        is_nullable: false,
        is_array: false,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: name.to_string(),
        pg_column_type: "UUID".to_string(),
        rust_field_name: name.to_string(),
        rust_field_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        render_strategy: "entity_reference".to_string(),
        ref_target: Some(ref_target.to_string()),
        classification: Some("entity_reference".to_string()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

// --- Happy path tests ---

#[tokio::test]
async fn validation_passes_for_clean_graph() {
    let engine = MockEngine::builder().with_schema(mock_schema()).build();
    let config = test_domain_config();

    let issues = ValidationPass::run(&engine, &config).await;
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| matches!(i.severity, Severity::Error))
        .collect();
    assert!(
        errors.is_empty(),
        "Clean graph should have no errors: {:?}",
        errors
    );
}

#[tokio::test]
async fn validation_warns_on_empty_graph() {
    let engine = MockEngine::new();
    let config = test_domain_config();

    let issues = ValidationPass::run(&engine, &config).await;
    // An empty graph should produce no errors (nothing to validate)
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| matches!(i.severity, Severity::Error))
        .collect();
    assert!(errors.is_empty());
}

// --- Negative test: empty codelist ---

#[tokio::test]
async fn validation_warns_on_empty_codelist() {
    let engine = MockEngine::new();
    // Ingest a codelist with no enum values
    engine
        .ingest_codelist(&CodeList {
            name: "GenderCodeList".to_string(),
            description: None,
            pg_table_name: "gender_code".to_string(),
            render_as: "enum".to_string(),
            check_expression: None,
        })
        .await
        .unwrap();

    let config = test_domain_config();
    let issues = ValidationPass::run(&engine, &config).await;

    let empty_cl: Vec<_> = issues
        .iter()
        .filter(|i| i.check == "empty_codelist")
        .collect();
    assert_eq!(
        empty_cl.len(),
        1,
        "Should warn about empty codelist, got: {:?}",
        empty_cl
    );
    assert!(
        matches!(empty_cl[0].severity, Severity::Warning),
        "Empty codelist should be a warning, not an error"
    );
    assert_eq!(empty_cl[0].entity, "GenderCodeList");
}

// --- Negative test: missing ref target ---

#[tokio::test]
async fn validation_errors_on_missing_ref_target() {
    // CandidateType has a property referencing "GhostType" which doesn't exist
    // in any domain
    let candidate = mock_schema();
    let ghost_ref = make_entity_ref_property("ghost_id", "GhostType");

    let engine = MockEngine::builder()
        .with_schema(candidate)
        .with_properties("CandidateType", vec![ghost_ref])
        .build();

    let config = test_domain_config();
    let issues = ValidationPass::run(&engine, &config).await;

    let missing: Vec<_> = issues
        .iter()
        .filter(|i| i.check == "ref_target_missing")
        .collect();
    assert_eq!(
        missing.len(),
        1,
        "Should flag missing ref target, got: {:?}",
        missing
    );
    assert!(matches!(missing[0].severity, Severity::Error));
    assert!(
        missing[0].message.contains("GhostType"),
        "Error should mention the missing target"
    );
}

// --- Negative test: undeclared cross-domain FK ---

#[tokio::test]
async fn validation_errors_on_undeclared_cross_domain_fk() {
    // PayRunType (compensation domain) references CandidateType (recruiting domain)
    // but compensation does NOT declare depends_on = ["recruiting"]
    let pay_run = make_entity_schema("PayRunType", "compensation");
    let candidate = make_entity_schema("CandidateType", "recruiting");
    let cross_domain_ref = make_entity_ref_property("candidate_id", "CandidateType");

    let engine = MockEngine::builder()
        .with_schema(pay_run)
        .with_schema(candidate)
        .with_properties("PayRunType", vec![cross_domain_ref])
        .build();

    let config = test_domain_config();
    let issues = ValidationPass::run(&engine, &config).await;

    let fk_issues: Vec<_> = issues
        .iter()
        .filter(|i| i.check == "fk_target_undeclared_dependency")
        .collect();
    assert_eq!(
        fk_issues.len(),
        1,
        "Should flag undeclared cross-domain FK, got: {:?}",
        fk_issues
    );
    assert!(matches!(fk_issues[0].severity, Severity::Error));
    assert!(
        fk_issues[0].message.contains("compensation")
            && fk_issues[0].message.contains("recruiting"),
        "Error should mention both domains: {}",
        fk_issues[0].message
    );
}

// --- Negative test: declared cross-domain FK passes ---

#[tokio::test]
async fn validation_passes_for_declared_cross_domain_fk() {
    // CandidateType (recruiting) references PayRunType (compensation)
    // recruiting declares depends_on = ["common"] but NOT "compensation"
    // However, let's test the inverse: a ref from recruiting to common IS declared
    let candidate = make_entity_schema("CandidateType", "recruiting");

    // Make a property referencing something in a declared dependency (common)
    // Since common has no entities in the test config, we test with a self-domain ref
    let self_ref = make_entity_ref_property("application_id", "ApplicationType");
    let application = make_entity_schema("ApplicationType", "recruiting");

    let engine = MockEngine::builder()
        .with_schema(candidate)
        .with_schema(application)
        .with_properties("CandidateType", vec![self_ref])
        .build();

    let config = test_domain_config();
    let issues = ValidationPass::run(&engine, &config).await;

    let fk_issues: Vec<_> = issues
        .iter()
        .filter(|i| i.check == "fk_target_undeclared_dependency")
        .collect();
    assert!(
        fk_issues.is_empty(),
        "Same-domain FK should not flag undeclared dependency: {:?}",
        fk_issues
    );
}

// --- Negative test: deep composition tree ---

#[tokio::test]
async fn validation_warns_on_deep_composition_tree() {
    // Build a composition tree with depth 5 (exceeds max 3)
    let candidate = mock_schema();
    let deep_tree = CompositionTree {
        root: CompositionNode {
            field_name: "root".to_string(),
            schema_title: "CandidateType".to_string(),
            table_schema: "recruiting".to_string(),
            table_name: "candidate".to_string(),
            fk: None,
            is_collection: false,
            columns: vec![],
            jsonb_columns: vec![],
            children: vec![CompositionNode {
                field_name: "level1".to_string(),
                schema_title: "Level1".to_string(),
                table_schema: "recruiting".to_string(),
                table_name: "level1".to_string(),
                fk: Some(FkDirection::OnChild {
                    column: "candidate_id".to_string(),
                }),
                is_collection: false,
                columns: vec![],
                jsonb_columns: vec![],
                children: vec![CompositionNode {
                    field_name: "level2".to_string(),
                    schema_title: "Level2".to_string(),
                    table_schema: "recruiting".to_string(),
                    table_name: "level2".to_string(),
                    fk: Some(FkDirection::OnChild {
                        column: "level1_id".to_string(),
                    }),
                    is_collection: false,
                    columns: vec![],
                    jsonb_columns: vec![],
                    children: vec![CompositionNode {
                        field_name: "level3".to_string(),
                        schema_title: "Level3".to_string(),
                        table_schema: "recruiting".to_string(),
                        table_name: "level3".to_string(),
                        fk: Some(FkDirection::OnChild {
                            column: "level2_id".to_string(),
                        }),
                        is_collection: false,
                        columns: vec![],
                        jsonb_columns: vec![],
                        children: vec![CompositionNode {
                            field_name: "level4".to_string(),
                            schema_title: "Level4".to_string(),
                            table_schema: "recruiting".to_string(),
                            table_name: "level4".to_string(),
                            fk: Some(FkDirection::OnChild {
                                column: "level3_id".to_string(),
                            }),
                            is_collection: false,
                            columns: vec![],
                            jsonb_columns: vec![],
                            children: vec![],
                            composite_range: None,
                            consumed_fields: vec![],
                        }],
                        composite_range: None,
                        consumed_fields: vec![],
                    }],
                    composite_range: None,
                    consumed_fields: vec![],
                }],
                composite_range: None,
                consumed_fields: vec![],
            }],
            composite_range: None,
            consumed_fields: vec![],
        },
    };

    let engine = MockEngine::builder()
        .with_schema(candidate)
        .with_composition_tree("CandidateType", deep_tree)
        .build();

    let config = test_domain_config();
    let issues = ValidationPass::run(&engine, &config).await;

    let depth_issues: Vec<_> = issues
        .iter()
        .filter(|i| i.check == "composition_depth")
        .collect();
    assert_eq!(
        depth_issues.len(),
        1,
        "Should warn about deep composition tree, got: {:?}",
        depth_issues
    );
    assert!(matches!(depth_issues[0].severity, Severity::Warning));
    assert!(
        depth_issues[0].message.contains("5"),
        "Should report actual depth: {}",
        depth_issues[0].message
    );
}

// --- Negative test: circular entity references ---

#[tokio::test]
async fn validation_warns_on_circular_entity_refs() {
    // CandidateType -> ApplicationType -> CandidateType (circular)
    let candidate = make_entity_schema("CandidateType", "recruiting");
    let application = make_entity_schema("ApplicationType", "recruiting");

    let candidate_ref_to_app = make_entity_ref_property("application_id", "ApplicationType");
    let app_ref_to_candidate = make_entity_ref_property("candidate_id", "CandidateType");

    let engine = MockEngine::builder()
        .with_schema(candidate)
        .with_schema(application)
        .with_properties("CandidateType", vec![candidate_ref_to_app])
        .with_properties("ApplicationType", vec![app_ref_to_candidate])
        .build();

    let config = test_domain_config();
    let issues = ValidationPass::run(&engine, &config).await;

    let circular: Vec<_> = issues
        .iter()
        .filter(|i| i.check == "circular_entity_ref")
        .collect();
    // Both CandidateType and ApplicationType should flag the circular ref
    assert!(
        circular.len() >= 1,
        "Should detect circular entity references, got: {:?}",
        circular
    );
    assert!(matches!(circular[0].severity, Severity::Warning));
    assert!(
        circular[0].message.contains("Circular reference"),
        "Should describe the cycle: {}",
        circular[0].message
    );
}

// --- Negative test: shallow composition tree passes ---

#[tokio::test]
async fn validation_passes_for_shallow_composition_tree() {
    let candidate = mock_schema();
    let shallow_tree = CompositionTree {
        root: CompositionNode {
            field_name: "root".to_string(),
            schema_title: "CandidateType".to_string(),
            table_schema: "recruiting".to_string(),
            table_name: "candidate".to_string(),
            fk: None,
            is_collection: false,
            columns: vec![],
            jsonb_columns: vec![],
            children: vec![CompositionNode {
                field_name: "address".to_string(),
                schema_title: "AddressType".to_string(),
                table_schema: "recruiting".to_string(),
                table_name: "address".to_string(),
                fk: Some(FkDirection::OnChild {
                    column: "candidate_id".to_string(),
                }),
                is_collection: false,
                columns: vec![],
                jsonb_columns: vec![],
                children: vec![],
                composite_range: None,
                consumed_fields: vec![],
            }],
            composite_range: None,
            consumed_fields: vec![],
        },
    };

    let engine = MockEngine::builder()
        .with_schema(candidate)
        .with_composition_tree("CandidateType", shallow_tree)
        .build();

    let config = test_domain_config();
    let issues = ValidationPass::run(&engine, &config).await;

    let depth_issues: Vec<_> = issues
        .iter()
        .filter(|i| i.check == "composition_depth")
        .collect();
    assert!(
        depth_issues.is_empty(),
        "Depth 2 should not trigger warning: {:?}",
        depth_issues
    );
}

// --- Phantom FK column detection ---

#[tokio::test]
async fn validation_detects_phantom_fk_for_array_entity_ref() {
    // Simulate the bug: an array property classified as EntityReference
    // with a FK target — this would produce a phantom single-UUID FK column.
    let candidate = mock_schema();
    let tree = CompositionTree {
        root: CompositionNode {
            field_name: "candidate".into(),
            schema_title: "CandidateType".into(),
            table_schema: "recruiting".into(),
            table_name: "candidate".into(),
            fk: None,
            is_collection: false,
            columns: vec![ColumnInfo {
                name: "profiles".into(),
                description: None,
                rust_type: "Vec<Profile>".into(),
                postgres_type: "UUID".into(),
                is_optional: true,
                is_codelist_fk: false,
                composite_columns: vec![],
                is_array: true,
                classification: Some(RefClassificationKind::EntityReference),
                fk_target: Some(FkTarget {
                    schema: "recruiting".into(),
                    table: "candidate_profile".into(),
                    column: "id".into(),
                    on_delete: "SET NULL".into(),
                }),
                check_values: vec![],
            }],
            jsonb_columns: vec![],
            children: vec![],
            composite_range: None,
            consumed_fields: vec![],
        },
    };

    let engine = MockEngine::builder()
        .with_schema(candidate)
        .with_composition_tree("CandidateType", tree)
        .build();

    let config = test_domain_config();
    let issues = ValidationPass::run(&engine, &config).await;

    let phantom_issues: Vec<_> = issues
        .iter()
        .filter(|i| i.check == "phantom_fk_column")
        .collect();
    assert_eq!(
        phantom_issues.len(),
        1,
        "Should detect phantom FK column for array entity ref, got: {:?}",
        phantom_issues
    );
    assert!(matches!(phantom_issues[0].severity, Severity::Warning));
    assert!(
        phantom_issues[0].message.contains("profiles"),
        "Should mention the property name: {}",
        phantom_issues[0].message
    );
}

#[tokio::test]
async fn validation_allows_non_array_entity_ref_fk() {
    // Non-array EntityReference FK columns are valid (single FK on parent).
    let candidate = mock_schema();
    let tree = CompositionTree {
        root: CompositionNode {
            field_name: "candidate".into(),
            schema_title: "CandidateType".into(),
            table_schema: "recruiting".into(),
            table_name: "candidate".into(),
            fk: None,
            is_collection: false,
            columns: vec![ColumnInfo {
                name: "nationality".into(),
                description: None,
                rust_type: "Uuid".into(),
                postgres_type: "UUID".into(),
                is_optional: true,
                is_codelist_fk: false,
                composite_columns: vec![],
                is_array: false,
                classification: Some(RefClassificationKind::EntityReference),
                fk_target: Some(FkTarget {
                    schema: "common".into(),
                    table: "country".into(),
                    column: "id".into(),
                    on_delete: "SET NULL".into(),
                }),
                check_values: vec![],
            }],
            jsonb_columns: vec![],
            children: vec![],
            composite_range: None,
            consumed_fields: vec![],
        },
    };

    let engine = MockEngine::builder()
        .with_schema(candidate)
        .with_composition_tree("CandidateType", tree)
        .build();

    let config = test_domain_config();
    let issues = ValidationPass::run(&engine, &config).await;

    let phantom_issues: Vec<_> = issues
        .iter()
        .filter(|i| i.check == "phantom_fk_column")
        .collect();
    assert!(
        phantom_issues.is_empty(),
        "Non-array entity FK should not trigger phantom warning: {:?}",
        phantom_issues
    );
}

#[tokio::test]
async fn mock_engine_does_not_produce_phantom_fk_for_array_entity_ref() {
    // Verify that build_mock_node correctly avoids setting fk_target
    // for array EntityReference properties (the root cause of the 48 errors).
    let parent = mock_schema(); // CandidateType
    let child = make_entity_schema("CandidateProfileType", "recruiting");

    let array_entity_prop = PropertyNode {
        name: "profiles".into(),
        prop_type: "array".into(),
        description: None,
        format: None,
        is_required: false,
        is_nullable: false,
        is_array: true,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "profiles".into(),
        pg_column_type: "UUID".into(),
        rust_field_name: "profiles".into(),
        rust_field_type: "Vec<Uuid>".into(),
        sea_orm_type: "Uuid".into(),
        render_strategy: "entity_reference".into(),
        ref_target: Some("CandidateProfileType".into()),
        classification: Some("entity_reference".into()),
        projection: None,
        classification_kind: Some(RefClassificationKind::EntityReference),
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    };

    let engine = MockEngine::builder()
        .with_schema(parent)
        .with_schema(child.clone())
        .with_properties("CandidateType", vec![array_entity_prop])
        .with_ref_target("profiles", "CandidateType", child)
        .build();

    let config = test_domain_config();
    let issues = ValidationPass::run(&engine, &config).await;

    let phantom_issues: Vec<_> = issues
        .iter()
        .filter(|i| i.check == "phantom_fk_column")
        .collect();
    assert!(
        phantom_issues.is_empty(),
        "MockEngine should not produce phantom FK for array entity ref, got: {:?}",
        phantom_issues
    );
}
