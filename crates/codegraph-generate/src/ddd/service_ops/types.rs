use std::collections::HashMap;

use codegraph_core::types::PropertyNode;
use codegraph_naming::to_snake_case;

// ── Route model ────────────────────────────────────────────────────────

/// A machine state name (declaration spelling, e.g. `AwaitingApproval`).
///
/// A newtype keeps state names from being confused with trigger names,
/// wire keys, or arbitrary prose at call sites.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct StateName(String);

impl StateName {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for StateName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A normalized machine trigger name (snake_case, e.g. `submit`).
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct TriggerName(String);

impl TriggerName {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TriggerName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// An Update-DTO wire key carrying the machine state field.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct WireKey(String);

impl WireKey {
    #[must_use]
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for WireKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One machine transition reduced to the wire-relevant `(from, to)` pair.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServiceOpTransition {
    pub from: StateName,
    pub to: StateName,
}

/// The machine-field binding an executable save/update delegation carries
/// (P0.5/P0): the state field on the response, its DTO wire key, the
/// machine's initial state, and the transitions this operation may apply.
///
/// Since issue #818 the RENDERED handler no longer enumerates `(from, to)`
/// pairs: the machine decides, through
/// `codegraph_workflow::MachineRuntime::transition_for` probes over the
/// registry machine this id names. The pair lists remain PLANNER-side
/// facts — the hurl contract's state simulation, the IFML action planner
/// and the e2e plans walk them — and the renderer bakes the probe
/// CANDIDATES (the target sets) from them.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServiceOpState {
    /// Rust expression producing the record's current state as a `String`
    /// (codelist/string aware, optional-aware).
    pub current_expr: String,
    /// The Update-DTO key carrying the state field (DTO casing applied).
    pub wire_key: WireKey,
    /// The machine's declared initial state — the hurl contract's starting
    /// point when simulating the action sequence.
    pub initial: StateName,
    /// The machine's registry id — the domain-types `machines` key the
    /// rendered kernel probe loads (`machines::machine_json(id)`).
    #[serde(default)]
    pub machine_id: String,
    /// Every machine transition — the machine-wide universe the PLANNERS
    /// walk (hurl state simulation, e2e plans). Since issue #831 the renderer
    /// no longer bakes a candidate list from it: a body-driven op resolves
    /// its target through `MachineRuntime::resolve_target` against the
    /// current state's own out-edges.
    pub all: Vec<ServiceOpTransition>,
    /// The transitions this op applies, selected at runtime by the current
    /// state. Empty = the op performs no state transition of its own.
    /// Planner-side: the renderer bakes the probe candidate set from the
    /// targets.
    pub op: Vec<ServiceOpTransition>,
}

/// The parent load-or-404 plan a collection-level child `add` renders
/// BEFORE its create command (issue #692).
///
/// The `add` writes the child row with the parent's FK taken from the
/// path, so the handler must prove the parent row exists — and is visible
/// to the session — before the write: a bogus or foreign-org parent
/// answers 404 through the RLS-scoped query plane, never Postgres `23503`
/// as a 500 (the domain error classifier maps only scope/role/RLS
/// markers). The plan resolves the parent's query slot AT GENERATION TIME
/// so the handler can never reference a method that does not exist; the
/// fail-closed refusals (no plain by-id load on the parent, a cross-domain
/// parent) are named generation errors raised in [`super::routes`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChildParentLoad {
    /// The parent aggregate's Rust type (`PayRun`) — the 404 prose.
    pub entity: String,
    /// The parent's query-slot module stem (`pay_run`) — the AppState slot
    /// is `state.{domain}_{module}_queries`.
    pub module: String,
    /// The parent's owning domain — the AppState slot prefix. Same-domain
    /// by construction (a cross-domain parent is a named generation error).
    pub domain: String,
    /// The parent's auditable arity — the `, false` include-deleted
    /// argument the query plane's `find_by_id` expects.
    pub auditable: bool,
}

/// The pre-rendered construction of an entity request DTO from a modeled
/// input boundary (issue #767).
///
/// A create-shaped op's body IS the modeled boundary, but the entity's
/// command plane takes `Create{Entity}Request` — so the handler deserializes
/// the boundary and CONSTRUCTS the request from it, field by field. The plan
/// is pre-rendered at generation time (the #338/#662 pattern), so the
/// template only splices text and the mapping refusals fire before any tree
/// is written.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BoundaryPayloadPlan {
    /// `Create` / `Update` — the request DTO prefix the handler constructs.
    pub dto_prefix: String,
    /// The handler-side extraction of the lone boundary parameter.
    pub extraction: String,
    /// The pre-rendered struct-literal field assignments.
    pub construct: String,
    /// Request fields the boundary does not carry, defaulted to `None` —
    /// documented in the emitted operation, never silent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub defaulted: Vec<String>,
    /// The modeled boundary's Rust path — the utoipa `request_body` type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_body: Option<String>,
    /// The minimal valid body the hurl contract sends (the boundary sample).
    pub sample: serde_json::Value,
    /// Required ENTITY-reference members. A generated contract cannot seed an
    /// arbitrary target row, so the hurl planner omits the leg for such a
    /// payload rather than posting the nil uuid (a guaranteed FK 500).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_class_refs: Vec<String>,
}

