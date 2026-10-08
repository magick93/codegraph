use crate::harness::{test_project_config, test_tera};
use codegraph::generate;
use codegraph::generate::ProjectConfig;
#[allow(unused_imports)]
use codegraph::generate::traits::{DomainGenerator, EntityGenerator, GlobalGenerator};
use codegraph_core::mock::MockEngine;
use codegraph_core::types::{PropertyNode, SchemaNode};

// ── Include Path Resolution Tests ─────────────────────────────────────────────

mod include_path_resolution_tests {
    use super::*;
    use codegraph::generate::api::include_path::resolve_include_paths;
    use codegraph_config::config::parse_domain_config_str;
    use codegraph_core::types::{DetectionSource, ParentCandidate};

    fn schema_node(title: &str, domain: &str, table: &str, is_entity: bool) -> SchemaNode {
        SchemaNode {
            namespace: None,
            schema_id: format!("{domain}/json/{title}.json"),
            title: title.to_string(),
            description: None,
            schema_type: "object".to_string(),
            classification: if is_entity {
                "entity_reference".to_string()
            } else {
                "value_object".to_string()
            },
            domain: Some(domain.to_string()),
            rel_path: format!("{domain}/json/{title}.json"),
            pg_type: "UUID".to_string(),
            rust_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            rust_type_name: title.to_string(),
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

    fn ref_property(name: &str, pg_column: &str, ref_target: &str, is_array: bool) -> PropertyNode {
        PropertyNode {
            is_id: false,
            name: name.to_string(),
            prop_type: "string".to_string(),
            description: None,
            format: None,
            is_required: false,
            is_nullable: true,
            is_array,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: pg_column.to_string(),
            pg_column_type: "UUID".to_string(),
            rust_field_name: pg_column.to_string(),
            rust_field_type: if is_array {
                "Vec<Uuid>".to_string()
            } else {
                "Uuid".to_string()
            },
            sea_orm_type: "Uuid".to_string(),
            render_strategy: if is_array {
                "array_wrapper".to_string()
            } else {
                "entity_reference".to_string()
            },
            ref_target: Some(ref_target.to_string()),
            classification: Some(if is_array {
                "array_wrapper".to_string()
            } else {
                "entity_reference".to_string()
            }),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
            type_expr: None,
        }
    }

    fn hr_domain_config() -> codegraph_config::DomainConfig {
        parse_domain_config_str(
            r#"
[defaults]
[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType"]
"#,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn resolve_single_segment_include() {
        let engine = MockEngine::builder()
            .with_schema(schema_node("WorkerType", "hr", "worker", true))
            .with_schema(schema_node("PersonType", "common", "person", true))
            .with_ref_target(
                "person_id",
                "WorkerType",
                schema_node("PersonType", "common", "person", true),
            )
            .with_properties(
                "WorkerType",
                vec![ref_property("person_id", "person_id", "PersonType", false)],
            )
            .build();

        let config = hr_domain_config();
        let paths = resolve_include_paths(
            &engine,
            &config,
            "hr",
            "WorkerType",
            Some(&vec!["person".to_string()]),
        )
        .await
        .unwrap();

        assert_eq!(paths.len(), 1, "should resolve 1 include path");
        assert_eq!(paths[0].alias, "person", "alias should match the segment");
        assert_eq!(
            paths[0].segments.len(),
            1,
            "single-segment path should have 1 segment"
        );
        assert_eq!(
            paths[0].segments[0].entity_name, "PersonType",
            "entity_name should be the target schema name"
        );
        assert!(
            !paths[0].segments[0].is_array,
            "scalar FK should not be array"
        );
        assert_eq!(
            paths[0].segments[0].fk_column, "person_id",
            "FK column should match the property pg_column_name"
        );
        assert!(
            paths[0].fetch_method.contains("fetch_person"),
            "fetch_method should include 'fetch_person', got: {}",
            paths[0].fetch_method
        );
    }

    #[tokio::test]
    async fn resolve_dot_notation_include() {
        let engine = MockEngine::builder()
            .with_schema(schema_node("WorkerType", "hr", "worker", true))
            .with_schema(schema_node("DeploymentType", "hr", "deployment", true))
            .with_schema(schema_node("PositionType", "hr", "position", true))
            .with_ref_target(
                "deployment_id",
                "WorkerType",
                schema_node("DeploymentType", "hr", "deployment", true),
            )
            .with_ref_target(
                "position_id",
                "DeploymentType",
                schema_node("PositionType", "hr", "position", true),
            )
            .with_properties(
                "WorkerType",
                vec![ref_property(
                    "deployment_id",
                    "deployment_id",
                    "DeploymentType",
                    false,
                )],
            )
            .with_properties(
                "DeploymentType",
                vec![ref_property(
                    "position_id",
                    "position_id",
                    "PositionType",
                    false,
                )],
            )
            .build();

        let config = hr_domain_config();
        let paths = resolve_include_paths(
            &engine,
            &config,
            "hr",
            "WorkerType",
            Some(&vec!["deployment.position".to_string()]),
        )
        .await
        .unwrap();

        assert_eq!(paths.len(), 1, "should resolve 1 include path");
        assert_eq!(
            paths[0].alias, "deployment.position",
            "alias should preserve dot-notation"
        );
        assert_eq!(
            paths[0].segments.len(),
            2,
            "dot-notation path should have 2 segments"
        );

        // First segment: DeploymentType (array via ItemsOf)
        assert_eq!(
            paths[0].segments[0].entity_name, "DeploymentType",
            "first segment entity should be DeploymentType"
        );
        assert!(
            !paths[0].segments[0].is_array,
            "deployment_id is a scalar FK (entity-ref arrays are junction tables, not include-able)"
        );

        // Second segment: PositionType (scalar FK)
        assert_eq!(
            paths[0].segments[1].entity_name, "PositionType",
            "second segment entity should be PositionType"
        );
        assert!(
            !paths[0].segments[1].is_array,
            "position_id is a scalar FK, not array"
        );

        assert_eq!(
            paths[0].response_rust_type, "DeploymentTypeCombinedResponse",
            "multi-segment path should produce enriched response type"
        );
    }

    #[tokio::test]
    async fn resolve_vo_through_allof_to_entity() {
        let legal_type = SchemaNode {
            custom_annotations: Default::default(),
            access: None,
            annotations: None,
            pg_table_name: String::new(),
            is_entity: false,
            classification: "value_object".to_string(),
            ..schema_node("PersonLegalType", "hr", "", false)
        };
        let person_type = schema_node("PersonType", "common", "person", true);
        let engine = MockEngine::builder()
            .with_schema(schema_node("WorkerType", "hr", "worker", true))
            .with_schema(legal_type.clone())
            .with_schema(person_type.clone())
            .with_ref_target("person", "WorkerType", legal_type.clone())
            .with_properties(
                "WorkerType",
                vec![ref_property("person", "person", "PersonLegalType", false)],
            )
            // PersonLegalType allOf → [PersonBaseType, PersonLegalInclusion]
            .with_allof_targets(
                "PersonLegalType",
                vec![
                    "PersonBaseType".to_string(),
                    "PersonLegalInclusion".to_string(),
                ],
            )
            // Both PersonLegalType and PersonType extend PersonBaseType
            .with_extending_schema("PersonBaseType", legal_type.clone())
            .with_extending_schema("PersonBaseType", person_type.clone())
            // Both also extend PersonLegalInclusion
            .with_extending_schema("PersonLegalInclusion", legal_type.clone())
            .with_extending_schema("PersonLegalInclusion", person_type.clone())
            .build();

        let config = hr_domain_config();
        let paths = resolve_include_paths(
            &engine,
            &config,
            "hr",
            "WorkerType",
            Some(&vec!["person".to_string()]),
        )
        .await
        .unwrap();

        assert_eq!(paths.len(), 1, "should resolve 1 include path");
        assert_eq!(
            paths[0].segments[0].entity_name, "PersonType",
            "VO→entity resolution should return PersonType, not PersonLegalType"
        );
    }

    #[tokio::test]
    async fn entity_model_emits_fk_for_vo_that_extends_entity() {
        let legal_type = SchemaNode {
            custom_annotations: Default::default(),
            access: None,
            annotations: None,
            pg_table_name: String::new(),
            is_entity: false,
            classification: "value_object".to_string(),
            ..schema_node("PersonLegalType", "hr", "", false)
        };
        let person_type = schema_node("PersonType", "common", "person", true);
        let mock = MockEngine::builder()
            .with_schema(schema_node("WorkerType", "hr", "worker", true))
            .with_schema(legal_type.clone())
            .with_schema(person_type.clone())
            .with_ref_target("person", "WorkerType", legal_type.clone())
            .with_properties(
                "WorkerType",
                vec![PropertyNode {
                    is_id: false,
                    name: "person".to_string(),
                    pg_column_name: "person".to_string(),
                    rust_field_name: "person".to_string(),
                    ref_target: Some("PersonLegalType".to_string()),
                    rust_field_type: "Uuid".to_string(),
                    sea_orm_type: "Uuid".to_string(),
                    pg_column_type: "UUID".to_string(),
                    render_strategy: "entity_reference".to_string(),
                    classification_kind: Some(
                        codegraph_type_contracts::RefClassificationKind::ValueObject,
                    ),
                    is_required: false,
                    is_nullable: true,
                    is_array: false,
                    min_items: None,
                    max_items: None,
                    prop_type: "string".to_string(),
                    description: None,
                    format: None,
                    pattern: None,
                    min_length: None,
                    max_length: None,
                    minimum: None,
                    maximum: None,
                    classification: None,
                    projection: None,
                    ui_override_detail: None,
                    ui_override_list_cell: None,
                    ui_override_form: None,
                    ui_override_inline: None,
                    type_expr: None,
                }],
            )
            .with_allof_targets(
                "PersonLegalType",
                vec![
                    "PersonBaseType".to_string(),
                    "PersonLegalInclusion".to_string(),
                ],
            )
            .with_extending_schema("PersonBaseType", legal_type.clone())
            .with_extending_schema("PersonBaseType", person_type.clone())
            .with_extending_schema("PersonLegalInclusion", legal_type.clone())
            .with_extending_schema("PersonLegalInclusion", person_type.clone())
            .build();

        let config = {
            let toml_str = r#"
[defaults]
operations = ["create", "read", "update", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType", "PersonType"]

[domains.hr.entity_config.WorkerType]
operations = ["create", "read", "update", "list"]
"#;
            parse_domain_config_str(toml_str).unwrap()
        };
        let tera = test_tera();
        let output_dir = tempfile::TempDir::new().unwrap();

        let generator = generate::db::entity::SeaOrmEntityGenerator::new(output_dir.path());
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

        let worker_file = files
            .iter()
            .find(|f| f.path.to_string_lossy().contains("hr_worker"))
            .expect("Should have a main entity file for WorkerType");
        let content = &worker_file.content;
        assert!(
            content.contains("person_id"),
            "VO→entity property should emit person_id FK column. Got:\n{content}"
        );
        assert!(
            content.contains("DeriveEntityModel"),
            "Entity model should contain DeriveEntityModel"
        );
        // Verify the FK column has the correct Rust field name (not a mismatched name)
        assert!(
            content.contains("pub person_id:"),
            "FK column must declare pub person_id: with _id suffix. Got:\n{content}"
        );
    }

    /// Shared setup: WorkerType → PersonLegalType (VO) → PersonType (entity) via allOf.
    fn setup_vo_entity_mock() -> MockEngine {
        let legal_type = SchemaNode {
            custom_annotations: Default::default(),
            access: None,
            annotations: None,
            pg_table_name: String::new(),
            is_entity: false,
            classification: "value_object".to_string(),
            ..schema_node("PersonLegalType", "hr", "", false)
        };
        let person_type = schema_node("PersonType", "common", "person", true);
        MockEngine::builder()
            .with_schema(schema_node("WorkerType", "hr", "worker", true))
            .with_schema(legal_type.clone())
            .with_schema(person_type.clone())
            .with_ref_target("person", "WorkerType", legal_type.clone())
            .with_properties(
                "WorkerType",
                vec![PropertyNode {
                    is_id: false,
                    name: "person".to_string(),
                    pg_column_name: "person".to_string(),
                    rust_field_name: "person".to_string(),
                    ref_target: Some("PersonLegalType".to_string()),
                    rust_field_type: "Uuid".to_string(),
                    sea_orm_type: "Uuid".to_string(),
                    pg_column_type: "UUID".to_string(),
                    render_strategy: "entity_reference".to_string(),
                    classification_kind: Some(
                        codegraph_type_contracts::RefClassificationKind::ValueObject,
                    ),
                    is_required: false,
                    is_nullable: true,
                    is_array: false,
                    min_items: None,
                    max_items: None,
                    prop_type: "string".to_string(),
                    description: None,
                    format: None,
                    pattern: None,
                    min_length: None,
                    max_length: None,
                    minimum: None,
                    maximum: None,
                    classification: None,
                    projection: None,
                    ui_override_detail: None,
                    ui_override_list_cell: None,
                    ui_override_form: None,
                    ui_override_inline: None,
                    type_expr: None,
                }],
            )
            .with_allof_targets(
                "PersonLegalType",
                vec![
                    "PersonBaseType".to_string(),
                    "PersonLegalInclusion".to_string(),
                ],
            )
            .with_extending_schema("PersonBaseType", legal_type.clone())
            .with_extending_schema("PersonBaseType", person_type.clone())
            .with_extending_schema("PersonLegalInclusion", legal_type)
            .with_extending_schema("PersonLegalInclusion", person_type)
            .build()
    }

    fn vo_entity_domain_config() -> codegraph_config::DomainConfig {
        let toml_str = r#"
[defaults]
operations = ["create", "read", "update", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType", "PersonType"]

[domains.hr.entity_config.WorkerType]
allow_include = ["person"]
operations = ["create", "read", "update", "list"]
"#;
        parse_domain_config_str(toml_str).unwrap()
    }

    /// Test 3+4: Include path resolution produces correct FK column name.
    /// The `person` include on WorkerType resolves to PersonType (via Tier 1.5
    /// allOf chain), and the resolved FK column must be `person_id`, matching
    /// the entity model emitted by the entity generator.
    #[tokio::test]
    async fn include_path_fk_column_is_person_id() {
        let mock = setup_vo_entity_mock();
        let config = vo_entity_domain_config();
        let paths = resolve_include_paths(
            &mock,
            &config,
            "hr",
            "WorkerType",
            Some(&vec!["person".to_string()]),
        )
        .await
        .unwrap();

        assert_eq!(paths.len(), 1, "should resolve 1 include path");
        assert_eq!(
            paths[0].segments[0].entity_name, "PersonType",
            "should resolve to PersonType through Tier 1.5 allOf chain"
        );
        assert_eq!(
            paths[0].segments[0].fk_column, "person_id",
            "fk_column must be person_id with _id suffix, matching entity model"
        );
    }

    /// Test 5: Repository emitter uses source.person_id not source.person.
    /// The repository code generated for fetch_person_for_worker must access
    /// the FK column by its correct entity-model field name (person_id).
    #[tokio::test]
    async fn repository_uses_person_id_not_person() {
        use codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;

        let mock = setup_vo_entity_mock();
        let config = vo_entity_domain_config();

        let path = codegraph::generate::api::include_path::ResolvedIncludePath {
            alias: "person".to_string(),
            segments: vec![codegraph::generate::api::include_path::IncludeSegment {
                entity_name: "PersonType".to_string(),
                schema_title: "PersonType".to_string(),
                module_name: "person".to_string(),
                domain: "common".to_string(),
                table: "\"common\".\"person\"".to_string(),
                fk_column: "person_id".to_string(),
                reverse_fk_column: "worker_id".to_string(),
                fk_is_required: false,
                reverse_fk_is_required: false,
                is_array: false,
                child_table_override: None,
            }],
            response_rust_type: "PersonResponse".to_string(),
            fetch_method: "fetch_person_for_worker".to_string(),
            batch_fetch_method: "fetch_person_batch_for_worker".to_string(),
        };

        let emitter = RepositoryImplEmitter;
        let code = emitter
            .emit(
                &mock,
                "WorkerType",
                "hr",
                &config,
                None,
                &[path],
                &ProjectConfig::default(),
            )
            .await
            .unwrap();

        // Must access the FK column by its entity-model field name
        assert!(
            code.contains("source.person_id"),
            "Repository must access fk_column=person_id on the entity model. Got:\n{code}"
        );
        // Must NOT use the property name without _id suffix
        assert!(
            !code.contains("source.person\n") && !code.contains("source.person\r"),
            "Repository must NOT access source.person (missing _id). Got:\n{code}"
        );
    }

    #[tokio::test]
    async fn resolve_auto_discovered_include() {
        let engine = MockEngine::builder()
            .with_schema(schema_node("WorkerType", "hr", "worker", true))
            .with_schema(schema_node("PersonType", "hr", "person", true))
            .with_properties(
                "WorkerType",
                vec![ref_property("person_id", "person_id", "PersonType", false)],
            )
            // ScalarRef candidates mean the CHILD holds the scalar ref to the
            // parent — PersonType must own the verified worker_id FK property
            // or the candidate is a phantom edge and discovery skips it.
            .with_properties(
                "PersonType",
                vec![ref_property("worker_id", "worker_id", "WorkerType", false)],
            )
            .with_parent_candidate(ParentCandidate {
                child_title: "PersonType".to_string(),
                parent_title: "WorkerType".to_string(),
                field_name: "person_id".to_string(),
                source: DetectionSource::ScalarRef,
            })
            .build();

        let config = hr_domain_config();
        let paths = resolve_include_paths(&engine, &config, "hr", "WorkerType", None)
            .await
            .unwrap();

        assert!(
            !paths.is_empty(),
            "auto-discovery should return at least 1 path"
        );

        let person_path = paths.iter().find(|p| p.alias == "person");
        assert!(
            person_path.is_some(),
            "auto-discovered paths should include 'person'"
        );

        if let Some(path) = person_path {
            assert_eq!(
                path.segments[0].entity_name, "PersonType",
                "auto-discovered entity should be PersonType"
            );
        }
    }

    /// Build a PropertyNode with test defaults. Only key fields are explicitly set;
    /// all others get sensible defaults.
    fn prop_defaults() -> PropertyNode {
        PropertyNode {
            is_id: false,
            name: String::new(),
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
            pg_column_name: String::new(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: String::new(),
            rust_field_type: "Option<String>".to_string(),
            sea_orm_type: "Text".to_string(),
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

    /// Simple struct/field parser using string matching (no regex dependency).
    /// Returns map of struct_name → (field_name → rust_type).
    fn parse_dto_fields(
        source: &str,
    ) -> std::collections::HashMap<String, std::collections::HashMap<String, String>> {
        let mut result = std::collections::HashMap::new();
        let mut current_struct: Option<String> = None;
        let mut current_fields: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("pub struct ") {
                if let Some(name) = current_struct.take() {
                    result.insert(name, std::mem::take(&mut current_fields));
                }
                current_struct = trimmed
                    .strip_prefix("pub struct ")
                    .and_then(|s| s.split(&[' ', '{'][..]).next())
                    .map(|s| s.to_string());
            } else if let Some(stripped) = trimmed.strip_prefix("pub ")
                && let Some(colon) = stripped.find(':')
            {
                let field_name = stripped[..colon].trim().to_string();
                let field_type = stripped[colon + 1..]
                    .trim()
                    .trim_end_matches(',')
                    .to_string();
                if !field_name.is_empty() && !field_type.is_empty() {
                    current_fields.insert(field_name, field_type);
                }
            }
        }
        if let Some(name) = current_struct {
            result.insert(name, current_fields);
        }
        result
    }

    /// Simple struct initializer parser using string matching.
    /// Returns map of struct_name → (field_name → expression).
    fn parse_repo_assignments(
        source: &str,
    ) -> std::collections::HashMap<String, std::collections::HashMap<String, String>> {
        let mut result = std::collections::HashMap::new();
        let mut in_struct: Option<String> = None;
        let mut current_fields: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for line in source.lines() {
            let trimmed = line.trim();
            // Detect struct construction: "    SomeType {" or "    Ok(Some(TypeName {"
            if (trimmed.ends_with('{') || trimmed.ends_with(" {"))
                && (trimmed.contains("Ok(Some(")
                    || trimmed.starts_with("let ")
                    || trimmed.contains("results.push")
                    || trimmed.contains("leaf_dto")
                    || trimmed.contains("}});")
                    || trimmed.contains(".push("))
            {
                // Extract type name
                let name = if let Some(start) = trimmed.find("Ok(Some(") {
                    trimmed[start + 8..]
                        .trim_end_matches(" {")
                        .trim()
                        .to_string()
                } else if let Some(start) = trimmed.find(".push(") {
                    let rest = &trimmed[start + 6..];
                    rest.split('{').next().unwrap_or("").trim().to_string()
                } else if trimmed.ends_with(" {") {
                    trimmed.trim_end_matches(" {").trim().to_string()
                } else {
                    continue;
                };
                if let Some(prev) = in_struct.take() {
                    result.insert(prev, std::mem::take(&mut current_fields));
                }
                in_struct = Some(name);
            } else if trimmed == "});" || trimmed == "}))" || trimmed.starts_with("..Default") {
                if let Some(prev) = in_struct.take() {
                    result.insert(prev, std::mem::take(&mut current_fields));
                }
            } else if let Some(ref _struct_name) = in_struct {
                // Parse field assignment: "    field_name: expression,"
                if let Some(colon) = trimmed.find(':') {
                    let key = trimmed[..colon].trim().to_string();
                    let val = trimmed[colon + 1..]
                        .trim()
                        .trim_end_matches(',')
                        .to_string();
                    if !key.is_empty()
                        && !val.is_empty()
                        && key != "created_at"
                        && key != "updated_at"
                        && key != "id"
                        && !key.starts_with("..")
                    {
                        current_fields.insert(key, val);
                    }
                }
            }
        }
        if let Some(prev) = in_struct {
            result.insert(prev, current_fields);
        }
        result
    }

    #[derive(Debug)]
    enum ExprPattern {
        Direct,         // row.field
        Parse,          // field.and_then(|v| v.parse().ok())
        SerdeFromValue, // serde_json::from_value(field).unwrap_or_default()
        SerdeAndThen,   // field.and_then(|v| serde_json::from_value(v).ok())
        ParseUnwrap,    // field.parse().unwrap_or_default()
    }

    /// Classify a single field assignment expression into its pattern.
    fn classify_expr(expr: &str) -> ExprPattern {
        let expr = expr.trim();
        if expr.contains("serde_json::from_value") && expr.contains(".and_then(") {
            ExprPattern::SerdeAndThen
        } else if expr.contains("serde_json::from_value") {
            ExprPattern::SerdeFromValue
        } else if expr.contains(".and_then(") && expr.contains(".parse()") {
            ExprPattern::Parse
        } else if expr.contains(".parse()") {
            ExprPattern::ParseUnwrap
        } else {
            ExprPattern::Direct
        }
    }

    /// Check if a DTO field type is compatible with a repository assignment expression pattern.
    fn is_compatible(dto_type: &str, pattern: &ExprPattern) -> bool {
        let dto = dto_type.trim();
        if dto == "Uuid"
            || dto == "Option<Uuid>"
            || dto == "uuid::Uuid"
            || dto == "Option<uuid::Uuid>"
        {
            return matches!(pattern, ExprPattern::Direct);
        }
        if dto.contains("IdentifierType") {
            return matches!(
                pattern,
                ExprPattern::SerdeFromValue | ExprPattern::SerdeAndThen
            );
        }
        if dto.ends_with("CodeList") || dto.ends_with("CodeList>") {
            return matches!(pattern, ExprPattern::Parse | ExprPattern::ParseUnwrap);
        }
        if dto == "String"
            || dto == "Option<String>"
            || dto == "bool"
            || dto == "Option<bool>"
            || dto == "i32"
            || dto == "Option<i32>"
            || dto == "i64"
            || dto == "Option<i64>"
            || dto == "f64"
            || dto == "Option<f64>"
            || dto.starts_with("chrono::")
            || dto.starts_with("rust_decimal::")
        {
            return matches!(pattern, ExprPattern::Direct);
        }
        true
    }

    #[tokio::test]
    async fn dto_field_types_match_repository_assignment_types() {
        // --- Setup: target entity with various property types ---
        // OrderType references TestEntityType via FK; include paths resolve to TestEntityType.
        // TestEntityType references ChildType via FK — dot-notation: test_entity.child.
        let mock = MockEngine::builder()
            .with_schema(schema_node("OrderType", "test", "order", true))
            .with_schema(schema_node("TestEntityType", "test", "test_entity", true))
            .with_schema(schema_node("ChildType", "test", "child", true))
            .with_ref_target(
                "test_entity_id",
                "OrderType",
                schema_node("TestEntityType", "test", "test_entity", true),
            )
            .with_ref_target(
                "child_id",
                "TestEntityType",
                schema_node("ChildType", "test", "child", true),
            )
            .with_properties(
                "OrderType",
                vec![PropertyNode {
                    is_id: false,
                    name: "test_entity_id".to_string(),
                    rust_field_name: "test_entity_id".to_string(),
                    pg_column_name: "test_entity_id".to_string(),
                    pg_column_type: "UUID".to_string(),
                    rust_field_type: "Uuid".to_string(),
                    sea_orm_type: "Uuid".to_string(),
                    ref_target: Some("TestEntityType".to_string()),
                    classification_kind: Some(
                        codegraph_type_contracts::RefClassificationKind::EntityReference,
                    ),
                    render_strategy: "entity_reference".to_string(),
                    ..prop_defaults()
                }],
            )
            .with_properties(
                "TestEntityType",
                vec![
                    PropertyNode {
                        is_id: false,
                        name: "language".to_string(),
                        rust_field_name: "language".to_string(),
                        pg_column_name: "language_code".to_string(),
                        rust_field_type: "Option<String>".to_string(),
                        ref_target: Some("LanguageCodeList".to_string()),
                        classification_kind: Some(
                            codegraph_type_contracts::RefClassificationKind::CodelistReference,
                        ),
                        render_strategy: "codelist_reference".to_string(),
                        ..prop_defaults()
                    },
                    PropertyNode {
                        is_id: false,
                        name: "package_id".to_string(),
                        rust_field_name: "package_id".to_string(),
                        pg_column_name: "package_id".to_string(),
                        pg_column_type: "JSONB".to_string(),
                        rust_field_type: "IdentifierType".to_string(),
                        sea_orm_type: "Json".to_string(),
                        classification_kind: Some(
                            codegraph_type_contracts::RefClassificationKind::StructuredWrapper,
                        ),
                        render_strategy: "structured_wrapper".to_string(),
                        prop_type: "object".to_string(),
                        ..prop_defaults()
                    },
                    PropertyNode {
                        is_id: false,
                        name: "assessment_status".to_string(),
                        rust_field_name: "assessment_status".to_string(),
                        pg_column_name: "status_code".to_string(),
                        rust_field_type: "String".to_string(),
                        ref_target: Some("AssessmentStatusCodeList".to_string()),
                        classification_kind: Some(
                            codegraph_type_contracts::RefClassificationKind::CodelistCheck,
                        ),
                        render_strategy: "codelist_check".to_string(),
                        is_required: true,
                        is_nullable: false,
                        ..prop_defaults()
                    },
                    PropertyNode {
                        is_id: false,
                        name: "person_id".to_string(),
                        rust_field_name: "person_id".to_string(),
                        pg_column_name: "person_id".to_string(),
                        pg_column_type: "UUID".to_string(),
                        rust_field_type: "Uuid".to_string(),
                        sea_orm_type: "Uuid".to_string(),
                        ref_target: Some("PersonType".to_string()),
                        classification_kind: Some(
                            codegraph_type_contracts::RefClassificationKind::EntityReference,
                        ),
                        render_strategy: "entity_reference".to_string(),
                        ..prop_defaults()
                    },
                    PropertyNode {
                        is_id: false,
                        name: "name".to_string(),
                        rust_field_name: "name".to_string(),
                        pg_column_name: "name".to_string(),
                        pg_column_type: "TEXT".to_string(),
                        rust_field_type: "Option<String>".to_string(),
                        ..prop_defaults()
                    },
                    PropertyNode {
                        is_id: false,
                        name: "child_id".to_string(),
                        rust_field_name: "child_id".to_string(),
                        pg_column_name: "child_id".to_string(),
                        pg_column_type: "UUID".to_string(),
                        rust_field_type: "Uuid".to_string(),
                        sea_orm_type: "Uuid".to_string(),
                        ref_target: Some("ChildType".to_string()),
                        classification_kind: Some(
                            codegraph_type_contracts::RefClassificationKind::EntityReference,
                        ),
                        render_strategy: "entity_reference".to_string(),
                        ..prop_defaults()
                    },
                ],
            )
            .build();

        let config = {
            let toml = r#"
[defaults]
operations = ["create", "read", "update", "list"]

[domains.test]
label = "Test"
schema_dir = "test"
postgres_schema = "test"
entities = ["OrderType", "TestEntityType", "ChildType"]

[domains.test.entity_config.OrderType]
allow_include = ["test_entity", "test_entity.child"]
operations = ["create", "read", "update", "list"]
"#;
            parse_domain_config_str(toml).unwrap()
        };

        // --- Step 1: Generate DTO code (use DomainTypesDtoGenerator for actual structs) ---
        let dto_output = tempfile::TempDir::new().unwrap();
        let dto_gen = generate::domain_types::dto::DomainTypesDtoGenerator::new_with_base(
            dto_output.path().to_path_buf(),
        );
        let dto_files = dto_gen
            .generate(
                &mock,
                "TestEntityType",
                "test",
                &config,
                &test_tera(),
                &test_project_config(),
            )
            .await
            .unwrap();
        let dto_source = dto_files
            .iter()
            .find(|f| f.path.to_string_lossy().contains("dto_response"))
            .map(|f| &f.content)
            .expect("DTO generator should produce dto_response.rs");

        // Also generate include-path compound DTOs for OrderType (dot-notation)
        let dto_include_gen = generate::ddd::dto::DtoGenerator::new(dto_output.path());
        let dto_include_files = dto_include_gen
            .generate(
                &mock,
                "OrderType",
                "test",
                &config,
                &test_tera(),
                &test_project_config(),
            )
            .await
            .unwrap();
        let dto_included_source = dto_include_files
            .iter()
            .find(|f| f.path.to_string_lossy().contains("dto_included"))
            .map(|f| f.content.as_str())
            .unwrap_or("");

        // --- Step 2: Parse DTO field types ---
        let mut dto_fields = parse_dto_fields(dto_source);
        if !dto_included_source.is_empty() {
            let included_fields = parse_dto_fields(dto_included_source);
            for (k, v) in included_fields {
                dto_fields.entry(k).or_insert(v);
            }
        }

        // --- Step 3: Generate repository code ---
        use codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
        let include_paths = resolve_include_paths(
            &mock,
            &config,
            "test",
            "OrderType",
            Some(&vec![
                "test_entity".to_string(),
                "test_entity.child".to_string(),
            ]),
        )
        .await
        .unwrap();

        let emitter = RepositoryImplEmitter;
        // Run for both TestEntityType (single-seg target) and OrderType (source with includes)
        let repo_code_test = emitter
            .emit(
                &mock,
                "TestEntityType",
                "test",
                &config,
                None,
                &include_paths,
                &ProjectConfig::default(),
            )
            .await
            .unwrap();
        let repo_code_order = emitter
            .emit(
                &mock,
                "OrderType",
                "test",
                &config,
                None,
                &include_paths,
                &ProjectConfig::default(),
            )
            .await
            .unwrap();
        let repo_code = format!("{}\n{}", repo_code_test, repo_code_order);

        // --- Step 4: Parse repository assignment expressions ---
        let repo_assignments = parse_repo_assignments(&repo_code);

        // --- Step 5: Assert compatibility ---
        let mut mismatches = Vec::new();
        for (struct_name, fields) in &repo_assignments {
            let lookup = struct_name.strip_suffix("Response").unwrap_or(struct_name);
            let lookup_with_type = format!("{}Type", lookup);
            // Try both naming conventions
            let dto_struct = dto_fields
                .get(struct_name)
                .or_else(|| dto_fields.get(&lookup_with_type));

            if let Some(dto_struct) = dto_struct {
                for (field_name, expr) in fields {
                    if let Some(dto_type) = dto_struct.get(field_name) {
                        let pattern = classify_expr(expr);
                        if !is_compatible(dto_type, &pattern) {
                            mismatches.push(format!(
                                "Struct '{}' field '{}': DTO type '{}' vs expr '{}' (pattern {:?})",
                                struct_name,
                                field_name,
                                dto_type,
                                expr.chars().take(80).collect::<String>(),
                                pattern
                            ));
                        }
                    }
                }
            }
        }

        if !mismatches.is_empty() {
            panic!(
                "Type mismatches found in generated code ({} total):\n{}",
                mismatches.len(),
                mismatches
                    .iter()
                    .map(|m| format!("  - {}", m))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }

    /// Test that include path resolution uses config parent_ref for reverse FK
    /// column when no graph $ref property exists on the child entity.
    #[tokio::test]
    async fn resolve_reverse_fk_uses_config_parent_ref() {
        let mock = MockEngine::builder()
            .with_schema(schema_node("WorkerType", "hr", "worker", true))
            .with_schema(schema_node("DeploymentType", "hr", "deployment", true))
            // Only WorkerType has a graph property referencing DeploymentType (ItemsOf).
            // DeploymentType has NO $ref property back to WorkerType — the FK column
            // comes from config parent_ref = "worker_type_id".
            .with_ref_target(
                "deployment_id",
                "WorkerType",
                schema_node("DeploymentType", "hr", "deployment", true),
            )
            .with_properties(
                "WorkerType",
                vec![PropertyNode {
                    is_id: false,
                    name: "deployment_id".to_string(),
                    rust_field_name: "deployment_id".to_string(),
                    pg_column_name: "deployment_id".to_string(),
                    pg_column_type: "UUID".to_string(),
                    rust_field_type: "Uuid".to_string(),
                    sea_orm_type: "Uuid".to_string(),
                    ref_target: Some("DeploymentType".to_string()),
                    classification_kind: Some(
                        codegraph_type_contracts::RefClassificationKind::EntityReference,
                    ),
                    render_strategy: "entity_reference".to_string(),
                    ..prop_defaults()
                }],
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
entities = ["WorkerType", "DeploymentType"]

[domains.hr.entity_config.WorkerType]
allow_include = ["deployment"]
operations = ["create", "read", "update", "list"]

[domains.hr.entity_config.DeploymentType]
role = "child"
parent = "WorkerType"
parent_ref = "worker_type_id"
"#;
            parse_domain_config_str(toml).unwrap()
        };

        let paths = resolve_include_paths(
            &mock,
            &config,
            "hr",
            "WorkerType",
            Some(&vec!["deployment".to_string()]),
        )
        .await
        .unwrap();

        assert_eq!(paths.len(), 1, "should resolve 1 include path");
        assert_eq!(
            paths[0].segments[0].fk_column, "deployment_id",
            "forward FK column should come from graph property"
        );
        assert_eq!(
            paths[0].segments[0].reverse_fk_column, "worker_type_id",
            "reverse FK column should come from config parent_ref, not graph fallback"
        );
    }

    /// Verify generated repository + DTO code parses as syntactically valid Rust.
    /// Catches template bugs like unbalanced parens, missing closes,
    /// malformed expressions — without needing the full dependency tree.
    #[tokio::test]
    async fn generated_code_parses_as_valid_rust() {
        let mock = setup_vo_entity_mock();
        let config = vo_entity_domain_config();

        let mut all_sources: Vec<(String, String)> = Vec::new();

        // Generate DTO code
        let dto_output = tempfile::TempDir::new().unwrap();
        let dto_gen = generate::domain_types::dto::DomainTypesDtoGenerator::new_with_base(
            dto_output.path().to_path_buf(),
        );
        if let Ok(files) = dto_gen
            .generate(
                &mock,
                "WorkerType",
                "hr",
                &config,
                &test_tera(),
                &test_project_config(),
            )
            .await
        {
            for f in files {
                all_sources.push((f.path.to_string_lossy().to_string(), f.content));
            }
        }

        // Generate include-path DTO for OrderType
        let dto_include_gen = generate::ddd::dto::DtoGenerator::new(dto_output.path());
        if let Ok(files) = dto_include_gen
            .generate(
                &mock,
                "OrderType",
                "test",
                &config,
                &test_tera(),
                &test_project_config(),
            )
            .await
        {
            for f in files {
                if f.path.to_string_lossy().contains("dto_included") {
                    all_sources.push((f.path.to_string_lossy().to_string(), f.content));
                }
            }
        }

        // Generate handler
        let handler_gen = generate::api::handler::HandlerGenerator::new(dto_output.path());
        if let Ok(files) = handler_gen
            .generate(
                &mock,
                "WorkerType",
                "hr",
                &config,
                &test_tera(),
                &test_project_config(),
            )
            .await
        {
            for f in files {
                all_sources.push((f.path.to_string_lossy().to_string(), f.content));
            }
        }

        // Generate repository
        use codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
        let include_paths = resolve_include_paths(
            &mock,
            &config,
            "hr",
            "WorkerType",
            Some(&vec!["person".to_string()]),
        )
        .await
        .unwrap();
        let emitter = RepositoryImplEmitter;
        if let Ok(code) = emitter
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
        {
            all_sources.push(("repository_impl.rs".to_string(), code));
        }

        // Parse all sources
        let mut errors = Vec::new();
        for (path, content) in &all_sources {
            if path.contains("mod.rs") || content.is_empty() {
                continue;
            }
            if let Err(e) = syn::parse_str::<syn::File>(content) {
                // Skip errors about unresolved imports — those are expected
                // since we don't have the full workspace. Only fail on actual
                // syntax errors.
                let msg = e.to_string();
                if !msg.contains("unresolved") && !msg.contains("cannot find") {
                    errors.push(format!("{}: {}", path, e));
                }
            }
        }

        assert!(
            !all_sources.is_empty(),
            "Should have generated at least some code to parse"
        );

        if !errors.is_empty() {
            panic!(
                "Generated code has {} syntax error(s):\n{}",
                errors.len(),
                errors
                    .iter()
                    .map(|e| format!("  - {}", e))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }
}
