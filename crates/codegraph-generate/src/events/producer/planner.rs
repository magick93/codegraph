//! The events-producer binding planner (issue #820 slice a).
//!
//! Pure: every graph fact arrives in [`ProducerInputs`] from a
//! [`super::ProducerSource`], so the decision table is unit-testable without
//! an engine and slice (b) can reuse the resulting [`ProducerPlan`] verbatim.
//!
//! Evaluation order per `(operation, event)` — FIRST blocker wins (one
//! refusal per pair; warnings gathered for the fields bound before the
//! blocker):
//!
//! 1. R5 duplicate emission (a repeated name in one op's `emits`);
//! 2. R4 `protected` / `child-bound` (no runtime site);
//! 3. R8 cross-contract domain;
//! 4. R4 `abstract` (a 501 seam) / `create-shaped` / `delete` / `read-only`;
//! 5. R9 no `{Entity}Response` surface at all;
//! 6. R6 a `…Changed` fact the plain patch cannot write;
//! 7. per payload field, in declaration order: record match (with the
//!    `{Entity}Id` surrogate arm) → context binding → R3 contained child →
//!    R1 nearest; a matched record then R2 nullable, R7 type disagreement.
//!
//! `docs/events-producer.md` owns the sentences (see [`super::messages`]).

use std::collections::{BTreeMap, BTreeSet};

use codegraph_naming::{strip_suffix, to_snake_case};

use crate::ddd::dto::strip_code_suffix_safe;
use crate::ddd::service_ops::ServiceOpState;
use crate::events::model::EventDefinition;

use super::messages::{self, Disagreement};

/// Everything the pure planner consumes, assembled by a `ProducerSource`.
#[derive(Debug, Clone, Default)]
pub struct ProducerInputs {
    /// Declared `.evt` events (declaration order).
    pub events: Vec<EventDefinition>,
    /// One entry per emitting `.ddd` service operation (declaration order).
    pub sites: Vec<EmissionSite>,
    /// The generated `{Entity}Response` surface, keyed by `(domain, title)`.
    pub entities: BTreeMap<(String, String), ProducerEntitySurface>,
    /// The configured class-name suffix (`Type`) — used for the `{Entity}Id`
    /// arm's entity-base spelling.
    pub type_suffix: String,
}

/// One `emits`-bearing service operation.
#[derive(Debug, Clone)]
pub struct EmissionSite {
    pub service: String,
    pub op: String,
    /// The `.ddd` module name — the op's bounded context (module = domain).
    pub module_domain: String,
    /// Event names, as authored, in declaration order.
    pub emits: Vec<String>,
    pub is_protected: bool,
    pub child_bound: bool,
    /// The entity title `bind_entity` resolved, when any.
    pub entity: Option<String>,
    /// The executable lowering's arm (or the named non-publishing reason).
    pub arm: EmissionArm,
    /// The op's OWN writes (params + body assignments + the machine state) —
    /// the W2 source. The free-form client patch body is deliberately absent.
    pub own_writes: BTreeSet<String>,
    /// Fields the entity's Update DTO can carry (R6's anchor check).
    pub writable_fields: BTreeSet<String>,
}

