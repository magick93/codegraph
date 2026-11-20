//! Design-driven generation tests (issue #449 Phase 3): the repository
//! trait, the SeaORM/cornucopia repository impls, and the DTO gating when a
//! `.ddd` design covers the entity — plus the byte-identity guard that no
//! designs in the graph means unchanged output.

use std::path::Path;

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{
    DddApplicationNode, DddDesignFlags, DddDesignNode, DddModelGraph, DddModuleNode, DddParam,
    DddRepositoryNode, DddRepositoryOperation, DddSearchField, DddSearchNode, PropertyNode,
    SchemaNode,
};
use codegraph_type_contracts::RefClassificationKind;
use tempfile::TempDir;

use crate::ddd::command::CommandGenerator;
use crate::ddd::dto::DtoGenerator;
use crate::ddd::query::QueryGenerator;
use crate::ddd::repository::RepositoryTraitGenerator;
use crate::profile::PersistenceProvider;
use crate::project_config::ProjectConfig;
use crate::template_engine::create_tera;
use crate::traits::EntityGenerator;
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

/// The designed entity: title `BookType` (resolved_title), rust type `Book`,
/// table `book`, domain `library`, with `title` and `status` text columns.
fn book_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "library/BookType".to_string(),
        title: "BookType".to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: "entity".to_string(),
        domain: Some("library".to_string()),
        namespace: None,
        rel_path: "library/BookType.json".to_string(),
        pg_type: "TABLE".to_string(),
        rust_type: "Book".to_string(),
        sea_orm_type: "Entity".to_string(),
        rust_type_name: "Book".to_string(),
        pg_table_name: "book".to_string(),
        api_path_segment: "books".to_string(),
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

fn builtin(name: &str, op: &str, ordinal: usize) -> DddRepositoryOperation {
    DddRepositoryOperation {
        name: name.to_string(),
        builtin: Some(op.to_string()),
        return_type: None,
        return_multiplicity: None,
        params: Vec::new(),
        ordinal,
        is_protected: false,
        query_json: None,
        capabilities: Vec::new(),
    }
}

fn declared(
    name: &str,
    return_type: serde_json::Value,
    return_multiplicity: Option<serde_json::Value>,
    params: Vec<DddParam>,
    ordinal: usize,
) -> DddRepositoryOperation {
    DddRepositoryOperation {
        name: name.to_string(),
        builtin: None,
        return_type: Some(return_type),
        return_multiplicity,
        params,
        ordinal,
        is_protected: false,
        query_json: None,
        capabilities: Vec::new(),
    }
}

fn string_param(name: &str) -> DddParam {
    DddParam {
        name: name.to_string(),
        type_json: serde_json::json!({"type": "primitive", "value": "string"}),
        multiplicity: None,
    }
}

fn design(class: &str, title: Option<&str>, flags: DddDesignFlags) -> DddDesignNode {
    DddDesignNode {
        application: "Library".to_string(),
        module: "media".to_string(),
        class: class.to_string(),
        resolved_title: title.map(|t| t.to_string()),
        stereotype: "entity".to_string(),
        is_abstract: false,
        flags,
        machine_json: None,
        child_operations: Vec::new(),
        aggregate_boundary: None,
        invariants: Vec::new(),
        ordinal: 0,
    }
}

fn repository(name: &str, class: &str, ops: Vec<DddRepositoryOperation>) -> DddRepositoryNode {
    DddRepositoryNode {
        application: "Library".to_string(),
        name: name.to_string(),
        design_class: class.to_string(),
        operations: ops,
    }
}

fn model(
    designs: Vec<DddDesignNode>,
    repositories: Vec<DddRepositoryNode>,
    searches: Vec<DddSearchNode>,
) -> DddModelGraph {
    DddModelGraph {
        source_path: "library.ddd".to_string(),
        application: DddApplicationNode {
            name: "Library".to_string(),
            base: Some("nz.example.library".to_string()),
            source_path: "library.ddd".to_string(),
        },
        modules: vec![DddModuleNode {
            application: "Library".to_string(),
            name: "media".to_string(),
            ordinal: 0,
        }],
        designs,
        repositories,
        services: Vec::new(),
        searches,
    }
}

fn config() -> DomainConfig {
    toml::from_str(
        r#"
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

async fn db_with_model(model: Option<DddModelGraph>) -> codegraph_core::mock::MockEngine {
    let db = codegraph_core::mock::MockEngine::builder()
        .with_schema(book_schema())
        .with_properties(
            "BookType",
            vec![string_prop("title", true), string_prop("status", false)],
        )
        .build();
    if let Some(model) = model {
        db.ingest_ddd_model(&model).await.expect("ingest ddd model");
    }
    db
}

async fn render_trait(db: &codegraph_core::mock::MockEngine, project: &ProjectConfig) -> String {
    try_render_trait(db, project)
        .await
        .expect("repository trait generates")
}

/// The fallible trait render — the DD9 query plane's refusals surface here
/// as generation errors.
async fn try_render_trait(
    db: &codegraph_core::mock::MockEngine,
    project: &ProjectConfig,
) -> codegraph_core::error::Result<String> {
    let dir = TempDir::new().unwrap();
    let generator = RepositoryTraitGenerator::new(dir.path());
    let files = generator
        .generate(db, "BookType", "library", &config(), &tera(), project)
        .await?;
    Ok(files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("repository.rs"))
        .map(|f| f.content.clone())
        .unwrap_or_default())
}

async fn render_impl(db: &codegraph_core::mock::MockEngine) -> String {
    let dir = TempDir::new().unwrap();
    let project = ProjectConfig::default();
    let generator = RepositoryTraitGenerator::new(dir.path());
    let files = generator
        .generate(db, "BookType", "library", &config(), &tera(), &project)
        .await
        .expect("repository generates");
    files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("repository_impl.rs"))
        .map(|f| f.content.clone())
        .expect("repository_impl.rs emitted")
}