/// The executable lowering for one service op.
///
/// Replaces the former `is_delete`/`is_read`/`is_update` boolean pack (which
/// could represent invalid combinations) with an exhaustive tagged enum.
/// `#[non_exhaustive]` lets new lowerings land without breaking external
/// matches — internal matches must stay exhaustive.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ExecutionKind {
    /// Delegates to the bound repository's `delete` builtin.
    Delete,
    /// Delegates to `findById` and returns the loaded record.
    Read,
    /// Delegates to `save`/`update` and patches the loaded row. `state` is
    /// present when the entity carries a machine (transition binding or
    /// body-driven target whitelist).
    Update {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        state: Option<ServiceOpState>,
    },
    /// A create-shaped operation (issue #767): no statement body, no
    /// record-scoped load, a return slot resolving to the bound entity, and
    /// exactly one modeled INPUT boundary parameter. The route is
    /// COLLECTION-level (`POST …/{entity}/actions/{op}`) and its body IS the
    /// boundary; `plan` carries the request-DTO construction.
    Create { plan: BoundaryPayloadPlan },
    /// A checked `.ddd` statement body (issue #662): `plan.block` is the
    /// pre-rendered executable region (the author's `let`/`guard`/`assign`/
    /// `if`/`return` statements lowered over the loaded aggregate), and
    /// `plan.state` carries the inferred machine transition when the body
    /// leaves the state field alone.
    Body {
        plan: Box<crate::ddd::body_lower::BodyPlan>,
    },
    /// A modeled aggregate CHILD operation (issue #661): a
    /// `children <Root> { <op> => <feature>.<action> }` operation lowers to
    /// a real route against the CHILD's command plane, nested under the
    /// parent's aggregate path (`POST …/{parent}/{parent_id}/{child}/
    /// actions/{op}` for `add`, `…/{child_id}/actions/{op}` for
    /// `remove`/`save`).
    ChildOp {
        /// The validated child action (`add`/`remove`/`save`).
        action: ChildAction,
        /// The root's child feature, as authored (`lines`).
        feature: String,
        /// The child class, package-qualified as authored.
        child_class: String,
        /// Whether the child exposes `read` — the `add` leg's read-back
        /// contract rides it (the created row is fetched by id).
        read_back: bool,
        /// The parent load-or-404 the collection-level `add` renders
        /// BEFORE its create command (issue #692): the FK write only runs
        /// for a parent the session can see, so a bogus or foreign-org
        /// parent answers 404, never a 500. `Some` for every lowered
        /// `add`; `None` for `remove`/`save` (their scoped record load IS
        /// the parent-ownership proof) and for seamed adds (the 501 arm
        /// never reaches the create).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_load: Option<ChildParentLoad>,
        /// Issue #767: the operation declares a modeled INPUT boundary whose
        /// source class IS this child, so the handler constructs the child's
        /// request DTO from the boundary (the same field-by-field mapping the
        /// collection-level create uses). `None` keeps the operation on the
        /// child's own generated request DTO — byte-identical.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        payload: Option<BoundaryPayloadPlan>,
    },
    /// A projection READ (issue #768): a public abstract op whose return
    /// slot is a projection boundary of the bound entity. The handler loads
    /// the record (item-addressed) or the RLS-scoped collection
    /// (collection-addressed, `Vec` returns) and constructs the read model
    /// through the boundary's `from_row` — the same constructor the
    /// repository finder uses, so both read paths cannot drift.
    ProjectionRead { plan: ProjectionReadPlan },
    /// No executable lowering; the route answers 501 naming the gap.
    Seam { reason: String },
}

