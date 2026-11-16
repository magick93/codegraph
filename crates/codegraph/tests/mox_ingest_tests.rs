//! mox domain ingest tests (issue #218).
//!
//! Ingest layer: a `.mox` domain source (vocabulary with facets + entries, a
//! class operation with an `expr` body, derived features) is compiled
//! in-process via `phero_driver::compile_files` and lands in the graph as
//! Vocabulary/Operation/DerivedFeature nodes with BelongsToClass /
//! VocabularyInPackage edges. Class attachment matches by NAME against
//! schema-ingested entities; mismatches warn and count as skipped — they
//! never fail ingestion.
//!
//! The consuming-generator layer (DTO read-only/exclusion) lives in
//! `mox_dto_tests.rs`; full-tree byte-identity with no mox ingested is
//! pinned by `policy_rls_tests::flag_off_full_output_hashes`, whose
//! snapshot was captured from a pre-mox master.

use std::fs;
use std::path::{Path, PathBuf};

use codegraph_core::traits::GraphQuerier;
use codegraph_grafeo::GrafeoEngine;

const VOCAB_SNAPSHOT: &str = r#"{
  "vocabulary": "iso:4217",
  "version": "2024-01-01",
  "entries": [
    { "alpha3": "USD", "symbol": "$", "minorUnits": 2 },
    { "alpha3": "EUR", "symbol": "€", "minorUnits": 2 }
  ]
}"#;

/// The canonical ingest fixture: a vocabulary with typed facets and vendored
/// entries, a class with a stored features, an operation with an `expr` body,
/// and a derived feature.
const CANONICAL_MOX: &str = r#"
package recruiting

vocabulary Currency from "iso:4217" {
    version "2024-01-01"
    key alpha3
    facet int minorUnits
    facet String symbol
}

class CandidateType {
    String candidateId
    String status

    derived String upperId {
        expr { candidateId }
    }

    op int idLength(String prefix) {
        expr { 1 }
    }
}
"#;

/// Same, plus a class that matches no ingested schema entity — its derived
/// feature must still ingest but must warn and count as skipped.
const CANONICAL_MOX_WITH_GHOST: &str = r#"
package recruiting

vocabulary Currency from "iso:4217" {
    version "2024-01-01"
    key alpha3
    facet int minorUnits
    facet String symbol
}

class CandidateType {
    String candidateId
    String status

    derived String upperId {
        expr { candidateId }
    }

    op int idLength(String prefix) {
        expr { 1 }
    }
}

class GhostType {
    String shadow

    derived String boo {
        expr { shadow }
    }
}
"#;

/// Probe: compile a mox source exercising classes, enums, datatypes,
/// extends, and every feature kind, and dump the lowered phero_ir model.
/// Pins what `phero_driver::compile_files` actually populates (issue #229).
#[test]
fn phero_compile_populates_enums_datatypes_and_feature_kinds() {
    let src = r#"
package probe.domain

enum Color {
    Red as "R" = 0
    Blue as "B" = 1
}

type Timestamp wraps String {
    format "date-time"
}

type Email wraps String {
    format "email"
}

class Base {
    id readonly String baseId
}

class Widget extends Base {
    String name = "widget"
    String code { pattern "[A-Z]{3}" minLength 3 maxLength 3 }
    int [2..4] count { minimum 0 maximum 100 }
    Color color
    Email email
    Timestamp ts
    contains Part[] parts opposite widget
    refers Widget[] related
}

class Part {
    container Widget widget opposite parts
    String label
}
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("probe.mox");
    fs::write(&path, src).unwrap();
    let compilation = phero_driver::compile_files(
        &[(path.display().to_string(), src.to_string())],
        &phero_driver::DomainImports::default(),
    );
    for (p, d) in &compilation.diagnostics {
        eprintln!("diag {p}: {}", d.message);
    }
    let model = compilation.model.expect("compilation must produce a model");
    for pkg in &model.packages {
        eprintln!("package {}", pkg.name);
        for e in &pkg.enums {
            eprintln!(
                "  enum {} literals={:?}",
                e.name,
                e.literals
                    .iter()
                    .map(|l| (&l.name, &l.label, l.value))
                    .collect::<Vec<_>>()
            );
        }
        for d in &pkg.datatypes {
            eprintln!(
                "  datatype {} platform={:?} format={:?} bindings={:?}",
                d.name, d.platform, d.format, d.target_bindings
            );
        }
        for c in &pkg.classes {
            eprintln!("  class {} extends={:?}", c.name, c.extends);
            for f in &c.features {
                eprintln!(
                    "    feature {} kind={:?} type={:?} mult=({},{:?}) default={:?} id={} ro={} derived={} constraints={:?}",
                    f.name,
                    f.kind,
                    f.type_,
                    f.multiplicity.lower,
                    f.multiplicity.upper,
                    f.default,
                    f.is_id,
                    f.is_read_only,
                    f.is_derived,
                    f.constraints
                );
            }
        }
    }

    let pkg = &model.packages[0];
    assert_eq!(pkg.name, "probe.domain");
    assert_eq!(pkg.enums.len(), 1, "enums must be populated");
    assert_eq!(pkg.enums[0].name, "Color");
    assert_eq!(pkg.enums[0].literals.len(), 2);
    assert_eq!(pkg.enums[0].literals[0].label.as_deref(), Some("R"));
    assert_eq!(pkg.datatypes.len(), 2, "datatypes must be populated");
    assert_eq!(pkg.datatypes[0].format.as_deref(), Some("date-time"));
    assert_eq!(pkg.classes.len(), 3);
    let widget = pkg.classes.iter().find(|c| c.name == "Widget").unwrap();
    assert_eq!(widget.extends.len(), 1);
    let kinds: Vec<_> = widget.features.iter().map(|f| f.kind).collect();
    assert!(kinds.contains(&phero_ir::FeatureKind::Attribute));
    assert!(kinds.contains(&phero_ir::FeatureKind::Containment));
    assert!(kinds.contains(&phero_ir::FeatureKind::CrossReference));
}

