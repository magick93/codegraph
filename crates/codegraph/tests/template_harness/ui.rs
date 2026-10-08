use crate::harness::{test_project_config, test_tera};
use codegraph::generate;
#[allow(unused_imports)]
use codegraph::generate::traits::{DomainGenerator, EntityGenerator, GlobalGenerator};
use codegraph_core::mock::MockEngine;
use codegraph_core::types::{PropertyNode, SchemaNode};

/// Permission-gated entities generate DID persona tokens in the TS spec and
/// stamp the persona DID into did-carrying fixture fields.
#[test]
fn playwright_ts_use_persona_token_renders_did_persona() {
    use generate::ProjectConfig;
    use generate::playwright::{TsEntityContext, TsFieldDef};

    let tera = test_tera();
    let project = ProjectConfig::default();

    let mk_context = |use_persona_token: bool| TsEntityContext {
        entity_name: "SupportPlan".to_string(),
        module_name: "support_plan".to_string(),
        domain: "support".to_string(),
        path_segment: "support-plans".to_string(),
        nsid: "community.os.support.supportPlan".to_string(),
        has_create: true,
        has_read: true,
        has_update: true,
        has_delete: true,
        has_list: true,
        create_fields: vec![
            TsFieldDef {
                name: "did".to_string(),
                label: "DID".to_string(),
                ts_type: "string".to_string(),
                required: true,
                example_value: "'did:example:0001'".to_string(),
                is_enum: false,
                fk_target_domain: None,
                fk_target_path: None,
                fk_target_module: None,
                fk_target_entity_name: None,
                js_var: None,
            },
            TsFieldDef {
                name: "subjectDid".to_string(),
                label: "Subject DID".to_string(),
                ts_type: "string".to_string(),
                required: true,
                example_value: "'did:example:0002'".to_string(),
                is_enum: false,
                fk_target_domain: None,
                fk_target_path: None,
                fk_target_module: None,
                fk_target_entity_name: None,
                js_var: None,
            },
        ],
        update_fields: vec![],
        update_patch_field: None,
        has_required_fields: true,
        fk_fields: vec![],
        schema_name: "support_plan".to_string(),
        has_fts: false,
        fts_search_field: String::new(),
        fts_search_field_required: false,
        fts_secondary_field: String::new(),
        use_persona_token,
        permission_record_scoped: true,
        persona_did: "did:plc:test.generated".to_string(),
    };

    let spec = generate::render_template_with_project(
        &tera,
        "playwright/ts_spec.tera",
        &mk_context(true),
        &project,
    )
    .unwrap();
    assert!(
        spec.contains("const authToken = 'test-mode:did:plc:test.generated';"),
        "Gated spec should use a DID persona token unconditionally (TEST_AUTH_TOKEN must not override it). Got:\n{spec}"
    );

    let fixture = generate::render_template_with_project(
        &tera,
        "playwright/ts_fixture.tera",
        &mk_context(true),
        &project,
    )
    .unwrap();
    assert!(
        fixture.contains("did: 'did:plc:test.generated',"),
        "Gated fixture should stamp the persona DID into the did field. Got:\n{fixture}"
    );
    assert!(
        fixture.contains("subjectDid: 'did:plc:test.generated',"),
        "Gated fixture should stamp the persona DID into the subjectDid field. Got:\n{fixture}"
    );
    assert!(
        !fixture.contains("'did:example:0001'"),
        "Gated fixture must override did example values. Got:\n{fixture}"
    );

    // Backward compat: non-gated entities keep the empty token + example values.
    let spec_plain = generate::render_template_with_project(
        &tera,
        "playwright/ts_spec.tera",
        &mk_context(false),
        &project,
    )
    .unwrap();
    assert!(
        spec_plain.contains("const authToken = process.env.TEST_AUTH_TOKEN || '';"),
        "Non-gated spec should keep the empty token. Got:\n{spec_plain}"
    );
    let fixture_plain = generate::render_template_with_project(
        &tera,
        "playwright/ts_fixture.tera",
        &mk_context(false),
        &project,
    )
    .unwrap();
    assert!(
        fixture_plain.contains("did: 'did:example:0001',"),
        "Non-gated fixture should keep example values. Got:\n{fixture_plain}"
    );
}