/// The lowering facts of one projection read (issue #768).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectionReadPlan {
    /// The projection boundary's generated module stem (`catalog`).
    pub module: String,
    /// The projection boundary name (`BookSummary`).
    pub projection: String,
    /// `true` for the collection-addressed shape (a `Vec` return over the
    /// RLS-scoped collection); `false` for the item-addressed read.
    pub collection: bool,
    /// The boundary's DERIVED members (`wire_key`, expression source): the
    /// hurl planner folds each over the contract's own create body and
    /// asserts the computed value — the derived member provably COMPUTES in
    /// the read path.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived: Vec<ProjectionDerived>,
}

/// One derived member's wire key + expression source (issue #768).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectionDerived {
    /// The member's wire key (project dto casing) the JSON assert reads.
    pub wire_key: String,
    /// The authored expression source (`playLength / 60`).
    pub expression: String,
}

/// The closed child-action vocabulary of `children <Root>` operations
/// (phero-ir `ChildRef.action`, validated upstream).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildAction {
    /// Append one element to the root's child collection.
    Add,
    /// Remove one element from it.
    Remove,
    /// Persist one element (insert or replace).
    Save,
}

impl ChildAction {
    /// Map the authored action string; `None` for anything outside the
    /// closed set (the upstream compiler already rejects those).
    #[must_use]
    pub fn from_scope(action: &str) -> Option<Self> {
        match action {
            "add" => Some(Self::Add),
            "remove" => Some(Self::Remove),
            "save" => Some(Self::Save),
            _ => None,
        }
    }

    /// The action's wire spelling (`add`/`remove`/`save`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Remove => "remove",
            Self::Save => "save",
        }
    }
}

impl ExecutionKind {
    /// Whether the route answers 501 for an unlowered op.
    #[must_use]
    pub fn is_seam(&self) -> bool {
        matches!(self, Self::Seam { .. })
    }

    /// The machine transition this lowering applies, if any — the hurl and
    /// e2e planners simulate the machine off it (a bodied op that leaves the
    /// state field alone carries the same shape as the `save`/`update` leg).
    #[must_use]
    pub fn transition_state(&self) -> Option<&ServiceOpState> {
        match self {
            Self::Update { state } => state.as_ref(),
            Self::Body { plan } => plan.state.as_ref(),
            Self::Delete
            | Self::Read
            | Self::Create { .. }
            | Self::ProjectionRead { .. }
            | Self::ChildOp { .. }
            | Self::Seam { .. } => None,
        }
    }

    /// The modeled-boundary payload an aggregate child operation binds
    /// (issue #767), when it declares one.
    #[must_use]
    pub fn child_payload(&self) -> Option<&BoundaryPayloadPlan> {
        match self {
            Self::ChildOp { payload, .. } => payload.as_ref(),
            _ => None,
        }
    }

    /// The child action this lowering drives, when it is an aggregate child
    /// operation (issue #661).
    #[must_use]
    pub fn child_action(&self) -> Option<ChildAction> {
        match self {
            Self::ChildOp { action, .. } => Some(*action),
            _ => None,
        }
    }
}

/// The handler-template names a pre-rendered execution block splices
/// (issue #778).
///
/// The `delete` arm addresses the entity's command slot and spells its
/// error prose with the very names the handler template's own context
/// carries (`domain`, `module_name`, `param_name`, `entity_name`, the
/// domain-types crate), and the `update` arm additionally branches on the
/// handler's `role == "child"` (the parent-scoped reload) and
/// `is_auditable` (the query plane's `include_deleted` argument). Those
/// judgements are made where the routes are built, so
/// [`ServiceOpRoute::execution_block`] bakes them in. They are
/// generation-time render facts, not model facts: the whole struct is
/// `#[serde(skip)]` on the route, so it never reaches the template context
/// (the template holds its own bindings of the same names).
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ExecutionRenderFacts {
    /// The route's domain key (`payroll`) — the AppState slot prefix and the
    /// `{Domain}Error` type.
    pub domain: String,
    /// The bound entity's Rust type name (`PayRunType`) — the prose.
    pub entity_name: String,
    /// The bound entity's module stem (its `pg_table_name`).
    pub entity_table: String,
    /// The handler's path-parameter binding (`pay_run_id`).
    pub id_param: String,
    /// The generated domain-types crate (`domain_types`).
    pub domain_types_crate: String,
    /// `true` when the handler route is nested under a parent — the
    /// template's `role == "child"`, whose reload is the parent-scoped
    /// `find_by_id_scoped(id, parent_id)`.
    pub nested_child: bool,
    /// `true` when the entity's query/command planes take the
    /// `include_deleted` argument — the template's `is_auditable`.
    pub auditable: bool,
    /// `true` when the entity's `list_filtered` query carries the issue
    /// #306 `sort: Option<(String, bool)>` argument — the template's
    /// `ux_sort_fields` (non-empty sortable-column list). Resolved with the
    /// SAME `resolve_ux_sort_plan` the query and handler planes use, so the
    /// collection projection read's `list_filtered` call keeps the arity the
    /// query plane emitted.
    pub ux_sort_fields: bool,
}

