use crate::harness::{
    include_domain_config, no_include_domain_config, setup_include_mock,
    setup_include_mock_with_refs, test_project_config, test_tera, worker_schema,
};
use codegraph::generate;
#[allow(unused_imports)]
use codegraph::generate::traits::{DomainGenerator, EntityGenerator, GlobalGenerator};
use codegraph_core::mock::MockEngine;
use codegraph_core::types::{PropertyNode, SchemaNode};

// ── Include Feature Tests (E4 + E5) ──────────────────────────────────────────

/// Like a plain `PropertyNode` builder but allows a split
/// `rust_field_name` ≠ `pg_column_name`, matching real ingestion where
/// `strip_code_suffix_safe` strips `_code` from `rust_field_name` while
/// `pg_column_name` retains it.
#[allow(clippy::too_many_arguments)]
fn prop_split(
    name: &str,
    rust_field_name: &str,
    pg_column_name: &str,
    rust_type: &str,
    pg_type: &str,
    required: bool,
    classification_kind: Option<codegraph_type_contracts::RefClassificationKind>,
    ref_target: Option<&str>,
    is_array: bool,
) -> PropertyNode {
    PropertyNode {
        name: name.to_string(),
        prop_type: "string".to_string(),
        description: None,
        format: None,
        is_required: required,
        is_nullable: !required,
        is_array,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: pg_column_name.to_string(),
        pg_column_type: pg_type.to_string(),
        rust_field_name: rust_field_name.to_string(),
        rust_field_type: rust_type.to_string(),
        sea_orm_type: "Text".to_string(),
        render_strategy: "direct_column".to_string(),
        ref_target: ref_target.map(|s| s.to_string()),
        classification: None,
        projection: None,
        classification_kind,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    }
}

// ── E6: DTO generation with ?include= ───────────────────────────────────

