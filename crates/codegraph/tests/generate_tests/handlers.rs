//! Cross-generator-layer consumers of the entity model: filter fields, gRPC
//! conversions, CLI command, UI form, Playwright TS fixtures. (The candidate
//! command/query/handler/router tests named in the issue sketch no longer live
//! in this target; these are the nearest cohesive stragglers.)

use crate::fixtures::{mock_schema, prop_split, test_domain_config, test_project_config};
use codegraph::generate;
use codegraph::generate::traits::EntityGenerator;
use codegraph_core::mock::MockEngine;
use codegraph_type_contracts::RefClassificationKind;

/// Verify resolve_filter_fields uses stripped rust_field_name for the
/// filter key, and pg_column_name for SQL column references.
#[tokio::test]
async fn filter_fields_use_stripped_names_for_codelist() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/DeploymentType.json",
            "DeploymentType",
            "deployment",
            "recruiting",
            "entity_reference",
        ))
        .with_properties(
            "DeploymentType",
            vec![prop_split(
                "assignment_reason",
                "assignment_reason",
                "assignment_reason_code",
                "AssignmentReasonCodeList",
                "TEXT",
                false,
                Some(RefClassificationKind::CodelistReference),
                Some("../common/json/codelist/AssignmentReasonCodeList.json"),
                false,
            )],
        )
        .build();

    let fields =
        codegraph::generate::filter_fields::resolve_filter_fields(&mock, "DeploymentType", None)
            .await
            .unwrap();

    assert!(!fields.is_empty(), "should resolve filter fields");
    for f in &fields {
        // field_name = rust_field_name (stripped, DTO side)
        // pg_column_name = pg_column_name (with _code suffix, DB side)
        if f.field_name.contains("assignment_reason") {
            assert_eq!(
                f.field_name, "assignment_reason",
                "filter field_name must be stripped, got: {}",
                f.field_name
            );
            assert_eq!(
                f.pg_column_name, "assignment_reason_code",
                "filter pg_column_name must keep _code suffix, got: {}",
                f.pg_column_name
            );
        }
    }
}

/// gRPC service generator: the proto↔domain conversion code accesses DTO
/// fields by their Rust field name. For codelist props the rust_field_name is
/// stripped of the `_code` suffix at ingestion, so the generated
/// `*_conversions.rs` must use `status` / `assignment_reason`, never the
/// pg_column_name `status_code` / `assignment_reason_code`.
#[tokio::test]
async fn grpc_service_conversions_use_stripped_names_for_codelist() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/DeploymentType.json",
            "DeploymentType",
            "deployment",
            "recruiting",
            "entity_reference",
        ))
        .with_properties(
            "DeploymentType",
            vec![
                prop_split(
                    "assignment_reason",
                    "assignment_reason",
                    "assignment_reason_code",
                    "AssignmentReasonCodeList",
                    "TEXT",
                    false,
                    Some(RefClassificationKind::CodelistReference),
                    Some("../common/json/codelist/AssignmentReasonCodeList.json"),
                    false,
                ),
                prop_split(
                    "status",
                    "status",
                    "status_code",
                    "DeploymentStatusCodeList",
                    "TEXT",
                    false,
                    Some(RefClassificationKind::CodelistReference),
                    Some("../common/json/codelist/DeploymentStatusCodeList.json"),
                    false,
                ),
            ],
        )
        .build();

    let config = test_domain_config();
    let template_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = generate::template_engine::create_tera(&template_dir).unwrap();
    let generator = codegraph::generate::grpc::service::GrpcServiceGenerator::new(
        std::path::Path::new("/tmp/test-grpc-service-code"),
    );
    let files = generator
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

    assert!(!files.is_empty(), "should generate gRPC service files");

    let conversions = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("_conversions.rs"))
        .expect("should produce a *_conversions.rs file");
    let content = &conversions.content;

    // Rust DTO field access must use the stripped rust_field_name.
    assert!(
        content.contains("assignment_reason"),
        "conversions must reference stripped field name"
    );
    assert!(
        content.contains("status"),
        "conversions must reference stripped field name"
    );

    // It must NEVER use the pg_column_name with the _code suffix.
    assert!(
        !content.contains("assignment_reason_code"),
        "conversions must NOT use pg_column_name with _code suffix"
    );
    assert!(
        !content.contains("status_code"),
        "conversions must NOT use pg_column_name with _code suffix"
    );
}