// ── repository trait: operations override ──────────────────────────────

#[tokio::test]
async fn repository_trait_design_with_read_only_builtins_has_no_create_delete() {
    let db = db_with_model(Some(model(
        vec![design("Book", Some("BookType"), DddDesignFlags::default())],
        vec![repository(
            "BookRepository",
            "Book",
            vec![
                builtin("findById", "findById", 0),
                builtin("findAll", "findAll", 1),
            ],
        )],
        vec![],
    )))
    .await;
    let trait_src = render_trait(&db, &ProjectConfig::default()).await;
    assert!(!trait_src.contains("async fn create("));
    assert!(!trait_src.contains("async fn delete("));
    assert!(!trait_src.contains("async fn update("));
    assert!(trait_src.contains("async fn find_by_id("));
    assert!(trait_src.contains("async fn list("));
}

#[tokio::test]
async fn builtin_query_finders_lower_with_consumer_signatures() {
    // findByKeys / findByExample (pherolang #47) lower onto the finder plane
    // with the consumer-known signatures: the surrogate key for an entity
    // with no explicit id feature, and an Option field per stored scalar.
    let db = db_with_model(Some(model(
        vec![design("Book", Some("BookType"), DddDesignFlags::default())],
        vec![repository(
            "BookRepository",
            "Book",
            vec![
                builtin("findByKeys", "findByKeys", 0),
                builtin("findByExample", "findByExample", 1),
            ],
        )],
        vec![],
    )))
    .await;
    let trait_src = render_trait(&db, &ProjectConfig::default()).await;
    assert!(
        trait_src.contains("async fn find_by_keys("),
        "findByKeys lowers onto the trait: {trait_src}"
    );
    assert!(
        trait_src.contains("id: Uuid,"),
        "the surrogate key is the consumer-known signature: {trait_src}"
    );
    assert!(
        trait_src.contains("async fn find_by_example("),
        "findByExample lowers onto the trait: {trait_src}"
    );
    assert!(
        trait_src.contains("title: Option<String>,"),
        "example fields are Option-wrapped: {trait_src}"
    );

    let impl_src = render_impl(&db).await;
    assert!(
        impl_src.contains("Column::Id.eq(id)"),
        "findByKeys equality on the primary key: {impl_src}"
    );
    assert!(
        impl_src.contains("let mut conditions = sea_orm::Condition::all();"),
        "findByExample builds a condition per example field: {impl_src}"
    );
    assert!(
        impl_src.contains("if let Some(title) = &title {"),
        "None fields filter out of the predicate: {impl_src}"
    );
    assert!(
        impl_src.contains(
            "conditions = conditions.add(crate::entity::library_book::Column::Title.eq(title.clone()));"
        ),
        "Some fields join the predicate: {impl_src}"
    );
    // Example finders return many rows.
    assert!(
        impl_src.contains("async fn find_by_example(") && impl_src.contains("Vec<BookResponse>"),
        "findByExample returns many rows: {impl_src}"
    );
}

#[tokio::test]
async fn protected_builtins_stay_off_the_public_operation_set() {
    // Sculptor visibility: a protected built-in lowers onto the repository
    // but maps no API operation.
    let mut save = builtin("save", "save", 0);
    save.is_protected = true;
    let mut find_by_id = builtin("findById", "findById", 1);
    find_by_id.is_protected = true;
    let surface = crate::ddd::design::DddDesignSurface::from_models(&[model(
        vec![design("Book", Some("BookType"), DddDesignFlags::default())],
        vec![repository(
            "BookRepository",
            "Book",
            vec![save, find_by_id, builtin("findAll", "findAll", 2)],
        )],
        vec![],
    )]);
    let ops = surface.operations_for("BookType").expect("design present");
    assert!(
        !ops.contains(&"create".to_string()) && !ops.contains(&"update".to_string()),
        "protected save maps no create/update: {ops:?}"
    );
    assert!(
        !ops.contains(&"read".to_string()),
        "protected findById maps no read: {ops:?}"
    );
    assert_eq!(ops, vec!["list".to_string()], "findAll still maps list");
}