/// Broken mox source: syntax error, no model lowered.
const BROKEN_MOX: &str = r#"
package recruiting

class Broken {
    String
}
"#;

/// Writes a `.mox` file plus its vendored vocabulary snapshot (the snapshot
/// must exist on disk next to the model or vocabulary lowering errors).
pub(crate) fn write_mox_fixture(dir: &Path, name: &str, source: &str) -> PathBuf {
    let vocab_dir = dir.join("vocab");
    fs::create_dir_all(&vocab_dir).unwrap();
    fs::write(vocab_dir.join("iso-4217@2024-01-01.json"), VOCAB_SNAPSHOT).unwrap();
    let path = dir.join(name);
    fs::write(&path, source).unwrap();
    path
}

/// Load the fixture domains.toml (schema entities must exist for class name
/// matching; also passed to `ingest_mox_files` for domain resolution).
pub(crate) fn load_fixture_config() -> codegraph_config::config::DomainConfig {
    codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml")).unwrap()
}

/// Ingest the fixture JSON schemas (schema entities must exist for class
/// name matching).
pub(crate) async fn ingest_fixture_schemas(
    engine: &GrafeoEngine,
    config: &codegraph_config::config::DomainConfig,
) {
    let classifier = codegraph_classifier::config::parse_classifier_config(Path::new(
        "tests/fixtures/classifier.toml",
    ))
    .unwrap();
    let entity_names: std::collections::HashSet<String> = config
        .domains
        .values()
        .flat_map(|d| d.entities.iter().cloned())
        .collect();
    codegraph::ingest::async_ingest::ingest_schemas(
        engine,
        Path::new("tests/fixtures/schemas"),
        &classifier,
        &entity_names,
        &codegraph_config::UiOverrideConfig::default(),
        &config.defaults.type_suffix,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn mox_vocabulary_ingests_with_facets_and_entries() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let config = load_fixture_config();
    ingest_fixture_schemas(&engine, &config).await;

    let dir = tempfile::tempdir().unwrap();
    let mox = write_mox_fixture(dir.path(), "model.mox", CANONICAL_MOX);

    let stats = codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        &[mox],
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap()
    .stats;

    assert_eq!(stats.vocabularies, 1);
    assert_eq!(stats.operations, 1);
    assert_eq!(stats.derived_features, 1);
    assert_eq!(stats.skipped, 0);

    let vocabs = engine.get_mox_vocabularies().await.unwrap();
    assert_eq!(vocabs.len(), 1);
    let vocab = &vocabs[0];
    assert_eq!(vocab.name, "Currency");
    assert_eq!(vocab.package, "recruiting");
    assert_eq!(vocab.source, "iso:4217");
    assert_eq!(vocab.version.as_deref(), Some("2024-01-01"));
    assert_eq!(vocab.key_facet, "alpha3");
    assert_eq!(
        vocab.facets,
        vec![
            codegraph_core::types::MoxFacet {
                name: "minorUnits".into(),
                type_: "int".into(),
            },
            codegraph_core::types::MoxFacet {
                name: "symbol".into(),
                type_: "string".into(),
            },
        ]
    );
    let keys: Vec<&str> = vocab.entries.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys, vec!["USD", "EUR"]);
    assert_eq!(vocab.entries[0].facets["minorUnits"], serde_json::json!(2));
    assert_eq!(vocab.entries[0].facets["symbol"], serde_json::json!("$"));
}