/// One service operation lowered to a route on one entity.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServiceOpRoute {
    /// The owning service name.
    pub service: String,
    /// Operation name as authored.
    pub op_name: String,
    /// Route path segment and handler-fn suffix (`to_snake_case(op_name)`).
    pub snake: String,
    /// Handler fn name (`service_op_{snake}`).
    pub fn_name: String,
    /// Actor capability names guarding the op, as authored. Empty = ungated.
    pub capabilities: Vec<String>,
    /// Pre-rendered authorization fn (the whole item, doc comment
    /// included). `None` = ungated: no capabilities, or no actor policy in
    /// the graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorize_fn: Option<String>,
    /// The executable lowering (delete / read / update / 501 seam).
    pub execution: ExecutionKind,
    /// `true` when the bound entity's query plane carries `find_by_id`
    /// (the #449 invariant: the design surface exposes `read`). When
    /// `false` — a capability bound to a non-root entity — the route
    /// renders without the record load, and the authorize fn (if any)
    /// takes no response parameter: capability membership only. The
    /// route body still 501s (no record ⇒ no `read`/`delete` lowering),
    /// so nothing flows past the seam.
    pub record_load: bool,
    /// The gate is `async` and takes `(&AppState, &ApiKeyInfo, &response)`
    /// (issue #663): the op's `when` guard references an injected
    /// repository, and the rendered fn loads the related records before
    /// evaluating. `false` keeps today's synchronous
    /// `(role, &response)` shape — byte-identical flag-off output.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub authorize_async: bool,
    /// The op's injected-scope `when` guard, resolved at generation time
    /// (issue #663). `None` for own-record-only guards — the hurl
    /// contract's state planner pins the conservative denial leg when a
    /// guard is present but not provably satisfied by a fresh fixture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub injected_guard: Option<InjectedGuard>,
    /// Pre-rendered design-invariant block (DD5, issue #666 slice 4d):
    /// the candidate (loaded record + action patch, machine state move
    /// included) evaluated BEFORE the write. `None` = the entity carries
    /// no enforceable invariants (or the plane is gated off) —
    /// byte-identical arm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invariant_block: Option<String>,
    /// The #338 create-eligibility guard block the create-shaped arm splices
    /// after the reload (an ineligible create is compensated and denied with
    /// 403). Whitespace-exact, leading newline included — the same text the
    /// handler's `create_guard_block` context key carries; the handler
    /// generator attaches it AFTER the routes are built (the
    /// `invariant_block` post-pass), so the routes plane leaves it empty and
    /// the wiring re-renders. Empty = no create-eligible rule.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub create_guard_block: String,
    /// The #666 slice-4d CREATE design-invariant block (server-generated
    /// fields exist only post-insert, so the CREATED record is evaluated and
    /// a violation compensates before the 409) — the handler's
    /// `create_invariant_block` key. Attached by the same post-pass as
    /// [`Self::create_guard_block`]; empty = no enforceable invariants.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub create_invariant_block: String,
    /// The op's declared `throws` set (DD7, issue #665), authored error
    /// names. Empty ⇒ the op declares no vocabulary — every consumer
    /// (utoipa documentation, hurl planning) renders nothing, byte-
    /// identical to the pre-#665 shape.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub throws: Vec<String>,
    /// The declared statuses the action path documents (issue #665): the
    /// op's `throws` resolved through the shared error derivation, MINUS
    /// the statuses the surface's own base set already documents (404 for
    /// a record load, 409 for a machine-bound update, 422 for a child
    /// payload, 501 for a seam). A `Body` execution documents its raises
    /// through the #662 plan instead, so this stays empty there.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raise_responses: Vec<crate::ddd::body_lower::BodyRaiseResponse>,
    /// The handler-template names this route's execution block renders with
    /// (issue #778). Generation-time only: [`Self::execution_block`] bakes
    /// them in, so they stay out of the serialized template context.
    #[serde(skip)]
    pub render_facts: ExecutionRenderFacts,
    /// Pre-rendered execution body for the handler template (issue #778).
    /// Empty until the per-kind lowering fills it; the template renders it
    /// verbatim once wired.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub execution_block: String,
}

