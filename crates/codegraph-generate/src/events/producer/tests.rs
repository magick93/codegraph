//! KATs for the events-producer sentences and decision table (issue #820
//! slice a). `docs/events-producer.md` is the source: each refusal/warning
//! assertion quotes the doc sentence exactly (substituting the fixture
//! values).

use std::collections::{BTreeMap, BTreeSet};

use crate::ddd::service_ops::{ServiceOpState, StateName, WireKey};
use crate::events::model::{EventDefinition, EventFieldDef};

use super::messages;
use super::planner::{
    self, EmissionArm, EmissionSite, ProducerArm, ProducerBinding, ProducerEntitySurface,
    ProducerInputs, SurfaceChild, SurfaceField,
};

// ── Fixtures ──────────────────────────────────────────────────────────────

fn field(name: &str, rust_type: &str) -> SurfaceField {
    SurfaceField {
        name: name.to_string(),
        rust_type: rust_type.to_string(),
        is_optional: false,
        is_array: false,
        is_entity_ref: false,
        is_codelist: false,
    }
}

fn optional(mut field: SurfaceField) -> SurfaceField {
    field.is_optional = true;
    field
}

fn codelist(name: &str, rust_type: &str) -> SurfaceField {
    let mut field = field(name, rust_type);
    field.is_codelist = true;
    field
}

fn entity_ref(name: &str) -> SurfaceField {
    let mut field = field(name, "uuid::Uuid");
    field.is_entity_ref = true;
    field
}

fn surface() -> ProducerEntitySurface {
    ProducerEntitySurface {
        domain: "orders".to_string(),
        title: "WorkerType".to_string(),
        rust_type_name: "WorkerType".to_string(),
        fields: vec![
            field("id", "uuid::Uuid"),
            field("worker_type", "String"),
            field("worker_status", "String"),
            codelist("status", "OrderStatus"),
            field("total", "i64"),
            field("count", "i32"),
            entity_ref("customer_ref"),
            entity_ref("manager"),
            field("name", "String"),
            field("organization_id", "String"),
            optional(field("note", "String")),
        ],
        children: vec![SurfaceChild {
            field_name: "lines".to_string(),
            fields: vec![field("quantity", "i32"), field("sku", "String")],
        }],
        alt_identities: vec!["employee_number".to_string()],
        state_field: Some("status".to_string()),
        state_type: Some("OrderStatus".to_string()),
    }
}

fn payload(name: &str, rust_type: &str) -> EventFieldDef {
    // The mapper marks primitive pherolang types `is_primitive`; a
    // class/enum name is not (the record-vs-id arm keys on this).
    let is_primitive = !(rust_type.chars().next().is_some_and(char::is_uppercase)
        && !matches!(rust_type, "String" | "Uuid"));
    EventFieldDef {
        name: name.to_string(),
        rust_type: rust_type.to_string(),
        resolved_title: None,
        is_primitive,
    }
}

fn event(name: &str, fields: &[(&str, &str)]) -> EventDefinition {
    EventDefinition {
        name: name.to_string(),
        version: Some("1.0.0".to_string()),
        fields: fields.iter().map(|(name, ty)| payload(name, ty)).collect(),
        domain: Some("orders".to_string()),
    }
}

fn state() -> ServiceOpState {
    ServiceOpState {
        current_expr: "response.status".to_string(),
        wire_key: WireKey::new("status"),
        initial: StateName::new("Draft"),
        machine_id: "worker_type_status".to_string(),
        all: Vec::new(),
        op: Vec::new(),
    }
}

fn site(emits: &[&str], arm: EmissionArm) -> EmissionSite {
    EmissionSite {
        service: "WorkerService".to_string(),
        op: "place".to_string(),
        module_domain: "orders".to_string(),
        emits: emits.iter().map(|e| (*e).to_string()).collect(),
        is_protected: false,
        child_bound: false,
        entity: Some("WorkerType".to_string()),
        arm,
        own_writes: BTreeSet::from(["status".to_string()]),
        writable_fields: surface()
            .fields
            .iter()
            .filter(|field| field.name != "id")
            .map(|field| field.name.clone())
            .collect(),
    }
}