#[tokio::test]
async fn mox_operations_and_derived_features_ingest_with_class_links() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let config = load_fixture_config();
    ingest_fixture_schemas(&engine, &config).await;

    let dir = tempfile::tempdir().unwrap();
    let mox = write_mox_fixture(dir.path(), "model.mox", CANONICAL_MOX);

    let stats = codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        &[mox],
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap()
    .stats;
    assert_eq!(stats.skipped, 0);

    // Operation: name, class, return type, params, and the verbatim body text.
    let ops = engine.get_mox_operations().await.unwrap();
    assert_eq!(ops.len(), 1);
    let op = &ops[0];
    assert_eq!(op.name, "idLength");
    assert_eq!(op.class, "CandidateType");
    assert_eq!(op.package, "recruiting");
    assert_eq!(op.return_type, "int");
    assert_eq!(op.params.len(), 1);
    assert_eq!(op.params[0].name, "prefix");
    assert_eq!(op.params[0].type_, "string");
    assert_eq!(op.bodies.get("expr").map(String::as_str), Some(" 1 "));

    // Derived feature: the "expr" body text is carried verbatim.
    let derived = engine.get_mox_derived_features().await.unwrap();
    assert_eq!(derived.len(), 1);
    let d = &derived[0];
    assert_eq!(d.name, "upperId");
    assert_eq!(d.class, "CandidateType");
    assert_eq!(d.package, "recruiting");
    assert_eq!(d.type_ref, "string");
    assert_eq!(d.expr.as_deref(), Some(" candidateId "));

    // BelongsToClass edge: the derived feature is linked to the
    // schema-ingested CandidateType entity.
    let linked = engine
        .get_mox_derived_features_for_schema("CandidateType")
        .await
        .unwrap();
    assert_eq!(linked.len(), 1);
    assert_eq!(linked[0].name, "upperId");
}

#[tokio::test]
async fn mox_class_mismatch_warns_counts_skipped_and_never_fails() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let config = load_fixture_config();
    ingest_fixture_schemas(&engine, &config).await;

    let dir = tempfile::tempdir().unwrap();
    let mox = write_mox_fixture(dir.path(), "model.mox", CANONICAL_MOX_WITH_GHOST);

    let stats = codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        &[mox],
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap()
    .stats;

    // GhostType has one derived feature and matches no schema entity: warned
    // and counted as skipped, but the ingest itself succeeds and the feature
    // is still persisted (keyed by class name) for later runs.
    assert_eq!(stats.skipped, 1);
    assert_eq!(stats.vocabularies, 1);
    assert_eq!(stats.operations, 1);
    assert_eq!(stats.derived_features, 2);

    let derived = engine.get_mox_derived_features().await.unwrap();
    assert_eq!(derived.len(), 2);
    let linked = engine
        .get_mox_derived_features_for_schema("CandidateType")
        .await
        .unwrap();
    assert_eq!(linked.len(), 1);
    assert_eq!(linked[0].name, "upperId");
}

/// A compile that lowers NO model is a hard error, not a silent skip: every
/// class would vanish and the run would generate an empty app with 0 errors
/// (the WS5 failure mode). The message carries the rendered pherolang
/// diagnostic, so the cause is in the error, not only on stderr.
#[tokio::test]
async fn mox_compile_failure_is_a_hard_error_naming_the_diagnostic() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let config = load_fixture_config();
    ingest_fixture_schemas(&engine, &config).await;

    let dir = tempfile::tempdir().unwrap();
    let mox = write_mox_fixture(dir.path(), "broken.mox", BROKEN_MOX);

    let error = codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        std::slice::from_ref(&mox),
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .expect_err("a compile that produces no model must not be skipped silently");

    let message = format!("{error}");
    assert!(
        message.contains("mox model error"),
        "the error must name the model failure: {message}"
    );
    assert!(
        message.contains("broken.mox"),
        "the error must name the offending file: {message}"
    );

    // Nothing was ingested on the failure path.
    assert_eq!(engine.get_mox_operations().await.unwrap().len(), 0);
    assert!(engine.get_mox_derived_features().await.unwrap().is_empty());
}

// ── Class bridge (issue #229): .mox classes → Schema/Property nodes ──

/// A mox source exercising every bridge mapping: class → schema node,
/// refers → entity reference, contains → value object (scalar + array),
/// extends → ExtendsSchema, enum → codelist, datatype formats, multiplicities
/// and constraints. The package name exactly matches a fixture domain so the
/// domains.toml resolution path is exercised.
const BRIDGE_MOX: &str = r#"
package recruiting

enum StatusKind {
    Draft as "D" = 0
    Live as "L" = 1
}

type Timestamp wraps String {
    format "date-time"
}

type ExternalId wraps String {
    format "uuid"
}