/// The Update describe PUTs a fresh value into the first safe mutable
/// string field, asserts echo + persistence, and both Create and Update
/// rows are registered in createdIds for the file-level afterAll cleanup.
#[test]
fn playwright_ts_update_roundtrip_and_cleanup() {
    use generate::ProjectConfig;
    use generate::playwright::{TsEntityContext, TsFieldDef};

    let tera = test_tera();
    let project = ProjectConfig::default();

    let mk_field = |name: &str, example: &str| TsFieldDef {
        name: name.to_string(),
        label: name.to_string(),
        ts_type: "string".to_string(),
        required: true,
        example_value: format!("'{example}'"),
        is_enum: false,
        fk_target_domain: None,
        fk_target_path: None,
        fk_target_module: None,
        fk_target_entity_name: None,
        js_var: None,
    };

    let patch_field = mk_field("preferredName", "Test");
    let mk_context = |patch: Option<TsFieldDef>| TsEntityContext {
        entity_name: "PersonRecord".to_string(),
        module_name: "crm_person_record".to_string(),
        domain: "crm".to_string(),
        path_segment: "person-record".to_string(),
        nsid: "community.os.crm.personRecord".to_string(),
        has_create: true,
        has_read: true,
        has_update: true,
        has_delete: true,
        has_list: true,
        create_fields: vec![patch_field.clone()],
        update_fields: vec![patch_field.clone()],
        update_patch_field: patch,
        has_required_fields: true,
        fk_fields: vec![],
        schema_name: "crm_person_record".to_string(),
        has_fts: false,
        fts_search_field: String::new(),
        fts_search_field_required: false,
        fts_secondary_field: String::new(),
        use_persona_token: false,
        permission_record_scoped: false,
        persona_did: "did:plc:test.generated".to_string(),
    };

    let spec = generate::render_template_with_project(
        &tera,
        "playwright/ts_spec.tera",
        &mk_context(Some(patch_field.clone())),
        &project,
    )
    .unwrap();
    assert!(
        spec.contains("import { uniqueSuffix } from '../../test-utils';"),
        "Update roundtrip spec should import the shared uniqueSuffix helper. Got:\n{spec}"
    );
    assert!(
        spec.contains("— Update"),
        "Spec with a patchable field should render the Update describe. Got:\n{spec}"
    );
    assert!(
        spec.contains("data: { preferredName: newValue },"),
        "Update test should PUT the chosen field. Got:\n{spec}"
    );
    assert!(
        spec.contains("const newValue = `${base} ${uniqueSuffix()}`;"),
        "Update test should use a fresh unique value. Got:\n{spec}"
    );
    assert!(
        spec.matches("createdIds.push(").count() == 2,
        "Create and Update suites should both register created rows. Got:\n{spec}"
    );
    assert!(
        spec.contains("test.afterAll"),
        "Spec should clean up created rows in afterAll. Got:\n{spec}"
    );
    assert!(
        !spec.contains("const newValue = `${'did:"),
        "DID-shaped fields must not be PATCHed with generated suffixes. Got:\n{spec}"
    );

    // No safe patch field → no Update roundtrip test (and no unused import).
    let spec_no_patch = generate::render_template_with_project(
        &tera,
        "playwright/ts_spec.tera",
        &mk_context(None),
        &project,
    )
    .unwrap();
    assert!(
        !spec_no_patch.contains("— Update"),
        "Spec without a safe patch field must skip the Update describe. Got:\n{spec_no_patch}"
    );
    assert!(
        !spec_no_patch.contains("uniqueSuffix"),
        "Spec without an Update test must not import the helper. Got:\n{spec_no_patch}"
    );
}

