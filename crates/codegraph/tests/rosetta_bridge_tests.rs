//! Rosetta data-plane bridge tests (issue #256).
//!
//! The bridge contract per `docs/rosetta-gap-analysis.md`: types/choices →
//! SchemaNode + properties; attributes → PropertyNode +
//! ReferencesSchema/ItemsOf; builtins → JSON-path primitive mappings;
//! enums → CodeList + EnumValue + codelist SchemaNode; extends →
//! ingest-time merge + ExtendsSchema; metadata/conditions →
//! custom_annotations payloads; namespaces recorded but NOT domains;
//! out-of-plane elements → needs_review, never silent drops.

use std::collections::HashMap;
use std::path::PathBuf;

use codegraph_backend::{create_backend, BackendConfig};
use codegraph_config::config::{parse_domain_config, DomainConfig};
use codegraph_type_contracts::RefClassificationKind;

/// The bridge fixture: a type with builtins, a reference, an enum ref, an
/// array, cardinalities, metadata + a condition; an enum with displayName;
/// an enum extends chain; a type extends with an override; a choice; and
/// an out-of-plane func.
const MODEL: &str = r#"
namespace rosetta.bridge
version "1.0.0"

type Product: <"A tradable product">
	[metadata key]
	productId string (1..1) <"identifier"> [metadata id]
	label string (0..1)
	price number (1..1)
	quantity int (0..1)
	active boolean (1..1)
	availableFrom date (0..1)
	createdOn dateTime (0..1)
	tags string (0..*)
	currency Currency (1..1)
	holder Holder (0..1)

	condition PositivePrice: price > 0

enum Currency: <"ISO currencies">
	EUR <"Euro">
	USD displayName "US Dollar"

enum SubCurrency extends Currency:
	JPY

type Holder:
	holderId string (1..1)

type ListedProduct extends Product:
	override label string (1..1)
	exchange string (0..1)

choice ProductType:
	ListedProduct
	Holder

func Quote: <"quote a product">
	[codeImplementation]
	inputs:
		p Product (1..1)
	output:
		result number (1..1)

	alias basePrice: p -> price

	set result:
		p -> price
"#;

const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.bridge]
label = "Bridge"
schema_dir = "bridge"
postgres_schema = "bridge"
"#;

struct BridgeGraph {
    backend: codegraph_backend::Backend,
    outcome: codegraph::ingest::rosetta_ingest::RosettaIngestOutcome,
    _dir: tempfile::TempDir,
}

async fn bridge_fixture() -> BridgeGraph {
    bridge_fixture_with(|_path, _model| {}).await
}

/// `decorate` receives the fixture path before ingestion (tests that need
/// extra files, e.g. cross-domain resolution fixtures, write them there).
async fn bridge_fixture_with<F>(mut decorate: F) -> BridgeGraph
where
    F: FnMut(&PathBuf, &PathBuf),
{
    let dir = tempfile::tempdir().unwrap();
    let model_path = dir.path().join("bridge.rosetta");
    std::fs::write(&model_path, MODEL).unwrap();
    let domains_path = dir.path().join("domains.toml");
    std::fs::write(&domains_path, DOMAINS_TOML).unwrap();
    decorate(&dir.path().to_path_buf(), &model_path);

    let backend = create_backend(&BackendConfig::default()).await.unwrap();
    let domain_config: DomainConfig = parse_domain_config(&domains_path).unwrap();
    let outcome = codegraph::ingest::rosetta_ingest::ingest_rosetta_files(
        backend.ingestor(),
        backend.querier(),
        &[model_path],
        &domain_config,
        "Type",
    )
    .await
    .unwrap();
    BridgeGraph {
        backend,
        outcome,
        _dir: dir,
    }
}

#[tokio::test]
async fn builtin_typed_attributes_map_to_json_path_primitives() {
    let g = bridge_fixture().await;
    let props = g.backend.querier().get_properties("Product").await.unwrap();
    let by_name: HashMap<&str, &codegraph_core::types::PropertyNode> =
        props.iter().map(|p| (p.name.as_str(), p)).collect();

    let product_id = by_name["productId"];
    assert_eq!(product_id.prop_type, "string");
    assert_eq!(product_id.pg_column_type, "TEXT");
    assert!(product_id.is_required);
    assert!(!product_id.is_nullable);

    let price = by_name["price"];
    assert_eq!(price.prop_type, "number");
    assert_eq!(price.pg_column_type, "DOUBLE PRECISION");

    let quantity = by_name["quantity"];
    assert_eq!(quantity.prop_type, "integer");
    assert_eq!(quantity.pg_column_type, "INTEGER");
    assert!(!quantity.is_required);
    assert!(quantity.is_nullable);

    let active = by_name["active"];
    assert_eq!(active.prop_type, "boolean");
    assert_eq!(active.pg_column_type, "BOOLEAN");

    let available_from = by_name["availableFrom"];
    assert_eq!(available_from.pg_column_type, "DATE");
    assert_eq!(available_from.format.as_deref(), Some("date"));

    let created_on = by_name["createdOn"];
    assert_eq!(created_on.pg_column_type, "TIMESTAMPTZ");
    assert_eq!(created_on.format.as_deref(), Some("date-time"));
}