class CustomerType {
    String name
    String [0..1] nickname
    String code { pattern "[A-Z]{3}" minLength 3 maxLength 3 }
    int [2..4] scores { minimum 0 maximum 100 }
    StatusKind status
    ExternalId externalRef
    Timestamp createdAt
    contains AddressType [0..1] billingAddress
    contains AddressType[] shippingAddresses
    refers CustomerType[] referrals
}

class AddressType extends BaseAddress {
    String city
    String line1
}

class BaseAddress {
    String label
}
"#;

async fn ingest_bridge_mox(engine: &GrafeoEngine) -> codegraph::ingest::mox_ingest::MoxIngestStats {
    let config = load_fixture_config();
    let dir = tempfile::tempdir().unwrap();
    let mox = write_mox_fixture(dir.path(), "bridge.mox", BRIDGE_MOX);
    let stats = codegraph::ingest::mox_ingest::ingest_mox_files(
        engine,
        engine,
        &[mox],
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap()
    .stats;
    dir.close().unwrap();
    stats
}

#[tokio::test]
async fn mox_class_bridges_to_entity_schema_node_with_provenance() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let stats = ingest_bridge_mox(&engine).await;

    assert_eq!(stats.classes, 3);
    assert_eq!(stats.properties, 14);
    assert_eq!(stats.edges, 5);
    assert_eq!(stats.enums, 1);
    assert_eq!(stats.enum_schemas, 1);
    assert_eq!(stats.skipped, 0);
    assert_eq!(stats.vocabularies, 0);

    // The enum bridges to a codelist Schema node (issue #233) — the shape
    // codelist DDL, FK resolution, and link generation key on.
    let status = engine.get_schema("StatusKind").await.unwrap().unwrap();
    assert!(status.is_codelist);
    assert!(!status.is_entity);
    assert_eq!(status.classification, "codelist");
    assert_eq!(status.pg_table_name, "status_kind");
    assert_eq!(
        status
            .custom_annotations
            .get("source")
            .and_then(|v| v.as_str()),
        Some("mox")
    );
    // The enum-typed feature carries a ReferencesSchema edge to the codelist
    // schema node (the JSON path's shape), so FK resolution emits
    // `REFERENCES common.<codelist>(code)`.
    let target = engine
        .get_property_ref_target("status", "CustomerType")
        .await
        .unwrap()
        .expect("ReferencesSchema edge to the codelist schema node");
    assert_eq!(target.title, "StatusKind");
    assert!(target.is_codelist);

    let schema = engine.get_schema("CustomerType").await.unwrap().unwrap();
    assert!(schema.is_entity);
    assert_eq!(schema.domain.as_deref(), Some("recruiting"));
    assert_eq!(schema.pg_table_name, "customer");
    assert_eq!(schema.rust_type_name, "Customer");
    assert_eq!(schema.api_path_segment, "customer");
    assert_eq!(schema.classification, "entity_reference");
    assert_eq!(schema.schema_id, "recruiting/CustomerType");
    assert_eq!(
        schema
            .custom_annotations
            .get("source")
            .and_then(|v| v.as_str()),
        Some("mox")
    );

    // A class targeted only by `contains` features is author-declared a
    // value object.
    let address = engine.get_schema("AddressType").await.unwrap().unwrap();
    assert!(!address.is_entity);
    assert_eq!(address.classification, "value_object");
    assert_eq!(
        address
            .custom_annotations
            .get("source")
            .and_then(|v| v.as_str()),
        Some("mox")
    );
}

#[tokio::test]
async fn mox_refers_maps_to_entity_reference_property_and_edge() {
    let engine = GrafeoEngine::in_memory().unwrap();
    ingest_bridge_mox(&engine).await;

    let props = engine.get_properties("CustomerType").await.unwrap();
    let referrals = props
        .iter()
        .find(|p| p.name == "referrals")
        .unwrap_or_else(|| panic!("referrals property missing: {props:?}"));
    assert_eq!(
        referrals.classification_kind,
        Some(codegraph_type_contracts::RefClassificationKind::EntityReference)
    );
    assert!(referrals.is_array);
    assert_eq!(referrals.ref_target.as_deref(), Some("CustomerType"));

    // The ReferencesSchema edge resolves through the graph to the target.
    let target = engine
        .get_array_item_schema("referrals", "CustomerType")
        .await
        .unwrap()
        .expect("ItemsOf edge must resolve");
    assert_eq!(target.title, "CustomerType");
}

