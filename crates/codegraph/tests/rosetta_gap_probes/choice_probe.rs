// WP1.3 — choice/one-of probe (see docs/rosetta/findings/wp1.3-choice.md).
//
// Characterizes how choice / one-of / any-of constructs are represented in
// the graph and in generated output. Hypothesis under test: only boolean
// presence flags on SchemaNode (`has_one_of` / `has_any_of`), no variant
// representation anywhere downstream.
//
// OBSERVED surprises pinned here (do not "fix" to match the hypothesis):
// 1. A zero-property root oneOf schema gets `is_enum = true` in
//    SchemaClassificationData and auto-classifies as "hard:enum" VO — but
//    generation still emits a FULL entity-shaped CRUD stack around a hollow
//    table (id/tenant/audit columns only, zero variant representation), and
//    no Rust enum / codelist is produced.
// 2. A property-level oneOf is silently DROPPED from every layer: DDL, entity
//    model, DTOs. The ValueObject catch-all (classify_plain_type) makes DDL
//    skip the column expecting a child CompositionNode that never exists.

use crate::support;
use std::collections::{BTreeMap, HashMap, HashSet};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Root-level `oneOf` whose branches are single-property objects, and ZERO
/// sibling properties on the root — the `is_enum` shape (querier.rs:312).
const PAYMENT_STATUS_JSON: &str = r#"{
    "title": "PaymentStatus",
    "type": "object",
    "oneOf": [
        {
            "type": "object",
            "properties": { "state": { "type": "string" } },
            "additionalProperties": false
        },
        {
            "type": "object",
            "properties": { "paidAt": { "type": "string", "format": "date-time" } },
            "additionalProperties": false
        }
    ]
}"#;

/// Root-level `oneOf` of primitive-ish variants, zero sibling properties —
/// also the `is_enum` shape.
const COLOR_TYPE_JSON: &str = r#"{
    "title": "ColorType",
    "type": "object",
    "oneOf": [
        { "type": "string" },
        { "type": "integer" }
    ]
}"#;

/// Root-level `anyOf` with zero properties — NOT the is_enum shape
/// (is_enum only reads has_one_of). Used to pin the asymmetry.
const WRAP_TYPE_JSON: &str = r#"{
    "title": "WrapType",
    "type": "object",
    "anyOf": [
        { "type": "string" },
        { "type": "object" }
    ]
}"#;

/// No composition keywords at all — control schema.
const PLAIN_TYPE_JSON: &str = r#"{
    "title": "PlainType",
    "type": "object",
    "properties": { "name": { "type": "string" } }
}"#;

/// Sanity entity so the probe-1 pipeline has a real table to compare against.
const ORDER_TYPE_JSON: &str = r#"{
    "title": "OrderType",
    "type": "object",
    "properties": { "note": { "type": "string" } },
    "required": ["note"]
}"#;

/// Entity with a PROPERTY-LEVEL oneOf: `value` is oneOf [string, object].
/// The root object itself has no oneOf key, so `has_one_of` must stay false.
const CONTACT_TYPE_JSON: &str = r#"{
    "title": "ContactType",
    "type": "object",
    "properties": {
        "email": { "type": "string" },
        "phone": { "type": "string" },
        "value": {
            "oneOf": [
                { "type": "string" },
                {
                    "type": "object",
                    "properties": {
                        "extension": { "type": "string" },
                        "label": { "type": "string" }
                    }
                }
            ]
        }
    },
    "required": ["email"]
}"#;

/// Mirror of ContactType with anyOf instead of oneOf.
const TICKET_TYPE_JSON: &str = r#"{
    "title": "TicketType",
    "type": "object",
    "properties": {
        "title": { "type": "string" },
        "payload": {
            "anyOf": [
                { "type": "string" },
                {
                    "type": "object",
                    "properties": {
                        "kind": { "type": "string" },
                        "detail": { "type": "string" }
                    }
                }
            ]
        }
    },
    "required": ["title"]
}"#;

// ---------------------------------------------------------------------------
// Local helpers (support.rs is shared — do not touch)
// ---------------------------------------------------------------------------

/// DomainEntry matching the shape support::domains_toml writes
/// (`entities = [...]`, no force/exclude overrides).
fn plain_domain_entry(entities: &[&str]) -> codegraph_config::config::DomainEntry {
    codegraph_config::config::DomainEntry {
        label: "shop".into(),
        schema_dir: "shop".into(),
        postgres_schema: "shop".into(),
        depends_on: vec![],
        entities: entities.iter().map(|s| s.to_string()).collect(),
        entity_config: HashMap::new(),
        auto_discover: None,
        exclude_entities: vec![],
        force_entities: vec![],
        force_value_objects: vec![],
        exclude: vec![],
        auditable: None,
        tier: "extended".into(),
        worker_name: None,
        custom_domain: None,
        service_bindings: None,
        hyperdrive_binding: None,
        cron_triggers: None,
        remote_include_mode: None,
        webhooks: None,
        queue_name: None,
        queue_binding: None,
        queue_max_retries: None,
        queue_max_concurrency: None,
        observability: None,
        custom_routes: false,
    }
}