#[tokio::test]
async fn cardinality_maps_to_required_and_array_bits() {
    let g = bridge_fixture().await;
    let props = g.backend.querier().get_properties("Product").await.unwrap();
    let by_name: HashMap<&str, &codegraph_core::types::PropertyNode> =
        props.iter().map(|p| (p.name.as_str(), p)).collect();

    let product_id = by_name["productId"];
    assert!(product_id.is_required);
    assert!(!product_id.is_array);

    let label = by_name["label"];
    assert!(!label.is_required);
    assert!(!label.is_array);

    let tags = by_name["tags"];
    assert!(!tags.is_required);
    assert!(tags.is_array);
    assert_eq!(tags.rust_field_type, "Vec<String>");
}

#[tokio::test]
async fn enum_bridges_to_codelist_schema_codelist_and_values() {
    let g = bridge_fixture().await;
    let q = g.backend.querier();

    let schemas = q.list_schemas(None).await.unwrap();
    let currency = schemas.iter().find(|s| s.title == "Currency").unwrap();
    assert!(currency.is_codelist);
    assert_eq!(currency.classification, "codelist");
    assert_eq!(
        currency
            .custom_annotations
            .get("origin")
            .and_then(|v| v.as_str()),
        Some("rosetta")
    );

    let codelists = q.list_codelists().await.unwrap();
    let currency_list = codelists.iter().find(|c| c.name == "Currency").unwrap();
    assert_eq!(currency_list.pg_table_name, "currency");

    let values = q.get_enum_values("Currency").await.unwrap();
    assert_eq!(values.len(), 2);
    // `EUR <"Euro">` — the angle-bracket text is the DEFINITION; only the
    // explicit `displayName` keyword sets display_name.
    assert_eq!(values[0].value, "EUR");
    assert_eq!(values[0].display_name, None);
    assert_eq!(values[1].value, "USD");
    assert_eq!(values[1].display_name.as_deref(), Some("US Dollar"));
}

#[tokio::test]
async fn enum_extends_merges_parent_values_first() {
    let g = bridge_fixture().await;
    let values = g
        .backend
        .querier()
        .get_enum_values("SubCurrency")
        .await
        .unwrap();
    let names: Vec<&str> = values.iter().map(|v| v.value.as_str()).collect();
    assert_eq!(names, vec!["EUR", "USD", "JPY"]);
}

#[tokio::test]
async fn enum_typed_attribute_keeps_codelist_ref_target_and_kind() {
    let g = bridge_fixture().await;
    let props = g.backend.querier().get_properties("Product").await.unwrap();
    let currency = props.iter().find(|p| p.name == "currency").unwrap();
    assert_eq!(
        currency.classification_kind,
        Some(RefClassificationKind::CodelistReference)
    );
    assert_eq!(currency.ref_target.as_deref(), Some("Currency"));
    assert_eq!(currency.render_strategy, "codelist");
}

#[tokio::test]
async fn type_reference_carries_entity_ref_and_referenceschema_edge() {
    let g = bridge_fixture().await;
    let q = g.backend.querier();
    let props = q.get_properties("Product").await.unwrap();
    let holder = props.iter().find(|p| p.name == "holder").unwrap();
    assert_eq!(
        holder.classification_kind,
        Some(RefClassificationKind::EntityReference)
    );
    assert_eq!(holder.ref_target.as_deref(), Some("Holder"));
}

