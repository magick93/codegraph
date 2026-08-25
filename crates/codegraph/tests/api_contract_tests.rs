//! Cross-check: the `api_contract` generator's emitted basePath/entity routes
//! must match the `router` generator's mounted paths for the same domain.
//! This guards issue #54 (workers-vs-monolith API contract drift) — in
//! particular the snake_case domains (`community_graph`, `provider_portal`)
//! whose base path is `/api/v1/{verbatim domain key}`.
//! Run with: cargo test -p codegraph --test api_contract_tests

use std::collections::HashMap;
use std::path::Path;

use codegraph::generate::traits::{DomainGenerator, GlobalGenerator};
use codegraph::generate::{GenerationEntry, ProjectConfig};
use codegraph_config::config::{DefaultsConfig, DomainConfig, DomainEntry};
use codegraph_core::mock::MockEngine;
use codegraph_core::types::SchemaNode;

fn schema(
    title: &str,
    rust_type_name: &str,
    pg_table_name: &str,
    api_path_segment: &str,
    domain: &str,
) -> SchemaNode {
    SchemaNode {
        schema_id: title.to_lowercase(),
        title: title.into(),
        description: Some("test schema".into()),
        schema_type: "object".into(),
        classification: "entity".into(),
        domain: Some(domain.into()),
        rel_path: format!("{domain}/json/{title}.json"),
        pg_type: "entity".into(),
        rust_type: title.into(),
        sea_orm_type: "Entity".into(),
        rust_type_name: rust_type_name.into(),
        pg_table_name: pg_table_name.into(),
        api_path_segment: api_path_segment.into(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
    }
}

fn domain_config(domain: &str, entities: &[&str]) -> DomainConfig {
    let mut domains = HashMap::new();
    domains.insert(
        domain.to_string(),
        DomainEntry {
            label: domain.to_string(),
            schema_dir: format!("schemas/{domain}"),
            postgres_schema: domain.to_string(),
            depends_on: vec!["common".into()],
            entities: entities.iter().map(|e| e.to_string()).collect(),
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
        },
    );
    DomainConfig {
        defaults: DefaultsConfig {
            operations: vec![
                "create".into(),
                "read".into(),
                "update".into(),
                "delete".into(),
                "list".into(),
            ],
            auto_discover: false,
            split_openapi_by_domain: false,
            app_name: "test-app".into(),
            max_bulk_size: 100,
            type_suffix: "Type".into(),
            types_import_prefix: "codegraph_type_contracts".into(),
            generation_mode: "full".into(),
            api_version: "v1".into(),
        },
        domains,
    }
}

fn community_graph_engine() -> MockEngine {
    MockEngine::builder()
        .with_schema(schema(
            "RelationshipType",
            "Relationship",
            "relationship",
            "relationship",
            "community_graph",
        ))
        .with_schema(schema(
            "TrustConnectionType",
            "TrustConnection",
            "trust_connection",
            "trust-connection",
            "community_graph",
        ))
        .with_schema(schema(
            "AmbassadorType",
            "Ambassador",
            "ambassador",
            "ambassador",
            "community_graph",
        ))
        .build()
}

fn events_engine() -> MockEngine {
    MockEngine::builder()
        .with_schema(schema(
            "PublicEventType",
            "PublicEvent",
            "public_event",
            "public-event",
            "events",
        ))
        .with_schema(schema(
            "EventsAppType",
            "EventsApp",
            "events_app",
            "events-app",
            "events",
        ))
        .build()
}

fn create_tera() -> tera::Tera {
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    codegraph::generate::template_engine::create_tera(&template_dir).unwrap()
}

/// Assert every `.nest("/<segment>"...` path the router mounts also appears as
/// an entity route in the generated contract (both `.ts` and `.json`).
fn assert_contract_matches_router(
    engine: &MockEngine,
    config: &DomainConfig,
    domain: &str,
    entity_titles: &[String],
) {
    codegraph::generate::type_registry::register_framework_types();
    let tera = create_tera();
    let project = ProjectConfig::default();

    let router_gen = codegraph::generate::api::router::RouterGenerator::new(Path::new(
        "/tmp/api-contract-router",
    ));
    let router_files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(router_gen.generate(engine, domain, entity_titles, config, &tera, &project))
        .expect("RouterGenerator failed");
    let router_rs = &router_files[0].content;

    let contract_gen = codegraph::generate::api::contract::ApiContractGenerator::new(Path::new(
        "/tmp/api-contract-out",
    ));
    let contract_files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(contract_gen.generate(engine, domain, entity_titles, config, &tera, &project))
        .expect("ApiContractGenerator failed");

    // Two files: {domain}.ts and {domain}.json.
    let ts = contract_files
        .iter()
        .find(|f| f.path.extension().and_then(|e| e.to_str()) == Some("ts"))
        .expect("contract must emit a .ts file");
    let json = contract_files
        .iter()
        .find(|f| f.path.extension().and_then(|e| e.to_str()) == Some("json"))
        .expect("contract must emit a .json file");
    let parsed: serde_json::Value =
        serde_json::from_str(&json.content).expect("contract json must parse");

    // The contract must claim the exact verbatim base path for this domain.
    let expected_base = format!("/api/v1/{domain}");
    assert_eq!(
        parsed["basePath"], expected_base,
        "json contract basePath must be {expected_base} for {domain}\n{}",
        json.content
    );
    assert!(
        ts.content
            .contains(&format!("basePath: \"{expected_base}\"")),
        "ts contract must contain basePath {expected_base} for {domain}\n{}",
        ts.content
    );

    // Extract the segments the router actually nests and verify each one is a
    // key in the contract entities with the correct full base path.
    let mut checked = 0;
    for line in router_rs.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(".nest(\"/") {
            // Segment ends at the closing quote: `.nest("/relationship", ...)`
            let segment = rest.split('"').next().unwrap_or("");
            if segment.is_empty() {
                continue;
            }
            let expected_path = format!("{expected_base}/{segment}");
            let entity = parsed["entities"].get(segment).unwrap_or_else(|| {
                panic!(
                    "router nests /{segment} but json contract has no entities[\"{segment}\"]\n{}",
                    json.content
                )
            });
            assert_eq!(
                entity["basePath"], expected_path,
                "router nests /{segment} but json contract basePath differs\n{}",
                json.content
            );
            assert!(
                ts.content
                    .contains(&format!("basePath: \"{expected_path}\"")),
                "router nests /{segment} but ts contract lacks basePath {expected_path}\n{}",
                ts.content
            );
            // The list route (GET) must be present for the entity.
            let list_route = entity["routes"]
                .as_array()
                .expect("routes must be an array")
                .iter()
                .any(|r| r["method"] == "GET" && r["path"] == expected_path);
            assert!(
                list_route,
                "json contract must list GET {expected_path}\n{}",
                json.content
            );
            checked += 1;
        }
    }
    assert!(
        checked >= entity_titles.len(),
        "router should nest at least one segment per entity, checked {checked}"
    );
}