/// The arm classification (issue #820 slice a: transition and plain patch
/// publish; every other executable arm is a named refusal).
#[derive(Debug, Clone)]
pub enum EmissionArm {
    /// A machine transition (the #818 planner facts).
    Transition(ServiceOpState),
    /// A plain patch (save/update without a transition, or a bodied save).
    Patch,
    /// An executable arm v1 does not publish from: `create-shaped`, `delete`,
    /// `read-only`.
    NonPublishing(&'static str),
    /// The 501 seam (`abstract`).
    Seam,
    /// The lowering could not be resolved (an existing validation error owns
    /// it) — the planner stays silent rather than guessing.
    Unknown,
}

/// The generated `{Entity}Response` surface (the DTO plane's own names).
#[derive(Debug, Clone, Default)]
pub struct ProducerEntitySurface {
    pub domain: String,
    pub title: String,
    /// The DTO struct stem (`WorkerType` → `WorkerTypeResponse`).
    pub rust_type_name: String,
    /// Top-level response fields, declaration order.
    pub fields: Vec<SurfaceField>,
    /// Contained child collections (their member fields are R3 sources).
    pub children: Vec<SurfaceChild>,
    /// `id`-marked feature columns (the W1 alternatives).
    pub alt_identities: Vec<String>,
    /// The machine field (snake), when the entity carries one.
    pub state_field: Option<String>,
    /// The machine field's generated DTO type.
    pub state_type: Option<String>,
}

/// One top-level response field.
#[derive(Debug, Clone)]
pub struct SurfaceField {
    pub name: String,
    pub rust_type: String,
    pub is_optional: bool,
    pub is_array: bool,
    pub is_entity_ref: bool,
    pub is_codelist: bool,
}

/// One contained child collection.
#[derive(Debug, Clone)]
pub struct SurfaceChild {
    pub field_name: String,
    pub fields: Vec<SurfaceField>,
}

/// The planner's successful output for one `(operation, event)` pair —
/// reused verbatim by the publishing slice.
#[derive(Debug, Clone)]
pub struct ProducerPlan {
    pub event: EventDefinition,
    pub service: String,
    pub op: String,
    pub domain: String,
    pub entity: String,
    pub arm: ProducerArm,
    pub bindings: Vec<ProducerBinding>,
}

/// The publishing arm a plan carries.
#[derive(Debug, Clone)]
pub enum ProducerArm {
    Transition(ServiceOpState),
    Patch,
}

/// How one payload field is supplied.
#[derive(Debug, Clone)]
pub enum ProducerBinding {
    Record {
        payload_field: String,
        response_field: String,
        rust_type: String,
        is_optional: bool,
    },
    Context {
        payload_field: String,
        fact: ContextFact,
        rust_type: String,
    },
}

/// The v1 reserved context vocabulary (`docs/events-producer.md` §Context
/// bindings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextFact {
    OccurredOn,
    PreviousState,
    NextState,
    CorrelationId,
    OrganizationId,
}

impl ContextFact {
    /// The canonical display spelling the sentences print.
    #[must_use]
    pub fn display(self) -> &'static str {
        match self {
            Self::OccurredOn => "occurredOn",
            Self::PreviousState => "previousState",
            Self::NextState => "nextState",
            Self::CorrelationId => "correlationId",
            Self::OrganizationId => "organizationId",
        }
    }

    /// The fact's Rust type (the state facts resolve from the entity).
    #[must_use]
    pub fn rust_type(self, surface: &ProducerEntitySurface) -> Option<String> {
        match self {
            Self::OccurredOn => Some("chrono::DateTime<chrono::Utc>".to_string()),
            Self::PreviousState | Self::NextState => surface.state_type.clone(),
            Self::CorrelationId | Self::OrganizationId => Some("uuid::Uuid".to_string()),
        }
    }
}

/// One refusal/warning line: the sentence body plus its binding identity.
#[derive(Debug, Clone)]
pub struct ProducerDiagnostic {
    pub event: String,
    pub service: String,
    pub op: String,
    /// The doc's `<artifact> <reason> — <remediation>` body (no prefix).
    pub body: String,
}

impl ProducerDiagnostic {
    /// The run-path / validation line (`events producer: …`).
    #[must_use]
    pub fn sentence(&self) -> String {
        messages::sentence(&self.body)
    }

    /// The doctor line (`WARN events producer — …`).
    #[must_use]
    pub fn doctor_line(&self) -> String {
        messages::doctor_line(&self.body)
    }
}

/// The planner's whole output.
#[derive(Debug, Clone, Default)]
pub struct ProducerReport {
    pub plans: Vec<ProducerPlan>,
    pub refusals: Vec<ProducerDiagnostic>,
    pub warnings: Vec<ProducerDiagnostic>,
}

