//! Issue #539: `refers` links must persist through create/update and hydrate
//! in responses on BOTH persistence providers.
//!
//! Before the fix the cornucopia path (workers topology) had no junction
//! writer at all: the DDL created the junction table, the create/update DTOs
//! accepted the ids, and every row silently dropped them while responses came
//! back `null`. These tests pin the emitted SQL, the adapter writes, the
//! SeaORM writes (regression guard), the nested contains-child contrast, and
//! the one-convention naming contract (create DTO field == response DTO field
//! == repository bind source).

use std::path::Path;

use codegraph_config::DomainConfig;
use codegraph_core::types::{PropertyNode, SchemaNode};
use codegraph_type_contracts::RefClassificationKind;
use tempfile::TempDir;

use crate::db::cornucopia_queries::CornucopiaQueryGenerator;
use crate::ddd::cornucopia_repo::CornucopiaRepoGenerator;

use crate::ddd::repository::RepositoryTraitGenerator;
use crate::profile::PersistenceProvider;
use crate::project_config::ProjectConfig;
use crate::template_engine::create_tera;
use crate::traits::EntityGenerator;

fn entity_schema(title: &str, table: &str, rust: &str) -> SchemaNode {
    SchemaNode {
        schema_id: format!("library/{}", title),
        title: title.to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: "entity".to_string(),
        domain: Some("library".to_string()),
        namespace: None,
        rel_path: format!("library/{}.json", title),
        pg_type: "TABLE".to_string(),
        rust_type: rust.to_string(),
        sea_orm_type: "Entity".to_string(),
        rust_type_name: rust.to_string(),
        pg_table_name: table.to_string(),
        api_path_segment: format!("{}s", table),
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

fn vo_schema(title: &str, table: &str, rust: &str) -> SchemaNode {
    let mut s = entity_schema(title, table, rust);
    s.is_entity = false;
    s.classification = "value_object".to_string();
    s.rust_type = rust.to_string();
    s
}

fn string_prop(name: &str, required: bool) -> PropertyNode {
    PropertyNode {
        name: name.to_string(),
        prop_type: "string".to_string(),
        description: None,
        format: None,
        is_required: required,
        is_nullable: !required,
        is_id: false,
        is_array: false,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: name.to_string(),
        pg_column_type: "TEXT".to_string(),
        rust_field_name: name.to_string(),
        rust_field_type: "String".to_string(),
        sea_orm_type: "text".to_string(),
        render_strategy: "primitive".to_string(),
        ref_target: None,
        classification: None,
        projection: None,
        classification_kind: Some(RefClassificationKind::PrimitiveWrapper),
        type_expr: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

/// Required array `refers`: junction semantics, raw field name (resolve_field
/// parity — array entity refs keep the raw name, the junction table is
/// `<parent>_<field>`).
fn required_array_ref_prop(name: &str, target: &str) -> PropertyNode {
    PropertyNode {
        name: name.to_string(),
        prop_type: "array".to_string(),
        description: None,
        format: None,
        is_required: true,
        is_nullable: false,
        is_id: false,
        is_array: true,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: name.to_string(),
        pg_column_type: String::new(),
        rust_field_name: name.to_string(),
        rust_field_type: target.to_string(),
        sea_orm_type: String::new(),
        render_strategy: "child_table".to_string(),
        ref_target: Some(target.to_string()),
        classification: Some("entity_reference".to_string()),
        projection: None,
        classification_kind: Some(RefClassificationKind::EntityReference),
        type_expr: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

/// Optional scalar `refers [0..1]`: FK column `<name>_id`, `_id`-suffixed
/// field name (resolve_field parity with the mox stored-FK projection).
fn optional_scalar_ref_prop(name: &str, target: &str) -> PropertyNode {
    PropertyNode {
        name: name.to_string(),
        prop_type: "object".to_string(),
        description: None,
        format: None,
        is_required: false,
        is_nullable: true,
        is_id: false,
        is_array: false,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: format!("{}_id", name),
        pg_column_type: "UUID".to_string(),
        rust_field_name: format!("{}_id", name),
        rust_field_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        render_strategy: "entity_reference".to_string(),
        ref_target: Some(target.to_string()),
        classification: Some("entity_reference".to_string()),
        projection: None,
        classification_kind: Some(RefClassificationKind::EntityReference),
        type_expr: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

/// Contained value-object array (`contains T[] field`): child table, writes
/// children — the working contrast the junction fix must not disturb.
fn vo_array_prop(name: &str, target: &str) -> PropertyNode {
    PropertyNode {
        name: name.to_string(),
        prop_type: "array".to_string(),
        description: None,
        format: None,
        is_required: false,
        is_nullable: true,
        is_id: false,
        is_array: true,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: name.to_string(),
        pg_column_type: String::new(),
        rust_field_name: name.to_string(),
        rust_field_type: target.to_string(),
        sea_orm_type: String::new(),
        render_strategy: "child_table".to_string(),
        ref_target: Some(target.to_string()),
        classification: Some("value_object".to_string()),
        projection: None,
        classification_kind: Some(RefClassificationKind::ValueObject),
        type_expr: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

fn config() -> DomainConfig {
    toml::from_str(
        r#"
[defaults]
operations = ["create", "read", "update", "list"]

[domains.library]
label = "Library"
schema_dir = "schemas/library"
postgres_schema = "library"
"#,
    )
    .expect("config parses")
}

fn tera() -> tera::Tera {
    create_tera(Path::new("")).expect("embedded templates")
}

/// FilingType with a REQUIRED array `refers PayRunType[] payRuns` (junction:
/// pay_run carries no back-ref column), an OPTIONAL scalar `refers WorkerType
/// worker` (FK column worker_id), and a contained VO array `contains
/// PaySlipType[] paySlips` — the issue #539 shape end to end.
async fn db_with_filing_model() -> codegraph_core::mock::MockEngine {
    let pay_run = entity_schema("PayRunType", "pay_run", "PayRun");
    let worker = entity_schema("WorkerType", "worker", "Worker");
    let pay_slip = vo_schema("PaySlipType", "filing_pay_slips", "PaySlip");
    codegraph_core::mock::MockEngine::builder()
        .with_schema(entity_schema("FilingType", "filing", "Filing"))
        .with_schema(pay_run.clone())
        .with_schema(worker.clone())
        .with_schema(pay_slip.clone())
        .with_ref_target("pay_runs", "FilingType", pay_run.clone())
        .with_ref_target("pay_slips", "FilingType", pay_slip)
        .with_properties(
            "FilingType",
            vec![
                string_prop("title", true),
                required_array_ref_prop("pay_runs", "PayRunType"),
                optional_scalar_ref_prop("worker", "WorkerType"),
                vo_array_prop("pay_slips", "PaySlipType"),
            ],
        )
        .with_properties("PayRunType", vec![string_prop("reference", true)])
        .with_properties("PaySlipType", vec![string_prop("amount", false)])
        .build()
}

async fn cornucopia_outputs(db: &codegraph_core::mock::MockEngine) -> (String, String) {
    let project = ProjectConfig {
        database: crate::project_config::DatabaseConfig {
            persistence_provider: PersistenceProvider::Cornucopia,
            ..Default::default()
        },
        ..Default::default()
    };
    let dir = TempDir::new().unwrap();
    let queries = CornucopiaQueryGenerator::new(dir.path())
        .generate(db, "FilingType", "library", &config(), &tera(), &project)
        .await
        .expect("cornucopia queries generate");
    let adapter = CornucopiaRepoGenerator::new(dir.path())
        .generate(db, "FilingType", "library", &config(), &tera(), &project)
        .await
        .expect("cornucopia adapter generates");
    (queries[0].content.clone(), adapter[0].content.clone())
}

// ── cornucopia: the silent-drop surface ────────────────────────────────

#[tokio::test]
async fn refers_links_persist_through_create_and_update() {
    let db = db_with_filing_model().await;
    let (sql, adapter) = cornucopia_outputs(&db).await;

    // Junction query surface exists (list/link/unlink on the junction table).
    assert!(
        sql.contains("--! list_filing_filing_pay_runs (filing_id) : (pay_run_id)"),
        "junction list query emitted: {sql}"
    );
    assert!(
        sql.contains("--! link_filing_filing_pay_runs (filing_id, pay_run_id)"),
        "junction link query emitted: {sql}"
    );
    assert!(
        sql.contains("--! unlink_filing_filing_pay_runs (filing_id)"),
        "junction unlink query emitted: {sql}"
    );

    // Create writes one junction row per accepted id.
    assert!(
        adapter.contains("for item in &cmd.pay_runs {"),
        "create iterates the junction DTO field: {adapter}"
    );
    assert!(
        adapter.contains("link_filing_filing_pay_runs().bind(tx, &id, item)"),
        "create links each id: {adapter}"
    );

    // Update replaces: unlink existing rows, re-link the carried ids.
    assert!(
        adapter.contains("if let Some(ref ids) = cmd.pay_runs {"),
        "update gates the replace on the carried field: {adapter}"
    );
    assert!(
        adapter.contains("unlink_filing_filing_pay_runs().bind(tx, &id)"),
        "update unlinks before re-linking: {adapter}"
    );

    // Responses hydrate the same field the create/update DTO accepts.
    assert!(
        adapter
            .contains("let pay_runs_rows: Vec<uuid::Uuid> = filing::list_filing_filing_pay_runs()"),
        "response reads junction ids: {adapter}"
    );
    // The junction list query selects one column, so cornucopia's row type
    // is the bare `uuid::Uuid` — the hydration collects the rows directly
    // (a `.map(|r| r.pay_run_id)` projection is an E0609 on the generated
    // crate; caught by the crewbase workers compile).
    assert!(
        adapter.contains(
            "list_filing_filing_pay_runs().bind(db, &row.id).all().await.map_err(|e| e.to_string())?.into_iter().collect();"
        ),
        "single-column junction rows collect directly (no struct projection): {adapter}"
    );
    assert!(
        adapter.contains("pay_runs: pay_runs_rows,"),
        "required junction field hydrates as the Vec: {adapter}"
    );
}

#[tokio::test]
async fn optional_scalar_ref_binds_fk_column_and_hydrates() {
    let db = db_with_filing_model().await;
    let (_, adapter) = cornucopia_outputs(&db).await;

    // Create/update write the FK column from the `_id`-suffixed DTO field.
    assert!(
        adapter.contains("cmd.worker_id"),
        "adapter binds the scalar-ref FK column: {adapter}"
    );
}

// ── SeaORM regression pins (worked before the fix — must keep working) ─

#[tokio::test]
async fn seaorm_junction_writes_and_scalar_bind_pinned() {
    let db = db_with_filing_model().await;
    let output_dir = TempDir::new().unwrap();
    let files = RepositoryTraitGenerator::new(output_dir.path())
        .generate(
            &db,
            "FilingType",
            "library",
            &config(),
            &tera(),
            &ProjectConfig::default(),
        )
        .await
        .expect("repository generates");
    let impl_src = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("repository_impl.rs"))
        .expect("impl file")
        .content
        .clone();

    assert!(
        impl_src.contains(
            "INSERT INTO library.filing_pay_runs (filing_id, pay_run_id) VALUES ($1, $2)"
        ),
        "create inserts junction rows: {impl_src}"
    );
    assert!(
        impl_src.contains("for item in &cmd.pay_runs {"),
        "create iterates the junction DTO field: {impl_src}"
    );
    assert!(
        impl_src.contains("DELETE FROM library.filing_pay_runs WHERE filing_id = $1"),
        "update replaces junction rows: {impl_src}"
    );
    assert!(
        impl_src.contains("if let Some(ref ids) = cmd.pay_runs {"),
        "update gates the replace on the carried field: {impl_src}"
    );
    assert!(
        impl_src.contains("worker_id: Set(cmd.worker_id)"),
        "scalar ref binds the FK column: {impl_src}"
    );
    assert!(
        impl_src.contains("pay_runs: pay_runs_rows,"),
        "required junction field hydrates as the Vec: {impl_src}"
    );
}

// ── nested contains-children still write (existing behavior, pinned) ───

#[tokio::test]
async fn nested_contains_children_still_write() {
    let db = db_with_filing_model().await;
    let (sql, adapter) = cornucopia_outputs(&db).await;

    assert!(
        sql.contains("--! insert_filing_filing_pay_slips ("),
        "VO child insert query emitted: {sql}"
    );
    assert!(
        adapter.contains("for item in &cmd.pay_slips {"),
        "create iterates the VO child field: {adapter}"
    );
    assert!(
        adapter.contains("insert_filing_filing_pay_slips().bind(tx,"),
        "create inserts VO child rows: {adapter}"
    );

    let output_dir = TempDir::new().unwrap();
    let files = RepositoryTraitGenerator::new(output_dir.path())
        .generate(
            &db,
            "FilingType",
            "library",
            &config(),
            &tera(),
            &ProjectConfig::default(),
        )
        .await
        .expect("repository generates");
    let impl_src = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("repository_impl.rs"))
        .expect("impl file")
        .content
        .clone();
    assert!(
        impl_src.contains("cmd.pay_slips"),
        "SeaORM create still writes VO children: {impl_src}"
    );
}

// ── one naming convention: create == response == repository bind ───────

#[tokio::test]
async fn ref_field_names_agree_across_create_response_and_repository() {
    let db = db_with_filing_model().await;

    let output_dir = TempDir::new().unwrap();
    let dto_files = crate::domain_types::dto::DomainTypesDtoGenerator::new_with_base(
        output_dir.path().to_path_buf(),
    )
    .generate(
        &db,
        "FilingType",
        "library",
        &config(),
        &tera(),
        &ProjectConfig::default(),
    )
    .await
    .expect("dtos generate");
    let find = |suffix: &str| {
        dto_files
            .iter()
            .find(|f| f.path.to_string_lossy().ends_with(suffix))
            .unwrap_or_else(|| panic!("{suffix} emitted"))
            .content
            .clone()
    };
    let create = find("dto_create.rs");
    let update = find("dto_update.rs");
    let response = find("dto_response.rs");

    // Junction field: `pay_runs` everywhere (never `pay_runs_ids`).
    assert!(
        create.contains("pub pay_runs: Vec<uuid::Uuid>,"),
        "create DTO carries the junction Vec field: {create}"
    );
    assert!(
        update.contains("pub pay_runs: Option<Vec<uuid::Uuid>>,"),
        "update DTO carries the optional junction Vec field: {update}"
    );
    assert!(
        response.contains("pub pay_runs:"),
        "response DTO exposes the same junction field name: {response}"
    );
    assert!(
        !create.contains("pay_runs_ids") && !response.contains("pay_runs_ids"),
        "no alternate spelling leaks into the DTO surface"
    );

    // Scalar ref: `worker_id` everywhere (never the bare model name).
    assert!(
        create.contains("pub worker_id: Option<uuid::Uuid>,"),
        "create DTO carries the _id-suffixed scalar field: {create}"
    );
    assert!(
        response.contains("pub worker_id: Option<uuid::Uuid>,"),
        "response DTO exposes the same scalar field name: {response}"
    );
    assert!(
        !create.contains("pub worker:") && !response.contains("pub worker:"),
        "the bare model name must not resurface as a DTO field"
    );

    // Repository bind sources use exactly those names (both providers).
    let (_, adapter) = cornucopia_outputs(&db).await;
    assert!(adapter.contains("cmd.pay_runs"));
    assert!(adapter.contains("cmd.worker_id"));
    let repo_dir = TempDir::new().unwrap();
    let repo_files = RepositoryTraitGenerator::new(repo_dir.path())
        .generate(
            &db,
            "FilingType",
            "library",
            &config(),
            &tera(),
            &ProjectConfig::default(),
        )
        .await
        .expect("repository generates");
    let impl_src = repo_files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("repository_impl.rs"))
        .expect("impl file")
        .content
        .clone();
    assert!(impl_src.contains("cmd.pay_runs"));
    assert!(impl_src.contains("cmd.worker_id"));
}

#[tokio::test]
async fn request_dtos_reject_unknown_fields_responses_stay_lax() {
    let db = db_with_filing_model().await;

    let output_dir = TempDir::new().unwrap();
    let dto_files = crate::domain_types::dto::DomainTypesDtoGenerator::new_with_base(
        output_dir.path().to_path_buf(),
    )
    .generate(
        &db,
        "FilingType",
        "library",
        &config(),
        &tera(),
        &ProjectConfig::default(),
    )
    .await
    .expect("dtos generate");
    let find = |suffix: &str| {
        dto_files
            .iter()
            .find(|f| f.path.to_string_lossy().ends_with(suffix))
            .unwrap_or_else(|| panic!("{suffix} emitted"))
            .content
            .clone()
    };

    // A key the struct does not carry (e.g. the array-shaped name for a
    // scalar `_id` field) must be a 422, never a silent serde ignore.
    assert!(
        find("dto_create.rs").contains("#[serde(deny_unknown_fields)]"),
        "create request rejects unknown fields"
    );
    assert!(
        find("dto_update.rs").contains("#[serde(deny_unknown_fields)]"),
        "update request rejects unknown fields"
    );
    assert!(
        !find("dto_response.rs").contains("deny_unknown_fields"),
        "responses stay lax (clients ignore extra response keys)"
    );
}

/// A `refers` array whose target cannot resolve (dangling alias, broken
/// $ref) emits NO DTO field: accepting ids no query would ever write is the
/// #539 silent-drop shape. Both planes skip together.
#[tokio::test]
async fn dangling_ref_target_emits_no_junction_field() {
    let pay_run = entity_schema("PayRunType", "pay_run", "PayRun");
    let db = codegraph_core::mock::MockEngine::builder()
        .with_schema(entity_schema("FilingType", "filing", "Filing"))
        .with_schema(pay_run)
        // NOTE: no with_ref_target — the `pay_runs` ref dangles.
        .with_properties(
            "FilingType",
            vec![
                string_prop("title", true),
                required_array_ref_prop("pay_runs", "PayRunType"),
            ],
        )
        .build();

    let output_dir = TempDir::new().unwrap();
    let dto_files = crate::domain_types::dto::DomainTypesDtoGenerator::new_with_base(
        output_dir.path().to_path_buf(),
    )
    .generate(
        &db,
        "FilingType",
        "library",
        &config(),
        &tera(),
        &ProjectConfig::default(),
    )
    .await
    .expect("dtos generate");
    let create = dto_files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("dto_create.rs"))
        .expect("dto_create emitted")
        .content
        .clone();
    let response = dto_files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("dto_response.rs"))
        .expect("dto_response emitted")
        .content
        .clone();

    assert!(
        !create.contains("pub pay_runs"),
        "create DTO must not accept a ref field nothing can write: {create}"
    );
    assert!(
        !response.contains("pub pay_runs"),
        "response DTO must not expose a ref field nothing can hydrate: {response}"
    );
}