#[tokio::test]
async fn protected_declared_finder_still_lowers_onto_the_repository() {
    let mut op = declared(
        "findByTitle",
        serde_json::json!({"type": "class", "value": {"package": "nz.example.library", "name": "Book"}}),
        None,
        vec![string_param("title")],
        0,
    );
    op.is_protected = true;
    let db = db_with_model(Some(model(
        vec![design("Book", Some("BookType"), DddDesignFlags::default())],
        vec![repository("BookRepository", "Book", vec![op])],
        vec![],
    )))
    .await;
    // The repository plane is internal — protection governs the API plane,
    // so the finder still lowers.
    let trait_src = render_trait(&db, &ProjectConfig::default()).await;
    assert!(
        trait_src.contains("async fn find_by_title("),
        "protected declared finders still lower onto the repository: {trait_src}"
    );
}

#[tokio::test]
async fn repository_trait_full_builtins_render_full_crud() {
    let db = db_with_model(Some(model(
        vec![design(
            "Book",
            Some("BookType"),
            DddDesignFlags {
                auditable: true,
                ..DddDesignFlags::default()
            },
        )],
        vec![repository(
            "BookRepository",
            "Book",
            vec![
                builtin("findById", "findById", 0),
                builtin("findAll", "findAll", 1),
                builtin("save", "save", 2),
                builtin("delete", "delete", 3),
            ],
        )],
        vec![],
    )))
    .await;
    let trait_src = render_trait(&db, &ProjectConfig::default()).await;
    assert!(trait_src.contains("async fn create("));
    assert!(trait_src.contains("async fn update("));
    assert!(trait_src.contains("async fn delete("));
    assert!(trait_src.contains("async fn find_by_id("));
    assert!(trait_src.contains("async fn list("));
}

// ── design finders ─────────────────────────────────────────────────────

fn finder_model(returns_many: bool) -> DddModelGraph {
    model(
        vec![design("Book", Some("BookType"), DddDesignFlags::default())],
        vec![repository(
            "BookRepository",
            "Book",
            vec![
                builtin("findById", "findById", 0),
                builtin("findAll", "findAll", 1),
                builtin("save", "save", 2),
                builtin("delete", "delete", 3),
                declared(
                    "findByTitle",
                    serde_json::json!({"type": "class", "value": {"package": "nz.example.library", "name": "Book"}}),
                    returns_many.then(|| serde_json::json!({"lower": 0, "upper": "unbounded"})),
                    vec![string_param("title")],
                    4,
                ),
            ],
        )],
        vec![],
    )
}

#[tokio::test]
async fn finder_emitted_in_trait_and_seaorm_impl_single() {
    let db = db_with_model(Some(finder_model(false))).await;
    let trait_src = render_trait(&db, &ProjectConfig::default()).await;
    assert!(
        trait_src.contains("async fn find_by_title("),
        "trait carries the declared finder"
    );
    assert!(trait_src.contains("title: String,"));
    assert!(
        trait_src.contains("Result<Option<BookResponse>, Box<dyn std::error::Error>>;"),
        "single finder returns Option"
    );
    let impl_src = render_impl(&db).await;
    assert!(impl_src.contains("async fn find_by_title("));
    assert!(
        impl_src.contains("crate::entity::library_book::Column::Title.eq(title.clone())"),
        "equality predicate on the title column: {impl_src}"
    );
    assert!(impl_src.contains(".one(db)"));
    assert!(!impl_src.contains(".all(db)"));
}

#[tokio::test]
async fn finder_emitted_in_trait_and_seaorm_impl_many() {
    let db = db_with_model(Some(finder_model(true))).await;
    let trait_src = render_trait(&db, &ProjectConfig::default()).await;
    assert!(
        trait_src.contains("Result<Vec<BookResponse>, Box<dyn std::error::Error>>;"),
        "many finder returns Vec"
    );
    let impl_src = render_impl(&db).await;
    assert!(impl_src.contains("async fn find_by_title("));
    assert!(impl_src.contains(".all(db)"));
}

// ── cornucopia parity ──────────────────────────────────────────────────

#[tokio::test]
async fn finder_mirrored_on_cornucopia_query_and_adapter() {
    let db = db_with_model(Some(finder_model(true))).await;
    let mut project = ProjectConfig::default();
    project.database.persistence_provider = PersistenceProvider::Cornucopia;
    let dir = TempDir::new().unwrap();

    let queries = crate::db::cornucopia_queries::CornucopiaQueryGenerator::new(dir.path())
        .generate(&db, "BookType", "library", &config(), &tera(), &project)
        .await
        .expect("cornucopia queries generate");
    let sql = queries[0].content.clone();
    assert!(
        sql.contains("--! find_by_title_book (title) : ("),
        "annotated finder query emitted: {sql}"
    );
    assert!(sql.contains("\"title\" = :title"));

    let repo = crate::ddd::cornucopia_repo::CornucopiaRepoGenerator::new(dir.path())
        .generate(&db, "BookType", "library", &config(), &tera(), &project)
        .await
        .expect("cornucopia adapter generates");
    let adapter = repo[0].content.clone();
    assert!(adapter.contains("async fn find_by_title("));
    assert!(adapter.contains("find_by_title_book().bind(db, &title)"));
    assert!(adapter.contains(".all().await"));
}

// ── issue #523: wraps-typed params + contained-value columns ───────────

fn datatype_param(name: &str, type_name: &str) -> DddParam {
    DddParam {
        name: name.to_string(),
        type_json: serde_json::json!({
            "type": "datatype",
            "value": {"package": "nz.example.library", "name": type_name},
        }),
        multiplicity: None,
    }
}