impl ProducerReport {
    /// True when the report carries no plans, refusals, or warnings.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.plans.is_empty() && self.refusals.is_empty() && self.warnings.is_empty()
    }
}

/// Run the pure planner.
#[must_use]
pub fn plan(inputs: &ProducerInputs) -> ProducerReport {
    let mut report = ProducerReport::default();
    let mut warned: BTreeSet<String> = BTreeSet::new();
    let events: BTreeMap<&str, &EventDefinition> = inputs
        .events
        .iter()
        .map(|event| (event.name.as_str(), event))
        .collect();

    for site in &inputs.sites {
        // R5 — a duplicate emission on one op. Both occurrences are refused
        // (one line per duplicated name): the pair is unpublishable because
        // the model does not declare which arm owns the fact.
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for event_name in &site.emits {
            *counts.entry(event_name.as_str()).or_default() += 1;
        }
        let mut reported_duplicates: BTreeSet<&str> = BTreeSet::new();
        for event_name in &site.emits {
            if counts.get(event_name.as_str()).copied().unwrap_or(0) > 1 {
                if reported_duplicates.insert(event_name.as_str()) {
                    report.refusals.push(diag(
                        site,
                        event_name,
                        messages::r5(event_name, &site.service, &site.op),
                    ));
                }
                continue;
            }

            // An `emits` naming no declared event is the union-integrity
            // check's business (`check_integrity`), not the producer's.
            let Some(event) = events.get(event_name.as_str()).copied() else {
                continue;
            };

            // R4 — no runtime site (protected / child-bound).
            if site.is_protected {
                report.refusals.push(diag(
                    site,
                    event_name,
                    messages::r4(event_name, &site.service, &site.op, "protected"),
                ));
                continue;
            }
            if site.child_bound {
                report.refusals.push(diag(
                    site,
                    event_name,
                    messages::r4(event_name, &site.service, &site.op, "child-bound"),
                ));
                continue;
            }

            // R8 — cross-contract domain (the event's inferred contract
            // domain vs the op's module).
            if let Some(contract_domain) = event.domain.as_deref()
                && contract_domain != site.module_domain
            {
                report.refusals.push(diag(
                    site,
                    event_name,
                    messages::r8(
                        event_name,
                        contract_domain,
                        &site.service,
                        &site.op,
                        &site.module_domain,
                    ),
                ));
                continue;
            }

            // R4 — an executable arm v1 does not publish from.
            match &site.arm {
                EmissionArm::Unknown => continue,
                EmissionArm::Seam => {
                    report.refusals.push(diag(
                        site,
                        event_name,
                        messages::r4(event_name, &site.service, &site.op, "abstract"),
                    ));
                    continue;
                }
                EmissionArm::NonPublishing(reason) => {
                    report.refusals.push(diag(
                        site,
                        event_name,
                        messages::r4(event_name, &site.service, &site.op, reason),
                    ));
                    continue;
                }
                EmissionArm::Transition(_) | EmissionArm::Patch => {}
            }

            // R9 — no `{Entity}Response` surface to match against. Reported
            // once per pair, naming the first payload field.
            let Some(surface) = resolve_surface(inputs, site) else {
                if let Some(first) = event.fields.first() {
                    report.refusals.push(diag(
                        site,
                        event_name,
                        messages::r9(event_name, &site.service, &site.op, &first.name),
                    ));
                }
                continue;
            };

            // R6 — a `…Changed` fact on a plain patch that cannot write its
            // anchor (the anchor is excluded from the Update DTO).
            if matches!(site.arm, EmissionArm::Patch)
                && let Some(anchor) = change_anchor(event_name, surface, &inputs.type_suffix)
                && event
                    .fields
                    .iter()
                    .any(|field| to_snake_case(&field.name) == anchor)
                && !site.writable_fields.contains(&anchor)
            {
                report.refusals.push(diag(
                    site,
                    event_name,
                    messages::r6(event_name, &site.service, &site.op, &anchor),
                ));
                continue;
            }

            // Level 5 — the fields.
            let arm = match &site.arm {
                EmissionArm::Transition(state) => ProducerArm::Transition(state.clone()),
                EmissionArm::Patch => ProducerArm::Patch,
                // R4 handled every other arm above.
                EmissionArm::Seam | EmissionArm::NonPublishing(_) | EmissionArm::Unknown => {
                    continue;
                }
            };
            if let Some(bindings) =
                bind_fields(inputs, site, event, surface, &mut report, &mut warned)
            {
                report.plans.push(ProducerPlan {
                    event: event.clone(),
                    service: site.service.clone(),
                    op: site.op.clone(),
                    domain: site.module_domain.clone(),
                    entity: surface.title.clone(),
                    arm,
                    bindings,
                });
            }
        }
    }

    report
}