fn inputs(events: Vec<EventDefinition>, sites: Vec<EmissionSite>) -> ProducerInputs {
    let mut entities = BTreeMap::new();
    entities.insert(("orders".to_string(), "WorkerType".to_string()), surface());
    ProducerInputs {
        events,
        sites,
        entities,
        type_suffix: "Type".to_string(),
    }
}

fn run(event: EventDefinition, site: EmissionSite) -> planner::ProducerReport {
    planner::plan(&inputs(vec![event], vec![site]))
}

fn refusal(report: &planner::ProducerReport) -> &str {
    assert_eq!(
        report.refusals.len(),
        1,
        "expected exactly one refusal, got: {:?}",
        report.refusals
    );
    &report.refusals[0].body
}

#[test]
fn r1_code_stripped_nearest_candidate() {
    let report = run(
        event("OrderPlaced", &[("workerTypeCode", "String")]),
        site(&["OrderPlaced"], EmissionArm::Patch),
    );
    assert_eq!(
        refusal(&report),
        "event `OrderPlaced` (operation `WorkerService.place`) payload field `workerTypeCode` \
         has no matching field on `WorkerTypeResponse`; nearest: `worker_type` — rename the \
         payload field in the .evt contract or the class feature so the spellings agree"
    );
}

#[test]
fn r1_edit_distance_nearest_candidate() {
    let report = run(
        event("OrderPlaced", &[("totl", "i64")]),
        site(&["OrderPlaced"], EmissionArm::Patch),
    );
    assert!(
        refusal(&report).contains("nearest: `total`"),
        "expected the edit-distance candidate, got: {}",
        refusal(&report)
    );
}

#[test]
fn r1_nearest_is_prefix_normalised() {
    // `workerNmae` → `worker_nmae` → strip the entity prefix → `nmae`,
    // one edit from `name`.
    let report = run(
        event("OrderPlaced", &[("workerNmae", "String")]),
        site(&["OrderPlaced"], EmissionArm::Patch),
    );
    assert!(
        refusal(&report).contains("nearest: `name`"),
        "expected the prefix-normalised candidate, got: {}",
        refusal(&report)
    );
}

#[test]
fn r1_none_within_two_clause() {
    let report = run(
        event("OrderPlaced", &[("xyzzy", "String")]),
        site(&["OrderPlaced"], EmissionArm::Patch),
    );
    assert_eq!(
        refusal(&report),
        "event `OrderPlaced` (operation `WorkerService.place`) payload field `xyzzy` has no \
         matching field on `WorkerTypeResponse`; nearest: (none within edit distance 2) — rename \
         the payload field in the .evt contract or the class feature so the spellings agree"
    );
}

#[test]
fn snake_casing_binds_without_a_refusal() {
    // camelCase payload against the snake_case DTO field: a match, not R1.
    let report = run(
        event("OrderPlaced", &[("workerStatus", "String")]),
        site(&["OrderPlaced"], EmissionArm::Patch),
    );
    assert!(
        report.refusals.is_empty(),
        "unexpected refusal: {:?}",
        report.refusals
    );
    assert_eq!(report.plans.len(), 1);
    assert!(matches!(
        &report.plans[0].bindings[0],
        ProducerBinding::Record { response_field, .. } if response_field == "worker_status"
    ));
}

#[test]
fn r2_nullable_source() {
    let report = run(
        event("OrderPlaced", &[("note", "String")]),
        site(&["OrderPlaced"], EmissionArm::Patch),
    );
    assert_eq!(
        refusal(&report),
        "event `OrderPlaced` (operation `WorkerService.place`) payload field `note` is required \
         but `WorkerTypeResponse.note` is nullable — declare the payload field optional or make \
         the source feature required (v1 refuses to invent a null policy)"
    );
}