/// A contained value-object array property (`contains T[] field`), lowered
/// the way mox ingest projects it (child table, no parent column).
fn vo_array_prop(name: &str, snake: &str, target: &str) -> PropertyNode {
    PropertyNode {
        name: name.to_string(),
        prop_type: "object".to_string(),
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
        pg_column_name: snake.to_string(),
        pg_column_type: String::new(),
        rust_field_name: snake.to_string(),
        rust_field_type: target.to_string(),
        sea_orm_type: String::new(),
        render_strategy: "child_table".to_string(),
        ref_target: Some(target.to_string()),
        classification: None,
        projection: None,
        classification_kind: Some(RefClassificationKind::ValueObject),
        type_expr: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

fn tax_identifier_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "library/TaxIdentifierType".to_string(),
        title: "TaxIdentifierType".to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: "value_object".to_string(),
        domain: Some("library".to_string()),
        namespace: None,
        rel_path: "library/TaxIdentifierType.json".to_string(),
        pg_type: "TABLE".to_string(),
        rust_type: "TaxIdentifier".to_string(),
        sea_orm_type: "Entity".to_string(),
        rust_type_name: "TaxIdentifier".to_string(),
        pg_table_name: "tax_identifier".to_string(),
        api_path_segment: "tax-identifiers".to_string(),
        parent_schema: None,
        is_entity: false,
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

/// A wraps-datatype parameter (issue #523) resolves to the named feature's
/// column projection — the trait method binds the TEXT column with the
/// DTO-facing base type, not a drop.
#[tokio::test]
async fn finder_param_wraps_datatype_resolves_to_column() {
    let op = declared(
        "findByIsbn",
        serde_json::json!({"type": "class", "value": {"package": "nz.example.library", "name": "Book"}}),
        None,
        vec![datatype_param("isbn", "IsbnCode")],
        4,
    );
    let db = codegraph_core::mock::MockEngine::builder()
        .with_schema(book_schema())
        .with_properties("BookType", vec![string_prop("isbn", true)])
        .build();
    db.ingest_ddd_model(&model(
        vec![design("Book", Some("BookType"), DddDesignFlags::default())],
        vec![repository("BookRepository", "Book", vec![op])],
        vec![],
    ))
    .await
    .expect("ingest ddd model");

    let trait_src = render_trait(&db, &ProjectConfig::default()).await;
    assert!(
        trait_src.contains("async fn find_by_isbn("),
        "wraps-typed param must lower the finder: {trait_src}"
    );
    assert!(
        trait_src.contains("isbn: String,"),
        "wraps param binds the column's base type (String/TEXT): {trait_src}"
    );
    let impl_src = render_impl(&db).await;
    assert!(
        impl_src.contains("crate::entity::library_book::Column::Isbn.eq(isbn.clone())"),
        "emitted impl binds the TEXT column: {impl_src}"
    );
}

/// Contained-value lookup (issue #523): a finder whose params name the
/// columns of a `contains T[]` child emits an EXISTS-subquery equality —
/// one EXISTS per child table with the params ANDed (same-row semantics) —
/// on BOTH providers.
#[tokio::test]
async fn finder_on_contained_value_is_actionable() {
    let op = declared(
        "findByTaxIdentifier",
        serde_json::json!({"type": "class", "value": {"package": "nz.example.library", "name": "Book"}}),
        None,
        vec![
            datatype_param("jurisdiction", "JurisdictionCode"),
            string_param("number"),
        ],
        4,
    );
    let tax = tax_identifier_schema();
    let db = codegraph_core::mock::MockEngine::builder()
        .with_schema(book_schema())
        .with_properties(
            "BookType",
            vec![
                string_prop("title", true),
                vo_array_prop("taxIdentifiers", "tax_identifiers", "TaxIdentifierType"),
            ],
        )
        .with_schema(tax.clone())
        .with_properties(
            "TaxIdentifierType",
            vec![
                string_prop("jurisdiction", true),
                string_prop("number", true),
            ],
        )
        .with_ref_target("taxIdentifiers", "BookType", tax)
        .build();
    db.ingest_ddd_model(&model(
        vec![design("Book", Some("BookType"), DddDesignFlags::default())],
        vec![repository("BookRepository", "Book", vec![op])],
        vec![],
    ))
    .await
    .expect("ingest ddd model");

    let trait_src = render_trait(&db, &ProjectConfig::default()).await;
    assert!(
        trait_src.contains("async fn find_by_tax_identifier("),
        "contained-value finder lowers: {trait_src}"
    );
    assert!(trait_src.contains("jurisdiction: String,"), "{trait_src}");
    assert!(trait_src.contains("number: String,"), "{trait_src}");

    let impl_src = render_impl(&db).await;
    assert!(
        impl_src.contains("async fn find_by_tax_identifier("),
        "{impl_src}"
    );
    // The EXISTS SQL rides the generated code as a Debug-escaped string
    // literal (`\"` inside the emitted Rust source) — unescape for asserts.
    let impl_sql = impl_src.replace("\\\"", "\"");
    assert!(
        impl_sql.contains("EXISTS (SELECT 1 FROM \"library\".\"book_tax_identifiers\""),
        "EXISTS reaches the child table: {impl_src}"
    );
    assert!(
        impl_sql.contains("EXISTS (SELECT 1 FROM"),
        "contained-value params join via EXISTS: {impl_src}"
    );
    let exists_count = impl_sql.matches("EXISTS (SELECT 1 FROM").count();
    assert_eq!(
        exists_count, 1,
        "same-child params AND into ONE EXISTS: {impl_src}"
    );
    assert!(
        impl_sql.contains("_child.\"jurisdiction\" = $1 AND _child.\"number\" = $2"),
        "both child columns bind inside the one EXISTS: {impl_src}"
    );

    // Cornucopia parity: the annotated SQL carries the same EXISTS join.
    let mut project = ProjectConfig::default();
    project.database.persistence_provider = PersistenceProvider::Cornucopia;
    let dir = TempDir::new().unwrap();
    let queries = crate::db::cornucopia_queries::CornucopiaQueryGenerator::new(dir.path())
        .generate(&db, "BookType", "library", &config(), &tera(), &project)
        .await
        .expect("cornucopia queries generate");
    let sql = queries[0].content.clone();
    assert!(
        sql.contains("EXISTS (SELECT 1 FROM"),
        "cornucopia finder joins the child table: {sql}"
    );
    assert!(
        sql.contains("\"jurisdiction\" = :jurisdiction") && sql.contains("\"number\" = :number"),
        "cornucopia binds both child params: {sql}"
    );
}

// ── DTO gating ─────────────────────────────────────────────────────────

async fn render_dtos(db: &codegraph_core::mock::MockEngine) -> Vec<(String, String)> {
    let dir = TempDir::new().unwrap();
    let generator = DtoGenerator::new(dir.path());
    let files = generator
        .generate(
            db,
            "BookType",
            "library",
            &config(),
            &tera(),
            &ProjectConfig::default(),
        )
        .await
        .expect("dtos generate");
    files
        .into_iter()
        .map(|f| {
            (
                f.path.file_name().unwrap().to_string_lossy().into_owned(),
                f.content.clone(),
            )
        })
        .collect()
}

#[tokio::test]
async fn dto_gating_design_without_save_emits_no_create_update_dtos() {
    let db = db_with_model(Some(model(
        vec![design("Book", Some("BookType"), DddDesignFlags::default())],
        vec![repository(
            "BookRepository",
            "Book",
            vec![
                builtin("findById", "findById", 0),
                builtin("findAll", "findAll", 1),
            ],
        )],
        vec![],
    )))
    .await;
    let dtos = render_dtos(&db).await;
    let names: Vec<&str> = dtos.iter().map(|(n, _)| n.as_str()).collect();
    assert!(!names.contains(&"dto_create.rs"), "names: {names:?}");
    assert!(!names.contains(&"dto_update.rs"), "names: {names:?}");
    assert!(names.contains(&"dto_response.rs"));
}

#[tokio::test]
async fn dto_gating_design_with_save_emits_create_update_dtos() {
    let db = db_with_model(Some(model(
        vec![design("Book", Some("BookType"), DddDesignFlags::default())],
        vec![repository(
            "BookRepository",
            "Book",
            vec![builtin("save", "save", 0)],
        )],
        vec![],
    )))
    .await;
    let dtos = render_dtos(&db).await;
    let names: Vec<&str> = dtos.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"dto_create.rs"), "names: {names:?}");
    assert!(names.contains(&"dto_update.rs"));
    assert!(names.contains(&"dto_response.rs"));
}

