//! The service-op action-leg planner (issues #590, #661–#663, #692, #767,
//! #768): the state-aware machine simulation, the aggregate-child legs,
//! and the declared-NotFound failure legs. `OpLeg` is the render model the
//! `actions.tera` template consumes.
use serde::Serialize;

use super::{DECLARED_NOT_FOUND_NOTE, derive_fold};
/// One service-op action leg in the actions contract (issue #590).
#[derive(Serialize, Default)]
pub(super) struct OpLeg {
    pub(super) op_name: String,
    pub(super) service: String,
    pub(super) snake: String,
    /// Expected status: 200 (wired delegation / lowered body), 409 (wrong
    /// state for a machine transition), 422 (malformed child payload), 404
    /// (unknown child under the parent), or 501 (seam).
    pub(super) expect: u16,
    pub(super) seam: bool,
    pub(super) seam_reason: String,
    /// A replay leg proving the wrong-state 409 contract.
    pub(super) wrong_state: bool,
    /// The JSON action body a bodied op's declared parameters need (issue
    /// #662); empty for every other lowering.
    pub(super) body: String,
    /// `true` for a COLLECTION-level child action (`add`, issue #661): the
    /// leg posts to `{item_url_base}/actions/{snake}` — no child id segment,
    /// because the element does not exist yet.
    collection: bool,
    /// Non-empty ⇒ the HTTP method (`GET` for a child `add` read-back leg);
    /// empty ⇒ `POST`.
    method: String,
    /// Non-empty ⇒ a URL SUFFIX appended to `item_url_base` (the child
    /// `add` read-back and the child failure legs); empty ⇒ the action
    /// route.
    url: String,
    /// Non-empty ⇒ the COMPLETE request URL, used verbatim (issue #692's
    /// bogus-parent leg: the collection route with the parent id segment
    /// replaced by the zero uuid — a suffix on `item_url_base` cannot
    /// express that). Empty ⇒ the `url`/`collection` selection.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(super) full_url: String,
    /// Non-empty ⇒ a `[Captures]` entry (`name: jsonpath "$.data.id"`) this
    /// leg records — the child `add` leg captures the created element.
    capture: String,
    /// Extra `[Asserts]` lines (the read-back leg).
    asserts: Vec<String>,
    /// A human note rendered after the leg's header (`— <note>`).
    note: String,
}

/// The action-body JSON one leg sends: one minimal value per declared
/// parameter (`{}` when the op declares none). Empty string ⇒ no body line.
pub(super) fn op_body_json(route: &crate::ddd::service_ops::ServiceOpRoute) -> String {
    use crate::ddd::service_ops::ExecutionKind;
    // Issue #767: a create-shaped op's body IS the modeled boundary object.
    if let ExecutionKind::Create { plan } = &route.execution {
        return plan.sample.to_string();
    }
    let ExecutionKind::Body { plan } = &route.execution else {
        return String::new();
    };
    if plan.params.is_empty() {
        return String::new();
    }
    // Issue #667: a lone modeled DTO parameter IS the request body, so the
    // contract sends the boundary object directly (not wrapped by its name).
    if plan.params.len() == 1 && plan.params[0].direct_body {
        return if plan.params[0].optional {
            "null".to_string()
        } else {
            plan.params[0].sample.to_string()
        };
    }
    let mut object = serde_json::Map::new();
    for param in &plan.params {
        object.insert(
            param.name.clone(),
            if param.optional {
                serde_json::Value::Null
            } else {
                param.sample.clone()
            },
        );
    }
    serde_json::Value::Object(object).to_string()
}

