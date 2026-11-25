//! The Ask 1 dead-surface diagnostics (issue #590) as narrowed by issue
//! #619: a `protected` operation is warned about ONLY when an `.ifml`
//! action binds it — and then exactly once, by [`ifml_dead_triggers`],
//! which names the action. [`dead_surface_warnings`] keeps the
//! entity-unbound class (a real defect: a public op the run cannot route)
//! and says nothing about `protected` on its own: Sculptor `protected` is
//! a valid, intentional visibility (upstream: "the operation stays off the
//! public interface"), so an unbound one is by design, not a defect.
//!
//! The binding rule is the one the IFML action plane and upstream's
//! `check_action_bindings` share: an action binds an operation by
//! ASCII-case-insensitive name equality.

use super::*;
use codegraph_core::mock::MockEngine;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{
    ActionNode, DddApplicationNode, DddChildRef, DddDesignFlags, DddDesignNode, DddModelGraph,
    DddRepositoryNode, DddServiceNode, DddServiceOperation,
};

fn service_op(name: &str, is_protected: bool) -> DddServiceOperation {
    DddServiceOperation {
        name: name.to_string(),
        return_type: None,
        return_multiplicity: None,
        params: Vec::new(),
        delegation_target: None,
        delegation_operation: None,
        capabilities: Vec::new(),
        is_protected,
        condition: None,
        emits: Vec::new(),
        invokes: Vec::new(),
        throws: Vec::new(),
        bodies: Default::default(),
        body_receiver: None,
        transition: None,
        child_scope: None,
        condition_paths: Vec::new(),
        ordinal: 0,
    }
}

/// A `children PayRunType { … => lines.add; }` operation.
fn child_op(name: &str, is_protected: bool) -> DddServiceOperation {
    let mut op = service_op(name, is_protected);
    op.child_scope = Some(DddChildRef {
        feature: "lines".to_string(),
        class: "payroll.PayRunLineType".to_string(),
        action: "add".to_string(),
    });
    op
}

/// `PayRunService` carrying `service_ops`, plus (when `child_ops` is
/// non-empty) the `PayRunType` root design owning them.
fn model(
    service_ops: Vec<DddServiceOperation>,
    child_ops: Vec<DddServiceOperation>,
) -> DddModelGraph {
    DddModelGraph {
        source_path: "test.ddd".to_string(),
        application: DddApplicationNode {
            name: "App".to_string(),
            base: None,
            source_path: "test.ddd".to_string(),
            errors: Vec::new(),
        },
        modules: Vec::new(),
        designs: if child_ops.is_empty() {
            Vec::new()
        } else {
            vec![DddDesignNode {
                application: "App".to_string(),
                module: "payroll".to_string(),
                class: "PayRunType".to_string(),
                resolved_title: Some("PayRunType".to_string()),
                stereotype: "entity".to_string(),
                is_abstract: false,
                flags: DddDesignFlags::default(),
                machine_json: None,
                machine_ir_json: None,
                machine_provenance_json: None,
                child_operations: child_ops,
                aggregate_boundary: None,
                invariants: Vec::new(),
                ordinal: 0,
            }]
        },
        repositories: Vec::new(),
        services: vec![DddServiceNode {
            application: "App".to_string(),
            module: "payroll".to_string(),
            name: "PayRunService".to_string(),
            description: None,
            dependencies: Vec::new(),
            decision_dependencies: Vec::new(),
            operations: service_ops,
            ordinal: 0,
        }],
        searches: Vec::new(),
        dto_shapes: Vec::new(),
    }
}

async fn engine(model: &DddModelGraph, actions: &[&str]) -> MockEngine {
    let db = MockEngine::builder().build();
    db.ingest_ddd_model(model).await.unwrap();
    for name in actions {
        db.ingest_action_node(&ActionNode {
            name: (*name).to_string(),
            domain: None,
        })
        .await
        .unwrap();
    }
    db
}

fn config() -> DomainConfig {
    toml::from_str(
        r#"
[domains.payroll]
label = "Payroll"
schema_dir = "schemas/payroll"
postgres_schema = "payroll"
"#,
    )
    .unwrap()
}