// ── search plumbing ────────────────────────────────────────────────────

fn search_model() -> DddModelGraph {
    model(
        vec![design("Book", Some("BookType"), DddDesignFlags::default())],
        vec![repository(
            "BookRepository",
            "Book",
            vec![
                builtin("findById", "findById", 0),
                builtin("findAll", "findAll", 1),
                builtin("save", "save", 2),
                builtin("delete", "delete", 3),
            ],
        )],
        vec![DddSearchNode {
            application: "Library".to_string(),
            module: "media".to_string(),
            name: "BookSearch".to_string(),
            description: None,
            entity_class: "Book".to_string(),
            entity_title: Some("BookType".to_string()),
            text: vec![DddSearchField {
                property: "title".to_string(),
                boost: Some(3.0),
                analyzer: None,
            }],
            filters: vec!["status".to_string()],
            sorts: vec!["title".to_string()],
            document: Vec::new(),
            ranking: None,
            analyzer: Some("english".to_string()),
            pagination: None,
            capabilities: Vec::new(),
            ordinal: 0,
        }],
    )
}

#[tokio::test]
async fn search_design_drives_fts_and_filter_fields() {
    let db = db_with_model(Some(search_model())).await;
    let impl_src = render_impl(&db).await;
    assert!(
        impl_src.contains("async fn search_ids("),
        "design search turns the FTS surface on"
    );
    assert!(
        impl_src.contains("websearch_to_tsquery('english'"),
        "the search's default analyzer becomes the FTS language"
    );
    assert!(
        impl_src.contains("filters.get(\"status\")"),
        "the search's filter keys become filter fields"
    );

    let trait_src = render_trait(&db, &ProjectConfig::default()).await;
    assert!(trait_src.contains("async fn search_ids("));

    // The query handler mirrors has_fts (search_ids surfaced; the query
    // template itself carries no per-field filter rendering — filter keys
    // ride the repository impl's list emission asserted above).
    let dir = TempDir::new().unwrap();
    let query_files = QueryGenerator::new(dir.path())
        .generate(
            &db,
            "BookType",
            "library",
            &config(),
            &tera(),
            &ProjectConfig::default(),
        )
        .await
        .expect("query generates");
    let query_src = query_files[0].content.clone();
    assert!(query_src.contains("search_ids"));
}

