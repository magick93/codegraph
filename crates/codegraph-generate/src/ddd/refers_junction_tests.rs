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
///
/// Also carries the issue #554 crewbase shapes: TaxCalculation with a scalar
/// `refers PayRunType pay_run` (writable as `pay_run_id`), and
/// PayRunFilingType with a junction NAMED `pay_run` (`refers PayRunType[]
/// pay_run` — the singular-named junction whose `_id` spelling is the
/// near-miss callers actually send).
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
        .with_ref_target("pay_run", "PayRunFilingType", pay_run.clone())
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
        .with_schema(entity_schema(
            "TaxCalculationType",
            "tax_calculation",
            "TaxCalculation",
        ))
        .with_properties(
            "TaxCalculationType",
            vec![
                string_prop("amount", true),
                optional_scalar_ref_prop("pay_run", "PayRunType"),
            ],
        )
        .with_schema(entity_schema(
            "PayRunFilingType",
            "pay_run_filing",
            "PayRunFiling",
        ))
        .with_properties(
            "PayRunFilingType",
            vec![
                string_prop("external_ref", true),
                required_array_ref_prop("pay_run", "PayRunType"),
            ],
        )
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

// ── issue #554: near-miss rejection, junction alias, naming sweep ──────

/// Render `dto_create.rs` for `title` under `project`. The two utoipa tokens
/// are stripped for the scratch crate (ToSchema is not the behavior under
/// test; every serde attribute stays verbatim).
async fn rendered_create_dto(
    db: &codegraph_core::mock::MockEngine,
    title: &str,
    project: &ProjectConfig,
) -> String {
    rendered_dto(db, title, project, "dto_create.rs").await
}

/// Render `dto_update.rs` for `title` under `project` (same utoipa strip).
async fn rendered_update_dto(
    db: &codegraph_core::mock::MockEngine,
    title: &str,
    project: &ProjectConfig,
) -> String {
    rendered_dto(db, title, project, "dto_update.rs").await
}

async fn rendered_dto(
    db: &codegraph_core::mock::MockEngine,
    title: &str,
    project: &ProjectConfig,
    file: &str,
) -> String {
    let output_dir = TempDir::new().unwrap();
    let dto_files = crate::domain_types::dto::DomainTypesDtoGenerator::new_with_base(
        output_dir.path().to_path_buf(),
    )
    .generate(db, title, "library", &config(), &tera(), project)
    .await
    .expect("dtos generate");
    let source = dto_files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with(file))
        .unwrap_or_else(|| panic!("{file} emitted for {title}"))
        .content
        .clone();
    source
        .replace("use utoipa::ToSchema;\n", "")
        .replace(", ToSchema", "")
}