#[test]
fn snake_case_domain_contract_matches_router() {
    let engine = community_graph_engine();
    let config = domain_config(
        "community_graph",
        &["RelationshipType", "TrustConnectionType", "AmbassadorType"],
    );
    let titles: Vec<String> = config.domains["community_graph"]
        .entities
        .iter()
        .cloned()
        .collect();
    assert_contract_matches_router(&engine, &config, "community_graph", &titles);
}

#[test]
fn lowercase_domain_contract_matches_router() {
    let engine = events_engine();
    let config = domain_config("events", &["PublicEventType", "EventsAppType"]);
    let titles: Vec<String> = config.domains["events"].entities.iter().cloned().collect();
    assert_contract_matches_router(&engine, &config, "events", &titles);
}

#[test]
fn index_generator_aggregates_every_domain() {
    let engine = community_graph_engine();
    let config = domain_config(
        "community_graph",
        &["RelationshipType", "TrustConnectionType", "AmbassadorType"],
    );
    let order = vec![
        GenerationEntry {
            schema_title: "RelationshipType".into(),
            domain: "community_graph".into(),
            pg_schema: "community_graph".into(),
            is_cyclic: false,
        },
        GenerationEntry {
            schema_title: "TrustConnectionType".into(),
            domain: "community_graph".into(),
            pg_schema: "community_graph".into(),
            is_cyclic: false,
        },
        GenerationEntry {
            schema_title: "AmbassadorType".into(),
            domain: "community_graph".into(),
            pg_schema: "community_graph".into(),
            is_cyclic: false,
        },
    ];
    let tera = create_tera();
    let project = ProjectConfig::default();
    let gen = codegraph::generate::api::contract::ApiContractIndexGenerator::new(Path::new(
        "/tmp/api-contract-index",
    ));
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(gen.generate(&engine, &config, &order, &tera, &project))
        .expect("ApiContractIndexGenerator failed");
    let index = files
        .iter()
        .find(|f| f.path.ends_with("index.ts"))
        .expect("index generator must emit index.ts");
    assert!(
        index.content.contains("CommunityGraphApiContract"),
        "index must re-export community_graph contract\n{}",
        index.content
    );
    assert!(
        index.content.contains("from \"./community_graph\""),
        "index must re-export from ./community_graph\n{}",
        index.content
    );
}