/// CLI command generator: field args and the request body builder access DTO
/// fields by the stripped rust_field_name. The generated command module must
/// use `status` / `assignment_reason`, never `status_code` /
/// `assignment_reason_code`.
#[tokio::test]
async fn cli_command_uses_stripped_names_for_codelist() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/DeploymentType.json",
            "DeploymentType",
            "deployment",
            "recruiting",
            "entity_reference",
        ))
        .with_properties(
            "DeploymentType",
            vec![
                prop_split(
                    "assignment_reason",
                    "assignment_reason",
                    "assignment_reason_code",
                    "AssignmentReasonCodeList",
                    "TEXT",
                    false,
                    Some(RefClassificationKind::CodelistReference),
                    Some("../common/json/codelist/AssignmentReasonCodeList.json"),
                    false,
                ),
                prop_split(
                    "status",
                    "status",
                    "status_code",
                    "DeploymentStatusCodeList",
                    "TEXT",
                    false,
                    Some(RefClassificationKind::CodelistReference),
                    Some("../common/json/codelist/DeploymentStatusCodeList.json"),
                    false,
                ),
            ],
        )
        .build();

    let config = test_domain_config();
    let template_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = generate::template_engine::create_tera(&template_dir).unwrap();
    let generator = codegraph::generate::cli::command::CliCommandGenerator::new(
        std::path::Path::new("/tmp/test-cli-command-code"),
    );
    let files = generator
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

    assert_eq!(files.len(), 1, "should generate one CLI command file");
    let content = &files[0].content;

    // CLI args + body builder must use the stripped field name.
    assert!(
        content.contains("assignment_reason"),
        "CLI must reference stripped field name"
    );
    assert!(
        content.contains("status"),
        "CLI must reference stripped field name"
    );

    // It must NEVER access fields via the pg_column_name with _code suffix.
    assert!(
        !content.contains("assignment_reason_code"),
        "CLI must NOT use pg_column_name with _code suffix in field access"
    );
    assert!(
        !content.contains("status_code"),
        "CLI must NOT use pg_column_name with _code suffix in field access"
    );
}

/// UI form generator: Svelte form bindings address the DTO field by its
/// stripped rust_field_name. The generated form must bind `status` /
/// `assignment_reason`, never `status_code` / `assignment_reason_code`.
#[tokio::test]
async fn ui_form_uses_stripped_names_for_codelist() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/DeploymentType.json",
            "DeploymentType",
            "deployment",
            "recruiting",
            "entity_reference",
        ))
        .with_properties(
            "DeploymentType",
            vec![
                prop_split(
                    "assignment_reason",
                    "assignment_reason",
                    "assignment_reason_code",
                    "AssignmentReasonCodeList",
                    "TEXT",
                    false,
                    Some(RefClassificationKind::CodelistReference),
                    Some("../common/json/codelist/AssignmentReasonCodeList.json"),
                    false,
                ),
                prop_split(
                    "status",
                    "status",
                    "status_code",
                    "DeploymentStatusCodeList",
                    "TEXT",
                    false,
                    Some(RefClassificationKind::CodelistReference),
                    Some("../common/json/codelist/DeploymentStatusCodeList.json"),
                    false,
                ),
            ],
        )
        .build();

    let config = test_domain_config();
    let template_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = generate::template_engine::create_tera(&template_dir).unwrap();
    let generator = codegraph::generate::ui::form::UiFormGenerator::new(std::path::Path::new(
        "/tmp/test-ui-form-code",
    ));
    let files = generator
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

    assert_eq!(files.len(), 1, "should generate one UI form component");
    let content = &files[0].content;

    // Form state + data bindings must use the stripped field name.
    assert!(
        content.contains("assignment_reason"),
        "form must bind stripped field name"
    );
    assert!(
        content.contains("status"),
        "form must bind stripped field name"
    );

    // It must NEVER bind via the pg_column_name with the _code suffix.
    assert!(
        !content.contains("assignment_reason_code"),
        "form must NOT bind pg_column_name with _code suffix"
    );
    assert!(
        !content.contains("status_code"),
        "form must NOT bind pg_column_name with _code suffix"
    );
}