// ── auditable flag override ────────────────────────────────────────────

#[tokio::test]
async fn design_auditable_false_overrides_default_true() {
    let db = db_with_model(Some(model(
        vec![design(
            "Book",
            Some("BookType"),
            DddDesignFlags::default(), // auditable: false
        )],
        vec![repository(
            "BookRepository",
            "Book",
            vec![
                builtin("findById", "findById", 0),
                builtin("findAll", "findAll", 1),
                builtin("save", "save", 2),
                builtin("delete", "delete", 3),
            ],
        )],
        vec![],
    )))
    .await;
    let trait_src = render_trait(&db, &ProjectConfig::default()).await;
    assert!(
        !trait_src.contains("include_deleted"),
        "flags.auditable=false wins over the domains.toml default-true: {trait_src}"
    );
}

#[tokio::test]
async fn design_auditable_true_forces_audit_tracking() {
    let db = db_with_model(Some(model(
        vec![design(
            "Book",
            Some("BookType"),
            DddDesignFlags {
                auditable: true,
                ..DddDesignFlags::default()
            },
        )],
        vec![repository(
            "BookRepository",
            "Book",
            vec![
                builtin("findById", "findById", 0),
                builtin("findAll", "findAll", 1),
                builtin("save", "save", 2),
                builtin("delete", "delete", 3),
            ],
        )],
        vec![],
    )))
    .await;
    let trait_src = render_trait(&db, &ProjectConfig::default()).await;
    assert!(trait_src.contains("include_deleted"));
}

// ── DD9 typed repository queries (issue #659) ─────────────────────────

fn book_return() -> serde_json::Value {
    serde_json::json!({"type": "class", "value": {"package": "nz.example.library", "name": "Book"}})
}

fn query_param(name: &str, type_json: serde_json::Value) -> DddParam {
    DddParam {
        name: name.to_string(),
        type_json,
        multiplicity: None,
    }
}

fn string_query_param(name: &str) -> DddParam {
    query_param(
        name,
        serde_json::json!({"type": "primitive", "value": "string"}),
    )
}

/// A DD9 `query` block carrying only the predicate.
fn query_block(predicate: &str) -> serde_json::Value {
    serde_json::json!({"predicate": predicate, "readOnly": true})
}

/// A DD9 `query` block with a sort key and a bounded page.
fn query_block_sorted(predicate: &str, field: &str, desc: bool) -> serde_json::Value {
    serde_json::json!({
        "predicate": predicate,
        "sort": [{
            "field": field,
            "fieldType": {"type": "primitive", "value": "string"},
            "direction": if desc { "desc" } else { "asc" },
        }],
        "pagination": {"limit": 10, "max": 50, "cursor": false},
        "readOnly": true,
    })
}

/// One declared DD9 query operation on `BookRepository`.
fn declared_query(
    name: &str,
    query_json: serde_json::Value,
    params: Vec<DddParam>,
    returns_many: bool,
) -> DddRepositoryOperation {
    DddRepositoryOperation {
        name: name.to_string(),
        builtin: None,
        return_type: Some(book_return()),
        return_multiplicity: returns_many
            .then(|| serde_json::json!({"lower": 0, "upper": "unbounded"})),
        params,
        ordinal: 4,
        is_protected: false,
        query_json: Some(query_json),
        capabilities: vec!["ReadBooks".to_string()],
    }
}

fn query_model(
    query_json: serde_json::Value,
    params: Vec<DddParam>,
    returns_many: bool,
) -> DddModelGraph {
    model(
        vec![design("Book", Some("BookType"), DddDesignFlags::default())],
        vec![repository(
            "BookRepository",
            "Book",
            vec![
                builtin("findById", "findById", 0),
                builtin("findAll", "findAll", 1),
                builtin("save", "save", 2),
                builtin("delete", "delete", 3),
                declared_query("recent", query_json, params, returns_many),
            ],
        )],
        vec![],
    )
}