#[tokio::test]
async fn dto_include_single_level() {
    let mock = setup_include_mock_with_refs();
    let config = include_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-dto-include-single");

    let generator = generate::ddd::dto::DtoGenerator::new(&output_dir);
    let files = generator
        .generate(
            &mock,
            "WorkerType",
            "hr",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let included_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_included"))
        .expect("Should have a dto_included.rs file");

    let content = &included_file.content;

    assert!(
        content.contains("struct WorkerIncludedData"),
        "Should contain WorkerIncludedData struct. Got:\n{content}"
    );
    assert!(
        content.contains("person: Option<PersonResponse>"),
        "Should contain person field with PersonResponse type. Got:\n{content}"
    );
    assert!(
        content.contains("struct WorkerWithIncludeResponse"),
        "Should contain WorkerWithIncludeResponse struct. Got:\n{content}"
    );
    assert!(
        content.contains("pub data: WorkerLinkedResponse"),
        "Should contain data field with WorkerLinkedResponse. Got:\n{content}"
    );
    assert!(
        content.contains("pub included: Option<WorkerIncludedData>"),
        "Should contain included field. Got:\n{content}"
    );
    assert!(
        content.contains("pub meta: Meta"),
        "Should contain meta field. Got:\n{content}"
    );
}

#[tokio::test]
async fn dto_include_not_generated_when_not_configured() {
    let mock = setup_include_mock();
    let config = no_include_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-dto-include-none");

    let generator = generate::ddd::dto::DtoGenerator::new(&output_dir);
    let files = generator
        .generate(
            &mock,
            "WorkerType",
            "hr",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let included_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_included"));

    assert!(
        included_file.is_none(),
        "Should NOT generate dto_included.rs when allow_include is not configured"
    );
}

#[tokio::test]
async fn dto_include_dot_notation() {
    let position_schema = SchemaNode {
        namespace: None,
        schema_id: "hr/json/PositionType.json".to_string(),
        title: "PositionType".to_string(),
        description: Some("A position".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("hr".to_string()),
        rel_path: "hr/json/PositionType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Position".to_string(),
        pg_table_name: "position".to_string(),
        api_path_segment: "positions".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    };

    let deployment_schema = SchemaNode {
        namespace: None,
        schema_id: "hr/json/DeploymentType.json".to_string(),
        title: "DeploymentType".to_string(),
        description: Some("A deployment".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("hr".to_string()),
        rel_path: "hr/json/DeploymentType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Deployment".to_string(),
        pg_table_name: "deployment".to_string(),
        api_path_segment: "deployments".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    };

    let worker_schema = worker_schema();

    let deployment_prop = PropertyNode {
        name: "deployment".to_string(),
        prop_type: "object".to_string(),
        description: Some("FK to deployment".to_string()),
        format: None,
        is_required: false,
        is_nullable: true,
        is_array: false,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "deployment_id".to_string(),
        pg_column_type: "UUID".to_string(),
        rust_field_name: "deployment".to_string(),
        rust_field_type: "Vec<Uuid>".to_string(),
        sea_orm_type: "Uuid".to_string(),
        render_strategy: "entity_reference".to_string(),
        ref_target: Some("DeploymentType".to_string()),
        classification: Some("entity_reference".to_string()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    };

    let position_prop = PropertyNode {
        name: "position".to_string(),
        prop_type: "object".to_string(),
        description: Some("FK to position".to_string()),
        format: None,
        is_required: false,
        is_nullable: true,
        is_array: false,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "position_id".to_string(),
        pg_column_type: "UUID".to_string(),
        rust_field_name: "position".to_string(),
        rust_field_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        render_strategy: "entity_reference".to_string(),
        ref_target: Some("PositionType".to_string()),
        classification: Some("entity_reference".to_string()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    };

    // Scalar properties on DeploymentType (non-FK fields that should appear in enriched type)
    let scalar_deployment_props = vec![
        PropertyNode {
            name: "assignment_reason_code".to_string(),
            prop_type: "string".to_string(),
            description: Some("Reason for the deployment assignment".to_string()),
            format: None,
            is_required: false,
            is_nullable: true,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "assignment_reason_code".to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: "assignment_reason_code".to_string(),
            rust_field_type: "Option<String>".to_string(),
            sea_orm_type: "Text".to_string(),
            render_strategy: "codelist".to_string(),
            ref_target: Some("AssignmentReasonCodeList".to_string()),
            classification: Some("codelist".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
            type_expr: None,
        },
        PropertyNode {
            name: "full_time_equivalent_ratio".to_string(),
            prop_type: "number".to_string(),
            description: Some("FTE ratio".to_string()),
            format: None,
            is_required: false,
            is_nullable: true,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "full_time_equivalent_ratio".to_string(),
            pg_column_type: "NUMERIC".to_string(),
            rust_field_name: "full_time_equivalent_ratio".to_string(),
            rust_field_type: "Option<rust_decimal::Decimal>".to_string(),
            sea_orm_type: "Decimal".to_string(),
            render_strategy: "primitive_wrapper".to_string(),
            ref_target: None,
            classification: Some("primitive_wrapper".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
            type_expr: None,
        },
    ];

    let mock = MockEngine::builder()
        .with_schema(position_schema.clone())
        .with_schema(deployment_schema.clone())
        .with_schema(worker_schema)
        .with_ref_target("deployment", "WorkerType", deployment_schema)
        .with_ref_target("position", "DeploymentType", position_schema)
        .with_properties("WorkerType", vec![deployment_prop])
        .with_properties("DeploymentType", {
            let mut all = scalar_deployment_props;
            all.push(position_prop);
            all
        })
        .build();

    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-dto-include-dot");

    let config = codegraph_config::config::parse_domain_config_str(
        r#"
[defaults]
operations = ["create", "read", "update", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType", "DeploymentType", "PositionType"]

[domains.hr.entity_config.WorkerType]
operations = ["create", "read", "update", "list"]
allow_include = ["deployment.position"]
"#,
    )
    .unwrap();

    let generator = generate::ddd::dto::DtoGenerator::new(&output_dir);
    let files = generator
        .generate(
            &mock,
            "WorkerType",
            "hr",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let included_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_included"))
        .expect("Should have a dto_included.rs file");

    let content = &included_file.content;

    assert!(
        content.contains("struct DeploymentCombinedResponse"),
        "Should contain enriched type for dot-notation path. Got:\n{content}"
    );
    assert!(
        content.contains("position: Option<PositionResponse>"),
        "Should contain position field in enriched type. Got:\n{content}"
    );
    // Verify the intermediate entity's scalar fields are included (not just id/timestamps)
    assert!(
        content.contains("assignment_reason_code:"),
        "Should contain intermediate entity's scalar fields. Got:\n{content}"
    );
    assert!(
        content.contains("full_time_equivalent_ratio:"),
        "Should contain intermediate entity's numeric fields. Got:\n{content}"
    );
}

/// TDD: the include-path enriched DTO (`dto_included.rs`) must use the
/// stripped `rust_field_name` for codelist fields on the intermediate entity,
/// never the `_code`-suffixed `pg_column_name`. `build_include_dtos` reads the
/// intermediate entity's properties and emits `field.name` from `rust_field_name`
/// (see dto.rs enriched base_fields), so a split prop where
/// `rust_field_name` ≠ `pg_column_name` proves the stripped name is used.
#[tokio::test]
async fn dto_included_enriched_codelist_fields_use_stripped_names() {
    let position_schema = SchemaNode {
        namespace: None,
        schema_id: "hr/json/PositionType.json".to_string(),
        title: "PositionType".to_string(),
        description: Some("A position".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("hr".to_string()),
        rel_path: "hr/json/PositionType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Position".to_string(),
        pg_table_name: "position".to_string(),
        api_path_segment: "positions".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    };

    let deployment_schema = SchemaNode {
        namespace: None,
        schema_id: "hr/json/DeploymentType.json".to_string(),
        title: "DeploymentType".to_string(),
        description: Some("A deployment".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("hr".to_string()),
        rel_path: "hr/json/DeploymentType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Deployment".to_string(),
        pg_table_name: "deployment".to_string(),
        api_path_segment: "deployments".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    };

    use codegraph_type_contracts::RefClassificationKind;

    // Worker → Deployment FK (the first segment of the include path).
    let worker_deployment_fk = prop_split(
        "deployment",
        "deployment",
        "deployment_id",
        "Uuid",
        "UUID",
        false,
        Some(RefClassificationKind::EntityReference),
        Some("DeploymentType"),
        false,
    );

    // Deployment → Position FK (the leaf segment, emitted as a nested field).
    let deployment_position_fk = prop_split(
        "position",
        "position",
        "position_id",
        "Uuid",
        "UUID",
        false,
        Some(RefClassificationKind::EntityReference),
        Some("PositionType"),
        false,
    );

    // Codelist props on Deployment: rust_field_name stripped (no _code),
    // pg_column_name retains the _code suffix. These should land in the
    // enriched struct's base_fields using the STRIPPED name.
    let assignment_reason_codelist = prop_split(
        "assignment_reason",
        "assignment_reason",
        "assignment_reason_code",
        "Option<AssignmentReasonCodeList>",
        "TEXT",
        false,
        Some(RefClassificationKind::CodelistReference),
        Some("AssignmentReasonCodeList"),
        false,
    );
    let status_codelist = prop_split(
        "status",
        "status",
        "status_code",
        "Option<DeploymentStatusCodeList>",
        "TEXT",
        false,
        Some(RefClassificationKind::CodelistReference),
        Some("DeploymentStatusCodeList"),
        false,
    );

    let mock = MockEngine::builder()
        .with_schema(position_schema.clone())
        .with_schema(deployment_schema.clone())
        .with_schema(worker_schema())
        .with_ref_target("deployment", "WorkerType", deployment_schema)
        .with_ref_target("position", "DeploymentType", position_schema)
        .with_properties("WorkerType", vec![worker_deployment_fk])
        .with_properties(
            "DeploymentType",
            vec![
                assignment_reason_codelist,
                status_codelist,
                deployment_position_fk,
            ],
        )
        .build();

    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-dto-include-stripped");

    let config = codegraph_config::config::parse_domain_config_str(
        r#"
[defaults]
operations = ["create", "read", "update", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType", "DeploymentType", "PositionType"]

[domains.hr.entity_config.WorkerType]
operations = ["create", "read", "update", "list"]
allow_include = ["deployment.position"]
"#,
    )
    .unwrap();

    let generator = generate::ddd::dto::DtoGenerator::new(&output_dir);
    let files = generator
        .generate(
            &mock,
            "WorkerType",
            "hr",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let included_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_included"))
        .expect("Should have a dto_included.rs file");

    let content = &included_file.content;

    // The enriched struct is generated for the dot-notation path.
    assert!(
        content.contains("struct DeploymentCombinedResponse"),
        "Should contain enriched type for dot-notation path. Got:\n{content}"
    );
    // Leaf entity reference is a nested field.
    assert!(
        content.contains("position: Option<PositionResponse>"),
        "Should contain nested position field. Got:\n{content}"
    );

    // Codelist base_fields must use the STRIPPED rust_field_name.
    assert!(
        content.contains("assignment_reason:"),
        "Enriched struct must use stripped codelist field name. Got:\n{content}"
    );
    assert!(
        content.contains("status:"),
        "Enriched struct must use stripped status field name. Got:\n{content}"
    );

    // It must NEVER leak the _code-suffixed pg_column_name into struct fields.
    assert!(
        !content.contains("assignment_reason_code"),
        "Enriched struct leaked _code-suffixed pg_column_name. Got:\n{content}"
    );
    assert!(
        !content.contains("status_code"),
        "Enriched struct leaked _code-suffixed pg_column_name. Got:\n{content}"
    );
}