fn diag(site: &EmissionSite, event: &str, body: String) -> ProducerDiagnostic {
    ProducerDiagnostic {
        event: event.to_string(),
        service: site.service.clone(),
        op: site.op.clone(),
        body,
    }
}

fn resolve_surface<'a>(
    inputs: &'a ProducerInputs,
    site: &EmissionSite,
) -> Option<&'a ProducerEntitySurface> {
    let entity = site.entity.as_deref()?;
    inputs
        .entities
        .get(&(site.module_domain.clone(), entity.to_string()))
}

#[allow(clippy::too_many_arguments)]
fn bind_fields(
    inputs: &ProducerInputs,
    site: &EmissionSite,
    event: &EventDefinition,
    surface: &ProducerEntitySurface,
    report: &mut ProducerReport,
    warned: &mut BTreeSet<String>,
) -> Option<Vec<ProducerBinding>> {
    let mut bindings = Vec::new();
    // W2's write set: a transition can only write its own params/assignments
    // plus the machine state; a plain patch can write every Update-DTO field.
    let written: &BTreeSet<String> = match &site.arm {
        EmissionArm::Transition(_) => &site.own_writes,
        _ => &site.writable_fields,
    };
    for field in &event.fields {
        let payload = &field.name;
        let payload_snake = to_snake_case(payload);

        // Record match first (a record source always wins, with W3 when a
        // context name shadows it).
        let record = record_match(surface, payload);
        let context = context_fact(payload);
        if let Some(context) = context
            && let Some(record) = record
        {
            push_warning(
                report,
                warned,
                diag(
                    site,
                    &event.name,
                    messages::w3(
                        &event.name,
                        &site.service,
                        &site.op,
                        payload,
                        &surface.rust_type_name,
                        &record.name,
                        context.display(),
                    ),
                ),
            );
        }
        if let Some(record) = record {
            // R2 — nullable source feeding a required payload field.
            if record.is_optional {
                report.refusals.push(diag(
                    site,
                    &event.name,
                    messages::r2(
                        &event.name,
                        &site.service,
                        &site.op,
                        payload,
                        &surface.rust_type_name,
                        &record.name,
                    ),
                ));
                return None;
            }
            // R7 — type disagreement.
            if let Some(disagreement) = type_disagreement(field, record) {
                report.refusals.push(diag(
                    site,
                    &event.name,
                    messages::r7(
                        &event.name,
                        &site.service,
                        &site.op,
                        payload,
                        &field.rust_type,
                        &surface.rust_type_name,
                        &record.name,
                        &record.rust_type,
                        disagreement,
                    ),
                ));
                return None;
            }
            bindings.push(ProducerBinding::Record {
                payload_field: payload.clone(),
                response_field: record.name.clone(),
                rust_type: record.rust_type.clone(),
                is_optional: record.is_optional,
            });
            if !written.contains(&record.name) {
                push_warning(
                    report,
                    warned,
                    diag(
                        site,
                        &event.name,
                        messages::w2(
                            payload,
                            &surface.rust_type_name,
                            &record.name,
                            &site.service,
                            &site.op,
                        ),
                    ),
                );
            }
            continue;
        }

        // The `{Entity}Id` surrogate arm (W1): a payload spelled after the
        // entity's own id binds the surrogate `id`, warning when the record
        // also carries an alternate identity column.
        if let Some(alt) = entity_id_binding(surface, &payload_snake, &inputs.type_suffix) {
            bindings.push(ProducerBinding::Record {
                payload_field: payload.clone(),
                response_field: "id".to_string(),
                rust_type: "uuid::Uuid".to_string(),
                is_optional: false,
            });
            if let Some(alt) = alt {
                push_warning(
                    report,
                    warned,
                    diag(
                        site,
                        &event.name,
                        messages::w1(&entity_base(surface, &inputs.type_suffix), &alt),
                    ),
                );
            }
            continue;
        }

        // Context binding (class B). The state facts exist only on a
        // transition arm (a plain patch has no previous/next state).
        if let Some(fact) = context {
            let available = match fact {
                ContextFact::PreviousState | ContextFact::NextState => {
                    matches!(site.arm, EmissionArm::Transition(_))
                }
                ContextFact::OccurredOn
                | ContextFact::CorrelationId
                | ContextFact::OrganizationId => true,
            };
            let context_type = if available {
                fact.rust_type(surface)
            } else {
                None
            };
            let Some(context_type) = context_type else {
                // The fact is not available on this arm/machine — R9.
                report.refusals.push(diag(
                    site,
                    &event.name,
                    messages::r9(&event.name, &site.service, &site.op, payload),
                ));
                return None;
            };
            let source = SurfaceField {
                name: fact.display().to_string(),
                rust_type: context_type.clone(),
                is_optional: false,
                is_array: false,
                is_entity_ref: context_type == "uuid::Uuid",
                is_codelist: surface.state_type.as_deref() == Some(context_type.as_str()),
            };
            if let Some(disagreement) = type_disagreement(field, &source) {
                report.refusals.push(diag(
                    site,
                    &event.name,
                    messages::r10(
                        &event.name,
                        &site.service,
                        &site.op,
                        payload,
                        &field.rust_type,
                        fact.display(),
                        &context_type,
                        disagreement,
                    ),
                ));
                return None;
            }
            bindings.push(ProducerBinding::Context {
                payload_field: payload.clone(),
                fact,
                rust_type: context_type,
            });
            continue;
        }

        // R3 — the field name lives in a contained child collection.
        if let Some((child, _member)) = child_match(surface, payload) {
            report.refusals.push(diag(
                site,
                &event.name,
                messages::r3(
                    &event.name,
                    &site.service,
                    &site.op,
                    payload,
                    &surface.rust_type_name,
                    &child.field_name,
                ),
            ));
            return None;
        }

        // R1 — no match; the nearest candidate (or the none clause).
        let candidate = nearest_candidate(payload, surface, &inputs.type_suffix);
        report.refusals.push(diag(
            site,
            &event.name,
            messages::r1(
                &event.name,
                &site.service,
                &site.op,
                payload,
                &surface.rust_type_name,
                candidate.as_deref(),
            ),
        ));
        return None;
    }
    Some(bindings)
}

