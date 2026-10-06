//! Golden-conformance gate for the rexlang event-contract artifact (issue
//! #454 Phase 2a): the committed `orders.evt` — verbatim from the rexlang
//! conformance suite at the pinned rev — compiles with zero error
//! diagnostics and serializes byte-identically to the committed
//! `orders.evt.json` wire artifact. This is the in-process equivalent of
//! the rexlang CLI gates — node-free and CI-safe. Also pins the wire facts
//! downstream consumers build on (the pinned event version and the
//! enum-typed payload field).

use std::path::PathBuf;

use rex_ir::TypeRef;
use rex_ir::events::EventModel;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/evt")
        .join(name)
}

/// The golden gate: `orders.evt` compiles cleanly (the fixture's own
/// `import "orders.mox"` resolves beside it) and the produced
/// [`EventModel`] byte-equals the committed `orders.evt.json`, which then
/// round-trips back through [`EventModel::from_json`] to an equal model.
///
/// Byte-equality is direct: rex's `to_json_pretty` (serde_json pretty,
/// 2-space) emits NO trailing newline and the committed artifact has none
/// (verified: it ends `}`) — no normalization applied.
#[test]
fn orders_evt_compiles_clean_and_byte_matches_the_committed_artifact() {
    let compiled = codegraph::ingest::evt_ingest::read_and_compile_evt(&fixture("orders.evt"))
        .expect("orders.evt reads and compiles");

    let errors: Vec<String> = compiled
        .compilation
        .diagnostics
        .iter()
        .filter(|(_, d)| d.is_error())
        .map(|(file, d)| format!("{file}: {}", d.message))
        .collect();
    assert!(
        errors.is_empty(),
        "golden fixture must compile with zero error diagnostics: {errors:?}"
    );

    let model = compiled
        .compilation
        .model
        .as_ref()
        .expect("a clean compile produces the event artifact");

    let committed = std::fs::read_to_string(fixture("orders.evt.json")).unwrap();
    assert_eq!(
        model.to_json_pretty().unwrap(),
        committed,
        "serialized artifact must byte-equal the committed orders.evt.json"
    );

    let round_tripped = EventModel::from_json(&committed).expect("artifact deserializes");
    assert_eq!(
        &round_tripped, model,
        "from_json(to_json_pretty(model)) must round-trip equal"
    );
}

/// Pinned wire facts the events-plane consumers build on: OrderPlaced
/// carries version Some("1.0.0") and its `status` field is the
/// enum-typed `nz.example.orders::OrderStatus` reference.
#[test]
fn orders_evt_pins_the_version_and_enum_field_wire_facts() {
    let compiled = codegraph::ingest::evt_ingest::read_and_compile_evt(&fixture("orders.evt"))
        .expect("orders.evt reads and compiles");
    let model = compiled
        .compilation
        .model
        .as_ref()
        .expect("a clean compile produces the event artifact");

    let placed = model
        .events
        .iter()
        .find(|e| e.name == "OrderPlaced")
        .expect("OrderPlaced declared");
    assert_eq!(placed.version.as_deref(), Some("1.0.0"));

    let status = placed
        .fields
        .iter()
        .find(|f| f.name == "status")
        .expect("status field declared");
    assert_eq!(
        status.ty,
        TypeRef::Enum {
            package: "nz.example.orders".to_string(),
            name: "OrderStatus".to_string(),
        }
    );
}
