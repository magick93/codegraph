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
        pg_table_name: codegraph_naming::to_snake_case(title),
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

/// The stored features the projection constructors read in the mock graph:
/// a required `String` and a required `int` (the payload-aligned `i32`),
/// keyed by the mox-authored camelCase names the bridge produces.
async fn ingest_book_properties(engine: &MockEngine) {
    use codegraph_core::types::PropertyNode;
    for (name, prop_type, pg_column_type, rust_field_type) in [
        ("title", "string", "TEXT", "String"),
        ("isbn", "string", "TEXT", "String"),
        ("playLength", "integer", "INTEGER", "i32"),
    ] {
        let prop = PropertyNode {
            name: name.to_string(),
            prop_type: prop_type.to_string(),
            description: None,
            format: None,
            is_required: true,
            is_nullable: false,
            is_id: false,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: codegraph_naming::to_snake_case(name),
            pg_column_type: pg_column_type.to_string(),
            rust_field_name: codegraph_naming::to_snake_case(name),
            rust_field_type: rust_field_type.to_string(),
            sea_orm_type: "String".to_string(),
            render_strategy: "direct_column".to_string(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: None,
            type_expr: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        };
        engine
            .ingest_property("BookType", "library/BookType", &prop)
            .await
            .unwrap();
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
    ingest_book_properties(&engine).await;
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
    assert!(
        content.contains("pub minutes: i32,"),
        "the boundary primitive mapping is the payload plane's (int→i32): {content}"
    );
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

/// Issue #768: a projection boundary emits ONE `from_row` construction site
/// beside its type — the read model computes, the finder and the endpoint
/// both call it, and neither can drift.
#[tokio::test]
async fn projection_boundaries_emit_a_from_row_constructor() {
    let files = generate(book_shapes()).await;
    let catalog = file(&files, "src/dto/catalog.rs");
    let content = &catalog.content;

    assert!(
        content.contains("use crate::library::book_type::dto_response::BookResponse;"),
        "the constructor reads the domain-types Response: {content}"
    );
    let impl_block = content
        .split("impl BookSummary {")
        .nth(1)
        .expect("BookSummary from_row impl")
        .split("\n}\n")
        .next()
        .expect("impl body");
    assert!(
        impl_block.contains("pub fn from_row(r: &BookResponse) -> Self {"),
        "{impl_block}"
    );
    // `select` copies the matching response field — `String` is not `Copy`,
    // so it clones.
    assert!(
        impl_block.contains("title: r.title.clone(),"),
        "{impl_block}"
    );
    // `derive` lowers the bounded arithmetic subset, adapted verbatim when
    // the member and column types agree.
    assert!(
        impl_block.contains("minutes: (r.play_length / 60),"),
        "{impl_block}"
    );

    // Non-projection shapes carry no constructor: the standalone boundary has
    // no source class, and the INPUT mirror is a request shape, not a read
    // model.
    assert!(!content.contains("impl NewBookInput {"), "{content}");
    assert!(!content.contains("impl BookMirror {"), "{content}");
}

/// Issue #768: explicit type adaptation — a `long` derive member over `int`
/// columns widens through `From`, never an `as` cast.
#[tokio::test]
async fn derive_member_over_narrower_columns_adapts_explicitly() {
    let mut shapes = book_shapes();
    shapes[1].members[1].type_json = serde_json::json!({"type": "primitive", "value": "long"});
    shapes[1].members[1].expression = Some("playLength + playLength".to_string());
    let files = generate(shapes).await;
    let content = &file(&files, "src/dto/catalog.rs").content;
    let impl_block = content
        .split("impl BookSummary {")
        .nth(1)
        .expect("BookSummary impl");
    assert!(
        impl_block.contains("minutes: (i64::from(r.play_length) + i64::from(r.play_length)),"),
        "explicit widening, never a cast: {impl_block}"
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
    ingest_book_properties(&engine).await;
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

    // ── Issue #767: the collection-level create body ────────────────────
    //
    // `registerBook` is abstract, returns the bound entity, and takes one
    // modeled input boundary — the create-shaped predicate. The route is
    // COLLECTION-level, the boundary IS the body, and the handler constructs
    // `CreateBookRequest` from it field by field (snake_case identity).
    assert!(
        handler.contains("/library/book/actions/register_book"),
        "the create-shaped route documents the collection-level path: {handler}"
    );
    assert!(
        handler.contains("request_body(content = domain_types::dto::catalog::NewBookDraft"),
        "the modeled boundary is the documented request body: {handler}"
    );
    assert!(
        handler.contains("let item = CreateBookRequest {"),
        "the boundary is mapped onto the entity's create DTO: {handler}"
    );
    assert!(
        handler.contains("title: payload.title,")
            && handler.contains("isbn: payload.isbn,")
            && handler.contains("play_length: payload.play_length,"),
        "every boundary member maps by snake_case identity: {handler}"
    );
    assert!(
        handler.contains("Ok((StatusCode::CREATED,"),
        "a collection-level create answers 201: {handler}"
    );
    // The non-create-shaped shapes keep their old lowerings: a primitive
    // parameter is still a door to the 501 seam, and `archive` (a parameterless
    // `save` delegation) is not reclassified as a create.
    assert!(
        handler.contains("/actions/import_book\"")
            && !handler.contains("book/actions/import_book\""),
        "a primitive-parameter op keeps its ITEM-addressed 501 seam: {handler}"
    );
    assert!(
        handler.contains("/actions/archive\"") && !handler.contains("book/actions/archive\""),
        "a parameterless `save` delegation is not reclassified as a create: {handler}"
    );
    let router = fs::read_to_string(output.join("src/api/library/router.rs")).unwrap();
    assert!(
        router.contains(".route(\"/actions/register_book\""),
        "the router mounts the collection-level action route: {router}"
    );

    // ── Issue #768: the projection READ endpoints ────────────────────────
    //
    // `summarise(Book book)` (record-scoped, single) is item-addressed;
    // `allSummaries()` (stateless, Vec) is collection-addressed. Both
    // construct through the boundary's `from_row`.
    assert!(
        handler.contains("service_op_summarise")
            && handler.contains("domain_types::dto::catalog::BookSummary::from_row(&response)"),
        "the item projection read constructs through from_row: {handler}"
    );
    assert!(
        handler.contains("body = domain_types::dto::catalog::BookSummary"),
        "the utoipa response documents the projection type: {handler}"
    );
    assert!(
        handler.contains("service_op_all_summaries")
            && handler
                .contains("rows.iter().map(domain_types::dto::catalog::BookSummary::from_row)"),
        "the collection read maps the collection through from_row: {handler}"
    );
    assert!(
        handler.contains("body = [domain_types::dto::catalog::BookSummary]"),
        "the collection read documents the Vec projection: {handler}"
    );
    assert!(
        router.contains("{book_id}/actions/summarise"),
        "the item read mounts item-addressed: {router}"
    );
    assert!(
        router.contains(".route(\"/actions/all_summaries\""),
        "the collection read mounts at the collection level: {router}"
    );
    assert!(
        !router.contains("{book_id}/actions/register_book"),
        "the create-shaped route never carries an id path: {router}"
    );

    // hurl: the create-shaped leg posts the boundary at the collection route
    // and pins 201; the seam op still pins 501.
    // The contract's number prefix follows generation order — discover it by
    // suffix (the ops suite's own convention) instead of pinning a literal.
    let hurl_dir = output.join("hurl");
    let actions_path = fs::read_dir(&hurl_dir)
        .expect("hurl dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("_library_book_actions.hurl"))
        })
        .expect("the actions contract is emitted for an entity with service ops");
    let actions = fs::read_to_string(actions_path).unwrap();
    assert!(
        actions.contains("/actions/register_book") && actions.contains("HTTP 201"),
        "the actions contract pins the collection-level create: {actions}"
    );
    assert!(
        actions.contains("HTTP 501"),
        "the abstract op with a primitive parameter keeps its 501 seam: {actions}"
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

// ── Issue #767: the boundary → request-DTO mapping ────────────────────────
//
// The pipeline test above pins the happy path end to end. These pins drive
// the mapping directly so every refusal class is exercised by a fixture that
// CAN fail: an unmappable member, a type disagreement, and an uncovered
// required field are named errors (never a silently dropped field), while an
// uncovered OPTIONAL field defaults to `None`.

use codegraph_core::types::PropertyNode;
use codegraph_generate::ddd::body_params::BodyParam;
use codegraph_generate::ddd::boundary_map::{RequestDtoKind, plan_boundary_request};

/// A stored BookType column. `rust_type`/`pg_type` mirror what the mox bridge
/// produces for `String` and `int` (so the DTO field types are the real ones).
fn stored(name: &str, rust_type: &str, pg_type: &str, required: bool) -> PropertyNode {
    PropertyNode {
        is_id: false,
        name: name.to_string(),
        prop_type: if rust_type == "i32" {
            "integer"
        } else {
            "string"
        }
        .to_string(),
        description: None,
        format: None,
        is_required: required,
        is_nullable: !required,
        is_array: false,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        min_items: None,
        max_items: None,
        pg_column_name: name.to_string(),
        pg_column_type: pg_type.to_string(),
        rust_field_name: name.to_string(),
        rust_field_type: rust_type.to_string(),
        sea_orm_type: "Text".to_string(),
        render_strategy: "scalar".to_string(),
        ref_target: None,
        classification: None,
        projection: None,
        classification_kind: None,
        type_expr: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

/// A BookType schema with `title`/`isbn` required strings, `play_length` a
/// required `int`, and `subtitle` an OPTIONAL string.
async fn mapping_engine() -> MockEngine {
    let engine = MockEngine::builder().build();
    engine
        .ingest_schema(&schema("BookType", "library"))
        .await
        .unwrap();
    for prop in [
        stored("title", "String", "TEXT", true),
        stored("isbn", "String", "TEXT", true),
        stored("play_length", "i32", "INTEGER", true),
        stored("subtitle", "String", "TEXT", false),
        PropertyNode {
            is_array: true,
            prop_type: "array".to_string(),
            ..stored("tags", "String", "TEXT", false)
        },
    ] {
        engine
            .ingest_property("BookType", "library/BookType", &prop)
            .await
            .unwrap();
    }
    engine
}

/// A `NewBookDraft`-shaped boundary with the given members.
fn draft(members: Vec<DddDtoMember>) -> DddDtoShapeNode {
    DddDtoShapeNode {
        application: "Library".to_string(),
        module: "catalog".to_string(),
        name: "NewBookDraft".to_string(),
        source_json: None,
        direction: "input".to_string(),
        members,
        ordinal: 1,
    }
}

fn draft_param() -> BodyParam {
    BodyParam {
        name: "input".to_string(),
        rust_type: "domain_types::dto::catalog::NewBookDraft".to_string(),
        optional: false,
        direct_body: true,
        sample: serde_json::json!({}),
        request_body: Some("domain_types::dto::catalog::NewBookDraft".to_string()),
    }
}

async fn plan(
    engine: &MockEngine,
    shape: &DddDtoShapeNode,
) -> Result<codegraph_generate::ddd::boundary_map::BoundaryRequestPlan, String> {
    plan_kind(engine, shape, RequestDtoKind::Create).await
}

async fn plan_kind(
    engine: &MockEngine,
    shape: &DddDtoShapeNode,
    kind: RequestDtoKind,
) -> Result<codegraph_generate::ddd::boundary_map::BoundaryRequestPlan, String> {
    let shapes = std::collections::BTreeMap::from([(shape.name.clone(), shape.clone())]);
    let module_of = std::collections::BTreeMap::from([(shape.name.clone(), "catalog".to_string())]);
    plan_boundary_request(
        codegraph_generate::ddd::boundary_map::BoundaryTarget {
            db: engine,
            config: &config(),
            domain: "library",
            entity_title: "BookType",
        },
        shape,
        codegraph_generate::ddd::boundary_map::BoundaryShapeSet {
            shapes: &shapes,
            module_of: &module_of,
        },
        kind,
        &draft_param(),
    )
    .await
}

fn int_member(name: &str) -> DddDtoMember {
    DddDtoMember {
        expression: None,
        path: Vec::new(),
        ..member(name, "declared", int_type())
    }
}

#[tokio::test]
async fn create_body_mapping_refuses_a_member_with_no_request_field() {
    let engine = mapping_engine().await;
    let shape = draft(vec![
        member("title", "declared", string_type()),
        member("bogus", "declared", string_type()),
    ]);
    let err = plan(&engine, &shape).await.expect_err("must refuse");
    assert!(
        err.contains("member `bogus` matches no `CreateBookRequest` field"),
        "the refusal names the member and the DTO: {err}"
    );
    assert!(
        err.contains("accepted: isbn, play_length, subtitle, tags, title"),
        "the refusal lists the accepted fields: {err}"
    );
}

#[tokio::test]
async fn create_body_mapping_refuses_a_type_disagreement_naming_both_sides() {
    let engine = mapping_engine().await;
    // `play_length` is an i32 column; a boundary member typed `string`
    // cannot fill it.
    let shape = draft(vec![
        member("title", "declared", string_type()),
        member("isbn", "declared", string_type()),
        member("playLength", "declared", string_type()),
    ]);
    let err = plan(&engine, &shape).await.expect_err("must refuse");
    assert!(
        err.contains("member `playLength` types `String`")
            && err.contains("`CreateBookRequest.play_length` types `i32`"),
        "the refusal names BOTH spellings: {err}"
    );
}

#[tokio::test]
async fn create_body_mapping_refuses_an_uncovered_required_field() {
    let engine = mapping_engine().await;
    // No `play_length` member: the create command would have no value for a
    // NOT NULL column, so the mapping refuses instead of guessing.
    let shape = draft(vec![
        member("title", "declared", string_type()),
        member("isbn", "declared", string_type()),
    ]);
    let err = plan(&engine, &shape).await.expect_err("must refuse");
    assert!(
        err.contains("does not carry the REQUIRED `CreateBookRequest` field `play_length`"),
        "the refusal names the missing required field: {err}"
    );
}

#[tokio::test]
async fn create_body_mapping_defaults_an_uncovered_optional_field() {
    let engine = mapping_engine().await;
    let shape = draft(vec![
        member("title", "declared", string_type()),
        member("isbn", "declared", string_type()),
        int_member("playLength"),
    ]);
    let plan = plan(&engine, &shape).await.expect("maps");
    assert!(
        plan.construct.contains("title: payload.title,")
            && plan.construct.contains("play_length: payload.play_length,"),
        "mapped members are copied by identity: {}",
        plan.construct
    );
    assert!(
        plan.construct.contains("subtitle: None,")
            && plan.defaulted == vec!["subtitle".to_string(), "tags".to_string()],
        "an optional field the boundary omits defaults to None (documented): {} / {:?}",
        plan.construct,
        plan.defaulted
    );
}

/// An OPTIONAL array column keeps `Option<Vec<T>>` in the create DTO while the
/// boundary's `[0..*]` is a plain `Vec<T>` — the mapping wraps it in `Some`
/// rather than refusing a legitimate model (issue #767 review).
#[tokio::test]
async fn create_body_mapping_wraps_an_optional_array_member() {
    let engine = mapping_engine().await;
    let shape = draft(vec![
        member("title", "declared", string_type()),
        member("isbn", "declared", string_type()),
        int_member("playLength"),
        DddDtoMember {
            multiplicity_json: Some(serde_json::json!({"lower": 0, "upper": "unbounded"})),
            ..member("tags", "declared", string_type())
        },
    ]);
    let plan = plan(&engine, &shape).await.expect("maps");
    assert!(
        plan.construct.contains("tags: Some(payload.tags),"),
        "an optional array column wraps the member: {}",
        plan.construct
    );
}

/// The UPDATE arm wraps every mapped member in `Some(..)` (the update DTO's
/// `Option<T>` shape) and passes an already-optional member through — the
/// comparison unit is the member WITHOUT its own `Option` (issue #767 review).
#[tokio::test]
async fn update_body_mapping_wraps_required_members_and_passes_optionals() {
    let engine = mapping_engine().await;
    let shape = draft(vec![
        member("title", "declared", string_type()),
        DddDtoMember {
            multiplicity_json: Some(serde_json::json!({"lower": 0, "upper": {"finite": 1}})),
            ..member("subtitle", "declared", string_type())
        },
        // The many-valued member is the case the base-type comparison got
        // wrong: `String` vs the field's `Vec<String>`.
        DddDtoMember {
            multiplicity_json: Some(serde_json::json!({"lower": 0, "upper": "unbounded"})),
            ..member("tags", "declared", string_type())
        },
    ]);
    let plan = plan_kind(&engine, &shape, RequestDtoKind::Update)
        .await
        .expect("maps");
    assert!(
        plan.construct.contains("title: Some(payload.title),"),
        "a required member fills the Option update slot: {}",
        plan.construct
    );
    assert!(
        plan.construct.contains("subtitle: payload.subtitle,"),
        "an optional member IS the Option update slot: {}",
        plan.construct
    );
    assert!(
        plan.construct.contains("tags: Some(payload.tags),"),
        "a many-valued member compares against the field's Vec, not its base: {}",
        plan.construct
    );
    assert!(
        plan.construct.contains("play_length: None,") && plan.construct.contains("isbn: None,"),
        "uncovered update fields default to None: {}",
        plan.construct
    );
}

/// The UPDATE arm seals the member universe like the DTO template does: an
/// immutable field is not accepted (it is absent from the update DTO), so a
/// boundary carrying one is refused by name rather than silently dropped.
#[tokio::test]
async fn update_body_mapping_refuses_a_member_the_update_dto_omits() {
    let engine = mapping_engine().await;
    let shape = draft(vec![member("bogus", "declared", string_type())]);
    let err = plan_kind(&engine, &shape, RequestDtoKind::Update)
        .await
        .expect_err("must refuse");
    assert!(
        err.contains("member `bogus` matches no `UpdateBookRequest` field"),
        "the refusal names the update DTO: {err}"
    );
}

/// Issue #767: a create-shaped op whose boundary parameter is OPTIONAL is a
/// NAMED hard refusal, and it fires BEFORE any tree is written (the
/// pre-generation validation walk — a refusal surfacing only in the
/// non-fatal generator phase would exit 0 with the action surface missing).
#[tokio::test]
async fn optional_create_body_is_refused_before_any_tree() {
    use std::fs;
    use std::path::PathBuf;

    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dto_boundaries_optional");
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
    let mox_files = vec![fixture.join("optional.mox")];
    let ddd_files = vec![fixture.join("optional.ddd")];

    let err = codegraph::driver::run(codegraph::driver::RunArgs {
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
    .expect_err("an optional create body must fail the run");

    // `Error::Validation` carries the finding COUNT; the named refusal is
    // printed by the run path (and the mapping/predicate KATs pin its
    // wording). What this pin proves is the part #1 of the review asked for:
    // the run FAILS, and it fails before any tree exists.
    let message = format!("{err}");
    assert!(
        message.contains("Validation error"),
        "the optional create body must fail the validation pass: {message}"
    );
    assert!(
        !output.join("src").exists(),
        "a refusal must fire BEFORE any tree is written (no src/ dir)"
    );
}

/// Issue #768 negative: a derive member the arithmetic subset cannot
/// compute (a STRING-typed derivation — legal upstream, refused here)
/// fails the VALIDATION pass by name, and fails it before any tree exists.
#[tokio::test]
async fn projection_derive_outside_the_subset_refuses_before_any_tree() {
    use std::fs;
    use std::path::PathBuf;

    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/dto_boundaries_projection_bad");
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
    let mox_files = vec![fixture.join("bad.mox")];
    let ddd_files = vec![fixture.join("bad.ddd")];

    let err = codegraph::driver::run(codegraph::driver::RunArgs {
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
    .expect_err("a non-numeric derive member must fail the run");

    let message = format!("{err}");
    assert!(
        message.contains("Validation error"),
        "the projection refusal must fail the validation pass: {message}"
    );
    assert!(
        !output.join("src").exists(),
        "a refusal must fire BEFORE any tree is written (no src/ dir)"
    );
}

/// Issue #768 review: the pre-generation walker's OP-SET arm — an
/// item-addressed projection read over an entity whose design exposes no
/// `read` op refuses PRE-TREE (the lowering layer would only refuse at
/// generation). Pinned over the mock graph the boundary tests already
/// build: no repository in the model ⇒ an EMPTY op set ⇒ the requirement
/// cannot be met.
#[tokio::test]
async fn walker_refuses_an_item_projection_read_over_an_entity_without_read() {
    use codegraph_core::types::{DddServiceNode, DddServiceOperation};

    let engine = MockEngine::builder().build();
    engine
        .ingest_schema(&schema("BookType", "library"))
        .await
        .unwrap();
    ingest_book_properties(&engine).await;

    let mut shapes = vec![{
        let mut s = book_shapes().remove(1); // BookSummary (output, from Book)
        s.members.clear(); // plans clean — only the op-set refusal fires
        s
    }];
    shapes.push({
        let mut s = book_shapes().remove(2); // NewBookInput (standalone)
        s.ordinal = 3;
        s
    });

    let op = DddServiceOperation {
        name: "summarise".to_string(),
        params: Vec::new(),
        return_type: Some(serde_json::json!({ "dto": "BookSummary" })),
        return_multiplicity: None,
        is_protected: false,
        capabilities: Vec::new(),
        condition: None,
        condition_paths: Vec::new(),
        bodies: Default::default(),
        body_receiver: None,
        throws: Vec::new(),
        emits: Vec::new(),
        transition: None,
        delegation_target: None,
        delegation_operation: None,
        child_scope: None,
        invokes: Vec::new(),
        ordinal: 0,
    };
    let mut model = model(shapes);
    // The routes predicate resolves the BOUND entity's design class — the
    // mock model carries the design node a real model always has.
    model.designs = vec![codegraph_core::types::DddDesignNode {
        application: "Library".to_string(),
        module: "catalog".to_string(),
        class: "Book".to_string(),
        resolved_title: Some("BookType".to_string()),
        stereotype: "entity".to_string(),
        ordinal: 0,
        is_abstract: false,
        flags: Default::default(),
        machine_json: None,
        machine_ir_json: None,
        machine_provenance_json: None,
        aggregate_boundary: None,
        invariants: Vec::new(),
        child_operations: Vec::new(),
    }];
    model.services = vec![DddServiceNode {
        application: "Library".to_string(),
        module: "catalog".to_string(),
        name: "Catalog".to_string(),
        description: None,
        ordinal: 1,
        dependencies: Vec::new(),
        decision_dependencies: Vec::new(),
        operations: vec![op],
    }];
    engine.ingest_ddd_model(&model).await.unwrap();

    let refusals =
        codegraph_generate::ddd::boundary_derive::projection_refusals(&engine, &config())
            .await
            .expect("walker");
    assert!(
        refusals
            .iter()
            .any(|r| r.contains("summarise") && r.contains("exposes no `read`")),
        "the walker must refuse the no-read projection read: {refusals:?}"
    );
}