/// Every line the two Ask 1 checks produce that mentions `op` — the
/// channel a reader sees, both checks combined. Triggers render through
/// [`dead_trigger_message`], the one production shape the run path and
/// doctor both print.
async fn mentions(db: &MockEngine, cfg: &DomainConfig, op: &str) -> Vec<String> {
    let mut lines = dead_surface_warnings(db, cfg).await.unwrap();
    lines.extend(
        ifml_dead_triggers(db, cfg)
            .await
            .unwrap()
            .into_iter()
            .map(|(action, op, reason)| dead_trigger_message(&action, &op, &reason)),
    );
    lines.into_iter().filter(|l| l.contains(op)).collect()
}

const PROTECTED: &str = "the op is `protected` (Sculptor visibility)";
const PROTECTED_CHILD: &str = "the child operation is `protected` (Sculptor visibility)";

// ── the shared join (one function for the run path and doctor) ─────────

/// An `OpRef` for the join KATs (a fn, not a closure: the two borrowed
/// names must share one lifetime).
fn op_ref<'a>(owner: &'a str, name: &'a str, protected: bool) -> OpRef<'a> {
    OpRef {
        owner,
        name,
        protected,
    }
}

#[test]
fn the_join_binds_the_first_service_op_then_the_first_child_op() {
    let op = op_ref;
    let none = std::iter::empty;
    // No operation at all.
    assert_eq!(
        protected_binding("recordStatus", none(), none()),
        None,
        "an action naming nothing is the upstream binding check's business"
    );
    // A public service op outranks a same-named protected child op.
    assert_eq!(
        protected_binding(
            "addLine",
            [op("PayRunService", "addLine", false)].into_iter(),
            [op("PayRunType", "addLine", true)].into_iter(),
        ),
        None,
        "the button targets the mounted service-op route"
    );
    // The first service op by name decides: a protected one is reported
    // even when a public op shares the name further down.
    assert_eq!(
        protected_binding(
            "RecordStatus",
            [
                op("VendorsService", "recordStatus", true),
                op("OtherService", "recordStatus", false),
            ]
            .into_iter(),
            none(),
        ),
        Some(("VendorsService::recordStatus".to_string(), PROTECTED))
    );
    // No service op named: the protected child op is the dead button …
    assert_eq!(
        protected_binding(
            "AddLine",
            [op("PayRunService", "removeLine", true)].into_iter(),
            [op("PayRunType", "addLine", true)].into_iter(),
        ),
        Some(("PayRunType::addLine".to_string(), PROTECTED_CHILD))
    );
    // … and a PUBLIC child op is not a trigger (it emits a route).
    assert_eq!(
        protected_binding(
            "addLine",
            none(),
            [op("PayRunType", "addLine", false)].into_iter(),
        ),
        None
    );
    // The qualified label the warning carries.
    assert_eq!(
        OpRef {
            owner: "PayRunService",
            name: "recordStatus",
            protected: true,
        }
        .qualified(),
        "PayRunService::recordStatus"
    );
}

#[test]
fn the_dead_trigger_line_is_one_shape_with_no_reflow_gap() {
    // The line the run path and doctor both print (issue #619): the two
    // literals used to carry a run of spaces where a `\` continuation was
    // intended ("…emits no API                      surface").
    assert_eq!(
        dead_trigger_message("RecordStatus", "VendorsService::recordStatus", PROTECTED),
        "action 'RecordStatus' targets op 'VendorsService::recordStatus' which emits no API \
         surface (the op is `protected` (Sculptor visibility)); the trigger is dead"
    );
    assert_eq!(
        dead_trigger_message("addLine", "PayRunType::addLine", PROTECTED_CHILD),
        "action 'addLine' targets op 'PayRunType::addLine' which emits no API surface (the \
         child operation is `protected` (Sculptor visibility)); the trigger is dead"
    );
}

// ── (a) unbound protected ops produce NO warning ───────────────────────