/// Build the state-aware action legs: walk the machine from its initial
/// state in declaration order. A transition valid from the current state
/// pins 200 and advances; an out-of-order transition pins 409 (the handler
/// refuses it); seams pin 501. Finally, replay one exercised non-self
/// transition from the post-sequence state to GUARANTEE a 409 leg even when
/// declaration order happens to be valid end to end.
///
/// `child_bodies` carries the entity's own create/update payloads: an
/// aggregate child `add` posts the create body and a child `save` posts the
/// update body (issue #661) — the same wire shapes the entity contract
/// exercises, so a DTO change cannot desync the action contract.
pub(super) fn state_aware_legs(
    ops: &[crate::ddd::service_ops::ServiceOpRoute],
    child_bodies: (&str, &str),
    item_url_base: &str,
    capture_ref: &str,
) -> Vec<OpLeg> {
    use crate::ddd::service_ops::{ExecutionKind, ServiceOpTransition};

    let mut current: Option<String> = ops
        .iter()
        .find_map(|op| op.execution.transition_state())
        .map(|state| state.initial.as_str().to_string());

    let mut legs = Vec::with_capacity(ops.len() + 1);
    // Delete legs run LAST: a delete removes the captured row, so any
    // read/transition after it would 404. The contract still exercises the
    // route; only the order is moved.
    let mut delete_legs = Vec::new();
    let mut exercised: Vec<(String, String, ServiceOpTransition)> = Vec::new();
    for op in ops {
        if matches!(op.execution, ExecutionKind::Delete) {
            delete_legs.push(OpLeg {
                op_name: op.op_name.clone(),
                service: op.service.clone(),
                snake: op.snake.clone(),
                expect: 200,
                seam: false,
                seam_reason: "wired".to_string(),
                wrong_state: false,
                body: String::new(),
                ..Default::default()
            });
            // A declared `throws NotFound` vocabulary gets its
            // deterministic 404 leg (DD7, issue #665) alongside — the
            // zero uuid never resolves, so the load-or-404 contract is
            // provable without guessing.
            if op.throws.iter().any(|name| name == "NotFound") {
                delete_legs.push(OpLeg {
                    op_name: op.op_name.clone(),
                    service: op.service.clone(),
                    snake: op.snake.clone(),
                    expect: 404,
                    seam: false,
                    seam_reason: String::new(),
                    wrong_state: false,
                    body: String::new(),
                    url: format!("/00000000-0000-0000-0000-000000000000/actions/{}", op.snake),
                    note: DECLARED_NOT_FOUND_NOTE.to_string(),
                    ..Default::default()
                });
            }
            continue;
        }
        // Aggregate child operations (issue #661). `add` appends a NEW
        // element (its own capture + read-back) and pins the create DTO's
        // unknown-field 422 as the failure leg; `save` patches the captured
        // element and pins an unknown id's 404; `remove` DELETES the
        // captured element, so it is deferred to the end like every delete.
        if let ExecutionKind::ChildOp {
            action, read_back, ..
        } = &op.execution
        {
            let child_legs =
                child_action_legs(op, *action, *read_back, child_bodies, item_url_base);
            match action {
                crate::ddd::service_ops::ChildAction::Remove => delete_legs.extend(child_legs),
                _ => legs.extend(child_legs),
            }
            continue;
        }
        // An injected-scope `when` guard (issue #663): the fresh-fixture
        // verdict decides the leg, never a guess — a mis-pinned status
        // fails the suite as hard in either direction.
        // - Denied: the fresh row PROVABLY fails the guard (an optional FK
        //   is NULL — the create body omits nullable props — so the gate
        //   denies before evaluating, or a machine-decided comparison
        //   fails on the initial state) → pin 403, machine stream NOT
        //   advanced.
        // - Unverifiable: some comparison the planner cannot decide → the
        //   leg is OMITTED with the documented header (the
        //   `Body { may_refuse }` precedent).
        // - Satisfied: the guard provably holds → the normal legs below.
        if let Some(guard) = &op.injected_guard {
            match guard.fresh_fixture {
                crate::ddd::service_ops::FreshFixtureGuard::Satisfied => {}
                crate::ddd::service_ops::FreshFixtureGuard::Denied => {
                    legs.push(OpLeg {
                        op_name: op.op_name.clone(),
                        service: op.service.clone(),
                        snake: op.snake.clone(),
                        expect: 403,
                        seam: false,
                        seam_reason: String::new(),
                        wrong_state: false,
                        body: String::new(),
                        note: format!(
                            "injected-scope `when` guard ({}): the fresh fixture row \
                             provably fails the guard (an optional related-reference FK \
                             is left NULL by the create body, or a machine-decided \
                             comparison fails on the related record's initial state), \
                             so the contract pins the denial",
                            guard.condition
                        ),
                        ..Default::default()
                    });
                    continue;
                }
                crate::ddd::service_ops::FreshFixtureGuard::Unverifiable => {
                    continue;
                }
            }
        }
        // A bodied op that may answer a declared 4xx has NO deterministic
        // expected status (it depends on the fixture row the contract just
        // created) — the leg is omitted rather than mis-pinned. The live
        // gate drives those with a hand-authored probe carrying the real
        // parameter values.
        if let ExecutionKind::Body { plan } = &op.execution
            && plan.may_refuse
        {
            continue;
        }
        // A projection read (issue #768): the read model is CONSTRUCTED in
        // the handler, so the contract can prove the derived members
        // COMPUTE — each derive expression folds over the contract's own
        // create body (the same seeded values the row carries) and the leg
        // asserts the computed value on the response. An expression that
        // names a value the create body does not seed (an optional column,
        // a non-numeric leaf) omits that one assert; the 200 leg still
        // covers the route. The item read adds the deterministic zero-uuid
        // 404 (the load-or-404 contract).
        if let ExecutionKind::ProjectionRead { plan } = &op.execution {
            let seeded: serde_json::Value =
                serde_json::from_str(child_bodies.0).unwrap_or(serde_json::Value::Null);
            let asserts: Vec<String> = plan
                .derived
                .iter()
                .filter_map(|member| {
                    // The contract's create body IS the request object (the
                    // `{ "data": … }` envelope is the RESPONSE wrapper) — the
                    // seeded values sit at the top level.
                    let value = derive_fold::fold_derive(&member.expression, &seeded)?;
                    Some(match (plan.collection, &value) {
                        (true, folded) => format!(
                            "jsonpath \"$.data[?(@.id == '{capture_ref}')].{}\" == {}",
                            member.wire_key, folded
                        ),
                        (false, folded) => {
                            format!("jsonpath \"$.data.{}\" == {}", member.wire_key, folded)
                        }
                    })
                })
                .collect();
            legs.push(OpLeg {
                op_name: op.op_name.clone(),
                service: op.service.clone(),
                snake: op.snake.clone(),
                expect: 200,
                seam: false,
                seam_reason: if plan.collection {
                    "collection projection read".to_string()
                } else {
                    "projection read".to_string()
                },
                wrong_state: false,
                body: String::new(),
                collection: plan.collection,
                asserts,
                note: "the projection's derived members are asserted against the seeded row"
                    .to_string(),
                ..Default::default()
            });
            if !plan.collection {
                legs.push(OpLeg {
                    op_name: op.op_name.clone(),
                    service: op.service.clone(),
                    snake: op.snake.clone(),
                    expect: 404,
                    seam: false,
                    seam_reason: String::new(),
                    wrong_state: false,
                    body: String::new(),
                    url: format!("/00000000-0000-0000-0000-000000000000/actions/{}", op.snake),
                    ..Default::default()
                });
            }
            continue;
        }
        let leg = match &op.execution {
            ExecutionKind::Seam { reason } => OpLeg {
                op_name: op.op_name.clone(),
                service: op.service.clone(),
                snake: op.snake.clone(),
                expect: 501,
                seam: true,
                seam_reason: reason.clone(),
                wrong_state: false,
                body: String::new(),
                ..Default::default()
            },
            ExecutionKind::Update { .. } | ExecutionKind::Body { .. }
                if op
                    .execution
                    .transition_state()
                    .is_some_and(|state| !state.op.is_empty()) =>
            {
                let state = op
                    .execution
                    .transition_state()
                    .expect("guarded by the match arm");
                let body = op_body_json(op);
                let from = current
                    .clone()
                    .unwrap_or_else(|| state.initial.as_str().to_string());
                match state.op.iter().find(|t| t.from.as_str() == from) {
                    Some(transition) => {
                        exercised.push((
                            op.op_name.clone(),
                            op.service.clone(),
                            transition.clone(),
                        ));
                        current = Some(transition.to.as_str().to_string());
                        OpLeg {
                            op_name: op.op_name.clone(),
                            service: op.service.clone(),
                            snake: op.snake.clone(),
                            expect: 200,
                            seam: false,
                            seam_reason: "wired".to_string(),
                            wrong_state: false,
                            body,
                            ..Default::default()
                        }
                    }
                    None => OpLeg {
                        op_name: op.op_name.clone(),
                        service: op.service.clone(),
                        snake: op.snake.clone(),
                        expect: 409,
                        seam: false,
                        seam_reason: format!("no transition from the current state `{from}`"),
                        wrong_state: true,
                        body,
                        ..Default::default()
                    },
                }
            }
            // Issue #767: a payload carrying a REQUIRED entity reference
            // cannot be seeded by a generated contract (the target row is not
            // created here), so the leg would post the nil uuid and 500 —
            // omit it and record the omission in the contract header.
            ExecutionKind::Create { plan } if !plan.required_class_refs.is_empty() => continue,
            // Issue #767: a create-shaped op is COLLECTION-level and answers
            // 201 with the created record (the entity create's own contract).
            ExecutionKind::Create { .. } => OpLeg {
                op_name: op.op_name.clone(),
                service: op.service.clone(),
                snake: op.snake.clone(),
                expect: 201,
                seam: false,
                seam_reason: "collection-level create".to_string(),
                wrong_state: false,
                collection: true,
                body: op_body_json(op),
                ..Default::default()
            },
            _ => OpLeg {
                op_name: op.op_name.clone(),
                service: op.service.clone(),
                snake: op.snake.clone(),
                expect: 200,
                seam: false,
                seam_reason: "wired".to_string(),
                wrong_state: false,
                body: op_body_json(op),
                ..Default::default()
            },
        };
        legs.push(leg);
        // A declared `throws NotFound` vocabulary gets a deterministic 404
        // leg (DD7, issue #665): the action on the zero uuid always 404s
        // (the load-or-404 is the route's existence contract), so the
        // declared-error path is pinned without guessing. Body ops with
        // 4xx raises already emit no leg (the may_refuse rule above); a
        // seam 501s regardless; child actions carry their own failure
        // legs.
        if op.throws.iter().any(|name| name == "NotFound")
            && !matches!(
                op.execution,
                ExecutionKind::Seam { .. }
                    | ExecutionKind::Body { .. }
                    | ExecutionKind::Create { .. }
                    | ExecutionKind::ChildOp { .. }
            )
        {
            legs.push(OpLeg {
                op_name: op.op_name.clone(),
                service: op.service.clone(),
                snake: op.snake.clone(),
                expect: 404,
                seam: false,
                seam_reason: String::new(),
                wrong_state: false,
                body: String::new(),
                url: format!("/00000000-0000-0000-0000-000000000000/actions/{}", op.snake),
                note: DECLARED_NOT_FOUND_NOTE.to_string(),
                ..Default::default()
            });
        }
    }

    // Guarantee the wrong-state edge is exercised. Replaying an already
    // exercised transition from the final state is a 409 unless the
    // transition is a self-edge (which leaves the state addressable again).
    if let Some(current) = current.as_deref()
        && let Some((op_name, service, transition)) = exercised
            .iter()
            .rev()
            .find(|(_, _, t)| t.to.as_str() == current && t.from != t.to)
    {
        legs.push(OpLeg {
            op_name: op_name.clone(),
            service: service.clone(),
            snake: codegraph_naming::to_snake_case(op_name),
            expect: 409,
            seam: false,
            seam_reason: format!(
                "wrong-state replay: `{}` has no transition from `{current}`",
                transition.from
            ),
            wrong_state: true,
            body: String::new(),
            ..Default::default()
        });
    }
    legs.extend(delete_legs);
    legs
}

