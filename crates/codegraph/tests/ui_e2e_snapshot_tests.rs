//! Snapshot of a canonical generated `owner.crud.test.ts`, guarding the
//! entity-ref dependency setup emitted by `_dep_setup.tera` (non-empty bodies,
//! scalar/array `depIds`, transitive required closure, reverse cleanup).

use std::path::Path;

use codegraph::generate::traits::EntityGenerator;
use codegraph::generate::ui::e2e_test::UiE2eTestGenerator;
use codegraph::generate::ProjectConfig;
use codegraph_config::config::parse_domain_config_str;
use codegraph_core::mock::MockEngine;
use codegraph_core::types::{PropertyNode, SchemaNode};
use codegraph_type_contracts::RefClassificationKind;

fn schema(
    title: &str,
    table: &str,
    rust_type_name: &str,
    domain: &str,
    segment: &str,
) -> SchemaNode {
    SchemaNode {
        custom_annotations: Default::default(),
        schema_id: format!("{domain}/json/{table}.schema.json"),
        title: title.into(),
        description: None,
        schema_type: "object".into(),
        classification: "entity_reference".into(),
        domain: Some(domain.into()),
        rel_path: format!("{domain}/json/{table}.schema.json"),
        pg_type: "UUID".into(),
        rust_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        rust_type_name: rust_type_name.into(),
        pg_table_name: table.into(),
        api_path_segment: segment.into(),
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

fn scalar(name: &str, pg_type: &str, is_required: bool) -> PropertyNode {
    PropertyNode {
        name: name.into(),
        prop_type: "string".into(),
        description: None,
        format: None,
        is_required,
        is_nullable: !is_required,
        is_array: false,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: name.into(),
        pg_column_type: pg_type.into(),
        rust_field_name: name.into(),
        rust_field_type: "String".into(),
        sea_orm_type: "Text".into(),
        render_strategy: "direct_column".into(),
        ref_target: None,
        classification: Some("primitive_wrapper".into()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

fn entity_ref(name: &str, ref_target: &str, is_array: bool, is_required: bool) -> PropertyNode {
    PropertyNode {
        name: name.into(),
        prop_type: if is_array {
            "array".into()
        } else {
            "object".into()
        },
        description: None,
        format: None,
        is_required,
        is_nullable: !is_required,
        is_array,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: name.into(),
        pg_column_type: "UUID".into(),
        rust_field_name: name.into(),
        rust_field_type: if is_array {
            "Vec<Uuid>".into()
        } else {
            "Uuid".into()
        },
        sea_orm_type: "Uuid".into(),
        render_strategy: "entity_reference".into(),
        ref_target: Some(ref_target.into()),
        classification: Some("entity_reference".into()),
        projection: None,
        classification_kind: Some(RefClassificationKind::EntityReference),
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

#[test]
fn canonical_owner_crud_snapshot() {
    let tenant = schema("TenantType", "tenant", "Tenant", "platform", "tenant");
    let person = schema("PersonType", "person", "Person", "hr", "person");
    let party = schema("PartyType", "party", "Party", "hr", "party");
    let worker = schema("WorkerType", "worker", "Worker", "hr", "worker");
    let engine = MockEngine::builder()
        .with_schema(worker)
        .with_schema(person.clone())
        .with_schema(party.clone())
        .with_schema(tenant.clone())
        .with_ref_target("person", "WorkerType", person)
        .with_ref_target("parties", "WorkerType", party)
        .with_ref_target("tenant", "PersonType", tenant)
        .with_properties(
            "WorkerType",
            vec![
                scalar("name", "TEXT", true),
                entity_ref("person", "hr/json/person.schema.json", false, true),
                entity_ref("parties", "hr/json/party.schema.json", true, true),
            ],
        )
        .with_properties(
            "PersonType",
            vec![
                scalar("full_name", "TEXT", true),
                entity_ref("tenant", "platform/json/tenant.schema.json", false, true),
            ],
        )
        .with_properties("PartyType", vec![scalar("party_name", "TEXT", true)])
        .with_properties("TenantType", vec![scalar("legal_name", "TEXT", true)])
        .build();

    let toml = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType", "PersonType", "PartyType"]

[domains.hr.entity_config.WorkerType]
role = "root"
[domains.hr.entity_config.PersonType]
role = "root"
[domains.hr.entity_config.PartyType]
role = "root"

[domains.platform]
label = "Platform"
schema_dir = "platform"
postgres_schema = "platform"
entities = ["TenantType"]

[domains.platform.entity_config.TenantType]
role = "root"
"#;
    let config = parse_domain_config_str(toml).unwrap();

    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = codegraph::generate::template_engine::create_tera(&template_dir).unwrap();
    let output = tempfile::TempDir::new().unwrap();
    let gen = UiE2eTestGenerator::new(output.path());
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(gen.generate(
            &engine,
            "WorkerType",
            "hr",
            &config,
            &tera,
            &ProjectConfig::default(),
        ))
        .expect("UiE2eTestGenerator failed");

    let content = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with(".owner.crud.test.ts"))
        .expect("owner.crud.test.ts should be generated")
        .content
        .clone();

    insta::assert_snapshot!("canonical_owner_crud", content);
}
