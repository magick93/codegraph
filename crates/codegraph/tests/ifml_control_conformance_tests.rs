//! Conformance between the two control-inference sides (issue #201):
//! scaffold-side `codegraph::ifml_control_inference::infer_control` vs the
//! route-generator-side `codegraph_generate::ifml::route_generator::
//! control_for_field`. Both run over the SAME ingested schema properties so
//! input-type + role decisions cannot drift again.
//!
//! The route-generator side only ever sees `(field_name, rust_field_type)`
//! pairs (its `fields_with_types` input), so the shared core is driven with
//! that subset. Properties whose canonical decision depends on graph-only
//! signals (classification kind, numeric bounds) are asserted as an explicit
//! divergence ledger instead of being silently allowed to drift.

use std::collections::HashSet;
use std::fs;

use codegraph::ifml_control_inference::infer_control;
use codegraph_backend::{create_backend, BackendConfig};
use codegraph_config::UiOverrideConfig;
use codegraph_core::types::PropertyNode;
use codegraph_generate::ifml::route_generator::control_for_field;

const CONTROL_PROBE_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "ControlProbeType",
  "description": "One property per control-inference heuristic class.",
  "type": "object",
  "properties": {
    "id": { "description": "Primary identifier.", "type": "string", "format": "uuid" },
    "list_id": { "description": "Foreign identifier.", "type": "string", "format": "uuid" },
    "contact_email": { "description": "Email by name.", "type": "string" },
    "api_secret": { "description": "Secret by name.", "type": "string" },
    "completed": { "description": "Boolean flag.", "type": "boolean" },
    "priority": { "description": "Integer rank.", "type": "integer" },
    "rating": { "description": "Numeric score.", "type": "number" },
    "amount": { "description": "Bounded string.", "type": "string", "minimum": 0, "maximum": 100 },
    "owner": { "description": "Entity reference.", "$ref": "ManagerType.schema.json" },
    "status": { "description": "Inline enum.", "type": "string", "enum": ["draft", "active", "done"] },
    "due_date": { "description": "Date format.", "type": "string", "format": "date" },
    "created_at": { "description": "Timestamp format.", "type": "string", "format": "date-time" },
    "window_from": { "description": "Temporal suffix.", "type": "string" },
    "start_time": { "description": "Temporal suffix.", "type": "string" },
    "title": { "description": "Plain text.", "type": "string" },
    "tags": { "description": "String array.", "type": "array", "items": { "type": "string" } },
    "phone": { "description": "Contact recognizer.", "type": "string" },
    "website_url": { "description": "Contact recognizer.", "type": "string" }
  },
  "required": ["id", "title"]
}"#;

const MANAGER_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "ManagerType",
  "description": "Referenced entity for the entity-reference class.",
  "type": "object",
  "properties": {
    "id": { "description": "Primary identifier.", "type": "string", "format": "uuid" },
    "name": { "description": "Display name.", "type": "string" }
  },
  "required": ["id"]
}"#;

struct Fixture {
    _dir: tempfile::TempDir,
    schemas: std::path::PathBuf,
    classifier: std::path::PathBuf,
    config: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let probe_dir = dir.path().join("schemas/probe");
        fs::create_dir_all(&probe_dir).unwrap();
        fs::write(
            probe_dir.join("ControlProbeType.json"),
            CONTROL_PROBE_SCHEMA,
        )
        .unwrap();
        fs::write(probe_dir.join("ManagerType.json"), MANAGER_SCHEMA).unwrap();

        fs::write(
            dir.path().join("domains.toml"),
            r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.probe]
label = "Probe"
schema_dir = "probe"
postgres_schema = "probe"
entities = ["ControlProbeType", "ManagerType"]
"#,
        )
        .unwrap();
        fs::write(dir.path().join("classifier.toml"), "").unwrap();

        Self {
            schemas: dir.path().join("schemas"),
            classifier: dir.path().join("classifier.toml"),
            config: dir.path().join("domains.toml"),
            _dir: dir,
        }
    }
}

