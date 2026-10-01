use std::path::Path;

use codegraph::generate::template_engine;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{CodeList, EnumValue, PropertyNode, SchemaNode};

use crate::harness::run_routing_generators_with_parts;

/// Two-domain mock with a codelist: compensation's `PayRunType` has a child
/// entity `PayLineType` (via parent candidate) whose `gender` property
/// references the ingested `GenderCodeList`; common's `CodeType` references
/// no codelists.  The auto-discovered include path makes the routed DTO
/// generator emit `pub use crate::codelist::GenderCodeList;` in
/// `dto_included.rs`.
async fn workers_codelist_test_setup() -> (
    codegraph_core::mock::MockEngine,
    codegraph_config::DomainConfig,
    tera::Tera,
    tempfile::TempDir,
) {
    let pay_run = SchemaNode {
        namespace: None,
        schema_id: "compensation/json/PayRunType.json".to_string(),
        title: "PayRunType".to_string(),
        description: Some("A pay run".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("compensation".to_string()),
        rel_path: "compensation/json/PayRunType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "PayRun".to_string(),
        pg_table_name: "pay_run".to_string(),
        api_path_segment: "pay-runs".to_string(),
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
    let pay_line = SchemaNode {
        namespace: None,
        schema_id: "compensation/json/PayLineType.json".to_string(),
        title: "PayLineType".to_string(),
        description: Some("A pay run line item".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("compensation".to_string()),
        rel_path: "compensation/json/PayLineType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "PayLine".to_string(),
        pg_table_name: "pay_line".to_string(),
        api_path_segment: "pay-lines".to_string(),
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
    let code = SchemaNode {
        namespace: None,
        schema_id: "common/json/CodeType.json".to_string(),
        title: "CodeType".to_string(),
        description: Some("A code value".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("common".to_string()),
        rel_path: "common/json/CodeType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Code".to_string(),
        pg_table_name: "code".to_string(),
        api_path_segment: "codes".to_string(),
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
    let work_item = SchemaNode {
        namespace: None,
        schema_id: "compensation/json/WorkItemType.json".to_string(),
        title: "WorkItemType".to_string(),
        description: Some("A work item".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("compensation".to_string()),
        rel_path: "compensation/json/WorkItemType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "WorkItem".to_string(),
        pg_table_name: "work_item".to_string(),
        api_path_segment: "work-items".to_string(),
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

    let name_prop = PropertyNode {
        name: "name".to_string(),
        prop_type: "string".to_string(),
        description: Some("Name".to_string()),
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
        pg_column_name: "name".to_string(),
        pg_column_type: "TEXT".to_string(),
        rust_field_name: "name".to_string(),
        rust_field_type: "String".to_string(),
        sea_orm_type: "Text".to_string(),
        render_strategy: "direct_column".to_string(),
        ref_target: None,
        classification: Some("primitive_wrapper".to_string()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    };
    let gender_prop = PropertyNode {
        name: "gender".to_string(),
        prop_type: "string".to_string(),
        description: Some("Gender code".to_string()),
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
        pg_column_name: "gender".to_string(),
        pg_column_type: "TEXT".to_string(),
        rust_field_name: "gender".to_string(),
        rust_field_type: "String".to_string(),
        sea_orm_type: "Text".to_string(),
        render_strategy: "fk_lookup".to_string(),
        ref_target: Some("common/json/codelist/GenderCodeList.json".to_string()),
        classification: Some("codelist".to_string()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    };
    let pay_line_ref_prop = PropertyNode {
        name: "pay_line".to_string(),
        prop_type: "string".to_string(),
        description: Some("FK to a pay line".to_string()),
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
        pg_column_name: "pay_line_id".to_string(),
        pg_column_type: "UUID".to_string(),
        rust_field_name: "pay_line".to_string(),
        rust_field_type: "Option<Uuid>".to_string(),
        sea_orm_type: "Uuid".to_string(),
        render_strategy: "entity_reference".to_string(),
        ref_target: Some("compensation/json/PayLineType.json".to_string()),
        classification: Some("entity_reference".to_string()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    };
    let code_ref_prop = PropertyNode {
        name: "code".to_string(),
        prop_type: "string".to_string(),
        description: Some("FK to a code".to_string()),
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
        pg_column_name: "code_id".to_string(),
        pg_column_type: "UUID".to_string(),
        rust_field_name: "code".to_string(),
        rust_field_type: "Option<Uuid>".to_string(),
        sea_orm_type: "Uuid".to_string(),
        render_strategy: "entity_reference".to_string(),
        ref_target: Some("common/json/CodeType.json".to_string()),
        classification: Some("entity_reference".to_string()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    };
    let work_item_ref_prop = PropertyNode {
        name: "work_item".to_string(),
        prop_type: "string".to_string(),
        description: Some("FK to a work item".to_string()),
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
        pg_column_name: "work_item_id".to_string(),
        pg_column_type: "UUID".to_string(),
        rust_field_name: "work_item".to_string(),
        rust_field_type: "Option<Uuid>".to_string(),
        sea_orm_type: "Uuid".to_string(),
        render_strategy: "entity_reference".to_string(),
        ref_target: Some("compensation/json/WorkItemType.json".to_string()),
        classification: Some("entity_reference".to_string()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    };

    let engine = codegraph_core::mock::MockEngine::builder()
        .with_schema(pay_run)
        .with_schema(pay_line.clone())
        .with_schema(work_item.clone())
        .with_schema(code.clone())
        .with_properties("PayRunType", vec![name_prop.clone(), pay_line_ref_prop])
        .with_properties(
            "PayLineType",
            vec![
                name_prop.clone(),
                gender_prop,
                work_item_ref_prop,
                code_ref_prop,
            ],
        )
        .with_properties("WorkItemType", vec![name_prop])
        .with_properties("CodeType", vec![])
        .with_ref_target("pay_line", "PayRunType", pay_line)
        .with_ref_target("work_item", "PayLineType", work_item)
        .with_ref_target("code", "PayLineType", code)
        .build();

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
    engine
        .ingest_enum_value(
            "GenderCodeList",
            &EnumValue {
                value: "MALE".to_string(),
                display_name: Some("Male".to_string()),
                sort_order: 0,
            },
        )
        .await
        .unwrap();

    let config = codegraph_config::config::parse_domain_config_str(
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.compensation]
label = "Compensation"
schema_dir = "compensation"
postgres_schema = "compensation"
entities = ["PayRunType", "PayLineType", "WorkItemType"]
worker_name = "hr-payroll-worker"
hyperdrive_binding = "PAYROLL_DB"
depends_on = ["common"]

[domains.compensation.entity_config.PayRunType]
allow_include = ["pay_line.work_item", "pay_line.code"]

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["CodeType"]
cron_triggers = ["0 0 * * *"]
"#,
    )
    .unwrap();

    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = template_engine::create_tera(&template_dir).unwrap();

    let output_dir = tempfile::TempDir::new().unwrap();

    (engine, config, tera, output_dir)
}

/// In workers topology every worker crate gets its own `src/codelist/mod.rs`:
/// the compensation worker (whose DTO references `GenderCodeList`) re-exports
/// that enum from the domain-types crate, the common worker (no codelist
/// references) still gets a module file (worker `lib.rs` declares
/// `pub mod codelist;` unconditionally), and nothing codelist-related leaks
/// to the monolith root beyond the (empty) root scan.
#[tokio::test]
async fn workers_topology_emits_per_worker_codelist_reexports() {
    let (mock, config, tera, output_dir) = workers_codelist_test_setup().await;

    let project = codegraph::generate::ProjectConfig {
        deployment: codegraph::generate::DeploymentConfig {
            deployment_topology: codegraph::profile::DeploymentTopology::Workers,
        },
        ..Default::default()
    };
    let (report, output_dir) = run_routing_generators_with_parts(
        mock,
        config,
        tera,
        output_dir,
        codegraph::profile::DeploymentTopology::Workers,
        project,
    )
    .await;

    let paths: Vec<String> = report
        .files
        .iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();
    let any = |needle: &str| paths.iter().any(|p| p.contains(needle));

    // The routed DTO references the codelist enum...
    assert!(any(
        "/workers/compensation/src/domain/compensation/pay_run/dto_included.rs"
    ));
    let dto_included = report
        .files
        .iter()
        .find(|f| {
            f.path
                .ends_with("workers/compensation/src/domain/compensation/pay_run/dto_included.rs")
        })
        .expect("compensation dto_included.rs must be generated");
    assert!(
        dto_included
            .content
            .contains("pub use crate::codelist::GenderCodeList;"),
        "routed DTO must re-export the codelist enum from crate::codelist, got:\n{}",
        dto_included.content
    );

    // ...and the compensation worker's codelist module re-exports it from
    // the shared domain-types crate.
    let compensation_codelist = report
        .files
        .iter()
        .find(|f| f.path.ends_with("workers/compensation/src/codelist/mod.rs"))
        .expect("compensation worker must get src/codelist/mod.rs");
    assert!(
        compensation_codelist
            .content
            .contains("pub use domain_types::codelist::GenderCodeList;"),
        "compensation codelist mod must re-export GenderCodeList from domain_types, got:\n{}",
        compensation_codelist.content
    );
    assert!(
        !compensation_codelist.content.contains("::codelist::Code"),
        "compensation codelist mod must not re-export unrelated codelists"
    );

    // The common worker references no codelists but still gets a module file
    // (worker lib.rs/main.rs declare `pub mod codelist;` unconditionally).
    let common_codelist = report
        .files
        .iter()
        .find(|f| f.path.ends_with("workers/common/src/codelist/mod.rs"))
        .expect("common worker must get src/codelist/mod.rs");
    assert!(
        !common_codelist
            .content
            .contains("pub use domain_types::codelist::"),
        "common worker must not re-export codelists it never references, got:\n{}",
        common_codelist.content
    );

    // Worker crate roots declare the codelist module.
    let compensation_lib = report
        .files
        .iter()
        .find(|f| f.path.ends_with("workers/compensation/src/lib.rs"))
        .unwrap();
    assert!(
        compensation_lib.content.contains("pub mod codelist;"),
        "worker lib.rs must declare pub mod codelist;"
    );
    let compensation_main = report
        .files
        .iter()
        .find(|f| f.path.ends_with("workers/compensation/src/main.rs"))
        .unwrap();
    assert!(
        compensation_main.content.contains("mod codelist;"),
        "worker main.rs must declare mod codelist;"
    );

    // No worker-specific codelist re-exports may land at the monolith root:
    // the root scan in workers topology finds no routed code, so the root
    // mod.rs stays a header-only placeholder.
    let root_codelist = report
        .files
        .iter()
        .find(|f| {
            f.path
                == output_dir
                    .path()
                    .join("src")
                    .join("codelist")
                    .join("mod.rs")
        })
        .expect("root src/codelist/mod.rs must still be generated");
    assert!(
        !root_codelist
            .content
            .contains("pub use domain_types::codelist::"),
        "root codelist mod must not re-export worker-scoped codelists, got:\n{}",
        root_codelist.content
    );

    // Cross-domain include paths are dropped in workers topology: the
    // compensation worker's `pay_line.code` path targets the common worker's
    // CodeType, so no generated worker file may reference the common domain's
    // modules or entities (they live in a different worker crate).
    let cross_domain_refs: Vec<String> = report
        .files
        .iter()
        .filter(|f| {
            f.path.to_string_lossy().contains("/workers/compensation/")
                && (f.content.contains("crate::domain::common")
                    || f.content.contains("crate::entity::common_code"))
        })
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();
    assert!(
        cross_domain_refs.is_empty(),
        "workers topology must drop cross-domain include paths; offending files: {:?}",
        cross_domain_refs
    );

    println!(
        "workers codelist routing produced {} files",
        report.files.len()
    );
}

/// Monolith topology with the same codelist-bearing fixture keeps the
/// original single root `src/codelist/mod.rs` re-export behaviour and never
/// produces a workers/ tree.
#[tokio::test]
async fn monolith_topology_keeps_root_codelist_reexport() {
    let (mock, config, tera, output_dir) = workers_codelist_test_setup().await;

    let project = codegraph::generate::ProjectConfig {
        deployment: codegraph::generate::DeploymentConfig {
            deployment_topology: codegraph::profile::DeploymentTopology::Monolith,
        },
        ..Default::default()
    };
    let (report, output_dir) = run_routing_generators_with_parts(
        mock,
        config,
        tera,
        output_dir,
        codegraph::profile::DeploymentTopology::Monolith,
        project,
    )
    .await;

    let paths: Vec<String> = report
        .files
        .iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();
    assert!(
        !paths.iter().any(|p| p.contains("/workers/")),
        "monolith topology must not produce a workers/ directory"
    );

    let root_codelist = report
        .files
        .iter()
        .find(|f| {
            f.path
                == output_dir
                    .path()
                    .join("src")
                    .join("codelist")
                    .join("mod.rs")
        })
        .expect("monolith root src/codelist/mod.rs must be generated");
    assert!(
        root_codelist
            .content
            .contains("pub use domain_types::codelist::GenderCodeList;"),
        "monolith codelist mod must re-export GenderCodeList from domain_types, got:\n{}",
        root_codelist.content
    );

    // Monolith keeps cross-domain include paths (SQL joins across schemas on
    // the shared DB) — the pay_line.code path survives with its CodeResponse
    // import, unlike workers topology which drops it.
    let dto_included = report
        .files
        .iter()
        .find(|f| {
            f.path
                .to_string_lossy()
                .ends_with("src/domain/compensation/pay_run/dto_included.rs")
        })
        .expect("monolith dto_included.rs must be generated");
    assert!(
        dto_included.content.contains("CodeResponse"),
        "monolith must keep the cross-domain include path, got:\n{}",
        dto_included.content
    );
    assert!(
        dto_included
            .content
            .contains("pub use crate::codelist::GenderCodeList;"),
        "monolith dto_included must keep the codelist re-export, got:\n{}",
        dto_included.content
    );
}
