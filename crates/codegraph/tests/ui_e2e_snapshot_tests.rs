//! Snapshots of canonical generated Playwright specs:
//! - `owner.crud.test.ts` — the entity-ref dependency setup from
//!   `_dep_setup.tera` (non-empty bodies, scalar/array `depIds`,
//!   transitive required closure, reverse cleanup).
//! - `ux.test.ts` — the ux-rules list-rendering spec (issue #302): the
//!   per-feature gated blocks over a canonical column zoo (identifier,
//!   readable lead, codelist chip, quantity, money, time point, audit
//!   stamp) with locale/currency mirrors and the sort allow-list.

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
        namespace: None,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
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
        min_items: None,
        max_items: None,
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
        type_expr: None,
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
        min_items: None,
        max_items: None,
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
        type_expr: None,
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

/// A numeric column (quantity, or money when the name carries the
/// keyword and the pg type is NUMERIC).
fn numeric(name: &str, pg_type: &str, is_required: bool) -> PropertyNode {
    PropertyNode {
        rust_field_type: if pg_type.starts_with("NUMERIC") {
            "Decimal".into()
        } else {
            "i32".into()
        },
        ..scalar(name, pg_type, is_required)
    }
}

/// A timestamp column.
fn timestamp(name: &str, is_required: bool) -> PropertyNode {
    PropertyNode {
        rust_field_type: "DateTime<Utc>".into(),
        ..scalar(name, "TIMESTAMPTZ", is_required)
    }
}

/// A codelist-backed column (CodelistReference prop + graph enum values).
fn codelist(name: &str, ref_target: &str, is_required: bool) -> PropertyNode {
    PropertyNode {
        name: name.into(),
        prop_type: "string".into(),
        description: None,
        format: None,
        is_required,
        is_nullable: !is_required,
        is_array: false,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: name.into(),
        pg_column_type: "TEXT".into(),
        rust_field_name: name.into(),
        rust_field_type: "String".into(),
        sea_orm_type: "Text".into(),
        render_strategy: "direct_column".into(),
        ref_target: Some(ref_target.into()),
        classification: Some("codelist_reference".into()),
        projection: None,
        classification_kind: Some(RefClassificationKind::CodelistReference),
        type_expr: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

fn enum_value(value: &str) -> codegraph_core::types::EnumValue {
    codegraph_core::types::EnumValue {
        value: value.into(),
        display_name: None,
        sort_order: 0,
    }
}

#[test]
fn canonical_ux_spec_snapshot() {
    // A plain `format: uuid` column (the identifier / copy-chip slot).
    let plain_uuid = |name: &str, is_required: bool| PropertyNode {
        format: Some("uuid".into()),
        rust_field_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        ..scalar(name, "UUID", is_required)
    };
    // The `id`-named uuid column must NOT resolve as a convention FK:
    // `id` has no `_id` stem, so it stays a plain Identifier column.
    let refund = schema("RefundType", "refund", "Refund", "hr", "refund");
    let engine = MockEngine::builder()
        .with_schema(refund)
        .with_properties(
            "RefundType",
            vec![
                plain_uuid("id", true),
                scalar("name", "TEXT", true),
                codelist("status", "hr/json/refund_status.json", true),
                numeric("headcount", "INTEGER", true),
                numeric("total_amount", "NUMERIC(10,2)", true),
                timestamp("due_at", true),
                timestamp("created_at", true),
            ],
        )
        .with_enum_values(
            "refund_status",
            vec![enum_value("draft"), enum_value("approved")],
        )
        .build();

    let toml = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["RefundType"]

[domains.hr.entity_config.RefundType]
role = "root"
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let rules = codegraph_config::builtin_ux_rules().unwrap().rules;
    let project = ProjectConfig {
        ux: codegraph::generate::UxConfig { ux: Some(rules) },
        ..ProjectConfig::default()
    };

    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = codegraph::generate::template_engine::create_tera(&template_dir).unwrap();
    let output = tempfile::TempDir::new().unwrap();
    let gen = UiE2eTestGenerator::new(output.path());
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(gen.generate(&engine, "RefundType", "hr", &config, &tera, &project))
        .expect("UiE2eTestGenerator failed");

    let content = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with(".ux.test.ts"))
        .expect("ux.test.ts should be generated")
        .content
        .clone();

    insta::assert_snapshot!("canonical_ux_spec", content);
}
