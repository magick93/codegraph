//! The events-producer refusal/warning sentences (issue #820 slice a).
//!
//! `docs/events-producer.md` is the NORMATIVE source: every function here
//! returns the sentence BODY (the doc's `<artifact> <reason> — <remediation>`
//! part), and the two surfaces add their own prefix:
//!
//! - pre-generation validation / run path: `events producer: {body}` (the
//!   doc's exact line, produced by [`sentence`]);
//! - doctor diagnostic mode: `WARN events producer — {body}` ([`doctor_line`]).
//!
//! One formatter, one test: the KATs in `tests.rs` pin the doc's exact
//! strings. Field spellings are interpolated verbatim (as authored), so a
//! rename in the model is a literal string search.

/// The run-path / validation line: the doc's full `events producer: …`.
#[must_use]
pub fn sentence(body: &str) -> String {
    format!("events producer: {body}")
}

/// The doctor diagnostic line (same body, house `WARN <area> — <detail>`
/// shape).
#[must_use]
pub fn doctor_line(body: &str) -> String {
    format!("WARN events producer — {body}")
}

/// The `{shape|nullability}` phrase R7/R10 append after "does not coerce".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disagreement {
    /// A plain scalar/value type mismatch.
    Type,
    /// A list-shaped payload against a scalar source (or vice versa).
    ListScalar,
    /// A record-shaped payload against an id column.
    RecordId,
    /// The `Option` split.
    Nullability,
}

impl Disagreement {
    /// The literal phrase (pinned by the KATs).
    #[must_use]
    pub fn phrase(self) -> &'static str {
        match self {
            Self::Type => "the value types",
            Self::ListScalar => "the list-vs-scalar shape",
            Self::RecordId => "the record-vs-id shape",
            Self::Nullability => "nullability",
        }
    }
}

/// R1 — unmatchable payload field (the near-miss class).
#[must_use]
pub fn r1(
    event: &str,
    service: &str,
    op: &str,
    field: &str,
    entity: &str,
    candidate: Option<&str>,
) -> String {
    let nearest = match candidate {
        Some(candidate) => format!("`{candidate}`"),
        None => "(none within edit distance 2)".to_string(),
    };
    format!(
        "event `{event}` (operation `{service}.{op}`) payload field `{field}` has no matching field \
         on `{entity}Response`; nearest: {nearest} — rename the payload field in the .evt contract \
         or the class feature so the spellings agree"
    )
}

/// R2 — nullable source feeding a required payload field.
#[must_use]
pub fn r2(
    event: &str,
    service: &str,
    op: &str,
    field: &str,
    entity: &str,
    candidate: &str,
) -> String {
    format!(
        "event `{event}` (operation `{service}.{op}`) payload field `{field}` is required but \
         `{entity}Response.{candidate}` is nullable — declare the payload field optional or make \
         the source feature required (v1 refuses to invent a null policy)"
    )
}

/// R3 — source lives in a contained child collection.
#[must_use]
pub fn r3(event: &str, service: &str, op: &str, field: &str, entity: &str, child: &str) -> String {
    format!(
        "event `{event}` (operation `{service}.{op}`) payload field `{field}` is sourced only from \
         the contained collection `{entity}.{child}` — v1 publishes record-compatible payloads \
         only; lift the value onto the record or emit a child-scoped event"
    )
}

/// R4 — no runtime site.
#[must_use]
pub fn r4(event: &str, service: &str, op: &str, reason: &str) -> String {
    format!(
        "event `{event}` is bound to operation `{service}.{op}` which has no runtime site \
         ({reason}) — the event cannot fire; bind it to an executing operation"
    )
}

/// R5 — duplicate emission.
#[must_use]
pub fn r5(event: &str, service: &str, op: &str) -> String {
    format!(
        "event `{event}` is bound to operation `{service}.{op}` more than once and the conditions \
         can both hold — split the operations or make the conditions exclusive"
    )
}

/// R6 — a `…Changed` fact on a plain patch that does not touch its anchor.
#[must_use]
pub fn r6(event: &str, service: &str, op: &str, anchor: &str) -> String {
    format!(
        "event `{event}` is a change fact bound to plain patch operation `{service}.{op}` which \
         does not write `{anchor}` — bind the emission to the operation that owns the fact"
    )
}

