use crate::harness::{
    include_domain_config, no_include_domain_config, setup_include_mock,
    setup_include_mock_with_refs, test_domain_config, test_project_config, test_tera,
};
use codegraph::generate;
#[allow(unused_imports)]
use codegraph::generate::traits::{DomainGenerator, EntityGenerator, GlobalGenerator};
use codegraph::generate::ProjectConfig;
use codegraph_core::mock::MockEngine;
use codegraph_core::types::{PropertyNode, SchemaNode};

// ── E4: Handler generation with ?include= ───────────────────────────────

#[tokio::test]
async fn handler_with_include_produces_include_code() {
    let mock = setup_include_mock_with_refs();
    let config = include_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-handler-include");

    let gen = generate::api::handler::HandlerGenerator::new(&output_dir);
    let files = gen
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

    assert_eq!(files.len(), 1, "Should produce exactly one handler file");
    let content = &files[0].content;

    // 1. ListParams struct has include: Option<String> field
    assert!(
        content.contains("pub include: Option<String>"),
        "ListParams must have include field. Got:\n{content}"
    );

    // 2. ALLOWED_INCLUDE_KEYS constant contains "person"
    assert!(
        content.contains("ALLOWED_INCLUDE_KEYS"),
        "Should have ALLOWED_INCLUDE_KEYS constant. Got:\n{content}"
    );
    assert!(
        content.contains("\"person\""),
        "ALLOWED_INCLUDE_KEYS should contain 'person'. Got:\n{content}"
    );

    // 3. get_by_id body contains include parsing with split(',')
    // The expression is multi-line in the generated code, so check key fragments.
    assert!(
        content.contains("params.include"),
        "get_by_id should reference params.include. Got:\n{content}"
    );
    assert!(
        content.contains(".as_ref()"),
        "get_by_id should call .as_ref() on include. Got:\n{content}"
    );
    assert!(
        content.contains(r#".split(',').map(|p| p.trim().to_string()).collect()"#),
        "get_by_id should split include by comma. Got:\n{content}"
    );

    // 4. Validation: ALLOWED_INCLUDE_KEYS.contains(&path.as_str())
    assert!(
        content.contains("ALLOWED_INCLUDE_KEYS.contains(&path.as_str())"),
        "Should validate include paths against ALLOWED_INCLUDE_KEYS. Got:\n{content}"
    );

    // 5. Validation: path.split('.').count() > 3
    assert!(
        content.contains("path.split('.').count() > 3"),
        "Should validate max include depth. Got:\n{content}"
    );

    // 6. Return type uses WorkerWithIncludeResponse instead of serde_json::Value
    // The get_by_id function should use the typed response; locate it by finding the
    // `async fn get_by_id` signature with WorkerWithIncludeResponse.
    assert!(
        content.contains("WorkerWithIncludeResponse"),
        "get_by_id should return WorkerWithIncludeResponse. Got:\n{content}"
    );
    assert!(
        content.contains("get_by_id") && content.contains("Result<Json<WorkerWithIncludeResponse"),
        "get_by_id function must return WorkerWithIncludeResponse. Got:\n{content}"
    );

    // 7. Match branch for 'person' using repo.fetch_person_for_worker
    assert!(
        content.contains(r#""person" => {"#),
        "Should have match arm for 'person'. Got:\n{content}"
    );
    assert!(
        content.contains("fetch_person_for_worker"),
        "Should reference fetch_person_for_worker in match arm. Got:\n{content}"
    );

    // 8. Response construction with WorkerWithIncludeResponse
    assert!(
        content.contains("WorkerWithIncludeResponse {"),
        "Should construct WorkerWithIncludeResponse. Got:\n{content}"
    );
    assert!(
        content.contains("included: Some(included)"),
        "Response should include included data. Got:\n{content}"
    );
    assert!(
        content.contains("meta: crate::api::meta::Meta"),
        "Response should have meta. Got:\n{content}"
    );

    // 9. Utoipa annotation references WorkerWithIncludeResponse in body =
    assert!(
        content.contains("body = WorkerWithIncludeResponse"),
        "Utoipa annotation should reference WorkerWithIncludeResponse. Got:\n{content}"
    );
}

#[tokio::test]
async fn handler_without_include_omits_include_code() {
    let mock = setup_include_mock();
    let config = no_include_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-handler-no-include");

    let gen = generate::api::handler::HandlerGenerator::new(&output_dir);
    let files = gen
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

    assert_eq!(files.len(), 1, "Should produce exactly one handler file");
    let content = &files[0].content;

    // Should NOT have include-related code
    assert!(
        !content.contains("pub include: Option<String>"),
        "ListParams must NOT have include field without allow_include. Got:\n{content}"
    );
    assert!(
        !content.contains("ALLOWED_INCLUDE_KEYS"),
        "Should NOT have ALLOWED_INCLUDE_KEYS without allow_include. Got:\n{content}"
    );
    assert!(
        !content.contains("fetch_person_for_worker"),
        "Should NOT reference fetch_person_for_worker without allow_include. Got:\n{content}"
    );

    // Should return serde_json::Value in get_by_id
    assert!(
        content.contains("-> Result<Json<serde_json::Value>"),
        "get_by_id should return serde_json::Value without allow_include. Got:\n{content}"
    );
    assert!(
        !content.contains("WorkerWithIncludeResponse"),
        "Should NOT reference WorkerWithIncludeResponse without allow_include. Got:\n{content}"
    );
}

/// Verify handler-generated filter keys use stripped rust_field_name,
/// not pg_column_name with _code suffix.
#[tokio::test]
async fn handler_filter_keys_use_stripped_codelist_names() {
    let deployment_schema = SchemaNode {
        namespace: None,
        schema_id: "recruiting/json/DeploymentType.json".to_string(),
        title: "DeploymentType".to_string(),
        description: Some("Deployment type".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("recruiting".to_string()),
        rel_path: "recruiting/json/DeploymentType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "DeploymentType".to_string(),
        pg_table_name: "deployment".to_string(),
        api_path_segment: "deployments".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: true,
        has_one_of: false,
        has_any_of: false,
        has_definitions: true,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    };

    // Codelist reference — rust_field_name stripped (no _code),
    // pg_column_name retains _code suffix.
    let codelist_prop = PropertyNode {
        name: "assignment_reason".to_string(),
        prop_type: "string".to_string(),
        description: None,
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
        rust_field_name: "assignment_reason".to_string(),
        rust_field_type: "AssignmentReasonCodeList".to_string(),
        sea_orm_type: "Text".to_string(),
        render_strategy: "codelist_reference".to_string(),
        ref_target: None,
        classification: Some("codelist_reference".to_string()),
        projection: None,
        classification_kind: Some(
            codegraph_type_contracts::RefClassificationKind::CodelistReference,
        ),
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    };

    let mock = MockEngine::builder()
        .with_schema(deployment_schema)
        .with_properties("DeploymentType", vec![codelist_prop])
        .build();

    let config = test_domain_config();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-handler-code");
    let tera = test_tera();

    let gen = codegraph::generate::api::handler::HandlerGenerator::new(&output_dir);
    let files = gen
        .generate(
            &mock,
            "DeploymentType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let handler = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("_handler.rs"))
        .expect("should generate handler file");

    // Filter keys must use stripped rust_field_name, not the _code-suffixed
    // pg_column_name. If the _code suffix appears without the stripped form,
    // the handler is leaking the pg_column_name into filter keys.
    if handler.content.contains("assignment_reason_code")
        && !handler.content.contains("assignment_reason\"")
    {
        panic!(
            "handler uses _code suffix for codelist filter key! Content:\n{}",
            &handler.content[..handler.content.len().min(2000)]
        );
    }
}

/// List endpoints must wire dot-notation includes through their batch fetch
/// methods (issue: list silently dropped `deployment.*` includes while
/// GET-by-id wired them). Asserts the rendered worker list handler calls the
/// per-path batch methods and merges leaf fields into
/// `included.<segments[0]>[<source_id>]`, and that the emitted repository
/// contains the dot batch methods and parses as valid Rust.
#[tokio::test]
async fn dot_include_list_handler_wires_batch_and_merge() {
    use codegraph::generate::api::include_path::resolve_include_paths;
    use codegraph_config::config::parse_domain_config_str;
    use codegraph_core::types::{DetectionSource, ParentCandidate, PropertyNode};

    fn schema_node(title: &str, domain: &str, table: &str, is_entity: bool) -> SchemaNode {
        SchemaNode {
            namespace: None,
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
            pg_table_name: table.to_string(),
            api_path_segment: table.to_string(),
            parent_schema: None,
            is_entity,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
            custom_annotations: Default::default(),
            access: None,
            annotations: None,
        }
    }

    fn prop_defaults() -> PropertyNode {
        PropertyNode {
            name: String::new(),
            prop_type: "object".to_string(),
            description: None,
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
            pg_column_name: String::new(),
            pg_column_type: "UUID".to_string(),
            rust_field_name: String::new(),
            rust_field_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            render_strategy: "entity_reference".to_string(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
            type_expr: None,
        }
    }

    fn prop_ref(name: &str, target: &str) -> PropertyNode {
        PropertyNode {
            name: name.to_string(),
            rust_field_name: name.to_string(),
            pg_column_name: name.to_string(),
            pg_column_type: "UUID".to_string(),
            rust_field_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            ref_target: Some(target.to_string()),
            classification_kind: Some(
                codegraph_type_contracts::RefClassificationKind::EntityReference,
            ),
            render_strategy: "entity_reference".to_string(),
            ..prop_defaults()
        }
    }

    let mock = MockEngine::builder()
        .with_schema(schema_node("WorkerType", "hr", "worker", true))
        .with_schema(schema_node("DeploymentType", "hr", "deployment", true))
        .with_schema(schema_node("PositionType", "hr", "position", true))
        .with_schema(schema_node("OrganizationType", "hr", "organization", true))
        .with_ref_target(
            "deployment_id",
            "WorkerType",
            schema_node("DeploymentType", "hr", "deployment", true),
        )
        .with_ref_target(
            "position",
            "DeploymentType",
            schema_node("PositionType", "hr", "position", true),
        )
        .with_ref_target(
            "organization",
            "DeploymentType",
            schema_node("OrganizationType", "hr", "organization", true),
        )
        .with_properties(
            "WorkerType",
            vec![prop_ref("deployment_id", "DeploymentType")],
        )
        .with_properties(
            "DeploymentType",
            vec![
                prop_ref("position", "PositionType"),
                prop_ref("organization", "OrganizationType"),
            ],
        )
        .with_parent_candidate(ParentCandidate {
            child_title: "DeploymentType".to_string(),
            parent_title: "WorkerType".to_string(),
            field_name: "worker_type_id".to_string(),
            source: DetectionSource::ScalarRef,
        })
        .build();

    let config = {
        let toml = r#"
[defaults]
type_suffix = "Type"
operations = ["create", "read", "update", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType", "DeploymentType", "PositionType", "OrganizationType"]

[domains.hr.entity_config.WorkerType]
role = "root"
allow_include = ["deployment.position", "deployment.organization"]

[domains.hr.entity_config.DeploymentType]
role = "child"
parent = "WorkerType"
parent_ref = "worker_type_id"
"#;
        parse_domain_config_str(toml).unwrap()
    };

    let include_paths = resolve_include_paths(
        &mock,
        &config,
        "hr",
        "WorkerType",
        Some(&vec![
            "deployment.position".to_string(),
            "deployment.organization".to_string(),
        ]),
    )
    .await
    .unwrap();
    assert_eq!(include_paths.len(), 2, "both dot paths must resolve");
    for p in &include_paths {
        assert!(
            !p.batch_fetch_method.is_empty(),
            "batch method name must be set for dot paths"
        );
    }

    // Handler: list block wires both batch methods + merge block.
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-dot-include-list");
    let gen = generate::api::handler::HandlerGenerator::new(&output_dir);
    let files = gen
        .generate(
            &mock,
            "WorkerType",
            "hr",
            &config,
            &test_tera(),
            &test_project_config(),
        )
        .await
        .unwrap();
    let handler = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("worker_handler"))
        .expect("worker handler generated");
    assert!(
        handler
            .content
            .contains("fetch_deployment_position_batch_for_worker"),
        "list handler must call the position dot batch method"
    );
    assert!(
        handler
            .content
            .contains("fetch_deployment_organization_batch_for_worker"),
        "list handler must call the organization dot batch method"
    );
    assert!(
        handler
            .content
            .contains("or_insert_with(|| serde_json::json!({}))"),
        "list handler must contain the merge block"
    );

    // Repository: dot batch methods exist and parse.
    use codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
    let code = RepositoryImplEmitter
        .emit(
            &mock,
            "WorkerType",
            "hr",
            &config,
            None,
            &include_paths,
            &ProjectConfig::default(),
        )
        .await
        .expect("repository emission should not fail");
    assert!(
        code.contains("fetch_deployment_position_batch_for_worker")
            && code.contains("fetch_deployment_organization_batch_for_worker"),
        "repository must emit both dot batch methods"
    );
    if let Err(e) = syn::parse_str::<syn::File>(&code) {
        panic!("emitted repository_impl.rs is not valid Rust: {e}");
    }
}

/// Include-fetch responses must hydrate the TARGET's child tables: a worker's
/// `included.person.<id>` payload previously carried all-None sub-objects
/// (e.g. `name`) because the fetch emitters only did flat field assignment.
/// Mirrors hr-specs: PersonType has a `name` VO child (PersonNameType).
#[tokio::test]
async fn person_include_hydrates_target_child_tables() {
    use codegraph::generate::api::include_path::resolve_include_paths;
    use codegraph_config::config::parse_domain_config_str;
    use codegraph_core::types::PropertyNode;

    fn schema_node(
        title: &str,
        domain: &str,
        table: &str,
        is_entity: bool,
    ) -> codegraph_core::types::SchemaNode {
        codegraph_core::types::SchemaNode {
            namespace: None,
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
            pg_table_name: table.to_string(),
            api_path_segment: table.to_string(),
            parent_schema: None,
            is_entity,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
            custom_annotations: Default::default(),
            access: None,
            annotations: None,
        }
    }

    fn prop_defaults() -> PropertyNode {
        PropertyNode {
            name: String::new(),
            prop_type: "object".to_string(),
            description: None,
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
            pg_column_name: String::new(),
            pg_column_type: "UUID".to_string(),
            rust_field_name: String::new(),
            rust_field_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            render_strategy: "direct_column".to_string(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
            type_expr: None,
        }
    }

    fn prop_ref(name: &str, target: &str, is_array: bool) -> PropertyNode {
        PropertyNode {
            is_array,
            min_items: None,
            max_items: None,
            prop_type: if is_array { "array" } else { "object" }.to_string(),
            ..prop_defaults()
        }
        .with_overrides(|p| {
            p.name = name.to_string();
            p.rust_field_name = name.to_string();
            p.pg_column_name = name.to_string();
            p.ref_target = Some(target.to_string());
        })
    }

    fn prop_vo(name: &str, target: &str) -> PropertyNode {
        PropertyNode {
            classification_kind: Some(codegraph_type_contracts::RefClassificationKind::ValueObject),
            render_strategy: "structured".to_string(),
            ..prop_ref(name, target, false)
        }
    }

    trait WithOverrides {
        fn with_overrides(self, f: impl FnOnce(&mut PropertyNode)) -> PropertyNode;
    }
    impl WithOverrides for PropertyNode {
        fn with_overrides(mut self, f: impl FnOnce(&mut PropertyNode)) -> PropertyNode {
            f(&mut self);
            self
        }
    }

    let person_vo = schema_node("PersonNameType", "hr", "person_person_name", false);
    let person = schema_node("PersonType", "hr", "person", true);
    let worker = schema_node("WorkerType", "hr", "worker", true);

    let mock = MockEngine::builder()
        .with_schema(worker.clone())
        .with_schema(person.clone())
        .with_schema(person_vo.clone())
        .with_ref_target("person", "WorkerType", person.clone())
        .with_ref_target("name", "PersonType", person_vo.clone())
        .with_properties("WorkerType", vec![prop_ref("person", "PersonType", false)])
        .with_properties("PersonType", vec![prop_vo("name", "PersonNameType")])
        .with_properties(
            "PersonNameType",
            vec![prop_ref("given", "", false), prop_ref("family", "", false)],
        )
        .build();

    let config = {
        let toml = r#"
[defaults]
type_suffix = "Type"
operations = ["create", "read", "update", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType", "PersonType"]

[domains.hr.entity_config.WorkerType]
role = "root"
allow_include = ["person"]
"#;
        parse_domain_config_str(toml).unwrap()
    };

    let include_paths = resolve_include_paths(
        &mock,
        &config,
        "hr",
        "WorkerType",
        Some(&vec!["person".to_string()]),
    )
    .await
    .unwrap();
    assert_eq!(include_paths.len(), 1, "person path must resolve");

    use codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
    let code = RepositoryImplEmitter
        .emit(
            &mock,
            "WorkerType",
            "hr",
            &config,
            None,
            &include_paths,
            &ProjectConfig::default(),
        )
        .await
        .expect("repository emission should not fail");

    assert!(
        code.contains("fetch_person_for_worker"),
        "single fetch method must be emitted"
    );
    assert!(
        code.contains("let name_rows"),
        "fetch_person_for_worker must hydrate the person's `name` child table"
    );
    assert!(
        code.contains("name: name_rows.into_iter().next(),"),
        "fetch_person_for_worker must populate the response's name field"
    );
    if let Err(e) = syn::parse_str::<syn::File>(&code) {
        panic!("emitted repository_impl.rs is not valid Rust: {e}");
    }
}

/// #162 Phase 2: VO-child (scoped-response) includes must hydrate nested child
/// tables. `worker?include=person` resolves via `child_table_override` (the
/// `person` VO materializes as `worker_person`); the scoped response type is
/// `WorkerPersonLegalResponse` — not entity-native — so the override branch
/// must hydrate from the SOURCE tree's matching child subtree, whose struct
/// names (`WorkerPersonLegal*`) align with the scoped prefix by construction.
#[tokio::test]
async fn person_include_hydrates_scoped_child_tables() {
    use codegraph::generate::api::include_path::resolve_include_paths;
    use codegraph_config::config::parse_domain_config_str;
    use codegraph_core::types::PropertyNode;

    fn schema_node(
        title: &str,
        domain: &str,
        table: &str,
        is_entity: bool,
    ) -> codegraph_core::types::SchemaNode {
        codegraph_core::types::SchemaNode {
            namespace: None,
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
            pg_table_name: table.to_string(),
            api_path_segment: table.to_string(),
            parent_schema: None,
            is_entity,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
            custom_annotations: Default::default(),
            access: None,
            annotations: None,
        }
    }

    fn prop_defaults() -> PropertyNode {
        PropertyNode {
            name: String::new(),
            prop_type: "object".to_string(),
            description: None,
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
            pg_column_name: String::new(),
            pg_column_type: "UUID".to_string(),
            rust_field_name: String::new(),
            rust_field_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            render_strategy: "direct_column".to_string(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
            type_expr: None,
        }
    }

    fn prop_plain(name: &str) -> PropertyNode {
        PropertyNode {
            prop_type: "object".to_string(),
            rust_field_type: "String".to_string(),
            ..prop_defaults()
        }
        .with_overrides(|p| {
            p.name = name.to_string();
            p.rust_field_name = name.to_string();
            p.pg_column_name = name.to_string();
        })
    }

    fn prop_vo(name: &str, target: &str, is_array: bool) -> PropertyNode {
        PropertyNode {
            classification_kind: Some(codegraph_type_contracts::RefClassificationKind::ValueObject),
            render_strategy: "structured".to_string(),
            is_array,
            min_items: None,
            max_items: None,
            prop_type: if is_array { "array" } else { "object" }.to_string(),
            ..prop_defaults()
        }
        .with_overrides(|p| {
            p.name = name.to_string();
            p.rust_field_name = name.to_string();
            p.pg_column_name = name.to_string();
            p.ref_target = Some(target.to_string());
        })
    }

    trait WithOverrides {
        fn with_overrides(self, f: impl FnOnce(&mut PropertyNode)) -> PropertyNode;
    }
    impl WithOverrides for PropertyNode {
        fn with_overrides(mut self, f: impl FnOnce(&mut PropertyNode)) -> PropertyNode {
            f(&mut self);
            self
        }
    }

    let worker = schema_node("WorkerType", "hr", "worker", true);
    let person = schema_node("PersonType", "hr", "person", true);
    // The `person` prop refs this VO, not the Person entity — the VO→entity
    // allOf chain is what triggers the child_table_override path.
    let person_legal_vo = schema_node("PersonLegalType", "hr", "person_legal", false);
    let person_name_vo = schema_node("PersonNameType", "hr", "person_name", false);
    let citizenship_vo = schema_node("CitizenshipType", "hr", "citizenship", false);

    let mock = MockEngine::builder()
        .with_schema(worker.clone())
        .with_schema(person.clone())
        .with_schema(person_legal_vo.clone())
        .with_schema(person_name_vo.clone())
        .with_schema(citizenship_vo.clone())
        // VO→entity: PersonType extends PersonLegalType (allOf chain).
        .with_extending_schema("PersonLegalType", person.clone())
        .with_ref_target("person", "WorkerType", person_legal_vo.clone())
        .with_ref_target("name", "PersonLegalType", person_name_vo.clone())
        .with_ref_target("citizenship", "PersonLegalType", citizenship_vo.clone())
        .with_properties(
            "WorkerType",
            vec![prop_vo("person", "PersonLegalType", false)],
        )
        .with_properties(
            "PersonLegalType",
            vec![
                prop_vo("name", "PersonNameType", false),
                prop_vo("citizenship", "CitizenshipType", true),
            ],
        )
        .with_properties(
            "PersonNameType",
            vec![prop_plain("given"), prop_plain("family")],
        )
        .with_properties("CitizenshipType", vec![prop_plain("country")])
        .build();

    let config = {
        let toml = r#"
[defaults]
type_suffix = "Type"
operations = ["create", "read", "update", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType", "PersonType"]

[domains.hr.entity_config.WorkerType]
role = "root"
allow_include = ["person"]
"#;
        parse_domain_config_str(toml).unwrap()
    };

    let include_paths = resolve_include_paths(
        &mock,
        &config,
        "hr",
        "WorkerType",
        Some(&vec!["person".to_string()]),
    )
    .await
    .unwrap();
    assert_eq!(include_paths.len(), 1, "person path must resolve");
    let path = &include_paths[0];
    assert!(
        path.segments[0].child_table_override.is_some(),
        "person must resolve via the VO child_table_override path"
    );
    assert_eq!(
        path.response_rust_type, "WorkerPersonLegalResponse",
        "scoped response type must use the WorkerPersonLegal prefix"
    );

    use codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
    let code = RepositoryImplEmitter
        .emit(
            &mock,
            "WorkerType",
            "hr",
            &config,
            None,
            &include_paths,
            &ProjectConfig::default(),
        )
        .await
        .expect("repository emission should not fail");

    assert!(
        code.contains("fetch_person_for_worker"),
        "single fetch method must be emitted"
    );

    // Slice out just the fetch method so assertions pin the override branch's
    // emission (the main find_by_id hydration shares the same struct names).
    let fetch_start = code
        .find("pub(crate) async fn fetch_person_for_worker")
        .expect("fetch_person_for_worker must be emitted");
    let fetch_code = &code[fetch_start..];
    let fetch_end = fetch_start
        + fetch_code
            .find("\n    pub(crate) async fn")
            .unwrap_or(fetch_code.len());
    let fetch_code = &code[fetch_start..fetch_end];

    assert!(
        fetch_code.contains("WorkerPersonLegalPersonNameResponse"),
        "fetch_person_for_worker must read the person's nested `name` child \
         table into WorkerPersonLegalPersonNameResponse"
    );
    assert!(
        fetch_code.contains("name: name_rows.into_iter().next(),"),
        "scoped response must populate the `name` field, not leave it None"
    );
    assert!(
        fetch_code.contains("citizenship: citizenship_rows,"),
        "scoped response must populate the `citizenship` array field"
    );
    assert!(
        fetch_code.contains("worker_person_name"),
        "nested name reads must query the worker_person_name child table"
    );
    if let Err(e) = syn::parse_str::<syn::File>(&code) {
        panic!("emitted repository_impl.rs is not valid Rust: {e}");
    }
}
