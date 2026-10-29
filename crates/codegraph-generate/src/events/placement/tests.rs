//! Unit tests for the workers-topology event placement (issue #505).

use std::collections::HashSet;

use codegraph_config::config::parse_domain_config_str;
use codegraph_core::types::{
    DeployApplicationNode, DeployComponentNode, DeployConnectionNode, DeployDeploymentNode,
    DeployModelGraph, DeployProfileNode,
};

use crate::events::placement::{EventPlacement, compute_placement};
use crate::events::{
    EventArchitecture, EventChannel, EventDefinition, EventSubscription, PgmqChannel, Publication,
    PublicationSource, Transport,
};
use crate::scaffold::worker::DeployShape;

fn config() -> codegraph_config::DomainConfig {
    parse_domain_config_str(
        r#"
[domains.orders]
label = "Orders"
schema_dir = "orders"
postgres_schema = "orders"

[domains.billing]
label = "Billing"
schema_dir = "billing"
postgres_schema = "billing"
"#,
    )
    .unwrap()
}

fn worker_domains(names: &[&str]) -> HashSet<String> {
    names.iter().map(|n| n.to_string()).collect()
}

fn event(name: &str, domain: Option<&str>) -> EventDefinition {
    EventDefinition {
        name: name.to_string(),
        version: None,
        fields: Vec::new(),
        domain: domain.map(|d| d.to_string()),
    }
}

fn channel(name: &str, events: &[&str]) -> EventChannel {
    EventChannel {
        name: name.to_string(),
        transport: Transport::Pgmq(PgmqChannel {
            queue: format!("events_{name}"),
        }),
        publications: events
            .iter()
            .map(|e| Publication {
                event: e.to_string(),
                source: PublicationSource::Application,
            })
            .collect(),
    }
}

fn domain_channel(domain: &str) -> EventChannel {
    EventChannel {
        name: domain.to_string(),
        transport: Transport::Pgmq(PgmqChannel {
            queue: format!("events_{domain}"),
        }),
        publications: Vec::new(),
    }
}

fn subscription(name: &str, events: &[&str], consumer: &str) -> EventSubscription {
    EventSubscription {
        name: name.to_string(),
        events: events.iter().map(|e| e.to_string()).collect(),
        consumer: consumer.to_string(),
        delivery: crate::events::DeliveryPolicy::at_least_once_default(),
    }
}

fn arch(
    events: Vec<EventDefinition>,
    channels: Vec<EventChannel>,
    domain_channels: Vec<EventChannel>,
    subscriptions: Vec<EventSubscription>,
) -> EventArchitecture {
    EventArchitecture {
        events,
        channels,
        domain_channels,
        subscriptions,
        function_consumers: Vec::new(),
    }
}

fn queue_component(application: &str, name: &str) -> DeployComponentNode {
    DeployComponentNode {
        application: application.to_string(),
        name: name.to_string(),
        kind: "queue".to_string(),
        requires: Vec::new(),
        settings: Vec::new(),
        designs: Vec::new(),
        flows: Vec::new(),
        ordinal: 0,
    }
}

fn worker_component(application: &str, name: &str) -> DeployComponentNode {
    DeployComponentNode {
        application: application.to_string(),
        name: name.to_string(),
        kind: "worker".to_string(),
        requires: Vec::new(),
        settings: Vec::new(),
        designs: Vec::new(),
        flows: Vec::new(),
        ordinal: 0,
    }
}

fn deploy_shape(
    components: Vec<DeployComponentNode>,
    connections: Vec<(&str, &str)>,
) -> DeployShape {
    DeployShape {
        components,
        connections: connections
            .into_iter()
            .map(|(from, to)| DeployConnectionNode {
                application: "app".to_string(),
                from: from.to_string(),
                to: to.to_string(),
            })
            .collect(),
    }
}

#[allow(dead_code)]
fn deploy_model(components: Vec<DeployComponentNode>) -> DeployModelGraph {
    DeployModelGraph {
        source_path: "fleet.deploy".to_string(),
        applications: vec![DeployApplicationNode {
            source_path: "fleet.deploy".to_string(),
            name: "app".to_string(),
            components,
            connections: Vec::new(),
        }],
        profiles: vec![DeployProfileNode {
            source_path: "fleet.deploy".to_string(),
            name: "cf".to_string(),
            target: "cloudflareWorkers".to_string(),
            defaults: Vec::new(),
            policies: Vec::new(),
            mappings: Vec::new(),
        }],
        deployments: vec![DeployDeploymentNode {
            source_path: "fleet.deploy".to_string(),
            name: "main".to_string(),
            profile: "cf".to_string(),
            application: "app".to_string(),
            configure: Vec::new(),
        }],
    }
}