/// Ingest exactly like `ifml_scaffold` does so the scaffold side sees the
/// same graph state it sees in production.
async fn probe_properties(fx: &Fixture) -> Vec<PropertyNode> {
    let backend = create_backend(&BackendConfig::default())
        .await
        .expect("backend");
    let classifier_config =
        codegraph_classifier::config::parse_classifier_config(&fx.classifier).expect("classifier");
    let domain_config = codegraph_config::config::parse_domain_config(&fx.config).expect("domains");
    let empty_entities = HashSet::new();
    codegraph::ingest::async_ingest::ingest_schemas(
        backend.ingestor(),
        &fx.schemas,
        &classifier_config,
        &empty_entities,
        &UiOverrideConfig::default(),
        &domain_config.defaults.type_suffix,
    )
    .await
    .expect("ingest");

    let entities: HashSet<String> = domain_config
        .domains
        .values()
        .flat_map(|d| d.entities.clone())
        .collect();
    codegraph::ingest::async_ingest::reclassify_with_entities(
        backend.ingestor(),
        backend.querier(),
        &entities,
    )
    .await
    .expect("reclassify");

    backend
        .querier()
        .get_properties("ControlProbeType")
        .await
        .expect("properties")
}

#[tokio::test]
async fn control_decisions_conform_across_scaffold_and_route_generator() {
    let fx = Fixture::new();
    let props = probe_properties(&fx).await;
    assert!(
        props.len() >= 18,
        "fixture must carry every heuristic class, got {}",
        props.len()
    );

    for prop in &props {
        let canonical = infer_control(prop);
        let generate_side = control_for_field(&prop.rust_field_type, &prop.name);
        if is_kind_poor_class(&prop.name) {
            assert_divergence_ledger(prop, &canonical, &generate_side);
            continue;
        }
        assert_eq!(
            generate_side.input, canonical.input,
            "field '{}': input-type divergence (rust type '{}')",
            prop.name, prop.rust_field_type
        );
        assert_eq!(
            generate_side.role, canonical.role,
            "field '{}': role divergence (rust type '{}')",
            prop.name, prop.rust_field_type
        );
    }
}

/// Classes whose canonical decision is driven by the classification kind,
/// which `fields_with_types` `(name, rust_type)` pairs cannot carry.
fn is_kind_poor_class(field_name: &str) -> bool {
    matches!(field_name, "owner" | "status")
}

fn assert_divergence_ledger(
    prop: &PropertyNode,
    canonical: &codegraph::ifml_control_inference::ControlInference,
    generate_side: &codegraph_generate::ifml::control_core::ControlInference,
) {
    match prop.name.as_str() {
        // Codelist kind → dropdown; the bare (name, rust) pair sees a plain
        // string. Converging needs kind propagation into fields_with_types.
        "status" => {
            assert_eq!(canonical.input_str(), "dropdown");
            assert_eq!(generate_side.input_str(), "text");
        }
        // Entity-reference kind → dropdown with options note; the bare pair
        // sees the ref stem as an opaque rust type.
        "owner" => {
            assert_eq!(canonical.input_str(), "dropdown");
            assert_eq!(generate_side.input_str(), "text");
        }
        other => panic!("unexpected kind-poor class '{other}'"),
    }
}

/// Numeric bounds: the canonical mapping's `has_bounds` branch fires on
/// in-memory properties, but bounds do not survive graph ingestion, so the
/// rehydrated property is a plain string on BOTH sides. The generate side
/// (driven by rehydrated `(name, rust_type)` pairs) can never see them.
#[tokio::test]
async fn numeric_bounds_diverge_only_for_in_memory_properties() {
    let fx = Fixture::new();
    let props = probe_properties(&fx).await;
    let amount = props
        .iter()
        .find(|p| p.name == "amount")
        .expect("amount property");
    assert!(
        amount.minimum.is_none() && amount.maximum.is_none(),
        "precondition: bounds are dropped during ingestion"
    );
    assert_eq!(infer_control(amount).input_str(), "text");
    assert_eq!(
        control_for_field(&amount.rust_field_type, "amount").input_str(),
        "text"
    );

    let mut in_memory = amount.clone();
    in_memory.minimum = Some(rust_decimal::Decimal::from(0));
    in_memory.maximum = Some(rust_decimal::Decimal::from(100));
    assert_eq!(infer_control(&in_memory).input_str(), "number");
    assert_eq!(
        control_for_field(&in_memory.rust_field_type, "amount").input_str(),
        "text",
        "bounds are invisible to (name, rust_type) pairs"
    );
}

#[tokio::test]
async fn entity_reference_note_is_scaffold_only_context() {
    let fx = Fixture::new();
    let props = probe_properties(&fx).await;
    let owner = props
        .iter()
        .find(|p| p.name == "owner")
        .expect("owner property");
    let canonical = infer_control(owner);
    assert_eq!(
        canonical.note,
        Some(codegraph::ifml_control_inference::NOTE_OPTIONS_ENDPOINT),
        "entity-reference dropdowns keep the options-endpoint note"
    );
}
