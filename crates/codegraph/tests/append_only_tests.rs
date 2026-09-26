//! Append-only snapshot entities (issue #284, CDM TradeState pattern).
//!
//! `entity_config.append_only = true` (or the same operations inference the
//! scaffold grants use) must produce insert-only persistence: no updated_at
//! column, no soft-delete audit band, no BEFORE UPDATE trigger, INSERT-only
//! domain event trigger, and SELECT/INSERT-only grants — on the entity table
//! AND its child tables. Entities without the flag emit byte-identical
//! output to pre-#284 (the existing suites pin that; a control entity is
//! asserted here too).

use std::path::Path;

use codegraph::generate::db::ddl::DdlGenerator;
use codegraph::generate::template_engine::create_tera;
use codegraph::generate::traits::EntityGenerator;
use codegraph::generate::ProjectConfig;
use codegraph_config::config::{parse_domain_config_str, DomainConfig};
use codegraph_core::mock::MockEngine;
use codegraph_core::types::{
    ColumnInfo, CompositionNode, CompositionTree, FkDirection, SchemaNode,
};
use codegraph_type_contracts::RefClassificationKind;

fn schema(title: &str, table: &str) -> SchemaNode {
    SchemaNode {
        namespace: None,
        schema_id: format!("{title}.json"),
        title: title.to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: "entity".to_string(),
        domain: Some("recruiting".to_string()),
        rel_path: format!("recruiting/json/{title}.json"),
        pg_type: "entity".to_string(),
        rust_type: title.to_string(),
        sea_orm_type: "Entity".to_string(),
        rust_type_name: title.trim_end_matches("Type").to_string(),
        pg_table_name: table.to_string(),
        api_path_segment: table.to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
    }
}

fn column(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.to_string(),
        description: None,
        rust_type: "String".to_string(),
        postgres_type: "TEXT".to_string(),
        is_optional: false,
        is_codelist_fk: false,
        composite_columns: vec![],
        is_array: false,
        classification: Some(RefClassificationKind::PrimitiveWrapper),
        fk_target: None,
        check_values: vec![],
    }
}

/// Parent entity with one VO child table (the reset_history shape).
fn engine_with_tree(parent: &str, parent_table: &str, child_field: &str) -> MockEngine {
    let child_table = format!("{parent_table}_{child_field}");
    let tree = CompositionTree {
        root: CompositionNode {
            field_name: parent_table.to_string(),
            schema_title: parent.to_string(),
            table_schema: "recruiting".to_string(),
            table_name: parent_table.to_string(),
            fk: None,
            is_collection: false,
            columns: vec![column("given_name")],
            jsonb_columns: vec![],
            children: vec![CompositionNode {
                field_name: child_field.to_string(),
                schema_title: parent.to_string(),
                table_schema: "recruiting".to_string(),
                table_name: child_table.clone(),
                fk: Some(FkDirection::OnParent {
                    column: format!("{parent_table}_id"),
                }),
                is_collection: true,
                columns: vec![column("observation")],
                jsonb_columns: vec![],
                children: vec![],
                composite_range: None,
                consumed_fields: Vec::new(),
            }],
            composite_range: None,
            consumed_fields: Vec::new(),
        },
    };
    MockEngine::builder()
        .with_schema(schema(parent, parent_table))
        .with_composition_tree(parent, tree)
        .build()
}

fn config_with_entity_config(entity: &str, extra: &str) -> DomainConfig {
    parse_domain_config_str(&format!(
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"
entities = ["{entity}"]

[domains.recruiting.entity_config.{entity}]
{extra}
"#
    ))
    .unwrap()
}

async fn generate_ddl(
    engine: &MockEngine,
    config: &DomainConfig,
) -> Vec<codegraph::generate::traits::GeneratedFile> {
    // create_tera uses the templates embedded by codegraph-generate's
    // build.rs; the path argument is ignored (signature compat).
    let tera = create_tera(Path::new("")).unwrap();
    let gen = DdlGenerator::new(Path::new("/tmp/append-only-test"));
    gen.generate(
        engine,
        "SnapshotType",
        "recruiting",
        config,
        &tera,
        &ProjectConfig::default(),
    )
    .await
    .expect("DDL generation")
}

fn table_sql(files: &[codegraph::generate::traits::GeneratedFile]) -> String {
    files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("recruiting_snapshot.sql"))
        .expect("table SQL file")
        .content
        .clone()
}

// ── Config validation ───────────────────────────────────────────────────

#[test]
fn append_only_flag_parses() {
    let config = config_with_entity_config(
        "SnapshotType",
        "append_only = true\noperations = [\"create\", \"read\", \"list\"]",
    );
    let ec = config.domains["recruiting"]
        .get_entity_config("SnapshotType")
        .unwrap();
    assert!(ec.is_append_only());
}

#[test]
fn append_only_with_explicit_update_operation_is_a_parse_error() {
    let err = parse_domain_config_str(
        r#"
[defaults]
operations = ["create", "read", "list"]

[domains.recruiting]
label = "R"
schema_dir = "recruiting"
postgres_schema = "recruiting"

[domains.recruiting.entity_config.SnapshotType]
append_only = true
operations = ["create", "update"]
"#,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("append_only"),
        "error names the append_only conflict: {err}"
    );
}