/// Build a scratch crate whose lib embeds `(module_name, dto_source)` pairs
/// verbatim and run the KAT `test_source` inside it. The scratch deps pin
/// exactly what the DTO surface needs (serde + uuid); cargo runs with an
/// isolated target dir, node-free and DB-free.
fn run_serde_kat(modules: &[(&str, &str)], test_source: &str) {
    let scratch = TempDir::new().unwrap();
    let mut kat = String::from("//! DTO serde-behavior KAT (issue #554).\n");
    for (module, source) in modules {
        kat.push_str(&format!("\npub mod {module} {{\n{source}\n}}\n"));
    }
    kat.push_str("\n#[cfg(test)]\nmod kat {\n    use super::*;\n\n");
    kat.push_str("    const ID: &str = \"01933e2c-0000-7000-8000-000000000001\";\n\n");
    kat.push_str(test_source);
    kat.push_str("\n}\n");

    std::fs::write(
        scratch.path().join("Cargo.toml"),
        r#"[package]
name = "dto-ref-name-kat"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
path = "kat.rs"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["serde"] }
garde = { version = "0.22", features = ["derive"] }
"#,
    )
    .unwrap();
    std::fs::write(scratch.path().join("kat.rs"), kat).unwrap();

    let out = std::process::Command::new("cargo")
        .arg("test")
        .arg("--manifest-path")
        .arg(scratch.path().join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", scratch.path().join("target"))
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("cargo test must run for the DTO serde KAT scratch crate");
    assert!(
        out.status.success(),
        "the emitted DTO must satisfy the serde behavior contract:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// The #554 TDD mandate: a wrong-named ref field (the bare model name for a
/// `_id`-spelled scalar ref, or a truly unknown key) must be a serde ERROR —
/// the 4xx the handler maps — whose message names the accepted spelling,
/// never a silent serde ignore that lands the link as NULL.
#[tokio::test]
async fn create_rejects_unknown_and_near_miss_field_names() {
    let db = db_with_filing_model().await;
    let project = ProjectConfig::default();
    let filing = rendered_create_dto(&db, "FilingType", &project).await;
    let tax = rendered_create_dto(&db, "TaxCalculationType", &project).await;

    let kat = r##"
    #[test]
    fn near_miss_scalar_ref_is_rejected_naming_the_accepted_spelling() {
        // The bare model name where the DTO carries `worker_id`.
        let body = format!(r#"{{"title":"t","pay_runs":["{ID}"],"worker":"{ID}"}}"#);
        let err = serde_json::from_str::<filing::CreateFilingRequest>(&body).unwrap_err();
        assert!(
            err.to_string().contains("worker_id"),
            "the accepted spelling must be named in the error: {err}"
        );
        // The cross-entity near-miss from the issue: `pay_run` where
        // TaxCalculation carries `pay_run_id`.
        let body = format!(r#"{{"amount":"1","pay_run":"{ID}"}}"#);
        let err = serde_json::from_str::<tax::CreateTaxCalculationRequest>(&body).unwrap_err();
        assert!(
            err.to_string().contains("pay_run_id"),
            "the accepted spelling must be named in the error: {err}"
        );
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let err = serde_json::from_str::<filing::CreateFilingRequest>(
            r#"{"title":"t","pay_runs":[],"bogus":1}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("bogus"), "{err}");
    }
"##;
    run_serde_kat(&[("filing", &filing), ("tax", &tax)], kat);
}

/// Requirement 2 of the issue: where the Vec (junction) form stays, a serde
/// alias covers both spellings — the `{name}_id` spelling lands on the same
/// canonical field (create AND update), under snake AND camel key casing.
/// Scalar `_id` refs stay alias-free: their bare near-miss is REJECTED
/// (pinned by `create_rejects_unknown_and_near_miss_field_names`).
#[tokio::test]
async fn junction_create_accepts_the_scalar_id_spelling_via_serde_alias() {
    let db = db_with_filing_model().await;
    let snake = ProjectConfig::default();
    let camel = ProjectConfig {
        codegen: crate::project_config::CodegenConfig {
            dto_key_casing: crate::project_config::DtoKeyCasing::Camel,
            ..Default::default()
        },
        ..Default::default()
    };
    // The generated dto dir nests create/update as sibling modules (the
    // update file imports children via `use super::dto_create::…`), so the
    // KAT embeds both under the generated module structure to compile the
    // update surface (stacked `alias` + `default` serde attrs) for real.
    let nest = |create: String, update: String| {
        format!("pub mod dto_create {{\n{create}\n}}\npub mod dto_update {{\n{update}\n}}")
    };
    let filing_snake = nest(
        rendered_create_dto(&db, "FilingType", &snake).await,
        rendered_update_dto(&db, "FilingType", &snake).await,
    );
    let pay_run_filing = nest(
        rendered_create_dto(&db, "PayRunFilingType", &snake).await,
        rendered_update_dto(&db, "PayRunFilingType", &snake).await,
    );
    let filing_camel = nest(
        rendered_create_dto(&db, "FilingType", &camel).await,
        rendered_update_dto(&db, "FilingType", &camel).await,
    );

    let kat = r##"
    #[test]
    fn junction_accepts_both_spellings_on_the_canonical_field() {
        // Canonical Vec spelling still works.
        let dto: filing::dto_create::CreateFilingRequest =
            serde_json::from_str(&format!(r#"{{"title":"t","pay_runs":["{ID}"]}}"#)).unwrap();
        assert_eq!(dto.pay_runs.len(), 1);
        // The `{name}_id` spelling aliases onto the same field.
        let dto: filing::dto_create::CreateFilingRequest =
            serde_json::from_str(&format!(r#"{{"title":"t","pay_runs_id":["{ID}"]}}"#)).unwrap();
        assert_eq!(dto.pay_runs.len(), 1);
        // The crewbase shape: a junction NAMED `pay_run` accepts `pay_run_id`
        // — the exact near-miss from the issue.
        let dto: pay_run_filing::dto_create::CreatePayRunFilingRequest =
            serde_json::from_str(&format!(r#"{{"external_ref":"x","pay_run_id":["{ID}"]}}"#))
                .unwrap();
        assert_eq!(dto.pay_run.len(), 1);
    }

    #[test]
    fn update_dto_aliases_onto_the_canonical_field_too() {
        let dto: filing::dto_update::UpdateFilingRequest =
            serde_json::from_str(&format!(r#"{{"pay_runs_id":["{ID}"]}}"#)).unwrap();
        assert_eq!(dto.pay_runs.unwrap().len(), 1);
        let dto: filing::dto_update::UpdateFilingRequest =
            serde_json::from_str(&format!(r#"{{"pay_runs":["{ID}"]}}"#)).unwrap();
        assert_eq!(dto.pay_runs.unwrap().len(), 1);
    }

    #[test]
    fn camel_casing_alias_follows_the_dto_key_casing() {
        let dto: filing_camel::dto_create::CreateFilingRequest =
            serde_json::from_str(&format!(r#"{{"title":"t","payRunsId":["{ID}"]}}"#)).unwrap();
        assert_eq!(dto.pay_runs.len(), 1);
        let dto: filing_camel::dto_update::UpdateFilingRequest =
            serde_json::from_str(&format!(r#"{{"payRunsId":["{ID}"]}}"#)).unwrap();
        assert_eq!(dto.pay_runs.unwrap().len(), 1);
    }
"##;
    run_serde_kat(
        &[
            ("filing", &filing_snake),
            ("pay_run_filing", &pay_run_filing),
            ("filing_camel", &filing_camel),
        ],
        kat,
    );
}

/// The #554 TDD mandate: for EVERY entity, each entity-ref field the create
/// DTO accepts must surface under the SAME name in the update and response
/// DTOs — one ref-naming convention across request/response.
#[tokio::test]
async fn request_and_response_ref_names_match() {
    let db = db_with_filing_model().await;
    let cfg = config();
    let project = ProjectConfig::default();

    for (title, ref_names) in [
        ("FilingType", vec!["pay_runs", "worker_id"]),
        ("TaxCalculationType", vec!["pay_run_id"]),
        ("PayRunFilingType", vec!["pay_run"]),
        ("PayRunType", vec![]),
        ("WorkerType", vec![]),
    ] {
        let ctx = crate::ddd::dto::build_dto_context(&db, title, "library", &cfg)
            .await
            .unwrap_or_else(|e| panic!("{title}: dto context builds: {e}"));
        let ctx_refs: Vec<&str> = ctx
            .fields
            .iter()
            .filter(|f| f.is_entity_ref && f.name != "id")
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(
            ctx_refs, ref_names,
            "{title}: the context carries exactly the modeled ref fields"
        );

        let output_dir = TempDir::new().unwrap();
        let dto_files = crate::domain_types::dto::DomainTypesDtoGenerator::new_with_base(
            output_dir.path().to_path_buf(),
        )
        .generate(&db, title, "library", &cfg, &tera(), &project)
        .await
        .unwrap_or_else(|e| panic!("{title}: dtos generate: {e}"));
        let find = |suffix: &str| {
            dto_files
                .iter()
                .find(|f| f.path.to_string_lossy().ends_with(suffix))
                .unwrap_or_else(|| panic!("{title}: {suffix} emitted"))
                .content
                .clone()
        };
        let create = find("dto_create.rs");
        let update = find("dto_update.rs");
        let response = find("dto_response.rs");

        for name in &ref_names {
            assert!(
                create.contains(&format!("pub {name}:")),
                "{title}: create DTO carries ref field `{name}`: {create}"
            );
            assert!(
                update.contains(&format!("pub {name}:")),
                "{title}: update DTO carries ref field `{name}`: {update}"
            );
            assert!(
                response.contains(&format!("pub {name}:")),
                "{title}: response DTO carries ref field `{name}`: {response}"
            );
        }
    }
}