// ── tier 0: trigger queues ──────────────────────────────────────────────

#[test]
fn domain_trigger_queue_is_owned_by_its_own_worker() {
    let a = arch(vec![], vec![], vec![domain_channel("orders")], vec![]);
    let p =
        compute_placement(&a, &config(), &worker_domains(&["orders", "billing"]), None).unwrap();
    assert_eq!(p.worker_drains.get("orders").unwrap(), &["orders"]);
    assert!(!p.worker_drains.contains_key("billing"));
    assert!(p.drains_any("orders"));
    assert!(!p.drains_any("billing"));
}

// ── tier 1: channel named like a domain ─────────────────────────────────

#[test]
fn channel_named_like_a_domain_collapses_into_the_domain_drain() {
    let a = arch(
        vec![event("OrderPlaced", Some("orders"))],
        vec![channel("orders", &["OrderPlaced"])],
        vec![domain_channel("orders")],
        vec![],
    );
    let p = compute_placement(&a, &config(), &worker_domains(&["orders"]), None).unwrap();
    assert_eq!(p.worker_drains.get("orders").unwrap(), &["orders"]);
}

// ── tier 2: payload-domain resolution ───────────────────────────────────

#[test]
fn channel_with_single_domain_events_drains_on_that_domain() {
    // `billing-events` is not a domain; its only event resolves to
    // `billing`.
    let a = arch(
        vec![event("InvoiceIssued", Some("billing"))],
        vec![channel("billing-events", &["InvoiceIssued"])],
        vec![domain_channel("orders")],
        vec![],
    );
    let p =
        compute_placement(&a, &config(), &worker_domains(&["orders", "billing"]), None).unwrap();
    assert_eq!(p.extra_drains("billing"), ["billing-events"]);
    assert_eq!(p.worker_drains.get("orders").unwrap(), &["orders"]);
}

#[test]
fn channel_with_ambiguous_event_domains_is_a_hard_error() {
    let a = arch(
        vec![
            event("OrderPlaced", Some("orders")),
            event("InvoiceIssued", Some("billing")),
        ],
        vec![channel("audit", &["OrderPlaced", "InvoiceIssued"])],
        vec![domain_channel("orders"), domain_channel("billing")],
        vec![],
    );
    let err = compute_placement(&a, &config(), &worker_domains(&["orders", "billing"]), None)
        .expect_err("ambiguous channel must error");
    let message = err.to_string();
    assert!(message.contains("`audit'"), "{message}");
    assert!(message.contains("events_audit"), "{message}");
    assert!(message.contains("billing, orders"), "{message}");
    assert!(message.contains("kind = \"queue\""), "{message}");
}

#[test]
fn all_primitive_events_leave_no_domain_candidates() {
    let a = arch(
        vec![event("Ping", None)],
        vec![channel("pings", &["Ping"])],
        vec![domain_channel("orders")],
        vec![],
    );
    let err = compute_placement(&a, &config(), &worker_domains(&["orders"]), None)
        .expect_err("unresolvable channel must error");
    assert!(err.to_string().contains("no worker domain"), "{err}");
}

// ── tier 3: the deploy queue-component override ─────────────────────────

#[test]
fn deploy_queue_component_connect_edge_places_an_ambiguous_channel() {
    let a = arch(
        vec![
            event("OrderPlaced", Some("orders")),
            event("InvoiceIssued", Some("billing")),
        ],
        vec![channel("audit", &["OrderPlaced", "InvoiceIssued"])],
        vec![domain_channel("orders"), domain_channel("billing")],
        vec![],
    );
    let shape = deploy_shape(
        vec![
            worker_component("app", "orders"),
            queue_component("app", "audit"),
        ],
        vec![("orders", "audit")],
    );
    let p = compute_placement(
        &a,
        &config(),
        &worker_domains(&["orders", "billing"]),
        Some(&shape),
    )
    .unwrap();
    assert_eq!(p.extra_drains("orders"), ["audit"]);
}

#[test]
fn deploy_reversed_connect_edge_is_accepted() {
    let a = arch(
        vec![event("Ping", None)],
        vec![channel("pings", &["Ping"])],
        vec![domain_channel("orders")],
        vec![],
    );
    let shape = deploy_shape(
        vec![
            queue_component("app", "pings"),
            worker_component("app", "orders"),
        ],
        vec![("pings", "orders")],
    );
    let p = compute_placement(&a, &config(), &worker_domains(&["orders"]), Some(&shape)).unwrap();
    assert_eq!(p.extra_drains("orders"), ["pings"]);
}