#[tokio::test]
async fn an_unbound_protected_service_op_warns_nowhere() {
    let cfg = config();
    let db = engine(
        &model(vec![service_op("recordStatus", true)], Vec::new()),
        &[],
    )
    .await;
    let warnings = dead_surface_warnings(&db, &cfg).await.unwrap();
    assert!(
        warnings.is_empty(),
        "an unbound protected op is by design: {warnings:?}"
    );
    let triggers = ifml_dead_triggers(&db, &cfg).await.unwrap();
    assert!(
        triggers.is_empty(),
        "no action binds it, so no trigger: {triggers:?}"
    );

    // An `.ifml` model that binds OTHER ops does not change that: only an
    // action naming the protected op makes it a dead trigger.
    let db = engine(
        &model(vec![service_op("recordStatus", true)], Vec::new()),
        &["approve"],
    )
    .await;
    assert_eq!(
        mentions(&db, &cfg, "recordStatus").await,
        Vec::<String>::new(),
        "an unrelated action must not wake the protected op"
    );
}

#[tokio::test]
async fn an_unbound_protected_child_op_warns_nowhere() {
    let cfg = config();
    let db = engine(&model(Vec::new(), vec![child_op("addLine", true)]), &[]).await;
    assert_eq!(
        mentions(&db, &cfg, "addLine").await,
        Vec::<String>::new(),
        "an unbound protected child op is by design"
    );
    let db = engine(
        &model(Vec::new(), vec![child_op("addLine", true)]),
        &["removeLine"],
    )
    .await;
    assert_eq!(
        mentions(&db, &cfg, "addLine").await,
        Vec::<String>::new(),
        "an unrelated action must not wake the protected child op"
    );
}

#[tokio::test]
async fn the_route_plane_stays_silent_on_an_unbound_protected_child_op() {
    // The THIRD site that used to warn unconditionally: the generation-time
    // `warn_once` in `routes_for_entity`. A protected child op still emits
    // no route (the API plane is untouched) — it just no longer claims that
    // "IFML actions bound to it are dead triggers" when none are.
    let db = MockEngine::builder()
        .with_schema(schema_node("PayRunType"))
        .with_schema(schema_node("PayRunLineType"))
        .build();
    db.ingest_ddd_model(&model(Vec::new(), vec![child_op("addLine", true)]))
        .await
        .unwrap();
    let cfg = config();
    let routes = routes_for_entity(
        &db,
        &cfg,
        "payroll",
        "PayRunLineType",
        &BodyRenderContext::default(),
    )
    .await
    .unwrap();
    assert!(routes.is_empty(), "a protected child op emits no route");
    assert!(
        !crate::ddd::design::warned_keys()
            .iter()
            .any(|key| key.starts_with("child-op-protected")),
        "the route plane must not warn on an unbound protected child op: {:?}",
        crate::ddd::design::warned_keys()
            .into_iter()
            .filter(|key| key.contains("protected"))
            .collect::<Vec<_>>()
    );
}

// ── (b) a bound protected op warns EXACTLY once, naming the action ────

#[tokio::test]
async fn a_bound_protected_service_op_is_reported_exactly_once_naming_the_action() {
    let cfg = config();
    let db = engine(
        &model(vec![service_op("recordStatus", true)], Vec::new()),
        &["recordStatus"],
    )
    .await;
    assert!(
        dead_surface_warnings(&db, &cfg).await.unwrap().is_empty(),
        "the trigger family owns the bound-protected report — no second line"
    );
    let triggers = ifml_dead_triggers(&db, &cfg).await.unwrap();
    assert_eq!(
        triggers,
        vec![(
            "recordStatus".to_string(),
            "PayRunService::recordStatus".to_string(),
            PROTECTED.to_string(),
        )],
        "one line, naming the action and the qualified op"
    );
    let lines = mentions(&db, &cfg, "recordStatus").await;
    assert_eq!(lines.len(), 1, "exactly once across both checks: {lines:?}");
    assert_eq!(
        lines[0],
        "action 'recordStatus' targets op 'PayRunService::recordStatus' which emits no API \
         surface (the op is `protected` (Sculptor visibility)); the trigger is dead",
        "the surviving line names the action and the op it binds"
    );
}

#[tokio::test]
async fn the_binding_is_ascii_case_insensitive() {
    // IFML actions are PascalCase labels, service ops camelCase — the
    // upstream `check_action_bindings` pairing.
    let cfg = config();
    let db = engine(
        &model(vec![service_op("recordStatus", true)], Vec::new()),
        &["RecordStatus"],
    )
    .await;
    let triggers = ifml_dead_triggers(&db, &cfg).await.unwrap();
    assert_eq!(
        triggers,
        vec![(
            "RecordStatus".to_string(),
            "PayRunService::recordStatus".to_string(),
            PROTECTED.to_string()
        )]
    );
}