fn push_warning(
    report: &mut ProducerReport,
    warned: &mut BTreeSet<String>,
    diagnostic: ProducerDiagnostic,
) {
    if warned.insert(diagnostic.body.clone()) {
        report.warnings.push(diagnostic);
    }
}

/// The record match: exact snake equality only. A `_code`-stripped or
/// edit-distance-only spelling is a NEAR MISS (R1), never a silent binding —
/// the contract's whole point is that the payload field and the DTO field
/// must spell the same thing.
fn record_match<'a>(surface: &'a ProducerEntitySurface, payload: &str) -> Option<&'a SurfaceField> {
    let payload = to_snake_case(payload);
    surface.fields.iter().find(|field| field.name == payload)
}

/// The `{Entity}Id` arm. Returns `Some(alt)` when the surrogate binding must
/// WARN (the record carries an alternate identity), `Some(None)`-equivalent
/// via the outer option:
///
/// - `None` — the payload is not an `{Entity}Id` spelling, or an ordinary
///   (non-identity) `{entity}_id` column exists and the generic match already
///   handles it;
/// - `Some(None)` — bind the surrogate `id`, no alternate identity;
/// - `Some(Some(alt))` — bind the surrogate `id`, WARN naming `alt`.
fn entity_id_binding(
    surface: &ProducerEntitySurface,
    payload_snake: &str,
    type_suffix: &str,
) -> Option<Option<String>> {
    let bases = entity_bases(surface, type_suffix);
    let matched = bases
        .iter()
        .map(|base| format!("{base}_id"))
        .find(|spelling| spelling == payload_snake)?;

    // An identity-marked column with the payload's own spelling is the
    // surrogate-vs-natural-key ambiguity W1 exists for.
    if let Some(alt) = surface
        .alt_identities
        .iter()
        .find(|alt| to_snake_case(alt) == matched)
    {
        return Some(Some(alt.clone()));
    }
    // An ordinary column with that spelling (an FK, say) is a real record
    // field — let the generic match bind it.
    if surface.fields.iter().any(|field| field.name == matched) {
        return None;
    }
    // The record carries some OTHER identity column: warn naming it.
    if let Some(alt) = surface.alt_identities.first() {
        return Some(Some(alt.clone()));
    }
    Some(None)
}

