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
use codegraph_core::traits::GraphIngestor;
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
async fn mirror_boundary_emits_its_own_members_not_the_whole_response() {
    // DD3 review finding: a mirror's members are every stored feature, but a
    // projection of the SAME shape must never widen into the entity response
    // (an alias would). Every boundary emits the exact modeled members.
    let files = generate(book_shapes()).await;
    let catalog = file(&files, "src/dto/catalog.rs");
    let mirror = catalog
        .content
        .split("pub struct BookMirror {")
        .nth(1)
        .expect("BookMirror struct");
    assert!(mirror.contains("pub title: String,"), "{mirror}");
    assert!(mirror.contains("pub isbn: String,"), "{mirror}");
    assert!(
        !catalog.content.contains("pub type BookMirror"),
        "no alias onto the entity response: {}",
        catalog.content
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

/// End-to-end: a `.ddd` with a standalone input boundary + a projection runs
/// through the real pipeline, generates the boundary module in the
/// domain-types crate, and binds the action payload to it (direct body).
#[tokio::test]
async fn pipeline_generates_boundaries_and_binds_the_action_payload() {
    use std::fs;
    use std::path::PathBuf;

    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dto_boundaries");
    let temp = tempfile::tempdir().expect("tempdir");
    let output = temp.path().join("out");
    let config = temp.path().join("domains.toml");
    fs::write(
        &config,
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.library]
label = "Library"
schema_dir = "schemas/library"
postgres_schema = "library"
"#,
    )
    .unwrap();

    let mox_files = vec![fixture.join("dto_boundaries.mox")];
    let ddd_files = vec![fixture.join("dto_boundaries.ddd")];
    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &config,
        output: &output,
        extension_points_path: None,
        integration_manifests: &[],
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(PathBuf::from("definitely-absent-profiles.toml")),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &mox_files,
        rosetta_files: &[],
        ddd_files: &ddd_files,
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await
    .expect("pipeline run");

    let boundary = fs::read_to_string(output.join("crates/domain-types/src/dto/catalog.rs"))
        .expect("generated boundary module");
    assert!(boundary.contains("pub struct NewBookInput {"), "{boundary}");
    assert!(
        boundary.contains("pub token: uuid::Uuid,"),
        "a `wraps opaque` datatype named Uuid must use the QUALIFIED path: {boundary}"
    );
    assert!(boundary.contains("pub extras: Vec<String>,"), "{boundary}");
    assert!(boundary.contains("pub struct BookSummary {"), "{boundary}");
    assert!(
        boundary.contains("/// Computed: `playLength / 60`"),
        "{boundary}"
    );

    let domain_types_lib =
        fs::read_to_string(output.join("crates/domain-types/src/lib.rs")).unwrap();
    assert!(
        domain_types_lib.contains("pub mod dto;"),
        "the crate index must declare the boundary module: {domain_types_lib}"
    );

    // The action handler binds the modeled boundary as the REQUEST BODY.
    let handler = fs::read_to_string(output.join("src/api/library/book_handler.rs"))
        .expect("generated handler");
    assert!(
        handler.contains("domain_types::dto::catalog::NewBookInput"),
        "the payload must resolve to the generated boundary type: {handler}"
    );
    assert!(
        handler.contains(
            "serde_json::from_value::<domain_types::dto::catalog::NewBookInput>(value.0.clone())"
        ),
        "a lone DTO parameter must bind the request body directly: {handler}"
    );

    // The boundary is documented as a component schema in BOTH documents.
    let all_doc = fs::read_to_string(output.join("src/api/openapi/all.rs")).unwrap();
    assert!(
        all_doc.contains("domain_types::dto::catalog::NewBookInput"),
        "the combined OpenAPI document must register the boundary: {all_doc}"
    );
    assert!(
        all_doc.contains("domain_types::dto::catalog::BookSummary"),
        "the projection must register too: {all_doc}"
    );
    let domain_doc = fs::read_to_string(output.join("src/api/openapi/library.rs")).unwrap();
    assert!(
        domain_doc.contains("domain_types::dto::catalog::NewBookInput"),
        "the per-domain OpenAPI document must register the boundary: {domain_doc}"
    );
}

/// Issue #667 review: the hurl contract body must be VALID for the struct it
/// addresses — project wire casing, arrays for many-valued members, and the
/// first codelist value (resolved through the graph) rather than a guess.
#[tokio::test]
async fn boundary_sample_matches_the_generated_wire_contract() {
    use codegraph_core::traits::GraphIngestor;
    use codegraph_core::types::{CodeList, EnumValue};

    let engine = MockEngine::builder().build();
    engine
        .ingest_codelist(&CodeList {
            name: "Colour".to_string(),
            description: None,
            pg_table_name: "colour".to_string(),
            render_as: "select".to_string(),
            check_expression: None,
        })
        .await
        .unwrap();
    engine
        .ingest_enum_value(
            "Colour",
            &EnumValue {
                value: "CRIMSON".to_string(),
                display_name: None,
                sort_order: 0,
            },
        )
        .await
        .unwrap();

    let shape = DddDtoShapeNode {
        application: "Library".to_string(),
        module: "catalog".to_string(),
        name: "PaintInput".to_string(),
        source_json: None,
        direction: "input".to_string(),
        members: vec![
            DddDtoMember {
                name: "playLength".to_string(),
                kind: "declared".to_string(),
                type_json: serde_json::json!({"type": "primitive", "value": "int"}),
                multiplicity_json: None,
                path: Vec::new(),
                expression: None,
            },
            DddDtoMember {
                name: "tags".to_string(),
                kind: "declared".to_string(),
                type_json: serde_json::json!({"type": "primitive", "value": "string"}),
                multiplicity_json: Some(serde_json::json!({"lower": 1, "upper": "unbounded"})),
                path: Vec::new(),
                expression: None,
            },
            DddDtoMember {
                name: "colour".to_string(),
                kind: "declared".to_string(),
                type_json: serde_json::json!({
                    "type": "enum",
                    "value": {"package": "nz.example.library", "name": "Colour"}
                }),
                multiplicity_json: None,
                path: Vec::new(),
                expression: None,
            },
            DddDtoMember {
                name: "note".to_string(),
                kind: "declared".to_string(),
                type_json: serde_json::json!({"type": "primitive", "value": "string"}),
                multiplicity_json: Some(serde_json::json!({"lower": 0, "upper": {"finite": 1}})),
                path: Vec::new(),
                expression: None,
            },
            DddDtoMember {
                name: "extras".to_string(),
                kind: "declared".to_string(),
                type_json: serde_json::json!({"type": "primitive", "value": "string"}),
                multiplicity_json: Some(serde_json::json!({"lower": 0, "upper": "unbounded"})),
                path: Vec::new(),
                expression: None,
            },
            DddDtoMember {
                name: "token".to_string(),
                kind: "declared".to_string(),
                type_json: serde_json::json!({
                    "type": "datatype",
                    "value": {"package": "", "name": "Uuid"}
                }),
                multiplicity_json: None,
                path: Vec::new(),
                expression: None,
            },
            DddDtoMember {
                name: "amount".to_string(),
                kind: "declared".to_string(),
                type_json: serde_json::json!({
                    "type": "datatype",
                    "value": {"package": "", "name": "Decimal"}
                }),
                multiplicity_json: None,
                path: Vec::new(),
                expression: None,
            },
            DddDtoMember {
                name: "publishedAt".to_string(),
                kind: "declared".to_string(),
                type_json: serde_json::json!({
                    "type": "datatype",
                    "value": {"package": "", "name": "DateTime"}
                }),
                multiplicity_json: None,
                path: Vec::new(),
                expression: None,
            },
            DddDtoMember {
                name: "dueOn".to_string(),
                kind: "declared".to_string(),
                type_json: serde_json::json!({
                    "type": "datatype",
                    "value": {"package": "", "name": "Date"}
                }),
                multiplicity_json: None,
                path: Vec::new(),
                expression: None,
            },
        ],
        ordinal: 0,
    };
    engine
        .ingest_ddd_model(&DddModelGraph {
            dto_shapes: vec![shape],
            ..model(Vec::new())
        })
        .await
        .unwrap();

    let index =
        codegraph_generate::ddd::dto_boundaries::DtoBoundaryIndex::from_graph(&engine, &config())
            .await
            .expect("index");

    assert_eq!(
        index.sample("PaintInput", false).cloned(),
        Some(serde_json::json!({
            "play_length": 42,
            "tags": ["contract-sample"],
            "colour": "CRIMSON",
            "extras": [],
            "token": "00000000-0000-0000-0000-000000000000",
            "amount": "1.0",
            "published_at": "2026-01-01T00:00:00Z",
            "due_on": "2026-01-01"
        })),
        "snake keys, arrays for many members, the first codelist value, empty lower-0 list, datatype sample, optional omitted"
    );
    assert_eq!(
        index.sample("PaintInput", true).cloned(),
        Some(serde_json::json!({
            "playLength": 42,
            "tags": ["contract-sample"],
            "colour": "CRIMSON",
            "extras": [],
            "token": "00000000-0000-0000-0000-000000000000",
            "amount": "1.0",
            "publishedAt": "2026-01-01T00:00:00Z",
            "dueOn": "2026-01-01"
        })),
        "camel casing applies to every key"
    );
}

/// Issue #667 review: a class-typed member is a `Uuid` only when it resolves
/// to an ENTITY (a stored FK). A contained value object is refused by name —
/// typing an object as a Uuid silently produced a 400 at runtime.
#[tokio::test]
async fn class_members_resolve_entities_to_uuid_and_refuse_value_objects() {
    use codegraph_core::traits::GraphIngestor;
    let engine = MockEngine::builder().build();
    let mut address = schema("Address", "library");
    address.classification = "value_object".to_string();
    address.is_entity = false;
    engine.ingest_schema(&address).await.unwrap();
    engine
        .ingest_schema(&schema("BookType", "library"))
        .await
        .unwrap();

    let shape = |member_class: &str| DddDtoShapeNode {
        application: "Library".to_string(),
        module: "catalog".to_string(),
        name: "Card".to_string(),
        source_json: None,
        direction: "input".to_string(),
        members: vec![DddDtoMember {
            name: "target".to_string(),
            kind: "declared".to_string(),
            type_json: serde_json::json!({
                "type": "class",
                "value": {"package": "nz.example.library", "name": member_class}
            }),
            multiplicity_json: None,
            path: Vec::new(),
            expression: None,
        }],
        ordinal: 0,
    };

    for (class, expect_uuid) in [("Book", true), ("Address", false)] {
        engine
            .ingest_ddd_model(&DddModelGraph {
                dto_shapes: vec![shape(class)],
                ..model(Vec::new())
            })
            .await
            .unwrap();
        let base = tempfile::tempdir().expect("tempdir");
        let generator = DddDtoBoundariesGenerator::new_with_base(base.path().to_path_buf());
        let order = vec![GenerationEntry {
            schema_title: "BookType".to_string(),
            domain: "library".to_string(),
            pg_schema: "library".to_string(),
            is_cyclic: false,
        }];
        let result = generator
            .generate(
                &engine,
                &config(),
                &order,
                &tera(),
                &ProjectConfig::default(),
            )
            .await;
        if expect_uuid {
            let files = result.expect("entity ref resolves");
            assert!(
                file(&files, "src/dto/catalog.rs")
                    .content
                    .contains("pub target: uuid::Uuid,"),
                "entity refs are stored FK ids"
            );
        } else {
            let err = result.expect_err("value object must refuse");
            let text = err.to_string();
            assert!(text.contains("non-entity class `Address`"), "{text}");
            assert!(text.contains("Card"), "{text}");
        }
    }
}