/// The compact injected-scope guard facts the hurl contract's planner
/// needs (issue #663) — a THREE-VERDICT model, never a guess, because a
/// mis-pinned contract leg fails the suite as hard in EITHER direction
/// (a guessed 403 that comes back 200 is a false positive, not a safe
/// fallback):
///
/// - [`FreshFixtureGuard::Satisfied`] — every scope's FK is REQUIRED and
///   every injected comparison targets its machine field with a value the
///   machine's initial state decides, and every decided comparison PASSES
///   on a fresh row → the guard provably holds; the contract keeps the
///   normal legs.
/// - [`FreshFixtureGuard::Denied`] — provable denial: some scope's FK is
///   OPTIONAL (the create body omits nullable props, so the fresh row's FK
///   is NULL and the rendered gate denies BEFORE evaluating), or some
///   machine-decided comparison FAILS on the fresh row → pin 403.
/// - [`FreshFixtureGuard::Unverifiable`] — any comparison the collector
///   does not fully understand (negation, `||` composition, non-machine
///   fields, non-name/date literals, field-to-field) or an unparseable
///   condition → the contract OMITS the leg with the documented header
///   (the `Body { may_refuse }` precedent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshFixtureGuard {
    Satisfied,
    Denied,
    Unverifiable,
}

/// The compact guard facts a route carries for the hurl contract's
/// planner (issue #663).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InjectedGuard {
    /// The guard's condition source, verbatim — the comment the emitted
    /// contract pins.
    pub condition: String,
    /// The fresh-fixture verdict (see [`FreshFixtureGuard`]).
    pub fresh_fixture: FreshFixtureGuard,
}

// ── Field knowledge ────────────────────────────────────────────────────

/// How a record field compares in the Rust lowering (the `when` guards and,
/// since issue #662, the statement-body expressions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    String,
    Bool,
    Integer,
    Float,
    Date,
    Codelist,
    Unsupported,
}

/// One entity property as the Rust lowering sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldInfo {
    pub(crate) rust_name: String,
    pub(crate) kind: FieldKind,
    pub(crate) optional: bool,
}

fn field_kind_for(prop: &PropertyNode) -> FieldKind {
    if prop.is_array {
        return FieldKind::Unsupported;
    }
    if matches!(
        prop.effective_kind(),
        Some(codegraph_type_contracts::RefClassificationKind::CodelistReference)
            | Some(codegraph_type_contracts::RefClassificationKind::CodelistCheck)
    ) {
        return FieldKind::Codelist;
    }
    match prop.rust_field_type.as_str() {
        "String" => FieldKind::String,
        "bool" => FieldKind::Bool,
        "chrono::NaiveDate" => FieldKind::Date,
        other => {
            let (numeric, integer) = numeric_class(other);
            if integer {
                FieldKind::Integer
            } else if numeric {
                FieldKind::Float
            } else {
                FieldKind::Unsupported
            }
        }
    }
}

/// `(numeric, integer)` — the `rules.rs` classification, minus the
/// collection unwrap (arrays are refused above).
fn numeric_class(rust_field_type: &str) -> (bool, bool) {
    let integer = matches!(
        rust_field_type,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "isize"
    );
    let numeric = integer || matches!(rust_field_type, "f32" | "f64");
    (numeric, integer)
}

pub(crate) fn fields_for(props: &[PropertyNode]) -> HashMap<String, FieldInfo> {
    let mut fields = HashMap::new();
    for p in props {
        let info = FieldInfo {
            rust_name: p.rust_field_name.clone(),
            kind: field_kind_for(p),
            optional: p.is_nullable,
        };
        fields.insert(p.name.clone(), info);
        // The authored name may be camelCase; the DTO field is snake_case.
        let snake = to_snake_case(&p.name);
        fields.entry(snake).or_insert_with(|| FieldInfo {
            rust_name: p.rust_field_name.clone(),
            kind: field_kind_for(p),
            optional: p.is_nullable,
        });
    }
    fields
}