#[tokio::test]
async fn mox_contains_maps_to_value_object_and_composition_child() {
    let engine = GrafeoEngine::in_memory().unwrap();
    ingest_bridge_mox(&engine).await;

    let props = engine.get_properties("CustomerType").await.unwrap();
    let billing = props
        .iter()
        .find(|p| p.name == "billingAddress")
        .unwrap_or_else(|| panic!("billingAddress property missing: {props:?}"));
    assert_eq!(
        billing.classification_kind,
        Some(codegraph_type_contracts::RefClassificationKind::ValueObject)
    );
    assert!(!billing.is_array);
    assert_eq!(billing.ref_target.as_deref(), Some("AddressType"));
    assert_eq!(billing.pg_column_name, "billing_address");

    let shipping = props
        .iter()
        .find(|p| p.name == "shippingAddresses")
        .unwrap();
    assert_eq!(
        shipping.classification_kind,
        Some(codegraph_type_contracts::RefClassificationKind::ValueObject)
    );
    assert!(shipping.is_array);

    // The composition tree nests the contained class as child tables.
    let tree = engine.get_composition_tree("CustomerType").await.unwrap();
    let address_children: Vec<_> = tree
        .root
        .children
        .iter()
        .filter(|c| c.schema_title == "AddressType")
        .collect();
    assert_eq!(address_children.len(), 2, "scalar + array containment");
    let scalar = address_children
        .iter()
        .find(|c| c.field_name == "billing_address")
        .expect("scalar containment child");
    assert!(!scalar.is_collection);
    assert_eq!(
        scalar.fk,
        Some(codegraph_core::types::FkDirection::OnChild {
            column: "customer_id".into()
        })
    );
    let array = address_children
        .iter()
        .find(|c| c.field_name == "shipping_addresses")
        .expect("array containment child");
    assert!(array.is_collection);
    // The child table carries the VO's own columns plus the inherited
    // `label` (allOf-canonical property merge, issue #233) — name-sorted.
    let child_columns: Vec<&str> = scalar.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(child_columns, vec!["city", "label", "line1"]);
}

#[tokio::test]
async fn mox_enum_maps_to_codelist_with_values() {
    let engine = GrafeoEngine::in_memory().unwrap();
    ingest_bridge_mox(&engine).await;

    let values = engine.get_enum_values("StatusKind").await.unwrap();
    assert_eq!(
        values,
        vec![
            codegraph_core::types::EnumValue {
                value: "Draft".into(),
                display_name: Some("D".into()),
                sort_order: 0,
            },
            codegraph_core::types::EnumValue {
                value: "Live".into(),
                display_name: Some("L".into()),
                sort_order: 1,
            },
        ]
    );

    // A feature typed with the enum classifies as a codelist reference.
    let props = engine.get_properties("CustomerType").await.unwrap();
    let status = props.iter().find(|p| p.name == "status").unwrap();
    assert_eq!(
        status.classification_kind,
        Some(codegraph_type_contracts::RefClassificationKind::CodelistReference)
    );
    assert_eq!(status.pg_column_type, "TEXT");
    assert_eq!(status.rust_field_type, "String");
    assert_eq!(status.ref_target.as_deref(), Some("StatusKind"));
}

#[tokio::test]
async fn mox_extends_maps_to_extends_schema_edge() {
    let engine = GrafeoEngine::in_memory().unwrap();
    ingest_bridge_mox(&engine).await;

    let targets = engine.get_allof_targets("AddressType").await.unwrap();
    assert_eq!(targets, vec!["BaseAddress"]);
}

#[tokio::test]
async fn mox_datatype_format_maps_to_pg_types() {
    let engine = GrafeoEngine::in_memory().unwrap();
    ingest_bridge_mox(&engine).await;

    let props = engine.get_properties("CustomerType").await.unwrap();
    let external = props.iter().find(|p| p.name == "externalRef").unwrap();
    assert_eq!(external.pg_column_type, "UUID");
    assert_eq!(external.rust_field_type, "uuid::Uuid");
    assert_eq!(external.format.as_deref(), Some("uuid"));

    let created = props.iter().find(|p| p.name == "createdAt").unwrap();
    assert_eq!(created.pg_column_type, "TIMESTAMPTZ");
    assert_eq!(created.rust_field_type, "chrono::DateTime<chrono::Utc>");
    assert_eq!(created.format.as_deref(), Some("date-time"));
}

#[tokio::test]
async fn mox_multiplicity_optional_required_and_array() {
    let engine = GrafeoEngine::in_memory().unwrap();
    ingest_bridge_mox(&engine).await;

    let props = engine.get_properties("CustomerType").await.unwrap();
    let name = props.iter().find(|p| p.name == "name").unwrap();
    assert!(name.is_required);
    assert!(!name.is_nullable);
    assert!(!name.is_array);

    let nickname = props.iter().find(|p| p.name == "nickname").unwrap();
    assert!(!nickname.is_required);
    assert!(nickname.is_nullable);
    assert!(!nickname.is_array);

    let scores = props.iter().find(|p| p.name == "scores").unwrap();
    assert!(scores.is_required, "lower bound 2 implies required");
    assert!(scores.is_array, "finite upper > 1 implies array");
    assert_eq!(scores.pg_column_type, "INTEGER[]");
    assert_eq!(scores.rust_field_type, "Vec<i32>");
}

