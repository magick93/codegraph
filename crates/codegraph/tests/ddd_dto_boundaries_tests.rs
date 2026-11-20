//! Modeled DTO boundary generation (DD3, issue #667).
//!
//! Node-free pins over the boundary generator: mirror boundaries alias the
//! source entity's response model, projection/standalone boundaries emit
//! real Rust types with the authored members, codelist and nested-boundary
//! members resolve, input boundaries keep `deny_unknown_fields`, an
//! unresolvable member refuses by name, boundary names cannot shadow a
//! generated entity DTO type, and a graph with no `dto` designs emits
//! nothing.

use codegraph_config::config::{DomainConfig, parse_domain_config_str};
use codegraph_core::mock::MockEngine;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{
    DddApplicationNode, DddDtoMember, DddDtoShapeNode, DddModelGraph, DddModuleNode, SchemaNode,
};
use codegraph_generate::ddd::dto_boundaries::DddDtoBoundariesGenerator;
use codegraph_generate::traits::GlobalGenerator;
use codegraph_generate::{GenerationEntry, ProjectConfig};

const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.library]
label = "Library"
schema_dir = "library"
postgres_schema = "library"
"#;

fn config() -> DomainConfig {
    parse_domain_config_str(DOMAINS_TOML).expect("domains config")
}

fn tera() -> tera::Tera {
    codegraph_generate::template_engine::create_tera_with_overrides(&[]).expect("tera")
}

fn schema(title: &str, domain: &str) -> SchemaNode {
    SchemaNode {
        schema_id: format!("{domain}/{title}"),
        title: title.to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: "entity".to_string(),
        domain: Some(domain.to_string()),
        namespace: None,
        rel_path: format!("{domain}/{title}.json"),
        pg_type: "TABLE".to_string(),
        rust_type: title.to_string(),
        sea_orm_type: "Entity".to_string(),
        rust_type_name: title.strip_suffix("Type").unwrap_or(title).to_string(),
        pg_table_name: title.to_string(),
        api_path_segment: title.to_string(),
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

fn member(name: &str, kind: &str, type_json: serde_json::Value) -> DddDtoMember {
    DddDtoMember {
        name: name.to_string(),
        kind: kind.to_string(),
        type_json,
        multiplicity_json: None,
        path: if kind == "select" {
            vec![name.to_string()]
        } else {
            Vec::new()
        },
        expression: None,
    }
}

fn string_type() -> serde_json::Value {
    serde_json::json!({"type": "primitive", "value": "string"})
}

fn int_type() -> serde_json::Value {
    serde_json::json!({"type": "primitive", "value": "int"})
}

fn book_shapes() -> Vec<DddDtoShapeNode> {
    vec![
        // Mirror: every member is a same-name selection of the source class.
        DddDtoShapeNode {
            application: "Library".to_string(),
            module: "catalog".to_string(),
            name: "BookMirror".to_string(),
            source_json: Some(serde_json::json!({
                "type": "class",
                "value": {"package": "nz.example.library", "name": "Book"}
            })),
            direction: "input".to_string(),
            members: vec![
                member("title", "select", string_type()),
                member("isbn", "select", string_type()),
            ],
            ordinal: 1,
        },
        // Projection: a selection plus a derived member (output-only).
        DddDtoShapeNode {
            application: "Library".to_string(),
            module: "catalog".to_string(),
            name: "BookSummary".to_string(),
            source_json: Some(serde_json::json!({
                "type": "class",
                "value": {"package": "nz.example.library", "name": "Book"}
            })),
            direction: "output".to_string(),
            members: vec![
                member("title", "select", string_type()),
                DddDtoMember {
                    expression: Some("playLength / 60".to_string()),
                    ..member("minutes", "derive", int_type())
                },
            ],
            ordinal: 2,
        },
        // Standalone input boundary, no source class.
        DddDtoShapeNode {
            application: "Library".to_string(),
            module: "catalog".to_string(),
            name: "NewBookInput".to_string(),
            source_json: None,
            direction: "input".to_string(),
            members: vec![
                member("title", "declared", string_type()),
                DddDtoMember {
                    multiplicity_json: Some(
                        serde_json::json!({"lower": 0, "upper": {"finite": 1}}),
                    ),
                    ..member("subtitle", "declared", string_type())
                },
            ],
            ordinal: 3,
        },
    ]
}

fn model(shapes: Vec<DddDtoShapeNode>) -> DddModelGraph {
    DddModelGraph {
        source_path: "design.ddd".to_string(),
        application: DddApplicationNode {
            name: "Library".to_string(),
            base: Some("nz.example.library".to_string()),
            source_path: "design.ddd".to_string(),
            errors: Vec::new(),
        },
        modules: vec![DddModuleNode {
            application: "Library".to_string(),
            name: "catalog".to_string(),
            ordinal: 0,
            errors: Vec::new(),
        }],
        designs: Vec::new(),
        repositories: Vec::new(),
        services: Vec::new(),
        searches: Vec::new(),
        dto_shapes: shapes,
    }
}

async fn generate(shapes: Vec<DddDtoShapeNode>) -> Vec<codegraph_generate::traits::GeneratedFile> {
    let engine = MockEngine::builder().build();
    engine
        .ingest_schema(&schema("BookType", "library"))
        .await
        .unwrap();
    engine.ingest_ddd_model(&model(shapes)).await.unwrap();

    let base = tempfile::tempdir().expect("tempdir");
    let generator = DddDtoBoundariesGenerator::new_with_base(base.path().to_path_buf());
    let order = vec![GenerationEntry {
        schema_title: "BookType".to_string(),
        domain: "library".to_string(),
        pg_schema: "library".to_string(),
        is_cyclic: false,
    }];
    generator
        .generate(
            &engine,
            &config(),
            &order,
            &tera(),
            &ProjectConfig::default(),
        )
        .await
        .expect("generation")
}

fn file<'a>(
    files: &'a [codegraph_generate::traits::GeneratedFile],
    suffix: &str,
) -> &'a codegraph_generate::traits::GeneratedFile {
    files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with(suffix))
        .unwrap_or_else(|| {
            panic!(
                "missing {suffix}; produced {:?}",
                files
                    .iter()
                    .map(|f| f.path.display().to_string())
                    .collect::<Vec<_>>()
            )
        })
}