#[tokio::test]
async fn extends_merges_ancestor_attributes_and_honors_override() {
    let g = bridge_fixture().await;
    let props = g
        .backend
        .querier()
        .get_properties("ListedProduct")
        .await
        .unwrap();
    let mut names: Vec<&str> = props.iter().map(|p| p.name.as_str()).collect();
    names.sort_unstable();

    // Full ancestor set + own attribute, no duplicates. (get_properties
    // serves name-sorted — the INSERTION-order contract, ancestors first,
    // is pinned against generated output in #259's suite, mirroring the
    // WP1.1 finding that query order ≠ generator-visible order.)
    assert_eq!(
        names,
        vec![
            "active",
            "availableFrom",
            "createdOn",
            "currency",
            "exchange",
            "holder",
            "label",
            "price",
            "productId",
            "quantity",
            "tags"
        ]
    );

    // The override wins: label becomes required.
    let label = props.iter().find(|p| p.name == "label").unwrap();
    assert!(label.is_required);

    // Inherited attribute survives exactly once.
    assert_eq!(
        props.iter().filter(|p| p.name == "productId").count(),
        1,
        "inherited attributes must not duplicate"
    );
}

#[tokio::test]
async fn extends_schema_edge_recorded_for_type_extends() {
    let g = bridge_fixture().await;
    // The bridge ingests ExtendsSchema with composition_type "allOf"; the
    // observable graph effect: the target schema exists and the child's
    // composition tree includes the parent (same machinery the JSON path
    // uses). Direct edge access goes through get_allof_targets.
    let targets = g
        .backend
        .querier()
        .get_allof_targets("ListedProduct")
        .await
        .unwrap();
    assert!(targets.iter().any(|t| t == "Product"));
}