#[tokio::test]
async fn mox_constraints_populate_validation_fields() {
    // The grafeo store does not persist min/max/minimum/maximum on Property
    // nodes for any ingestion path (pre-existing gap), so the full-fidelity
    // assertion runs against the in-memory engine.
    let mock = codegraph_core::mock::MockEngine::new();
    let config = load_fixture_config();
    let dir = tempfile::tempdir().unwrap();
    let mox = write_mox_fixture(dir.path(), "bridge.mox", BRIDGE_MOX);
    codegraph::ingest::mox_ingest::ingest_mox_files(
        &mock,
        &mock,
        &[mox],
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap();

    let props = mock.get_properties("CustomerType").await.unwrap();
    let code = props.iter().find(|p| p.name == "code").unwrap();
    assert_eq!(code.pattern.as_deref(), Some("[A-Z]{3}"));
    assert_eq!(code.min_length, Some(3));
    assert_eq!(code.max_length, Some(3));

    let scores = props.iter().find(|p| p.name == "scores").unwrap();
    assert_eq!(scores.minimum.map(|d| d.to_string()), Some("0".into()));
    assert_eq!(scores.maximum.map(|d| d.to_string()), Some("100".into()));
}

#[tokio::test]
async fn mox_class_matching_json_schema_is_never_overridden() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let config = load_fixture_config();
    ingest_fixture_schemas(&engine, &config).await;
    let schemas_before = engine.list_schemas(None).await.unwrap().len();
    let props_before = engine.get_properties("CandidateType").await.unwrap().len();

    let dir = tempfile::tempdir().unwrap();
    let mox = write_mox_fixture(dir.path(), "model.mox", CANONICAL_MOX);
    let stats = codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        &[mox],
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap()
    .stats;

    // CandidateType already exists as a JSON-ingested entity: the bridge
    // creates no schema node and no properties for it.
    assert_eq!(stats.classes, 0);
    assert_eq!(stats.properties, 0);
    assert_eq!(stats.skipped, 0);
    let schemas_after = engine.list_schemas(None).await.unwrap();
    assert_eq!(schemas_after.len(), schemas_before);
    assert_eq!(
        schemas_after
            .iter()
            .filter(|s| s.title == "CandidateType")
            .count(),
        1
    );
    let props_after = engine.get_properties("CandidateType").await.unwrap().len();
    assert_eq!(
        props_after, props_before,
        "mox features must not leak into a JSON-owned schema"
    );
}

#[tokio::test]
async fn auto_classifier_does_not_reclassify_mox_schemas() {
    let engine = GrafeoEngine::in_memory().unwrap();
    ingest_bridge_mox(&engine).await;

    let all_data = engine.get_classification_data().await.unwrap();
    let customer = all_data.iter().find(|d| d.title == "CustomerType").unwrap();
    assert_eq!(customer.source.as_deref(), Some("mox"));
    assert!(customer.is_entity);

    let config = load_fixture_config();
    let entry = &config.domains["recruiting"];
    let domain_schemas: Vec<_> = all_data
        .iter()
        .filter(|d| d.domain.as_deref() == Some("recruiting"))
        .cloned()
        .collect();
    let classifier = codegraph::classify::AutoClassifier::new(
        std::collections::HashSet::new(),
        std::collections::HashMap::new(),
    );
    let result = classifier.classify_domain("recruiting", entry, &domain_schemas);

    // Author-declared classification wins: provenance reason, entity stays
    // an entity, contained-only class stays a value object.
    let customer_score = result
        .entities
        .iter()
        .find(|s| s.title == "CustomerType")
        .expect("mox entity must classify as entity");
    assert!(
        customer_score
            .reasons
            .contains(&"override:source=mox".to_string())
    );
    let address_score = result
        .value_objects
        .iter()
        .find(|s| s.title == "AddressType")
        .expect("mox value object must classify as value object");
    assert!(
        address_score
            .reasons
            .contains(&"override:source=mox".to_string())
    );
    assert!(!result.entities.iter().any(|s| s.title == "AddressType"));
}