#[tokio::test]
async fn query_emitted_in_trait_and_seaorm_impl_with_parameters_order_and_page() {
    let db = db_with_model(Some(query_model(
        query_block_sorted(
            "self.status == target_status || self.title == target_title",
            "title",
            true,
        ),
        vec![
            string_query_param("target_status"),
            string_query_param("target_title"),
        ],
        true,
    )))
    .await;

    let trait_src = render_trait(&db, &ProjectConfig::default()).await;
    assert!(
        trait_src.contains("/// Design query `recent` (declared in the .ddd design): typed"),
        "the trait documents the query: {trait_src}"
    );
    assert!(
        trait_src.contains("/// Read capability: ReadBooks."),
        "the resolved read capability is documented: {trait_src}"
    );
    assert!(
        trait_src.contains("Result<Vec<BookResponse>, Box<dyn std::error::Error>>;"),
        "many-valued query returns Vec: {trait_src}"
    );
    assert!(
        trait_src.contains("        limit: Option<u64>,\n        offset: Option<u64>,"),
        "a declared page adds the runtime page parameters: {trait_src}"
    );

    let impl_src = render_impl(&db).await;
    assert!(
        impl_src.contains("crate::entity::library_book::Column::Status.eq(target_status.clone())"),
        "the predicate binds the parameter (parameterized, never interpolated): {impl_src}"
    );
    assert!(
        impl_src.contains("crate::entity::library_book::Column::Title.eq(target_title.clone())"),
        "{impl_src}"
    );
    assert!(
        impl_src.contains("sea_orm::Condition::any()"),
        "the OR nests as a Condition::any: {impl_src}"
    );
    assert!(
        impl_src.contains("order_by_desc(crate::entity::library_book::Column::Title)"),
        "{impl_src}"
    );
    assert!(
        impl_src.contains("order_by_asc(crate::entity::library_book::Column::Id)"),
        "the identity tiebreaker makes the page deterministic: {impl_src}"
    );
    assert!(
        impl_src.contains(
            "let page_size = limit.or(Some(10u64)).map(|page_size| page_size.min(50u64));"
        ),
        "{impl_src}"
    );
    assert!(
        impl_src.contains("query.offset(offset.unwrap_or(0))"),
        "{impl_src}"
    );
    assert!(impl_src.contains("query.all(db)"), "many rows: {impl_src}");

    // The query never maps CRUD operations onto the API plane.
    let surface = codegraph_generate_design_surface(&db).await;
    let ops = surface.operations_for("BookType").expect("design present");
    assert_eq!(
        ops,
        vec![
            "create".to_string(),
            "update".to_string(),
            "read".to_string(),
            "delete".to_string(),
            "list".to_string()
        ],
        "a query is a repository read, not a CRUD mapping"
    );
}

/// The design surface over the test engine (the query plane's own view).
async fn codegraph_generate_design_surface(
    db: &codegraph_core::mock::MockEngine,
) -> crate::ddd::design::DddDesignSurface {
    crate::ddd::design::DddDesignSurface::from_graph(db)
        .await
        .expect("design surface")
}

#[tokio::test]
async fn cornucopia_query_literals_bind_through_distinct_temporaries() {
    // Two distinct string literals used across three atoms: the SQL renderer
    // dedupes by value (`"A"` binds once), and the adapter must emit one
    // typed temporary per bind — sharing one name would silently alias the
    // second value onto the first.
    let db = db_with_model(Some(query_model(
        query_block("self.title == \"A\" && self.status == \"B\" || self.title != \"A\""),
        vec![],
        true,
    )))
    .await;
    let project = ProjectConfig {
        database: crate::project_config::DatabaseConfig {
            persistence_provider: PersistenceProvider::Cornucopia,
            ..Default::default()
        },
        ..Default::default()
    };
    let dir = TempDir::new().unwrap();
    let files = crate::db::cornucopia_queries::CornucopiaQueryGenerator::new(dir.path())
        .generate(&db, "BookType", "library", &config(), &tera(), &project)
        .await
        .expect("query SQL generates");
    let sql: String = files.iter().map(|file| file.content.clone()).collect();
    assert!(
        sql.contains("--! recent_book (_lit0, _lit1) : ("),
        "one bind per distinct literal, in first-occurrence order: {sql}"
    );
    assert!(
        sql.contains("WHERE ((\"title\" = :_lit0::text AND \"status\" = :_lit1::text) OR \"title\" <> :_lit0::text)"),
        "{sql}"
    );

    let adapter = crate::ddd::cornucopia_repo::CornucopiaRepoGenerator::new(dir.path())
        .generate(&db, "BookType", "library", &config(), &tera(), &project)
        .await
        .expect("adapter generates");
    let code: String = adapter.iter().map(|file| file.content.clone()).collect();
    assert!(
        code.contains("        let _lit0: String = \"A\".to_string();"),
        "the first literal binds through its own temporary: {code}"
    );
    assert!(
        code.contains("        let _lit1: String = \"B\".to_string();"),
        "the second literal binds through its own temporary: {code}"
    );
    assert!(
        code.contains("recent_book().bind(db, &_lit0, &_lit1)"),
        "the adapter binds the temporaries in placeholder order: {code}"
    );
}

#[tokio::test]
async fn query_without_a_page_has_no_runtime_page_parameters() {
    let db = db_with_model(Some(query_model(
        query_block("self.status == target_status"),
        vec![string_query_param("target_status")],
        false,
    )))
    .await;
    let trait_src = render_trait(&db, &ProjectConfig::default()).await;
    assert!(
        !trait_src.contains("limit: Option<u64>"),
        "no `page` ⇒ no page parameters: {trait_src}"
    );
    let impl_src = render_impl(&db).await;
    assert!(!impl_src.contains(".limit("), "{impl_src}");
    assert!(
        impl_src.contains("query.one(db)"),
        "0..1 → one row: {impl_src}"
    );
    assert!(
        impl_src.contains("Result<Option<BookResponse>, Box<dyn std::error::Error>>"),
        "0..1 returns an optional and never throws not-found: {impl_src}"
    );
}