#[test]
fn r3_contained_child_collection() {
    let report = run(
        event("OrderPlaced", &[("quantity", "i32")]),
        site(&["OrderPlaced"], EmissionArm::Patch),
    );
    assert_eq!(
        refusal(&report),
        "event `OrderPlaced` (operation `WorkerService.place`) payload field `quantity` is \
         sourced only from the contained collection `WorkerType.lines` — v1 publishes \
         record-compatible payloads only; lift the value onto the record or emit a child-scoped \
         event"
    );
}

#[test]
fn r4_protected() {
    let mut site = site(&["OrderPlaced"], EmissionArm::Unknown);
    site.is_protected = true;
    let report = run(event("OrderPlaced", &[("total", "i64")]), site);
    assert_eq!(
        refusal(&report),
        "event `OrderPlaced` is bound to operation `WorkerService.place` which has no runtime \
         site (protected) — the event cannot fire; bind it to an executing operation"
    );
}

#[test]
fn r4_child_bound() {
    let mut site = site(&["OrderPlaced"], EmissionArm::Unknown);
    site.child_bound = true;
    let report = run(event("OrderPlaced", &[("total", "i64")]), site);
    assert!(refusal(&report).contains("(child-bound)"));
}

#[test]
fn r4_abstract() {
    let report = run(
        event("OrderPlaced", &[("total", "i64")]),
        site(&["OrderPlaced"], EmissionArm::Seam),
    );
    assert!(refusal(&report).contains("(abstract)"));
}

#[test]
fn r4_create_shaped_delete_and_read_only() {
    for (reason, sentence) in [
        ("create-shaped", "create-shaped"),
        ("delete", "delete"),
        ("read-only", "read-only"),
    ] {
        let report = run(
            event("OrderPlaced", &[("total", "i64")]),
            site(&["OrderPlaced"], EmissionArm::NonPublishing(reason)),
        );
        assert!(
            refusal(&report).contains(&format!("({sentence})")),
            "expected ({sentence}), got: {}",
            refusal(&report)
        );
    }
}

#[test]
fn r5_duplicate_emission() {
    let report = run(
        event("OrderPlaced", &[("total", "i64")]),
        site(&["OrderPlaced", "OrderPlaced"], EmissionArm::Patch),
    );
    assert_eq!(
        refusal(&report),
        "event `OrderPlaced` is bound to operation `WorkerService.place` more than once and the \
         conditions can both hold — split the operations or make the conditions exclusive"
    );
}

#[test]
fn r6_change_fact_the_patch_cannot_write() {
    let mut site = site(&["WorkerStatusChanged"], EmissionArm::Patch);
    site.writable_fields = BTreeSet::from(["total".to_string()]);
    let report = run(
        event("WorkerStatusChanged", &[("status", "OrderStatus")]),
        site,
    );
    assert_eq!(
        refusal(&report),
        "event `WorkerStatusChanged` is a change fact bound to plain patch operation \
         `WorkerService.place` which does not write `status` — bind the emission to the \
         operation that owns the fact"
    );
}

#[test]
fn r7_type_disagreement() {
    let report = run(
        event("OrderPlaced", &[("total", "String")]),
        site(&["OrderPlaced"], EmissionArm::Patch),
    );
    assert_eq!(
        refusal(&report),
        "event `OrderPlaced` (operation `WorkerService.place`) payload field `total` is `String` \
         but `WorkerTypeResponse.total` is `i64` — v1 does not coerce the value types; align the \
         model"
    );
}

#[test]
fn r7_record_vs_id_shape() {
    let report = run(
        event("OrderPlaced", &[("manager", "PersonType")]),
        site(&["OrderPlaced"], EmissionArm::Patch),
    );
    assert!(
        refusal(&report).contains("v1 does not coerce the record-vs-id shape; align the model"),
        "expected the record-vs-id phrase, got: {}",
        refusal(&report)
    );
}

#[test]
fn r8_cross_contract_domain() {
    let mut event = event("OrderPlaced", &[("total", "i64")]);
    event.domain = Some("billing".to_string());
    let report = run(event, site(&["OrderPlaced"], EmissionArm::Patch));
    assert_eq!(
        refusal(&report),
        "event `OrderPlaced` is declared by the contract of domain `billing` but bound to \
         operation `WorkerService.place` of domain `orders` — cross-domain emission is not \
         lowered in v1"
    );
}