#[tokio::test]
async fn reclassify_pass_leaves_mox_value_object_containment_intact() {
    let engine = GrafeoEngine::in_memory().unwrap();
    ingest_bridge_mox(&engine).await;

    // Driver flow: classify (mox provenance respected), then re-derive
    // entity flags and property classifications graph-wide.
    let all_data = engine.get_classification_data().await.unwrap();
    let config = load_fixture_config();
    let entry = &config.domains["recruiting"];
    let domain_schemas: Vec<_> = all_data
        .iter()
        .filter(|d| d.domain.as_deref() == Some("recruiting"))
        .cloned()
        .collect();
    let classifier = codegraph::classify::AutoClassifier::new(
        std::collections::HashSet::new(),
        std::collections::HashMap::new(),
    );
    let result = classifier.classify_domain("recruiting", entry, &domain_schemas);
    let mut entity_names = std::collections::HashSet::new();
    for score in &result.entities {
        entity_names.insert(score.title.clone());
    }
    codegraph::ingest::async_ingest::reclassify_with_entities(&engine, &engine, &entity_names)
        .await
        .unwrap();

    // The containment property must still be a value object pointing at the
    // (non-entity) contained class — not flipped to an entity reference.
    let address = engine.get_schema("AddressType").await.unwrap().unwrap();
    assert!(!address.is_entity);
    let props = engine.get_properties("CustomerType").await.unwrap();
    let billing = props.iter().find(|p| p.name == "billingAddress").unwrap();
    assert_eq!(
        billing.classification_kind,
        Some(codegraph_type_contracts::RefClassificationKind::ValueObject)
    );
    // The mox entity keeps its author-declared flag.
    let customer = engine.get_schema("CustomerType").await.unwrap().unwrap();
    assert!(customer.is_entity);
}

// ── Issue #668: explicit table names, register classes, description and
// constraint fidelity ────────────────────────────────────────────────────

/// `class X table "…"` (upstream pherolang#177 `ClassDef::table_name`): the
/// declared spelling is the effective storage table on EVERY plane, because
/// the bridge writes it straight into `SchemaNode.pg_table_name` — the field
/// the DDL, SeaORM entity, repository SQL, filter and hurl/openapi naming all
/// read. URLs are unaffected (`api_path_segment` still derives from the
/// title).
const TABLE_NAME_MOX: &str = r#"
package compensation

class PayRunRows {
    String reference
    String status
}
"#;

const TABLE_NAME_DECLARED_MOX: &str = r#"
package compensation

class PayRunType table "pay_run_rows" {
    String reference
    String status
}
"#;

#[tokio::test]
async fn class_table_override_becomes_the_effective_table_everywhere() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let config = load_fixture_config();

    let dir = tempfile::tempdir().unwrap();
    let mox = write_mox_fixture(dir.path(), "model.mox", TABLE_NAME_DECLARED_MOX);
    let stats = codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        std::slice::from_ref(&mox),
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap()
    .stats;
    assert_eq!(stats.classes, 1);

    let schema = engine
        .get_schema("PayRunType")
        .await
        .unwrap()
        .expect("the class must bridge");
    // The declared table wins over the title-derived `pay_run`.
    assert_eq!(schema.pg_table_name, "pay_run_rows");
    assert!(schema.declares_table_name(&config.defaults.type_suffix));
    // The URL segment still derives from the TITLE — table overrides never
    // move an API path.
    assert_eq!(schema.api_path_segment, "pay-run");
    assert_eq!(schema.rust_type_name, "PayRun");
}

#[tokio::test]
async fn class_without_a_table_override_keeps_the_derived_name() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let config = load_fixture_config();

    let dir = tempfile::tempdir().unwrap();
    let mox = write_mox_fixture(dir.path(), "model.mox", TABLE_NAME_MOX);
    codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        std::slice::from_ref(&mox),
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap();

    let schema = engine
        .get_schema("PayRunRows")
        .await
        .unwrap()
        .expect("the class must bridge");
    assert_eq!(schema.pg_table_name, "pay_run_rows");
    assert!(!schema.declares_table_name(&config.defaults.type_suffix));
}

/// A `register` declaration (upstream pherolang#181 / `ClassKind::Register`)
/// is an org/role noun space, not a transactional aggregate: it bridges onto
/// the reference-data (codelist/seed-table) plane — a `common.{table}` seed
/// migration plus the codelist Rust enum — and never onto the entity plane.
/// The graph is the contract: `is_codelist`, `classification = "codelist"`,
/// the `is_register_class` marker, and a `CodeList` node the codelist
/// generators own.
const REGISTER_MOX: &str = r#"
package compensation

register Role {
    String display_name
    String[0..1] badge = "none"
    String code { pattern "[A-Z]{2,8}" minLength 2 }
}

class PayRunType {
    String reference
    refers Role[0..1] role
}
"#;