#[tokio::test]
async fn a_bound_protected_child_op_is_reported_exactly_once() {
    // The IFML action planner resolves an action against
    // `routes_for_entity`, which carries child-op routes too — so an
    // action naming a protected child op is the same dead button.
    let cfg = config();
    let db = engine(
        &model(Vec::new(), vec![child_op("addLine", true)]),
        &["addLine"],
    )
    .await;
    assert!(
        dead_surface_warnings(&db, &cfg).await.unwrap().is_empty(),
        "the trigger family owns the bound-protected report — no second line"
    );
    let triggers = ifml_dead_triggers(&db, &cfg).await.unwrap();
    assert_eq!(
        triggers,
        vec![(
            "addLine".to_string(),
            "PayRunType::addLine".to_string(),
            PROTECTED_CHILD.to_string()
        )]
    );
}

#[tokio::test]
async fn a_service_op_outranks_a_same_named_protected_child_op() {
    // The action binds the SERVICE op (a mounted route) — the same-named
    // protected child op is not what the button targets.
    let mut design_model = model(Vec::new(), vec![child_op("addLine", true)]);
    design_model.services[0].operations = vec![DddServiceOperation {
        delegation_target: Some("PayRunLineRepository".to_string()),
        delegation_operation: Some("save".to_string()),
        ..service_op("addLine", false)
    }];
    design_model.repositories = vec![DddRepositoryNode {
        application: "App".to_string(),
        name: "PayRunLineRepository".to_string(),
        design_class: "PayRunLineType".to_string(),
        operations: Vec::new(),
    }];
    let db = MockEngine::builder()
        .with_schema(schema_node("PayRunLineType"))
        .build();
    db.ingest_ddd_model(&design_model).await.unwrap();
    db.ingest_action_node(&ActionNode {
        name: "addLine".to_string(),
        domain: None,
    })
    .await
    .unwrap();
    let cfg = config();
    assert!(
        ifml_dead_triggers(&db, &cfg).await.unwrap().is_empty(),
        "a public, mounted op is no dead trigger"
    );
    assert!(
        dead_surface_warnings(&db, &cfg).await.unwrap().is_empty(),
        "nothing is unbound and nothing protected is bound"
    );
}

// ── (c) entity-unbound ops still warn ─────────────────────────────────

#[tokio::test]
async fn entity_unbound_ops_still_warn() {
    // The OTHER class inside `dead_surface_warnings`: a public op the run
    // cannot route. Untouched by #619 — it warns whether or not an action
    // binds it, exactly as before.
    let cfg = config();
    let db = engine(&model(vec![service_op("ping", false)], Vec::new()), &[]).await;
    let warnings = dead_surface_warnings(&db, &cfg).await.unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("PayRunService::ping") && warnings[0].contains("binds no entity"),
        "the entity-unbound warning keeps its wording: {warnings:?}"
    );
    assert!(
        ifml_dead_triggers(&db, &cfg).await.unwrap().is_empty(),
        "no action ⇒ no trigger"
    );

    let db = engine(
        &model(vec![service_op("ping", false)], Vec::new()),
        &["ping"],
    )
    .await;
    let warnings = dead_surface_warnings(&db, &cfg).await.unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("binds no entity"), "{warnings:?}");
    let triggers = ifml_dead_triggers(&db, &cfg).await.unwrap();
    assert_eq!(
        triggers,
        vec![(
            "ping".to_string(),
            "ping".to_string(),
            "the op binds no entity (no capability class, repository delegation, or class-typed signature)".to_string(),
        )],
        "the unbound trigger keeps its (action, bare op, reason) shape"
    );
}

fn schema_node(title: &str) -> codegraph_core::types::SchemaNode {
    codegraph_core::types::SchemaNode {
        namespace: None,
        schema_id: format!("payroll/json/{title}.json"),
        title: title.to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("payroll".to_string()),
        rel_path: format!("payroll/json/{title}.json"),
        pg_type: "UUID".to_string(),
        rust_type: title.to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: title.to_string(),
        pg_table_name: to_snake_case(title),
        api_path_segment: to_snake_case(title),
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