/// R7 — type disagreement (record source).
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn r7(
    event: &str,
    service: &str,
    op: &str,
    field: &str,
    payload_type: &str,
    entity: &str,
    candidate: &str,
    dto_type: &str,
    disagreement: Disagreement,
) -> String {
    format!(
        "event `{event}` (operation `{service}.{op}`) payload field `{field}` is `{payload_type}` \
         but `{entity}Response.{candidate}` is `{dto_type}` — v1 does not coerce {}; align the model",
        disagreement.phrase()
    )
}

/// R8 — cross-contract event.
#[must_use]
pub fn r8(event: &str, contract_domain: &str, service: &str, op: &str, domain: &str) -> String {
    format!(
        "event `{event}` is declared by the contract of domain `{contract_domain}` but bound to \
         operation `{service}.{op}` of domain `{domain}` — cross-domain emission is not lowered in v1"
    )
}

/// R9 — no model source at all.
#[must_use]
pub fn r9(event: &str, service: &str, op: &str, field: &str) -> String {
    format!(
        "event `{event}` (operation `{service}.{op}`) payload field `{field}` has no model source \
         — v1 publishes record-compatible payloads only; add the feature (or a context binding) to \
         the model"
    )
}

/// R10 — context type disagreement.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn r10(
    event: &str,
    service: &str,
    op: &str,
    field: &str,
    payload_type: &str,
    context: &str,
    context_type: &str,
    disagreement: Disagreement,
) -> String {
    format!(
        "event `{event}` (operation `{service}.{op}`) payload field `{field}` is `{payload_type}` \
         but the context binding `{context}` supplies `{context_type}` — v1 does not coerce {}; \
         align the model",
        disagreement.phrase()
    )
}

/// R11 — no post-write re-read (the entity exposes no `read`).
#[must_use]
pub fn r11(event: &str, service: &str, op: &str, entity: &str) -> String {
    format!(
        "event `{event}` (operation `{service}.{op}`) publishes from `{entity}`, which exposes no `read` operation — v1 builds the payload from an in-transaction re-read; add `read` to the class design or bind the emission to a record with one"
    )
}

/// R12 — no persisted publishing channel.
#[must_use]
pub fn r12(event: &str) -> String {
    format!(
        "event `{event}` is published on no persisted channel — the emission cannot fire; declare a channel that publishes it on a queue-backed target"
    )
}

/// W1 — surrogate identity binding.
#[must_use]
pub fn w1(entity: &str, alt: &str) -> String {
    format!(
        "`{entity}Id` binds to the surrogate `id` (the record also carries `{alt}`) — confirm the \
         payload means the surrogate"
    )
}

/// W2 — field not written by the emitting op.
#[must_use]
pub fn w2(field: &str, entity: &str, candidate: &str, service: &str, op: &str) -> String {
    format!(
        "payload field `{field}` binds `{entity}Response.{candidate}`, which operation \
         `{service}.{op}` does not write (the value comes from creation, not the transition)"
    )
}

/// W3 — record field shadows a context binding.
#[must_use]
pub fn w3(
    event: &str,
    service: &str,
    op: &str,
    field: &str,
    entity: &str,
    candidate: &str,
    context: &str,
) -> String {
    format!(
        "event `{event}` (operation `{service}.{op}`) payload field `{field}` matches both \
         `{entity}Response.{candidate}` and the context binding `{context}` — bound to the record \
         field; rename one side to disambiguate"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surfaces_share_one_body_and_only_the_prefix_differs() {
        let body = r5("OrderPlaced", "OrderService", "place");
        assert_eq!(
            sentence(&body),
            "events producer: event `OrderPlaced` is bound to operation `OrderService.place` more \
             than once and the conditions can both hold — split the operations or make the \
             conditions exclusive"
        );
        assert_eq!(
            doctor_line(&body),
            "WARN events producer — event `OrderPlaced` is bound to operation `OrderService.place` \
             more than once and the conditions can both hold — split the operations or make the \
             conditions exclusive"
        );
    }

    #[test]
    fn disagreement_phrases_cover_the_documented_arms() {
        assert_eq!(
            Disagreement::ListScalar.phrase(),
            "the list-vs-scalar shape"
        );
        assert_eq!(Disagreement::RecordId.phrase(), "the record-vs-id shape");
        assert_eq!(Disagreement::Nullability.phrase(), "nullability");
        assert_eq!(Disagreement::Type.phrase(), "the value types");
    }
}