#[tokio::test]
async fn ui_e2e_include_test_generated_when_allow_include_configured() {
    let dep_schema = SchemaNode {
        namespace: None,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
        schema_id: "hr/json/DepEntityType.json".into(),
        title: "DepEntityType".into(),
        schema_type: "object".into(),
        classification: "entity_reference".into(),
        domain: Some("hr".into()),
        rel_path: "hr/json/DepEntityType.json".into(),
        pg_type: "UUID".into(),
        rust_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        rust_type_name: "DepEntityType".into(),
        pg_table_name: "dep_entity".into(),
        api_path_segment: "dep-entities".into(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        description: None,
    };

    let worker_schema = SchemaNode {
        namespace: None,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
        schema_id: "hr/json/WorkerType.json".into(),
        title: "WorkerType".into(),
        schema_type: "object".into(),
        classification: "entity_reference".into(),
        domain: Some("hr".into()),
        rel_path: "hr/json/WorkerType.json".into(),
        pg_type: "UUID".into(),
        rust_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        rust_type_name: "WorkerType".into(),
        pg_table_name: "worker".into(),
        api_path_segment: "workers".into(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        description: None,
    };

    let worker_props = vec![PropertyNode {
        is_id: false,
        name: "dep_entity_id".into(),
        prop_type: "string".into(),
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
        pg_column_name: "dep_entity_id".into(),
        pg_column_type: "UUID".into(),
        rust_field_name: "dep_entity_id".into(),
        rust_field_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        render_strategy: "entity_reference".into(),
        ref_target: Some("DepEntityType".into()),
        classification: Some("entity_reference".into()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    }];

    let mock = MockEngine::builder()
        .with_schema(dep_schema.clone())
        .with_schema(worker_schema)
        .with_ref_target("dep_entity_id", "WorkerType", dep_schema)
        .with_properties("WorkerType", worker_props)
        .build();

    let tera = test_tera();
    let output_dir = tempfile::TempDir::new().unwrap();

    let config_str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = []

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
depends_on = ["common"]
entities = ["WorkerType", "DepEntityType"]

[domains.hr.entity_config.WorkerType]
role = "root"
allow_include = ["dep_entity"]
[domains.hr.entity_config.DepEntityType]
role = "root"
"#;
    let config = codegraph_config::config::parse_domain_config_str(config_str).unwrap();

    let generator = crate::generate::ui::e2e_test::UiE2eTestGenerator::new(output_dir.path());
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

    let include_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("include.test.ts"))
        .expect("Should generate an include.test.ts file when allow_include is configured");

    let content = &include_file.content;
    assert!(
        content.contains("WorkerType Include"),
        "Should contain entity name in test description"
    );
    assert!(
        content.contains("dep_entity"),
        "Should test the configured include path alias"
    );
    assert!(
        content.contains("unknown include path returns 400"),
        "Should test invalid include path"
    );
    assert!(
        content.contains("include depth beyond 3 returns 400"),
        "Should test depth limit"
    );
    assert!(
        content.contains("list with ?include= returns included data"),
        "Should test list with include when has_list"
    );
    assert!(content.contains("test.afterAll"), "Should include cleanup");
}

/// #162 Phase 3: config-driven extension points on entity detail pages.
/// `ui_detail_extensions` on an entity config must make the generated
/// `+page.svelte` import and mount the consumer-owned panel components from
/// `#lib/components/extensions/` with a conventional testid — replacing the
/// client-side overlay workaround (rsync-owned generated pages).
#[tokio::test]
async fn detail_page_emits_extension_points() {
    use codegraph::generate::ui::page::UiPageGenerator;
    use codegraph_config::config::parse_domain_config_str;
    use codegraph_core::types::{PropertyNode, SchemaNode};

    fn worker_schema() -> SchemaNode {
        SchemaNode {
            namespace: None,
            schema_id: "hr/json/WorkerType.json".to_string(),
            title: "WorkerType".to_string(),
            description: None,
            schema_type: "object".to_string(),
            classification: "entity_reference".to_string(),
            domain: Some("hr".to_string()),
            rel_path: "hr/json/WorkerType.json".to_string(),
            pg_type: "UUID".to_string(),
            rust_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            rust_type_name: "Worker".to_string(),
            pg_table_name: "worker".to_string(),
            api_path_segment: "workers".to_string(),
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
        }
    }

    fn full_name_prop() -> PropertyNode {
        PropertyNode {
            is_id: false,
            name: "fullName".to_string(),
            prop_type: "string".to_string(),
            description: None,
            format: None,
            is_required: true,
            is_nullable: false,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "full_name".to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: "full_name".to_string(),
            rust_field_type: "String".to_string(),
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

    fn config_with_extensions(extensions: Option<&str>) -> codegraph_config::DomainConfig {
        let ext_line = match extensions {
            Some(e) => format!("ui_detail_extensions = [{e}]"),
            None => String::new(),
        };
        let toml = format!(
            r#"
[defaults]
type_suffix = "Type"
operations = ["create", "read", "update", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType"]

[domains.hr.entity_config.WorkerType]
role = "root"
{ext_line}
"#
        );
        parse_domain_config_str(&toml).unwrap()
    }

    let mock = MockEngine::builder()
        .with_schema(worker_schema())
        .with_properties("WorkerType", vec![full_name_prop()])
        .build();
    let tera = {
        let template_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
        codegraph::generate::template_engine::create_tera(&template_dir).unwrap()
    };
    let project = codegraph::generate::ProjectConfig::default();
    let output_dir = tempfile::tempdir().unwrap();

    // ── Positive: entity with ui_detail_extensions mounts the panel ──
    let config = config_with_extensions(Some("\"ird-registration-panel\""));
    let generator = UiPageGenerator::new(output_dir.path());
    let files = generator
        .generate(&mock, "WorkerType", "hr", &config, &tera, &project)
        .await
        .expect("detail page generation should not fail");

    let page = files
        .iter()
        .find(|f| {
            f.path.ends_with("+page.svelte") && f.path.to_string_lossy().contains("[worker_id]")
        })
        .expect("detail +page.svelte must be emitted");

    assert!(
        page.content
            .contains("import IrdRegistrationPanel from '#lib/components/extensions/IrdRegistrationPanel.svelte';"),
        "detail page must import the extension component from the consumer-owned extensions dir"
    );
    assert!(
        page.content.contains("<IrdRegistrationPanel"),
        "detail page must mount the extension component"
    );
    assert!(
        page.content
            .contains("data-testid=\"ird-registration-panel\""),
        "extension mount must carry the conventional testid"
    );

    // ── Negative: extension-less entities render byte-identical to today ──
    let config_plain = config_with_extensions(None);
    let output_plain = tempfile::tempdir().unwrap();
    let files_plain = UiPageGenerator::new(output_plain.path())
        .generate(&mock, "WorkerType", "hr", &config_plain, &tera, &project)
        .await
        .expect("plain detail page generation should not fail");
    let page_plain = files_plain
        .iter()
        .find(|f| {
            f.path.ends_with("+page.svelte") && f.path.to_string_lossy().contains("[worker_id]")
        })
        .expect("plain detail +page.svelte must be emitted");
    assert!(
        !page_plain.content.contains("components/extensions/"),
        "extension-less entities must not emit extension imports"
    );
    assert!(
        !page_plain.content.contains("IrdRegistrationPanel"),
        "extension-less entities must not reference extension components"
    );

    // Explicit empty list must equal the no-key render (default = empty).
    let config_empty = config_with_extensions(Some(""));
    let output_empty = tempfile::tempdir().unwrap();
    let files_empty = UiPageGenerator::new(output_empty.path())
        .generate(&mock, "WorkerType", "hr", &config_empty, &tera, &project)
        .await
        .expect("empty-list detail page generation should not fail");
    let page_empty = files_empty
        .iter()
        .find(|f| {
            f.path.ends_with("+page.svelte") && f.path.to_string_lossy().contains("[worker_id]")
        })
        .expect("empty-list detail +page.svelte must be emitted");
    assert_eq!(
        page_plain.content, page_empty.content,
        "ui_detail_extensions = [] must render byte-identical to the config being absent"
    );
}