/// Generated files whose CONTENT contains `needle`.
fn files_containing<'a>(files: &'a BTreeMap<String, String>, needle: &str) -> Vec<&'a str> {
    files
        .iter()
        .filter(|(_, v)| v.contains(needle))
        .map(|(k, _)| k.as_str())
        .collect()
}

/// Debug dump of matching generated files (set ROSETTA_CHOICE_DEBUG=1;
/// empty needle dumps everything).
fn dbg_dump(files: &BTreeMap<String, String>, needle: &str) {
    if std::env::var("ROSETTA_CHOICE_DEBUG").is_ok() {
        for (k, v) in files {
            if needle.is_empty() || k.contains(needle) {
                println!("=== {k} ===\n{v}\n");
            }
        }
    }
}

fn classification<'a>(
    data: &'a [codegraph_core::types::SchemaClassificationData],
    title: &str,
) -> &'a codegraph_core::types::SchemaClassificationData {
    data.iter()
        .find(|d| d.title == title)
        .unwrap_or_else(|| panic!("no classification data for {title}"))
}

// ---------------------------------------------------------------------------
// Probe 1 — propertyless oneOf: the is_enum shape
// ---------------------------------------------------------------------------

/// A zero-property root oneOf schema is flagged `is_enum` by the graph
/// querier (the ONLY semantic consumer of `has_one_of`) and lands in the
/// auto-classifier's value_objects with reason "hard:enum".
///
/// Despite the enum-like classification, generation emits a full
/// entity-shaped CRUD stack around a HOLLOW table: only id/tenant/audit
/// columns, no variant representation, no Rust enum, no codelist migration.
#[tokio::test]
async fn propertyless_oneof_is_enum_shaped() {
    let files: Vec<(&str, &str)> = vec![
        ("payment_status.json", PAYMENT_STATUS_JSON),
        ("color_type.json", COLOR_TYPE_JSON),
        ("order_type.json", ORDER_TYPE_JSON),
    ];

    // --- Graph level: is_enum flag + hard:enum auto-classification ---
    let root = tempfile::tempdir().unwrap();
    let be = support::ingest_into_graph(root.path(), "shop", &["OrderType"], &files).await;
    let q = be.querier();

    let data = q.get_classification_data().await.unwrap();
    assert!(classification(&data, "PaymentStatus").is_enum);
    assert!(classification(&data, "ColorType").is_enum);
    assert!(!classification(&data, "OrderType").is_enum);

    let classifier = codegraph::classify::AutoClassifier::new(HashSet::new(), HashMap::new());
    let result = classifier.classify_domain("shop", &plain_domain_entry(&["OrderType"]), &data);
    for title in ["PaymentStatus", "ColorType"] {
        let score = result
            .value_objects
            .iter()
            .find(|s| s.title == title)
            .unwrap_or_else(|| panic!("{title} should auto-classify as VO"));
        assert!(
            score.reasons.iter().any(|r| r == "hard:enum"),
            "{title} reasons={:?}",
            score.reasons
        );
    }

    // --- Pipeline level: what does generation actually produce? ---
    let out_root = tempfile::tempdir().unwrap();
    let generated = support::run_pipeline(out_root.path(), "shop", &["OrderType"], &files).await;
    dbg_dump(&generated, "migrations");

    // Sanity: the real entity produced a normal table.
    let order_ddl = support::file_by_suffix(&generated, "_shop_order.sql").expect("order DDL");
    assert!(order_ddl.contains("note TEXT"));

    // SURPRISE: the is_enum-shaped schemas are NOT skipped and NOT turned
    // into enums — they become full entity-style generation targets with
    // their own (hollow) tables.
    for (table, table_sql, branch_fragments) in [
        ("shop_color", "shop.color", vec!["string", "integer"]),
        (
            "shop_payment_status",
            "shop.payment_status",
            vec!["state", "paid_at", "paidAt"],
        ),
    ] {
        let ddl = support::file_by_suffix(&generated, &format!("_{table}.sql"))
            .unwrap_or_else(|| panic!("expected a migration for {table}"));
        assert!(
            ddl.contains(&format!("CREATE TABLE IF NOT EXISTS {table_sql}")),
            "hollow table for {table}:\n{ddl}"
        );
        // The variants are gone: the table body is only
        // id/platform_organization_id/audit columns.
        for fragment in branch_fragments {
            assert!(
                !ddl.contains(fragment),
                "variant fragment `{fragment}` leaked into {table} DDL:\n{ddl}"
            );
        }
        // No variant representation of any kind in the DDL.
        assert!(
            !ddl.contains("CREATE TYPE") && !ddl.contains("CHECK"),
            "enum type / CHECK constraint emitted for {table}:\n{ddl}"
        );
    }

    // No codelist artifacts either: is_enum does not route into the codelist
    // pipeline (those migrations are named *_codelist.sql).
    let codelist_files: Vec<&str> = generated
        .keys()
        .filter(|k| k.contains("color") || k.contains("payment_status"))
        .filter(|k| k.ends_with("_codelist.sql"))
        .map(|k| k.as_str())
        .collect();
    assert!(
        codelist_files.is_empty(),
        "codelist migrations emitted for is_enum schemas: {codelist_files:?}"
    );

    // ...yet the full CRUD stack is emitted around the hollow table.
    assert!(
        generated
            .keys()
            .any(|k| k.ends_with("src/entity/shop_color.rs")),
        "expected a SeaORM entity for the is_enum-shaped ColorType"
    );
    assert!(
        generated
            .keys()
            .any(|k| k.ends_with("src/entity/shop_payment_status.rs")),
        "expected a SeaORM entity for the is_enum-shaped PaymentStatus"
    );

    // And no Rust enum type was generated for either (word-boundary match so
    // lifecycle enums like `PaymentStatusEvent` / `PaymentStatusCommand` don't
    // false-positive).
    for type_name in ["ColorType", "Color", "PaymentStatus", "Payment"] {
        let enum_hits: Vec<&str> = files_containing(&generated, &format!("enum {type_name} "))
            .into_iter()
            .filter(|k| k.ends_with(".rs"))
            .collect();
        assert!(
            enum_hits.is_empty(),
            "Rust enum generated for {type_name}: {enum_hits:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Probe 2 — entity with a property-level oneOf: branches are dropped
// ---------------------------------------------------------------------------

/// Characterizes what a property-level `oneOf` produces. Graph level: the
/// property is ingested with the ValueObject JSONB catch-all classification.
/// Pipeline level: the property is silently DROPPED from every layer — DDL,
/// entity model, DTOs — because DDL treats ValueObject columns as child
/// CompositionNodes and no child node exists for a $ref-less oneOf.
#[tokio::test]
async fn entity_with_oneof_branches_loses_branches() {
    let files: Vec<(&str, &str)> = vec![("contact_type.json", CONTACT_TYPE_JSON)];

    // --- Graph level ---
    let root = tempfile::tempdir().unwrap();
    let be = support::ingest_into_graph(root.path(), "shop", &["ContactType"], &files).await;
    let q = be.querier();

    // Root-level flag stays false: property-level oneOf is invisible.
    let contact = q.get_schema("ContactType").await.unwrap().unwrap();
    assert!(!contact.has_one_of);
    assert!(!contact.has_any_of);

    let props = q.get_properties("ContactType").await.unwrap();
    let value = props
        .iter()
        .find(|p| p.name == "value")
        .expect("value property");
    // Catch-all classification: oneOf with no `type` key → ValueObject JSONB.
    assert_eq!(
        value.prop_type, "object",
        "oneOf carries no type; defaulted"
    );
    assert_eq!(value.pg_column_type, "JSONB");
    assert_eq!(value.rust_field_type, "serde_json::Value");
    assert_eq!(value.sea_orm_type, "JsonBinary");
    assert_eq!(value.render_strategy, "value_object");
    assert!(
        value.ref_target.is_none(),
        "no $ref → no child-table target"
    );

    // Branch fields are not ingested anywhere.
    assert!(
        !props
            .iter()
            .any(|p| p.name == "extension" || p.name == "label"),
        "oneOf branch fields leaked into properties: {:?}",
        props.iter().map(|p| &p.name).collect::<Vec<_>>()
    );

    // --- Pipeline level ---
    let out_root = tempfile::tempdir().unwrap();
    let generated = support::run_pipeline(out_root.path(), "shop", &["ContactType"], &files).await;
    dbg_dump(&generated, "contact");

    // DDL: the property survived ingestion but is DROPPED from the table —
    // not even a JSONB column, no tag/discriminator, no branch child table.
    let ddl = support::file_by_suffix(&generated, "_shop_contact.sql").expect("contact DDL");
    assert!(ddl.contains("email TEXT NOT NULL"), "sanity:\n{ddl}");
    assert!(
        !ddl.contains("value"),
        "oneOf property must be dropped from DDL entirely:\n{ddl}"
    );
    assert!(
        !generated.keys().any(|k| k.contains("contact_value")),
        "branch child table generated: {:?}",
        generated.keys().collect::<Vec<_>>()
    );

    // Entity model + DTOs: also dropped, with no untyped JSON escape hatch.
    let entity = &generated["src/entity/shop_contact.rs"];
    assert!(entity.contains("pub email"), "sanity:\n{entity}");
    assert!(
        !entity.contains("pub value"),
        "entity kept the oneOf property:\n{entity}"
    );
    let dto_hits: Vec<&str> = generated
        .iter()
        .filter(|(k, _)| k.starts_with("src/") && k.contains("contact") && k.contains("dto"))
        .filter(|(_, v)| v.contains("pub value") || v.contains("serde_json::Value"))
        .map(|(k, _)| k.as_str())
        .collect();
    assert!(
        dto_hits.is_empty(),
        "oneOf property leaked into DTOs as untyped JSON: {dto_hits:?}"
    );
}

// ---------------------------------------------------------------------------
// Probe 3 — property-level anyOf mirrors oneOf
// ---------------------------------------------------------------------------

/// anyOf behaves identically to oneOf: no root flag, JSONB catch-all
/// classification at ingest, silent drop from DDL/entity/DTO at generation.
#[tokio::test]
async fn anyof_flag_only() {
    let files: Vec<(&str, &str)> = vec![("ticket_type.json", TICKET_TYPE_JSON)];

    // --- Graph level ---
    let root = tempfile::tempdir().unwrap();
    let be = support::ingest_into_graph(root.path(), "shop", &["TicketType"], &files).await;
    let q = be.querier();

    let ticket = q.get_schema("TicketType").await.unwrap().unwrap();
    assert!(!ticket.has_one_of);
    assert!(
        !ticket.has_any_of,
        "property-level anyOf must not set the root flag"
    );

    let props = q.get_properties("TicketType").await.unwrap();
    let payload = props
        .iter()
        .find(|p| p.name == "payload")
        .expect("payload property");
    assert_eq!(payload.pg_column_type, "JSONB");
    assert_eq!(payload.rust_field_type, "serde_json::Value");
    assert!(
        !props.iter().any(|p| p.name == "kind" || p.name == "detail"),
        "anyOf branch fields leaked into properties"
    );

    // --- Pipeline level ---
    let out_root = tempfile::tempdir().unwrap();
    let generated = support::run_pipeline(out_root.path(), "shop", &["TicketType"], &files).await;
    dbg_dump(&generated, "ticket");

    let ddl = support::file_by_suffix(&generated, "_shop_ticket.sql").expect("ticket DDL");
    assert!(ddl.contains("title TEXT NOT NULL"), "sanity:\n{ddl}");
    assert!(
        !ddl.contains("payload"),
        "anyOf property must be dropped from DDL entirely:\n{ddl}"
    );
    let entity = &generated["src/entity/shop_ticket.rs"];
    assert!(
        !entity.contains("pub payload"),
        "entity kept the anyOf property:\n{entity}"
    );
    assert!(
        !generated.keys().any(|k| k.contains("ticket_payload")),
        "branch child table generated"
    );
}

// ---------------------------------------------------------------------------
// Probe 4 — flags persisted on the graph nodes
// ---------------------------------------------------------------------------

/// Root-level oneOf/anyOf persist as SchemaNode boolean flags; anything
/// nested (property-level) does not. Also pins the is_enum asymmetry:
/// only oneOf feeds is_enum, anyOf does not.
#[tokio::test]
async fn oneof_flags_persisted() {
    let files: Vec<(&str, &str)> = vec![
        ("payment_status.json", PAYMENT_STATUS_JSON),
        ("wrap_type.json", WRAP_TYPE_JSON),
        ("plain_type.json", PLAIN_TYPE_JSON),
        ("contact_type.json", CONTACT_TYPE_JSON),
    ];
    let root = tempfile::tempdir().unwrap();
    let be = support::ingest_into_graph(root.path(), "shop", &[], &files).await;
    let q = be.querier();

    let payment = q.get_schema("PaymentStatus").await.unwrap().unwrap();
    assert!(payment.has_one_of);
    assert!(!payment.has_any_of);

    let wrap = q.get_schema("WrapType").await.unwrap().unwrap();
    assert!(!wrap.has_one_of);
    assert!(wrap.has_any_of);

    let plain = q.get_schema("PlainType").await.unwrap().unwrap();
    assert!(!plain.has_one_of);
    assert!(!plain.has_any_of);

    let contact = q.get_schema("ContactType").await.unwrap().unwrap();
    assert!(
        !contact.has_one_of,
        "property-level oneOf must not set the root flag"
    );
    assert!(!contact.has_any_of);

    // is_enum reads has_one_of only — a zero-property anyOf schema is NOT
    // enum-shaped by this definition (asymmetry).
    let data = q.get_classification_data().await.unwrap();
    assert!(classification(&data, "PaymentStatus").is_enum);
    assert!(
        !classification(&data, "WrapType").is_enum,
        "anyOf must not feed is_enum (asymmetry pin)"
    );
    assert!(!classification(&data, "PlainType").is_enum);
}