#[tokio::test]
async fn choice_type_bridges_with_rosetta_choice_annotation() {
    let g = bridge_fixture().await;
    let schemas = g.backend.querier().list_schemas(None).await.unwrap();
    let choice = schemas.iter().find(|s| s.title == "ProductType").unwrap();
    assert_eq!(
        choice
            .custom_annotations
            .get("rosetta_choice")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    // Choice options keep implicit (0..1) cardinality → optional props.
    let props = g
        .backend
        .querier()
        .get_properties("ProductType")
        .await
        .unwrap();
    assert!(props.iter().all(|p| !p.is_required));
    assert_eq!(g.outcome.stats.choices, 1);
}

#[tokio::test]
async fn metadata_and_conditions_recorded_as_custom_annotation_payloads() {
    let g = bridge_fixture().await;
    let schemas = g.backend.querier().list_schemas(None).await.unwrap();
    let product = schemas.iter().find(|s| s.title == "Product").unwrap();

    // Type-level annotations (`[metadata key]` in the type header block).
    let schema_annotations = product
        .custom_annotations
        .get("rosetta_annotations")
        .and_then(|v| v.as_array())
        .expect("type-level annotation refs array");
    assert!(!schema_annotations.is_empty());

    // Attribute-level annotations ride the property-keyed map.
    let attr_annotations = product
        .custom_annotations
        .get("rosetta_attribute_annotations")
        .and_then(|v| v.get("productId"))
        .expect("productId metadata must be recorded");
    let annotations = attr_annotations
        .get("annotations")
        .and_then(|v| v.as_array())
        .expect("annotation refs array");
    assert!(!annotations.is_empty());

    // Conditions embed Expr::to_json() payloads.
    let conditions = product
        .custom_annotations
        .get("rosetta_conditions")
        .and_then(|v| v.as_array())
        .expect("conditions array");
    assert_eq!(conditions.len(), 1);
    assert_eq!(
        conditions[0].get("name").and_then(|v| v.as_str()),
        Some("PositivePrice")
    );
    let expression = conditions[0].get("expression").expect("expression payload");
    assert!(
        expression.get("kind").is_some(),
        "Expr::to_json is kind-tagged"
    );
    assert_eq!(g.outcome.stats.conditions_recorded, 1);
}

#[tokio::test]
async fn namespace_recorded_but_never_mapped_onto_domains() {
    let g = bridge_fixture().await;
    let schemas = g.backend.querier().list_schemas(None).await.unwrap();
    for schema in &schemas {
        // Every bridged schema records its rosetta namespace...
        assert_eq!(
            schema
                .custom_annotations
                .get("rosetta_namespace")
                .and_then(|v| v.as_str()),
            Some("rosetta.bridge"),
            "schema {} missing namespace provenance",
            schema.title
        );
        // ...and lands in the fallback domain (bridge config domain), NOT
        // a namespace-derived domain.
        assert_eq!(schema.domain.as_deref(), Some("bridge"));
    }
    assert_eq!(g.outcome.stats.namespaces, 1);
}

#[tokio::test]
async fn out_of_plane_elements_counted_as_needs_review() {
    let g = bridge_fixture().await;
    // #263 landed function nodes: the fixture's func is bridged as a
    // FunctionNode (alias + operation payload), NOT recorded as
    // needs_review. #264 landed rule nodes the same way (this model has
    // none), so nothing remains out-of-plane.
    assert_eq!(
        g.outcome.stats.functions_ingested, 1,
        "the fixture func bridges"
    );
    assert!(
        !g.outcome
            .stats
            .needs_review_names
            .iter()
            .any(|n| n.starts_with("func ") || n.starts_with("rule ")),
        "functions and rules are in-plane since #263/#264: {:?}",
        g.outcome.stats.needs_review_names
    );

    let functions = g.backend.querier().list_functions().await.unwrap();
    let quote = functions
        .iter()
        .find(|f| f.name == "Quote")
        .expect("func Quote lands as a FunctionNode");
    assert_eq!(quote.domain.as_deref(), Some("bridge"));
    assert_eq!(
        quote.output.as_ref().map(|o| o.type_ref.as_str()),
        Some("number")
    );
    assert_eq!(quote.aliases.len(), 1);
    assert_eq!(quote.aliases[0].name, "basePrice");
    assert!(quote.aliases[0].expr_json.contains("FeatureCall"));
    assert_eq!(quote.operations.len(), 1);
    assert_eq!(quote.operations[0].assign_root, "result");
    assert_eq!(
        quote.properties.get("origin").and_then(|v| v.as_str()),
        Some("rosetta")
    );
}

#[tokio::test]
async fn provenance_is_origin_rosetta_not_source_mox() {
    let g = bridge_fixture().await;
    let schemas = g.backend.querier().list_schemas(None).await.unwrap();
    for schema in &schemas {
        assert_eq!(
            schema
                .custom_annotations
                .get("origin")
                .and_then(|v| v.as_str()),
            Some("rosetta")
        );
        assert!(
            !schema.custom_annotations.contains_key("source"),
            "rosetta must not ride the mox 'source' provenance key"
        );
    }
}

#[tokio::test]
async fn syntax_error_is_a_hard_error_with_span() {
    let dir = tempfile::tempdir().unwrap();
    let model_path = dir.path().join("broken.rosetta");
    std::fs::write(&model_path, "type Broken:\n\tfield no_type_here (1..1)\n").unwrap();
    let domains_path = dir.path().join("domains.toml");
    std::fs::write(&domains_path, DOMAINS_TOML).unwrap();
    let backend = create_backend(&BackendConfig::default()).await.unwrap();
    let domain_config: DomainConfig = parse_domain_config(&domains_path).unwrap();
    let result = codegraph::ingest::rosetta_ingest::ingest_rosetta_files(
        backend.ingestor(),
        backend.querier(),
        &[model_path],
        &domain_config,
        "Type",
    )
    .await;
    let err = result.expect_err("a syntax-broken model must hard-error");
    let msg = err.to_string();
    assert!(
        msg.contains("rosetta"),
        "error should name the surface: {msg}"
    );
}

#[tokio::test]
async fn resolution_error_is_a_hard_error() {
    let dir = tempfile::tempdir().unwrap();
    let model_path = dir.path().join("unresolved.rosetta");
    std::fs::write(
        &model_path,
        "namespace rosetta.broken\n\ntype Widget:\n\tgadget Bogus (1..1)\n",
    )
    .unwrap();
    let domains_path = dir.path().join("domains.toml");
    std::fs::write(&domains_path, DOMAINS_TOML).unwrap();
    let backend = create_backend(&BackendConfig::default()).await.unwrap();
    let domain_config: DomainConfig = parse_domain_config(&domains_path).unwrap();
    let result = codegraph::ingest::rosetta_ingest::ingest_rosetta_files(
        backend.ingestor(),
        backend.querier(),
        &[model_path],
        &domain_config,
        "Type",
    )
    .await;
    let err = result.expect_err("an unresolved reference must hard-error");
    assert!(err.to_string().contains("resolution"), "{err}");
}

#[tokio::test]
async fn stats_bridge_counts_are_consistent() {
    let g = bridge_fixture().await;
    let s = &g.outcome.stats;
    assert_eq!(s.files, 1);
    // Product, Holder, ListedProduct, ProductType = 4 data types.
    assert_eq!(s.types, 4);
    // Currency + SubCurrency.
    assert_eq!(s.enums, 2);
    assert_eq!(s.enum_schemas, 2);
    // Only TYPE extends creates an ExtendsSchema edge (ListedProduct);
    // enum extends merges values without an edge — the JSON codelist path
    // has no extends concept for the DDL machinery to consume.
    assert_eq!(s.extends, 1);
    assert!(!s.bridged_titles.is_empty());
}