/// The entity base spellings (`WorkerType` → `worker`; the full
/// `worker_type` is accepted too).
fn entity_bases(surface: &ProducerEntitySurface, type_suffix: &str) -> Vec<String> {
    let mut bases = Vec::new();
    let stripped = strip_suffix(&surface.rust_type_name, type_suffix);
    bases.push(to_snake_case(&stripped));
    let full = to_snake_case(&surface.rust_type_name);
    if full != bases[0] {
        bases.push(full);
    }
    bases
}

fn entity_base(surface: &ProducerEntitySurface, type_suffix: &str) -> String {
    strip_suffix(&surface.rust_type_name, type_suffix)
}

fn child_match<'a>(
    surface: &'a ProducerEntitySurface,
    payload: &str,
) -> Option<(&'a SurfaceChild, &'a SurfaceField)> {
    let payload = to_snake_case(payload);
    for child in &surface.children {
        if let Some(member) = child.fields.iter().find(|field| field.name == payload) {
            return Some((child, member));
        }
    }
    None
}

/// The R1 nearest candidate (snake equality already failed): prefix-normalised
/// edit distance ≤ 2, ties lexicographic, `None` when nothing is close.
fn nearest_candidate(
    payload: &str,
    surface: &ProducerEntitySurface,
    type_suffix: &str,
) -> Option<String> {
    let mut prefixes = vec![
        to_snake_case(&surface.rust_type_name),
        to_snake_case(&surface.title),
        to_snake_case(&strip_suffix(&surface.rust_type_name, type_suffix)),
    ];
    prefixes.sort();
    prefixes.dedup();
    // Step 2 — the `_code`-stripped equality (both directions).
    let payload_snake = to_snake_case(payload);
    let stripped = strip_code_suffix_safe(&payload_snake);
    let code_match = surface.fields.iter().find(|field| {
        field.name == stripped
            || strip_code_suffix_safe(&field.name) == stripped
            || strip_code_suffix_safe(&field.name) == payload_snake
    });
    if let Some(field) = code_match {
        return Some(field.name.clone());
    }

    // Step 3 — prefix-normalised edit distance.
    let needle = normalize(payload, &prefixes);
    let mut best: Option<(usize, String)> = None;
    for field in &surface.fields {
        let candidate = normalize(&field.name, &prefixes);
        let distance = levenshtein(&needle, &candidate);
        if distance > 2 {
            continue;
        }
        let better = match &best {
            None => true,
            Some((best_distance, best_name)) => {
                distance < *best_distance || (distance == *best_distance && candidate < *best_name)
            }
        };
        if better {
            best = Some((distance, field.name.clone()));
        }
    }
    best.map(|(_, name)| name)
}