#[test]
fn queue_component_without_a_connect_edge_is_still_a_miss() {
    let a = arch(
        vec![event("Ping", None)],
        vec![channel("pings", &["Ping"])],
        vec![domain_channel("orders")],
        vec![],
    );
    let shape = deploy_shape(vec![queue_component("app", "pings")], vec![]);
    let err = compute_placement(&a, &config(), &worker_domains(&["orders"]), Some(&shape))
        .expect_err("an unconnected queue component owns nothing");
    assert!(err.to_string().contains("`pings'"), "{err}");
}

#[test]
fn non_worker_component_connect_edge_is_not_an_owner() {
    let a = arch(
        vec![event("Ping", None)],
        vec![channel("pings", &["Ping"])],
        vec![domain_channel("orders")],
        vec![],
    );
    // The edge targets the gateway (an api component NOT named like a
    // worker domain) — the gateway is not a drain owner under the
    // hard-error fallback.
    let shape = deploy_shape(
        vec![
            queue_component("app", "pings"),
            DeployComponentNode {
                kind: "api".to_string(),
                name: "gateway".to_string(),
                ..worker_component("app", "gateway")
            },
        ],
        vec![("pings", "gateway")],
    );
    let err = compute_placement(&a, &config(), &worker_domains(&["orders"]), Some(&shape))
        .expect_err("the gateway is not a drain owner");
    assert!(err.to_string().contains("`pings'"), "{err}");
}

// ── consumer placement: consumers ride queue owners ─────────────────────

#[test]
fn consumers_are_placed_on_every_owner_of_their_queues() {
    // `billing_service` subscribes to events on the orders channel queue
    // AND the billing-events channel queue: placed in BOTH workers.
    let a = arch(
        vec![
            event("OrderPlaced", Some("orders")),
            event("InvoiceIssued", Some("billing")),
        ],
        vec![
            channel("orders", &["OrderPlaced"]),
            channel("billing-events", &["InvoiceIssued"]),
        ],
        vec![domain_channel("orders"), domain_channel("billing")],
        vec![
            subscription(
                "billing",
                &["OrderPlaced", "InvoiceIssued"],
                "billing_service",
            ),
            subscription("orders-only", &["OrderPlaced"], "orders_watch"),
        ],
    );
    let p =
        compute_placement(&a, &config(), &worker_domains(&["orders", "billing"]), None).unwrap();
    assert_eq!(
        p.consumers_for("orders"),
        ["billing_service", "orders_watch"]
    );
    assert_eq!(p.consumers_for("billing"), ["billing_service"]);
}

#[test]
fn undeclared_subscription_events_are_skipped_like_the_consumers_context() {
    let a = arch(
        vec![event("OrderPlaced", Some("orders"))],
        vec![channel("orders", &["OrderPlaced"])],
        vec![domain_channel("orders")],
        vec![
            subscription("ghost", &["GhostEvent"], "ghost_worker"),
            subscription("real", &["OrderPlaced"], "real_worker"),
        ],
    );
    let p = compute_placement(&a, &config(), &worker_domains(&["orders"]), None).unwrap();
    assert_eq!(p.consumers_for("orders"), ["real_worker"]);
}

// ── inertness ───────────────────────────────────────────────────────────

#[test]
fn empty_architecture_yields_empty_placement() {
    let p = compute_placement(
        &EventArchitecture::default(),
        &config(),
        &worker_domains(&["orders"]),
        None,
    )
    .unwrap();
    assert_eq!(p, EventPlacement::default());
}

#[test]
fn placement_is_deterministic() {
    let a = arch(
        vec![
            event("OrderPlaced", Some("orders")),
            event("InvoiceIssued", Some("billing")),
        ],
        vec![
            channel("billing-events", &["InvoiceIssued"]),
            channel("alerts", &["OrderPlaced"]),
        ],
        vec![domain_channel("orders"), domain_channel("billing")],
        vec![
            subscription(
                "billing",
                &["OrderPlaced", "InvoiceIssued"],
                "billing_service",
            ),
            subscription("alerts", &["OrderPlaced"], "alerter"),
        ],
    );
    let p1 =
        compute_placement(&a, &config(), &worker_domains(&["orders", "billing"]), None).unwrap();
    let p2 =
        compute_placement(&a, &config(), &worker_domains(&["orders", "billing"]), None).unwrap();
    assert_eq!(p1, p2);
    assert_eq!(p1.extra_drains("orders"), ["alerts"]);
    assert_eq!(p1.extra_drains("billing"), ["billing-events"]);
    assert_eq!(
        p1.worker_drains.get("orders").unwrap(),
        &["alerts", "orders"]
    );
}