#[test]
fn r9_no_entity_surface() {
    let mut site = site(&["OrderPlaced"], EmissionArm::Patch);
    site.entity = None;
    let report = run(event("OrderPlaced", &[("total", "i64")]), site);
    assert_eq!(
        refusal(&report),
        "event `OrderPlaced` (operation `WorkerService.place`) payload field `total` has no model \
         source — v1 publishes record-compatible payloads only; add the feature (or a context \
         binding) to the model"
    );
}

#[test]
fn r9_unavailable_context_fact() {
    // previousState on a plain patch (no machine move).
    let report = run(
        event("OrderPlaced", &[("previousState", "OrderStatus")]),
        site(&["OrderPlaced"], EmissionArm::Patch),
    );
    assert!(refusal(&report).contains("has no model source"));
}

#[test]
fn w1_surrogate_identity_binding() {
    let report = run(
        event("OrderPlaced", &[("WorkerId", "uuid::Uuid")]),
        site(&["OrderPlaced"], EmissionArm::Patch),
    );
    assert!(
        report.refusals.is_empty(),
        "unexpected refusal: {:?}",
        report.refusals
    );
    assert_eq!(
        report.warnings[0].body,
        "`WorkerId` binds to the surrogate `id` (the record also carries `employee_number`) — \
         confirm the payload means the surrogate"
    );
    assert!(matches!(
        &report.plans[0].bindings[0],
        ProducerBinding::Record { response_field, .. } if response_field == "id"
    ));
}

#[test]
fn w2_field_not_written_by_the_op() {
    let report = run(
        event("OrderPlaced", &[("name", "String")]),
        site(&["OrderPlaced"], EmissionArm::Transition(state())),
    );
    assert!(
        report.refusals.is_empty(),
        "unexpected refusal: {:?}",
        report.refusals
    );
    assert_eq!(
        report.warnings[0].body,
        "payload field `name` binds `WorkerTypeResponse.name`, which operation \
         `WorkerService.place` does not write (the value comes from creation, not the \
         transition)"
    );
}

#[test]
fn w2_is_silent_for_the_machine_state() {
    let report = run(
        event("OrderPlaced", &[("status", "OrderStatus")]),
        site(&["OrderPlaced"], EmissionArm::Transition(state())),
    );
    assert!(
        report.refusals.is_empty(),
        "unexpected refusal: {:?}",
        report.refusals
    );
    assert!(
        report.warnings.is_empty(),
        "unexpected warning: {:?}",
        report.warnings
    );
}

#[test]
fn w3_record_field_shadows_a_context_binding() {
    let report = run(
        event("OrderPlaced", &[("organizationId", "String")]),
        site(&["OrderPlaced"], EmissionArm::Patch),
    );
    assert!(
        report.refusals.is_empty(),
        "unexpected refusal: {:?}",
        report.refusals
    );
    assert_eq!(
        report.warnings[0].body,
        "event `OrderPlaced` (operation `WorkerService.place`) payload field `organizationId` \
         matches both `WorkerTypeResponse.organization_id` and the context binding \
         `organizationId` — bound to the record field; rename one side to disambiguate"
    );
    assert!(matches!(
        &report.plans[0].bindings[0],
        ProducerBinding::Record { response_field, .. } if response_field == "organization_id"
    ));
}