#[tokio::test]
async fn register_class_bridges_to_the_reference_data_plane() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let config = load_fixture_config();

    let dir = tempfile::tempdir().unwrap();
    let mox = write_mox_fixture(dir.path(), "model.mox", REGISTER_MOX);
    let stats = codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        std::slice::from_ref(&mox),
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap()
    .stats;
    assert_eq!(stats.registers, 1, "the register must be counted");
    assert_eq!(stats.enums, 0, "a register is not an enum literal set");

    let role = engine
        .get_schema("Role")
        .await
        .unwrap()
        .expect("the register must bridge");
    assert!(role.is_register_class());
    assert!(role.is_codelist, "registers live on the codelist plane");
    assert!(!role.is_entity, "a register must never become an entity");
    assert_eq!(role.classification, "codelist");
    assert_eq!(role.pg_table_name, "role");

    // The codelist node is what the seed-migration generator keys on, so the
    // register has a real reference table (`common.role`) with no entity
    // module, repository or handler of its own.
    let codelists = engine.list_codelists().await.unwrap();
    let role_list = codelists
        .iter()
        .find(|c| c.name == "Role")
        .expect("a CodeList node must exist for the register");
    assert_eq!(role_list.pg_table_name, "role");
    assert_eq!(role_list.render_as, "codelist");

    // The register's attributes are still ingested (non-lossy model graph;
    // the reference-table projection is a documented follow-up).
    let props = engine.get_properties("Role").await.unwrap();
    let names: Vec<&str> = props.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"display_name"), "props: {names:?}");
    assert!(names.contains(&"code"), "props: {names:?}");
}

#[tokio::test]
async fn register_refs_lower_like_any_class_reference() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let config = load_fixture_config();

    let dir = tempfile::tempdir().unwrap();
    let mox = write_mox_fixture(dir.path(), "model.mox", REGISTER_MOX);
    codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        std::slice::from_ref(&mox),
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap();

    // `refers Role[0..1] role` stays an entity-reference property: the REGISTER's
    // own plane is the codelist one, but referring rows store the register
    // row's id (`role_id UUID`), exactly as any other class reference.
    let props = engine.get_properties("PayRunType").await.unwrap();
    let role = props.iter().find(|p| p.name == "role").unwrap();
    assert_eq!(
        role.classification_kind,
        Some(codegraph_type_contracts::RefClassificationKind::EntityReference)
    );
    assert_eq!(role.ref_target.as_deref(), Some("Role"));
    assert_eq!(role.pg_column_name, "role_id");
}

/// Issue #668 (d): doc-comment descriptions and the value-constraint block
/// must survive ingest non-lossily at every level — class, feature, enum and
/// (via the label) the literal, plus all five value constraints.
const AUDIT_MOX: &str = r#"
package compensation

/// A pay run groups the rows of one payroll cycle.
class AuditRun {
    /// The pay run's human reference.
    String reference { pattern "[A-Z]{2,8}" minLength 2 maxLength 24 }
    int cycles { minimum 1 maximum 12 }
    /// The run's lifecycle state.
    AuditRunStatus status
}

/// The lifecycle states of a pay run.
enum AuditRunStatus {
    /// The run is being prepared.
    Draft as "Draft" = 0
    /// The run has been approved.
    Approved as "Approved" = 1
}
"#;

#[tokio::test]
async fn descriptions_and_value_constraints_ingest_without_loss() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let config = load_fixture_config();

    let dir = tempfile::tempdir().unwrap();
    let mox = write_mox_fixture(dir.path(), "model.mox", AUDIT_MOX);
    codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        std::slice::from_ref(&mox),
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap();

    // Class description.
    let schema = engine
        .get_schema("AuditRun")
        .await
        .unwrap()
        .expect("class bridges");
    assert_eq!(
        schema.description.as_deref(),
        Some("A pay run groups the rows of one payroll cycle.")
    );

    // Feature description + all five value constraints.
    let props = engine.get_properties("AuditRun").await.unwrap();
    let reference = props.iter().find(|p| p.name == "reference").unwrap();
    assert_eq!(
        reference.description.as_deref(),
        Some("The pay run's human reference.")
    );
    assert_eq!(reference.pattern.as_deref(), Some("[A-Z]{2,8}"));
    assert_eq!(reference.min_length, Some(2));
    assert_eq!(reference.max_length, Some(24));
    let cycles = props.iter().find(|p| p.name == "cycles").unwrap();
    assert_eq!(cycles.minimum, Some(rust_decimal::Decimal::from(1)));
    assert_eq!(cycles.maximum, Some(rust_decimal::Decimal::from(12)));

    // Enum description (CodeList + codelist schema) and literal labels.
    let codelists = engine.list_codelists().await.unwrap();
    let status = codelists
        .iter()
        .find(|c| c.name == "AuditRunStatus")
        .expect("enum codelist node");
    assert_eq!(
        status.description.as_deref(),
        Some("The lifecycle states of a pay run.")
    );
    let literals = engine.get_enum_values("AuditRunStatus").await.unwrap();
    let labels: Vec<_> = literals
        .iter()
        .map(|v| (v.value.as_str(), v.display_name.as_deref()))
        .collect();
    assert_eq!(
        labels,
        vec![("Draft", Some("Draft")), ("Approved", Some("Approved"))]
    );
}