#[test]
fn append_only_inheriting_update_from_defaults_is_a_parse_error() {
    let err = parse_domain_config_str(
        r#"
[defaults]
operations = ["create", "read", "update", "list"]

[domains.recruiting]
label = "R"
schema_dir = "recruiting"
postgres_schema = "recruiting"

[domains.recruiting.entity_config.SnapshotType]
append_only = true
"#,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("update"),
        "effective-operations conflict surfaces the offending op: {err}"
    );
}

#[test]
fn append_only_with_narrowed_operations_parses() {
    let config = config_with_entity_config(
        "SnapshotType",
        "append_only = true\noperations = [\"create\", \"read\", \"list\"]",
    );
    assert!(config.domains["recruiting"]
        .get_entity_config("SnapshotType")
        .unwrap()
        .is_append_only());
}

// ── DDL emission ────────────────────────────────────────────────────────

#[tokio::test]
async fn append_only_ddl_is_insert_only_including_child_tables() {
    let engine = engine_with_tree("SnapshotType", "snapshot", "reset_history");
    let config = config_with_entity_config(
        "SnapshotType",
        "append_only = true\noperations = [\"create\", \"read\", \"list\"]",
    );
    let files = generate_ddl(&engine, &config).await;
    let sql = table_sql(&files);

    // Main table: created_at survives, updated_at and the audit band go.
    assert!(sql.contains("created_at TIMESTAMPTZ NOT NULL DEFAULT now()"));
    assert!(
        !sql.contains("updated_at"),
        "append-only main table has no updated_at: {sql}"
    );
    assert!(!sql.contains("deleted_at"), "no soft-delete band");
    assert!(
        sql.contains("GRANT SELECT, INSERT ON TABLE recruiting.snapshot TO app_user"),
        "main grant narrowed: {sql}"
    );
    assert!(!sql.contains("GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE recruiting.snapshot"));

    // Child table inherits: created_at only, narrowed grant.
    assert!(sql.contains("GRANT SELECT, INSERT ON TABLE recruiting.snapshot_reset_history"));
    assert!(!sql.contains(
        "GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE recruiting.snapshot_reset_history"
    ));
    let child_section = sql
        .split("Child table: recruiting.snapshot_reset_history")
        .nth(1)
        .expect("child section");
    assert!(child_section.contains("created_at TIMESTAMPTZ NOT NULL DEFAULT now()"));
    assert!(
        !child_section.contains("updated_at"),
        "child table has no updated_at: {child_section}"
    );

    // Triggers: the BEFORE UPDATE trigger file is gone; the domain event
    // trigger fires INSERT-only.
    let updated_at_trigger = format!(
        "{schema}_{table}_trigger.sql",
        schema = "recruiting",
        table = "snapshot"
    );
    assert!(
        !files.iter().any(|f| f
            .path
            .file_name()
            .is_some_and(|n| n.to_string_lossy() == updated_at_trigger)),
        "no updated_at trigger file: {:?}",
        files
            .iter()
            .map(|f| f.path.to_string_lossy())
            .collect::<Vec<_>>()
    );
    let event = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("_event_trigger.sql"))
        .expect("event trigger file");
    assert!(
        event
            .content
            .contains("AFTER INSERT ON recruiting.snapshot")
            && !event.content.contains("AFTER INSERT OR UPDATE OR DELETE"),
        "event trigger is INSERT-only: {}",
        event.content
    );
}

#[tokio::test]
async fn append_only_infers_from_operations_without_the_explicit_flag() {
    let engine = engine_with_tree("SnapshotType", "snapshot", "reset_history");
    let config = config_with_entity_config("SnapshotType", "operations = [\"create\", \"read\"]");
    let files = generate_ddl(&engine, &config).await;
    let sql = table_sql(&files);
    assert!(sql.contains("GRANT SELECT, INSERT ON TABLE recruiting.snapshot"));
    assert!(!sql.contains("updated_at"));
}

#[tokio::test]
async fn non_append_only_entity_emits_the_full_crud_surface() {
    let engine = engine_with_tree("SnapshotType", "snapshot", "reset_history");
    let config = config_with_entity_config("SnapshotType", "");
    let files = generate_ddl(&engine, &config).await;
    let sql = table_sql(&files);

    assert!(sql.contains("updated_at TIMESTAMPTZ NOT NULL DEFAULT now()"));
    assert!(sql.contains("GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE recruiting.snapshot"));
    assert!(sql.contains(
        "GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE recruiting.snapshot_reset_history"
    ));
    assert!(
        files.iter().any(|f| f
            .path
            .file_name()
            .is_some_and(|n| n.to_string_lossy() == "recruiting_snapshot_trigger.sql")),
        "updated_at trigger still emitted for mutable entities"
    );
    let event = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("_event_trigger.sql"))
        .expect("event trigger file");
    assert!(event.content.contains("AFTER INSERT OR UPDATE OR DELETE"));
}