#[tokio::test]
async fn a_construct_outside_the_subset_is_a_named_generation_error() {
    // `length(…)` is a package-function call — type-correct only if the
    // domain declares it, but the refusal must fire before any store work.
    let db = db_with_model(Some(query_model(
        query_block("self.title == target_title && length(self.title) > 3"),
        vec![string_query_param("target_title")],
        true,
    )))
    .await;
    let err = try_render_trait(&db, &ProjectConfig::default())
        .await
        .expect_err("the query must be refused");
    let message = err.to_string();
    assert!(
        message.contains("query `BookRepository.recent`"),
        "the refusal names the repository and operation: {message}"
    );
    assert!(
        message.contains("package-function call"),
        "the refusal names the construct: {message}"
    );
}

#[tokio::test]
async fn a_query_colliding_with_a_pagination_parameter_is_refused() {
    let db = db_with_model(Some(query_model(
        query_block("self.status == limit"),
        vec![string_query_param("limit")],
        true,
    )))
    .await;
    let err = try_render_trait(&db, &ProjectConfig::default())
        .await
        .expect_err("the collision must be refused");
    let message = err.to_string();
    assert!(
        message.contains("query `BookRepository.recent`") && message.contains("`limit`"),
        "{message}"
    );
}

#[tokio::test]
async fn cornucopia_query_sql_and_adapter_render_the_same_plan() {
    let db = db_with_model(Some(query_model(
        query_block_sorted("self.status == target_status", "title", false),
        vec![string_query_param("target_status")],
        true,
    )))
    .await;
    let project = ProjectConfig {
        database: crate::project_config::DatabaseConfig {
            persistence_provider: PersistenceProvider::Cornucopia,
            ..Default::default()
        },
        ..Default::default()
    };
    let dir = TempDir::new().unwrap();
    let files = crate::db::cornucopia_queries::CornucopiaQueryGenerator::new(dir.path())
        .generate(&db, "BookType", "library", &config(), &tera(), &project)
        .await
        .expect("query SQL generates");
    let sql = files
        .iter()
        .map(|file| file.content.clone())
        .collect::<String>();
    assert!(
        sql.contains("--! recent_book (target_status, page_size, offset) : ("),
        "the query annotation lists the predicate bind then the page: {sql}"
    );
    assert!(
        sql.contains("WHERE \"status\" = :target_status::text"),
        "the predicate is parameterized with an explicit cast: {sql}"
    );
    assert!(
        sql.contains("ORDER BY \"title\" ASC, \"id\" ASC"),
        "the sort keys then the identity tiebreaker: {sql}"
    );
    assert!(sql.contains("LIMIT :page_size OFFSET :offset"), "{sql}");

    let adapter = crate::ddd::cornucopia_repo::CornucopiaRepoGenerator::new(dir.path())
        .generate(&db, "BookType", "library", &config(), &tera(), &project)
        .await
        .expect("adapter generates");
    let code = adapter
        .iter()
        .map(|file| file.content.clone())
        .collect::<String>();
    assert!(
        code.contains("recent_book().bind(db, &target_status, &page_size, &page_offset)"),
        "the adapter binds the predicate args then the page: {code}"
    );
    assert!(
        code.contains("let page_size = limit.or(Some(10u64)).map(|page_size| page_size.min(50u64)).unwrap_or(i64::MAX as u64) as i64;"),
        "{code}"
    );
}

// ── byte-identity guard (no designs in the graph) ──────────────────────

#[tokio::test]
async fn no_designs_keeps_schema_derived_output() {
    let db = db_with_model(None).await;

    let trait_src = render_trait(&db, &ProjectConfig::default()).await;
    assert!(trait_src.contains("async fn create("));
    assert!(trait_src.contains("async fn update("));
    assert!(trait_src.contains("async fn delete("));
    assert!(trait_src.contains("async fn list("));
    assert!(trait_src.contains("include_deleted: bool"));
    assert!(!trait_src.contains("Design finder"));

    let impl_src = render_impl(&db).await;
    assert!(impl_src.contains("async fn create("));
    assert!(impl_src.contains("async fn list("));
    assert!(!impl_src.contains("async fn find_by_title("));
    assert!(!impl_src.contains("search_ids"));
    assert!(!impl_src.contains("Design finder"));

    let dtos = render_dtos(&db).await;
    let names: Vec<&str> = dtos.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"dto_create.rs"));
    assert!(names.contains(&"dto_update.rs"));
    assert!(names.contains(&"dto_response.rs"));

    // command.rs still generates under the default ops (create present).
    let dir = TempDir::new().unwrap();
    let command_files = CommandGenerator::new(dir.path())
        .generate(
            &db,
            "BookType",
            "library",
            &config(),
            &tera(),
            &ProjectConfig::default(),
        )
        .await
        .expect("command generates");
    assert!(!command_files.is_empty());
}