#[tokio::test]
async fn mirror_boundary_aliases_the_source_entity_response() {
    let files = generate(book_shapes()).await;
    let catalog = file(&files, "src/dto/catalog.rs");
    assert!(
        catalog
            .content
            .contains("pub type BookMirror = crate::library::book::dto_response::BookResponse;"),
        "{}",
        catalog.content
    );
    assert!(
        !catalog.content.contains("pub struct BookMirror"),
        "the mirror form must not duplicate the response struct"
    );
}

#[tokio::test]
async fn projection_and_standalone_boundaries_emit_real_types() {
    let files = generate(book_shapes()).await;
    let catalog = file(&files, "src/dto/catalog.rs");
    let content = &catalog.content;

    // Projection: selection + derived member, output direction (no
    // deny_unknown_fields on a read model).
    assert!(content.contains("pub struct BookSummary {"), "{content}");
    assert!(content.contains("pub title: String,"), "{content}");
    assert!(content.contains("pub minutes: i64,"), "{content}");
    assert!(
        content.contains("/// Computed: `playLength / 60` (read-only)."),
        "{content}"
    );

    // Standalone: declared members, optional wrapped in Option, and the
    // input discipline (unknown-field rejection).
    assert!(content.contains("pub struct NewBookInput {"), "{content}");
    assert!(
        content.contains("#[serde(deny_unknown_fields)]"),
        "{content}"
    );
    let standalone = content
        .split("pub struct NewBookInput {")
        .nth(1)
        .expect("NewBookInput struct");
    assert!(standalone.contains("pub title: String,"), "{standalone}");
    assert!(
        standalone.contains("pub subtitle: Option<String>,"),
        "{standalone}"
    );
    assert!(
        standalone.contains("#[serde(default, skip_serializing_if = \"Option::is_none\")]"),
        "{standalone}"
    );
}

#[tokio::test]
async fn no_dto_designs_emit_nothing() {
    let files = generate(Vec::new()).await;
    assert!(files.is_empty(), "flag-off output must be byte-identical");
}

#[tokio::test]
async fn boundary_name_colliding_with_a_generated_dto_type_is_a_config_error() {
    let mut shapes = book_shapes();
    shapes[2].name = "BookResponse".to_string();
    let engine = MockEngine::builder().build();
    engine
        .ingest_schema(&schema("BookType", "library"))
        .await
        .unwrap();
    engine.ingest_ddd_model(&model(shapes)).await.unwrap();
    let base = tempfile::tempdir().expect("tempdir");
    let generator = DddDtoBoundariesGenerator::new_with_base(base.path().to_path_buf());
    let order = vec![GenerationEntry {
        schema_title: "BookType".to_string(),
        domain: "library".to_string(),
        pg_schema: "library".to_string(),
        is_cyclic: false,
    }];
    let err = generator
        .generate(
            &engine,
            &config(),
            &order,
            &tera(),
            &ProjectConfig::default(),
        )
        .await
        .expect_err("name collision must fail");
    let text = err.to_string();
    assert!(text.contains("BookResponse"), "{text}");
    assert!(text.contains("collides"), "{text}");
}

#[tokio::test]
async fn unresolvable_member_type_refuses_naming_the_boundary_and_member() {
    let mut shapes = book_shapes();
    shapes[2].members.push(member(
        "weird",
        "declared",
        serde_json::json!({"type": "interface", "value": {"package": "x", "name": "Port"}}),
    ));
    let engine = MockEngine::builder().build();
    engine
        .ingest_schema(&schema("BookType", "library"))
        .await
        .unwrap();
    engine.ingest_ddd_model(&model(shapes)).await.unwrap();
    let base = tempfile::tempdir().expect("tempdir");
    let generator = DddDtoBoundariesGenerator::new_with_base(base.path().to_path_buf());
    let order = vec![GenerationEntry {
        schema_title: "BookType".to_string(),
        domain: "library".to_string(),
        pg_schema: "library".to_string(),
        is_cyclic: false,
    }];
    let err = generator
        .generate(
            &engine,
            &config(),
            &order,
            &tera(),
            &ProjectConfig::default(),
        )
        .await
        .expect_err("interface member must refuse");
    let text = err.to_string();
    assert!(text.contains("NewBookInput"), "{text}");
    assert!(text.contains("weird"), "{text}");
}