/// The hurl legs one aggregate child action emits (issue #661):
///
/// - `add` → POST the collection action with the entity's create body
///   (200, capturing the created element), an optional read-back GET by the
///   captured id (200 — proves the element really landed and is readable
///   under the parent), then the failure legs: the create DTO denies unknown
///   fields, so a bogus key pins 422; and (issue #692) the SAME collection
///   route with the parent id segment replaced by the zero uuid pins 404 —
///   the handler's parent load-or-404 is the route's existence contract, so
///   a bogus parent must never reach the FK (a 500, pre-fix);
/// - `save` → POST the item action with the entity's update body (200), then
///   the failure leg: an unknown child id under the parent pins 404;
/// - `remove` → POST the item action (200, the caller defers it to the end),
///   then the same unknown-id 404 failure leg.
pub(super) fn child_action_legs(
    op: &crate::ddd::service_ops::ServiceOpRoute,
    action: crate::ddd::service_ops::ChildAction,
    read_back: bool,
    child_bodies: (&str, &str),
    item_url_base: &str,
) -> Vec<OpLeg> {
    use crate::ddd::service_ops::ChildAction;

    let base = &op.service;
    // Issue #767: when the operation declares a modeled INPUT boundary whose
    // source class IS the child, the handler deserializes THAT type (its own
    // `deny_unknown_fields`), not the child's generated request DTO — so the
    // contract must send the boundary's sample, or every leg would 422 on a
    // key the boundary does not declare.
    let payload = op.execution.child_payload();
    let payload_body = payload.map(|plan| plan.sample.to_string());
    let add_body = payload_body
        .clone()
        .unwrap_or_else(|| child_bodies.0.to_string());
    let save_body = payload_body.unwrap_or_else(|| child_bodies.1.to_string());
    // A required entity reference in the boundary payload cannot be seeded by
    // a generated contract (the target row is not created here), so a payload
    // leg would post the nil uuid and 500 — the same rule the collection-level
    // create leg follows. The ownership / unknown-field failure legs stay.
    let payload_unseedable = payload.is_some_and(|plan| !plan.required_class_refs.is_empty());
    let mut legs = Vec::new();
    match action {
        ChildAction::Add => {
            let capture = format!("added_{}_id", op.snake);
            if !payload_unseedable {
                legs.push(OpLeg {
                    op_name: op.op_name.clone(),
                    service: base.clone(),
                    snake: op.snake.clone(),
                    expect: 200,
                    body: add_body.clone(),
                    collection: true,
                    capture: capture.clone(),
                    note: "aggregate child `add` — create-and-link under the parent".to_string(),
                    ..Default::default()
                });
            }
            if read_back && !payload_unseedable {
                legs.push(OpLeg {
                    op_name: format!("{} (read-back)", op.op_name),
                    service: base.clone(),
                    snake: op.snake.clone(),
                    expect: 200,
                    method: "GET".to_string(),
                    url: format!("/{{{{{capture}}}}}"),
                    asserts: vec![r#"jsonpath "$.data.id" exists"#.to_string()],
                    note: "read-back: the added element is readable under the parent".to_string(),
                    ..Default::default()
                });
            }
            legs.push(OpLeg {
                op_name: format!("{} (failure)", op.op_name),
                service: base.clone(),
                snake: op.snake.clone(),
                expect: 422,
                collection: true,
                body: "{\n  \"__unknown_field__\": true\n}".to_string(),
                note: "failure leg: the create body denies unknown fields (422)".to_string(),
                ..Default::default()
            });
            // Issue #692: the bogus-parent 404 leg — the same collection
            // route under a parent id that does not exist. The handler's
            // parent load-or-404 must answer BEFORE the create; a leg that
            // 500s (the FK failure surfacing) or 200s (a missing parent
            // check) fails the contract.
            legs.push(OpLeg {
                op_name: format!("{} (failure)", op.op_name),
                service: base.clone(),
                snake: op.snake.clone(),
                expect: 404,
                full_url: format!(
                    "{}/actions/{}",
                    zero_uuid_parent_base(item_url_base),
                    op.snake
                ),
                body: add_body.clone(),
                note: "failure leg: a bogus parent answers 404 through the parent \
                       load-or-404 — never a 500"
                    .to_string(),
                ..Default::default()
            });
        }
        ChildAction::Save => {
            if !payload_unseedable {
                legs.push(OpLeg {
                    op_name: op.op_name.clone(),
                    service: base.clone(),
                    snake: op.snake.clone(),
                    expect: 200,
                    body: save_body.clone(),
                    note: "aggregate child `save` — update the parent-scoped element".to_string(),
                    ..Default::default()
                });
            }
            legs.push(child_failure_leg(op));
        }
        ChildAction::Remove => {
            legs.push(OpLeg {
                op_name: op.op_name.clone(),
                service: base.clone(),
                snake: op.snake.clone(),
                expect: 200,
                note: "aggregate child `remove` — delete the parent-scoped element".to_string(),
                ..Default::default()
            });
            legs.push(child_failure_leg(op));
        }
    }
    legs
}

/// The nested entity URL with every parent id capture replaced by the zero
/// uuid (issue #692): the bogus-parent probe's base. `{{base_url}}` stays
/// verbatim (it is the hurl origin variable); the parent captures end in
/// `_id`. Depth-2 chains replace every ancestor id — a bogus grandparent
/// fails the parent load just as deterministically.
pub(super) fn zero_uuid_parent_base(item_url_base: &str) -> String {
    const ZERO_UUID: &str = "00000000-0000-0000-0000-000000000000";
    let mut out = String::with_capacity(item_url_base.len());
    let mut rest = item_url_base;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let Some(end_offset) = rest[start..].find("}}") else {
            break;
        };
        let name = rest[start + 2..start + end_offset].trim();
        if name == "base_url" {
            out.push_str(&rest[start..start + end_offset + 2]);
        } else {
            out.push_str(ZERO_UUID);
        }
        rest = &rest[start + end_offset + 2..];
    }
    out.push_str(rest);
    out
}

/// The child action's failure leg: an id the parent does not own (the zero
/// uuid) must 404 — the scoped read is the route's ownership contract.
pub(super) fn child_failure_leg(op: &crate::ddd::service_ops::ServiceOpRoute) -> OpLeg {
    OpLeg {
        op_name: format!("{} (failure)", op.op_name),
        service: op.service.clone(),
        snake: op.snake.clone(),
        expect: 404,
        url: format!("/00000000-0000-0000-0000-000000000000/actions/{}", op.snake),
        note: "failure leg: an element the parent does not own answers 404".to_string(),
        ..Default::default()
    }
}