fn normalize(name: &str, prefixes: &[String]) -> String {
    let mut normalized = to_snake_case(name);
    for prefix in prefixes {
        if prefix.is_empty() {
            continue;
        }
        if let Some(stripped) = normalized.strip_prefix(&format!("{prefix}_")) {
            normalized = stripped.to_string();
            break;
        }
    }
    normalized
}

/// Levenshtein distance (the nearest-candidate metric; no new dependency).
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

/// The R6 anchor: the longest `{Entity}<Anchor>` prefix of a `…Changed` fact
/// (the entity spellings are the title, the DTO stem, and the suffix-stripped
/// base), returned snake-cased.
fn change_anchor(
    event_name: &str,
    surface: &ProducerEntitySurface,
    type_suffix: &str,
) -> Option<String> {
    let base = event_name.strip_suffix("Changed")?;
    let mut candidates = vec![surface.title.clone(), surface.rust_type_name.clone()];
    let stripped = strip_suffix(&surface.rust_type_name, type_suffix);
    if stripped != surface.rust_type_name {
        candidates.push(stripped);
    }
    let mut best: Option<&str> = None;
    for entity in &candidates {
        if entity.is_empty() {
            continue;
        }
        if let Some(rest) = base.strip_prefix(entity.as_str())
            && !rest.is_empty()
            && best.is_none_or(|best| rest.len() < best.len())
        {
            best = Some(rest);
        }
    }
    best.map(to_snake_case)
}

fn context_fact(payload: &str) -> Option<ContextFact> {
    match to_snake_case(payload).as_str() {
        "occurred_on" => Some(ContextFact::OccurredOn),
        "previous_state" | "from_state" => Some(ContextFact::PreviousState),
        "next_state" | "to_state" => Some(ContextFact::NextState),
        "correlation_id" => Some(ContextFact::CorrelationId),
        "organization_id" => Some(ContextFact::OrganizationId),
        _ => None,
    }
}

/// The supported-conversion table (`docs/events-producer.md` R7): identical,
/// `Uuid → String`, `i32 → i64`, codelist enum → the same enum or `String`.
fn type_disagreement(
    field: &crate::events::model::EventFieldDef,
    source: &SurfaceField,
) -> Option<Disagreement> {
    let payload = field.rust_type.trim();
    let source_type = source.rust_type.trim();

    // List-vs-scalar: the current `.evt` mapping carries no multiplicity, so
    // this arm is defensive (a future list-shaped payload refuses cleanly).
    let payload_is_list = payload.starts_with("Vec<") || payload.starts_with('[');
    if payload_is_list != source.is_array {
        return Some(Disagreement::ListScalar);
    }
    if source.is_optional {
        return Some(Disagreement::Nullability);
    }
    // Record-vs-id: a class-shaped payload against a Uuid FK column.
    if source.is_entity_ref && !field.is_primitive && !is_uuid_type(payload) {
        return Some(Disagreement::RecordId);
    }
    if same_type(payload, source_type) {
        return None;
    }
    // `Uuid → String`.
    if is_uuid_type(source_type) && payload == "String" {
        return None;
    }
    // `i32 → i64`.
    if source_type == "i32" && payload == "i64" {
        return None;
    }
    // A codelist enum → the same enum (already identical) or `String`.
    if source.is_codelist && payload == "String" {
        return None;
    }
    Some(Disagreement::Type)
}

fn is_uuid_type(ty: &str) -> bool {
    matches!(ty, "uuid::Uuid" | "Uuid")
}

fn same_type(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if is_uuid_type(a) && is_uuid_type(b) {
        return true;
    }
    // The op-clock spellings the generated DTOs use.
    let clock = |ty: &str| {
        matches!(
            ty,
            "chrono::DateTime<chrono::Utc>" | "chrono::DateTime<Utc>" | "chrono::NaiveDateTime"
        )
    };
    clock(a) && clock(b)
}