#[test]
fn context_facts_bind() {
    // transitionState facts on a transition arm, plus the clock/ids.
    let site = {
        let mut site = site(&["OrderPlaced"], EmissionArm::Transition(state()));
        site.own_writes.insert("status".to_string());
        site
    };
    let report = run(
        event(
            "OrderPlaced",
            &[
                ("occurredOn", "chrono::DateTime<chrono::Utc>"),
                ("previousState", "OrderStatus"),
                ("nextState", "OrderStatus"),
                ("correlationId", "uuid::Uuid"),
            ],
        ),
        site,
    );
    assert!(
        report.refusals.is_empty(),
        "unexpected refusal: {:?}",
        report.refusals
    );
    assert_eq!(report.plans.len(), 1);
    assert_eq!(report.plans[0].bindings.len(), 4);
    assert!(matches!(
        &report.plans[0].bindings[1],
        ProducerBinding::Context {
            fact: super::planner::ContextFact::PreviousState,
            ..
        }
    ));
}

#[test]
fn r10_context_type_disagreement() {
    let report = run(
        event("OrderPlaced", &[("correlationId", "i64")]),
        site(&["OrderPlaced"], EmissionArm::Patch),
    );
    assert_eq!(
        refusal(&report),
        "event `OrderPlaced` (operation `WorkerService.place`) payload field `correlationId` is \
         `i64` but the context binding `correlationId` supplies `uuid::Uuid` — v1 does not coerce \
         the value types; align the model"
    );
}

#[test]
fn supported_conversions_are_not_refusals() {
    // Uuid → String, i32 → i64, codelist enum → String, identical enum.
    for (field_name, payload_type) in [
        ("customer_ref", "String"),
        ("count", "i64"),
        ("status", "String"),
        ("status", "OrderStatus"),
    ] {
        let report = run(
            event("OrderPlaced", &[(field_name, payload_type)]),
            site(&["OrderPlaced"], EmissionArm::Patch),
        );
        assert!(
            report.refusals.is_empty(),
            "unexpected refusal for {field_name}:{payload_type}: {:?}",
            report.refusals
        );
    }
}

#[test]
fn a_clean_transition_plan_carries_the_arm_and_bindings() {
    let report = run(
        event(
            "OrderPlaced",
            &[("total", "i64"), ("status", "OrderStatus")],
        ),
        site(&["OrderPlaced"], EmissionArm::Transition(state())),
    );
    assert!(
        report.refusals.is_empty(),
        "unexpected refusal: {:?}",
        report.refusals
    );
    let plan = &report.plans[0];
    assert!(matches!(plan.arm, ProducerArm::Transition(_)));
    assert_eq!(plan.event.name, "OrderPlaced");
    assert_eq!(plan.entity, "WorkerType");
    assert_eq!(plan.domain, "orders");
    assert_eq!(plan.bindings.len(), 2);
}

#[test]
fn first_blocker_wins_and_suppresses_later_fields() {
    // `total` binds (a warning), `xyzzy` refuses; the second field's failure
    // is the ONE refusal and the pair yields no plan.
    let report = run(
        event("OrderPlaced", &[("name", "String"), ("xyzzy", "String")]),
        site(&["OrderPlaced"], EmissionArm::Transition(state())),
    );
    assert_eq!(report.refusals.len(), 1);
    assert!(report.refusals[0].body.contains("`xyzzy`"));
    assert!(report.plans.is_empty());
    assert_eq!(report.warnings.len(), 1);
}

#[test]
fn undeclared_events_are_left_to_union_integrity() {
    let report = planner::plan(&inputs(
        vec![],
        vec![site(&["NotDeclared"], EmissionArm::Patch)],
    ));
    assert!(report.is_empty(), "expected silence, got: {report:?}");
}

#[test]
fn doctor_and_validation_surfaces_share_the_body() {
    let report = run(
        event("OrderPlaced", &[("xyzzy", "String")]),
        site(&["OrderPlaced"], EmissionArm::Patch),
    );
    let diagnostic = &report.refusals[0];
    assert!(
        diagnostic
            .sentence()
            .starts_with("events producer: event `OrderPlaced`")
    );
    assert!(
        diagnostic
            .doctor_line()
            .starts_with("WARN events producer — event `OrderPlaced`")
    );
    assert_eq!(
        diagnostic.sentence(),
        format!("events producer: {}", diagnostic.body)
    );
    assert_eq!(
        diagnostic.doctor_line(),
        messages::doctor_line(&diagnostic.body)
    );
}
