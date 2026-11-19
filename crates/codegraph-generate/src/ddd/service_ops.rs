//! Service operations lower to API surface (issue #590).
//!
//! A `.ddd` service operation is the application surface, not
//! documentation: a public (non-`protected`) op with a resolvable entity
//! binding lowers to `POST /api/{version}/{domain}/{entity}/{id}/actions/
//! {op_snake}` — the exact wire shape the IFML action buttons already
//! emit (issue #559, `RenderActionButton`/`RemoteCommandKind::Op`). The
//! actor policy authored in `.actor` imports becomes runtime enforcement
//! at the route: capability membership, `never_both` separation-of-duties
//! conflicts, and per-grant `when` conditions evaluate server-side over
//! the loaded record.
//!
//! Entity binding resolution (first hit wins, deterministic):
//! 0. an authored `transition` binding's delegation target (issue #660) —
//!    the binding names a machine on that repository's designed class, so
//!    that class is the entity the op acts on (a capability only GATES the
//!    route, so it cannot outrank it);
//! 1. capability class binding — an op capability bound (`.actor`
//!    `capability X for Class`) to a schema title (exact → `+Type`);
//! 2. delegation to a repository — `op => SomeRepository.op` where the
//!    repository's design class resolves to a schema title;
//! 3. the statement body's `=> Repository` receiver (issue #662) — a bodied
//!    op declares no delegation, so its primary repository binds the entity;
//! 4. signature — a class-typed parameter or return type.
//!
//! Ops that bind no entity (or are `protected`) emit no route; they are
//! reported by [`dead_surface_warnings`] (the Ask 1 diagnostic).
//!
//! Aggregate child operations (issue #661). A module-level
//! `children <Root> { <op> => <feature>.<action> }` block lowers onto the
//! ROOT's design (`Design::child_operations`, each op carrying a
//! `child_scope`), but it ACTS on the child. [`routes_for_entity`] therefore
//! collects, for a CHILD entity, every child op whose `childScope.class`
//! resolves to it and emits the route into the CHILD's router — the parent
//! nests that router (the WS4 shape), so the nested path and the
//! parent-scoped ownership load are inherited, not re-invented. The lowering
//! drives the CHILD's command plane: `add` → create-and-link
//! (`create(item, parent_id, …)`, a COLLECTION-level route because the
//! element does not exist yet), `save` → update, `remove` → delete — all
//! over the parent-scoped record. A child action that cannot be bound
//! faithfully (a missing child plane op, a scalar parameter, an `add` guard
//! with no record) answers the 501 seam naming the fix; a child entity that
//! is not declared a nested child of that root warns and emits nothing.
//! Contained children (mox `contains` ⇒ value object) are out of scope:
//! upstream Rule 6 forbids a standalone repository for them.
//!
//! Execution lowering: a checked `.ddd` statement body takes PRECEDENCE and
//! lowers into the handler ([`crate::ddd::body_lower`], issue #662).
//! Otherwise repository `delete` delegations wire to the entity's command
//! delete, `findById` delegations return the loaded record, and
//! `save`/`update` delegations patch the loaded row through the update
//! command (P0).
//!
//! Machine transitions (issue #660). Where the entity carries a `.ddd`
//! machine, the operation's transition is selected by, in order:
//!
//! 1. **the authored `transition <Trigger>` binding**
//!    (`name => Repository.save transition <EnumType>.<Trigger>`) — the
//!    PRIMARY selector. It resolves against the machine bound to the entity
//!    (the delegation target's designed class): the trigger must be one the
//!    machine declares (an explicit `via` name or a bare arrow's derived
//!    lowerCamel name), the optional qualifier must name that machine's
//!    enum type, and a module with more than one machine-bearing design
//!    requires it. An unresolvable/ambiguous binding is a generation error
//!    ([`transition_binding_errors`]) — NEVER a fallback to an inference;
//! 2. otherwise the pre-#660 heuristics, kept ONLY as the no-binding
//!    fallback for models that predate the syntax: op-name → trigger
//!    (exact, then entity-suffix-stripped), the op-level `when` source
//!    states, and the capability-sharing `protected`-op arm
//!    (`preparePayRun` → `calculate`). Every op resolved this way reports a
//!    WARN naming the missing binding ([`transition_binding_warnings`]).
//!
//! The target state is then resolved from the record's current state — a
//! no-transition state change fails closed with 409. Ops with no executable
//! lowering (abstract bodies, service-to-service/search/finder delegation)
//! answer 501 naming the gap.

use std::collections::{BTreeMap, HashMap};

use codegraph_config::DomainConfig;
use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PropertyNode;
use codegraph_naming::to_snake_case;
use heck::ToLowerCamelCase;
use phero_expr::{BinOp, Expr, ExprKind, UnOp};

use crate::ddd::design::DddDesignSurface;
use crate::project_config::DtoKeyCasing;

// ── Route model ────────────────────────────────────────────────────────

/// A machine state name (declaration spelling, e.g. `AwaitingApproval`).
///
/// A newtype keeps state names from being confused with trigger names,
/// wire keys, or arbitrary prose at call sites.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ServiceOpTransition {
    pub from: StateName,
    pub to: StateName,
}

/// The machine-field binding an executable save/update delegation carries
/// (P0.5/P0): the state field on the response, its DTO wire key, the
/// machine's initial state, and the transitions this operation may apply.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceOpState {
    /// Rust expression producing the record's current state as a `String`
    /// (codelist/string aware, optional-aware).
    pub current_expr: String,
    /// The Update-DTO key carrying the state field (DTO casing applied).
    pub wire_key: WireKey,
    /// The machine's declared initial state — the hurl contract's starting
    /// point when simulating the action sequence.
    pub initial: StateName,
    /// Every machine transition — the body-driven target whitelist when the
    /// op has no name/guard/capability binding.
    pub all: Vec<ServiceOpTransition>,
    /// The transitions this op applies, selected at runtime by the current
    /// state. Empty = the op performs no state transition of its own.
    pub op: Vec<ServiceOpTransition>,
}

/// The executable lowering for one service op.
///
/// Replaces the former `is_delete`/`is_read`/`is_update` boolean pack (which
/// could represent invalid combinations) with an exhaustive tagged enum.
/// `#[non_exhaustive]` lets new lowerings land without breaking external
/// matches — internal matches must stay exhaustive.
#[derive(Debug, Clone, serde::Serialize)]
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
        #[serde(skip_serializing_if = "Option::is_none")]
        state: Option<ServiceOpState>,
    },
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
    },
    /// No executable lowering; the route answers 501 naming the gap.
    Seam { reason: String },
}

/// The closed child-action vocabulary of `children <Root>` operations
/// (phero-ir `ChildRef.action`, validated upstream).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
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
            Self::Delete | Self::Read | Self::ChildOp { .. } | Self::Seam { .. } => None,
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

/// One service operation lowered to a route on one entity.
#[derive(Debug, Clone, serde::Serialize)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub authorize_async: bool,
    /// The op's injected-scope `when` guard, resolved at generation time
    /// (issue #663). `None` for own-record-only guards — the hurl
    /// contract's state planner pins the conservative denial leg when a
    /// guard is present but not provably satisfied by a fresh fixture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub injected_guard: Option<InjectedGuard>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshFixtureGuard {
    Satisfied,
    Denied,
    Unverifiable,
}

/// The compact guard facts a route carries for the hurl contract's
/// planner (issue #663).
#[derive(Debug, Clone, serde::Serialize)]
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

fn fields_for(props: &[PropertyNode]) -> HashMap<String, FieldInfo> {
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

// ── Rust lowering of `when` conditions ──────────────────────────────────

/// Lower a pherolang `when` condition source to a Rust boolean expression
/// over `response` (the loaded `{Entity}Response` record).
///
/// The documented closed subset mirrors `db::expr_sql` (the RLS lowering):
/// bare own-record field refs, string/number/boolean/date literals,
/// comparisons, `&& || !`, parentheses, and null comparisons. Codelist
/// fields compare via their `Display` (the codelist code); date fields
/// compare via their ISO `Display`. EVERYTHING else is refused — what
/// cannot be lowered faithfully is a hard generation error naming the
/// capability (the pherolang Cedar-backend philosophy).
#[cfg(test)]
pub(crate) fn lower_when_to_rust(
    source: &str,
    capability: &str,
    entity: &str,
    fields: &HashMap<String, FieldInfo>,
) -> Result<String, String> {
    lower_when_to_rust_with_states(source, capability, entity, fields, &[])
}

/// [`lower_when_to_rust`] with the entity's designed machine states bound as
/// enum-valued bare names (the pherolang `check_condition` convention): a
/// `when (runStatus == AwaitingApproval)` lowers the bare `AwaitingApproval`
/// to the string literal the codelist/string comparison path consumes.
/// Unknown names fail closed with a diagnostic naming both the fields and
/// the declared states.
#[cfg(test)]
pub(crate) fn lower_when_to_rust_with_states(
    source: &str,
    capability: &str,
    entity: &str,
    fields: &HashMap<String, FieldInfo>,
    states: &[String],
) -> Result<String, String> {
    lower_when_scoped(source, capability, entity, fields, states, &[])
}

/// [`lower_when_to_rust_with_states`] over a scope table that may carry
/// injected repositories (issue #663): an injected path
/// (`TimecardRepository.timecardStatus`) lowers over the loaded related
/// record the rendered gate binds. The closed subset is unchanged — a
/// dotted chain deeper than one segment under an injected root, or a root
/// that names no injected scope, is refused with the construct named.
pub(crate) fn lower_when_scoped(
    source: &str,
    capability: &str,
    entity: &str,
    fields: &HashMap<String, FieldInfo>,
    states: &[String],
    injected: &[InjectedScope],
) -> Result<String, String> {
    let parsed = phero_expr::parse(source);
    match parsed.ast {
        Some(ast) if parsed.errors.is_empty() => {
            lower_scoped(&ast, capability, entity, fields, states, injected)
        }
        _ => Err(format!(
            "capability `{capability}`: `when` condition is not valid \
             pherolang expression syntax: {source:?}"
        )),
    }
}

/// One injected scope a `when` lowering may reference (issue #663),
/// resolved at generation time from the op's `condition_paths`.
#[derive(Debug, Clone)]
pub(crate) struct InjectedScope {
    /// The injection root exactly as authored — the declared dependency
    /// name (`TimecardRepository`), which is the condition path's first
    /// segment.
    pub root: String,
    /// The Rust local the rendered gate binds (`to_snake_case(root)`).
    pub local: String,
    /// The AppState query slot prefix (`{domain}_{module}`) of the injected
    /// entity's query plane.
    pub slot: String,
    /// The injected plane's auditable flag — `find_by_id`'s
    /// include-deleted arity.
    pub auditable: bool,
    /// The own-record FK field carrying the injected id (the response's
    /// Rust field name, e.g. `timecard_id`).
    pub fk_field: String,
    /// Whether the FK is `Option` (an optional `refers`).
    pub fk_optional: bool,
    /// The injected class's fields — the comparison domain over the loaded
    /// record.
    pub fields: HashMap<String, FieldInfo>,
    /// The injected class's machine when the class carries one — the hurl
    /// planner's fresh-fixture reasoning and (via its states) the guard's
    /// enum-literal namespace.
    pub machine: Option<InjectedMachine>,
}

/// One injected class's designed machine (issue #663).
#[derive(Debug, Clone)]
pub struct InjectedMachine {
    /// The machine's field as authored (`timecardStatus`).
    pub field: String,
    /// The machine's initial state (`Pending`) — a fresh fixture row sits
    /// here.
    pub initial: String,
    /// Every declared state — the enum literals the guard may compare
    /// against (the module's enum namespace is global upstream).
    pub states: Vec<String>,
}

/// Resolve one operand to its scope root and field: `None` for a
/// non-field operand (literal), the own record (`response`) for a bare
/// entity field, the scope's local for a one-segment injected path.
fn resolve_operand(
    scopes: &(&str, &HashMap<String, FieldInfo>, &[InjectedScope]),
    expr: &Expr,
    capability: &str,
) -> Result<Option<(String, FieldInfo)>, String> {
    let (entity, fields, injected) = scopes;
    match &expr.kind {
        ExprKind::Name(name) => match fields.get(name) {
            Some(field) => Ok(Some(("response".to_string(), field.clone()))),
            None => {
                // A bare name that is not a field is a literal operand (a
                // declared machine state, most often) — `lower` already
                // validated or lowered it; the comparison rendering treats
                // it as the literal side, exactly as the pre-#663
                // `operand_field` did.
                let _ = entity;
                Ok(None)
            }
        },
        ExprKind::FeatureAccess {
            receiver,
            name,
            optional_safe,
        } => {
            if *optional_safe {
                return Err(format!(
                    "capability `{capability}`: `when` condition uses `?.` \
                     (safe navigation), which cannot be lowered"
                ));
            }
            let ExprKind::Name(root) = &receiver.kind else {
                return Err(format!(
                    "capability `{capability}`: `when` condition uses field \
                     access `{}`, which cannot be lowered — a dotted chain \
                     under an injected repository is one segment deep at most",
                    write_path(expr)
                ));
            };
            let Some(scope) = injected.iter().find(|scope| &scope.root == root) else {
                return Err(format!(
                    "capability `{capability}`: `when` condition uses field \
                     access `{root}`.{}, which is not an own-record field; \
                     only bare field references and one segment under an \
                     injected repository are supported",
                    name.value
                ));
            };
            match scope
                .fields
                .get(&name.value)
                .or_else(|| scope.fields.get(&to_snake_case(&name.value)))
            {
                Some(field) => Ok(Some((scope.local.clone(), field.clone()))),
                None => Err(format!(
                    "capability `{capability}`: `when` condition references \
                     `{root}`.{}, which is not a field of the injected \
                     repository's class",
                    name.value
                )),
            }
        }
        _ => Ok(None),
    }
}

fn lower_scoped(
    expr: &Expr,
    capability: &str,
    entity: &str,
    fields: &HashMap<String, FieldInfo>,
    states: &[String],
    injected: &[InjectedScope],
) -> Result<String, String> {
    let scopes = (entity, fields, injected);
    lower(expr, capability, &scopes, states)
}

fn lower(
    expr: &Expr,
    capability: &str,
    scopes: &(&str, &HashMap<String, FieldInfo>, &[InjectedScope]),
    states: &[String],
) -> Result<String, String> {
    let (entity, fields, _injected) = scopes;
    let reject = |construct: &str| {
        Err(format!(
            "capability `{capability}`: `when` condition uses {construct}, \
             which cannot be lowered to a Rust guard on `{entity}`; the \
             supported subset is own-record field refs, one segment under \
             an injected repository, string/number/boolean/date literals, \
             comparisons, && || !, parentheses, and null comparisons"
        ))
    };
    match &expr.kind {
        ExprKind::Int(value) => Ok(value.to_string()),
        ExprKind::Float(value) => Ok(float_literal(*value)),
        ExprKind::String(text) => Ok(rust_string_literal(text)),
        ExprKind::Bool(true) => Ok("true".to_string()),
        ExprKind::Bool(false) => Ok("false".to_string()),
        ExprKind::Null => reject("`null` (compare a field to null instead)"),
        ExprKind::Date { text, .. } => {
            // Format-validated here; the comparison renders the ISO text
            // (`NaiveDate`'s Display) at the use site.
            if crate::db::expr_sql::parse_date_text(text).is_some() {
                Ok(rust_string_literal(text))
            } else {
                Err(format!(
                    "capability `{capability}`: `when` condition uses the \
                     date literal {text:?}, which is not a calendar date \
                     `YYYY-MM-DD`"
                ))
            }
        }
        ExprKind::Name(name) => {
            if let Some(field) = fields.get(name) {
                return Ok(field_ref("response", field));
            }
            // A bare name that is a declared machine state lowers to the
            // enum value's wire spelling: the codelist/string comparison
            // path then renders `response.<field>.to_string() == "<state>"`.
            if states.iter().any(|state| state == name) {
                return Ok(rust_string_literal(name));
            }
            let states_note = if states.is_empty() {
                String::new()
            } else {
                format!(
                    " nor a declared state of its machine ({})",
                    states.join(", ")
                )
            };
            Err(format!(
                "capability `{capability}`: `when` condition references \
                 `{name}`, which is not a field of `{entity}`{states_note}"
            ))
        }
        ExprKind::Unary { op, expr } => {
            let inner = lower(expr, capability, scopes, states)?;
            match op {
                UnOp::Not => Ok(format!("(!({inner}))")),
                UnOp::Neg => match &expr.kind {
                    ExprKind::Int(value) => Ok(format!("-{value}")),
                    ExprKind::Float(value) => Ok(format!("-{}", float_literal(*value))),
                    _ => reject("negation of a non-literal (arithmetic)"),
                },
            }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            if matches!(op, BinOp::Eq | BinOp::Ne)
                && (matches!(lhs.kind, ExprKind::Null) || matches!(rhs.kind, ExprKind::Null))
            {
                let both_null =
                    matches!(lhs.kind, ExprKind::Null) && matches!(rhs.kind, ExprKind::Null);
                if both_null {
                    return Ok(match op {
                        BinOp::Eq => "true".to_string(),
                        _ => "false".to_string(),
                    });
                }
                let field_expr_expr = if matches!(lhs.kind, ExprKind::Null) {
                    rhs
                } else {
                    lhs
                };
                // The non-null operand must be a field ref for the
                // Option check to be honest; a required field's null
                // comparison is vacuous and lowers to a literal.
                let (root, field) = resolve_operand(scopes, field_expr_expr, capability)?
                    .ok_or_else(|| {
                        format!(
                            "capability `{capability}`: `when` condition uses \
                             a null comparison against a non-field"
                        )
                    })?;
                return Ok(match (op, field.optional) {
                    (BinOp::Eq, true) => format!("({root}.{}.is_none())", field.rust_name),
                    (BinOp::Ne, true) => {
                        format!("(!({root}.{}.is_none()))", field.rust_name)
                    }
                    (BinOp::Eq, false) => "false".to_string(),
                    (BinOp::Ne, false) => "true".to_string(),
                    _ => unreachable!("only Eq/Ne enter this branch"),
                });
            }
            let rust_op = match op {
                BinOp::Eq => "==",
                BinOp::Ne => "!=",
                BinOp::Lt => "<",
                BinOp::Le => "<=",
                BinOp::Gt => ">",
                BinOp::Ge => ">=",
                BinOp::And => "&&",
                BinOp::Or => "||",
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    return reject(&format!("arithmetic (`{op}`)"));
                }
            };
            comparison(
                op,
                &lower(lhs, capability, scopes, states)?,
                &lower(rhs, capability, scopes, states)?,
                rust_op,
                capability,
                scopes,
                lhs,
                rhs,
            )
        }
        ExprKind::FeatureAccess {
            receiver,
            name,
            optional_safe,
        } => {
            if *optional_safe {
                return reject("`?.` (safe navigation)");
            }
            // A dotted path lowers ONLY through an injected scope; every
            // other receiver is the own-record dotted-navigation refusal.
            match resolve_operand(scopes, expr, capability) {
                Ok(Some((root, field))) => Ok(field_ref(&root, &field)),
                Ok(None) => reject(&format!(
                    "field access on a non-field receiver (`{}`.{})",
                    write_path(receiver),
                    name.value
                )),
                Err(message) => Err(message),
            }
        }
        ExprKind::Coalesce { .. } => reject("`?:`"),
        ExprKind::Call { .. } | ExprKind::FunctionCall { .. } => reject("an operation call"),
        ExprKind::Algebra { kind, .. } => reject(&format!("collection algebra `{kind}`")),
        ExprKind::If { .. } => reject("`if`"),
        ExprKind::Let { .. } => reject("`let`"),
        ExprKind::Lambda { .. } => reject("a lambda"),
        ExprKind::ListLiteral(_) => reject("list literals"),
    }
}

/// Render a comparison. Field-vs-literal comparisons unwrap `Option`
/// fields honestly; field-vs-field comparisons require matching
/// requiredness (mixed requiredness cannot compare without a policy
/// decision the model does not make). Cross-scope comparisons (an own
/// field against an injected field) are refused: no root is honest for
/// both sides.
#[allow(clippy::too_many_arguments)]
fn comparison(
    op: &BinOp,
    lhs_text: &str,
    rhs_text: &str,
    rust_op: &str,
    capability: &str,
    scopes: &(&str, &HashMap<String, FieldInfo>, &[InjectedScope]),
    lhs: &Expr,
    rhs: &Expr,
) -> Result<String, String> {
    let is_ordering = matches!(op, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge);
    let lhs_field = resolve_operand(scopes, lhs, capability)?;
    let rhs_field = resolve_operand(scopes, rhs, capability)?;
    match (lhs_field, rhs_field) {
        (Some((root, field)), None) => {
            let literal = strip_parens(rhs_text);
            field_vs_literal(
                &root,
                &field,
                &literal,
                rust_op,
                is_ordering,
                capability,
                scopes.0,
            )
        }
        (None, Some((root, field))) => {
            let literal = strip_parens(lhs_text);
            // Flip the ordering operator so the field stays on the left.
            let flipped = match op {
                BinOp::Lt => ">",
                BinOp::Le => ">=",
                BinOp::Gt => "<",
                BinOp::Ge => "<=",
                BinOp::Eq => "==",
                BinOp::Ne => "!=",
                BinOp::And => "&&",
                BinOp::Or => "||",
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    unreachable!("arithmetic is rejected before comparison rendering")
                }
            };
            field_vs_literal(
                &root,
                &field,
                &literal,
                flipped,
                is_ordering,
                capability,
                scopes.0,
            )
        }
        (Some((lhs_root, a)), Some((rhs_root, b))) => {
            if lhs_root != rhs_root {
                return Err(format!(
                    "capability `{capability}`: comparison between `{}` and `{}` across \
                     different records (an own-record field vs an injected field, or two \
                     injected scopes) cannot be lowered faithfully — compare each side \
                     against a literal instead",
                    a.rust_name, b.rust_name
                ));
            }
            if is_ordering && (a.optional || b.optional || a.kind != b.kind) {
                return Err(format!(
                    "capability `{capability}`: ordering comparison between \
                     fields `{}` and `{}` cannot be lowered faithfully",
                    a.rust_name, b.rust_name
                ));
            }
            if a.optional != b.optional || a.kind != b.kind {
                return Err(format!(
                    "capability `{capability}`: field-to-field comparison \
                     between `{}` and `{}` mixes requiredness or types",
                    a.rust_name, b.rust_name
                ));
            }
            Ok(format!(
                "({lhs_root}.{} {rust_op} {rhs_root}.{})",
                a.rust_name, b.rust_name
            ))
        }
        (None, None) => Ok(format!("({lhs_text} {rust_op} {rhs_text})")),
    }
}

fn field_vs_literal(
    root: &str,
    field: &FieldInfo,
    literal: &str,
    rust_op: &str,
    is_ordering: bool,
    capability: &str,
    entity: &str,
) -> Result<String, String> {
    let name = &field.rust_name;
    let root = root.to_string();
    match field.kind {
        FieldKind::String => match field.optional {
            false => Ok(format!("({root}.{name} {rust_op} {literal})")),
            true if is_ordering => Ok(format!(
                "({root}.{name}.as_deref().is_some_and(|v| v {rust_op} {literal}))"
            )),
            true => Ok(format!(
                "({root}.{name}.as_deref() {} Some({literal}))",
                eq_op(rust_op)
            )),
        },
        FieldKind::Bool => {
            if is_ordering {
                return Err(format!(
                    "capability `{capability}`: ordering comparison on bool \
                     field `{name}` of `{entity}`"
                ));
            }
            match field.optional {
                false => Ok(format!("({root}.{name} {rust_op} {literal})")),
                true => Ok(format!(
                    "({root}.{name} {} Some({literal}))",
                    eq_op(rust_op)
                )),
            }
        }
        FieldKind::Integer | FieldKind::Float => match field.optional {
            false => Ok(format!("({root}.{name} {rust_op} {literal})")),
            true if is_ordering => Ok(format!(
                "({root}.{name}.is_some_and(|v| v {rust_op} {literal}))"
            )),
            true => Ok(format!(
                "({root}.{name} {} Some({literal}))",
                eq_op(rust_op)
            )),
        },
        FieldKind::Codelist | FieldKind::Date => {
            if is_ordering {
                return Err(format!(
                    "capability `{capability}`: ordering comparison on \
                     `{kind}` field `{name}` of `{entity}`",
                    kind = if field.kind == FieldKind::Codelist {
                        "codelist"
                    } else {
                        "date"
                    },
                ));
            }
            match field.optional {
                false => Ok(format!("({root}.{name}.to_string() {rust_op} {literal})")),
                true => Ok(format!(
                    "({root}.{name}.as_ref().map(|v| v.to_string()).as_deref() {} Some({literal}))",
                    eq_op(rust_op)
                )),
            }
        }
        FieldKind::Unsupported => Err(format!(
            "capability `{capability}`: `when` condition references `{name}`, \
             which is not a comparable scalar field of `{entity}`"
        )),
    }
}

fn eq_op(rust_op: &str) -> &'static str {
    match rust_op {
        "==" => "==",
        _ => "!=",
    }
}

/// Render one field reference over its scope root (`response` for the own
/// record, the injected scope's local otherwise).
fn field_ref(root: &str, field: &FieldInfo) -> String {
    format!("{root}.{}", field.rust_name)
}

fn strip_parens(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

fn write_path(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Name(name) => name.clone(),
        ExprKind::FeatureAccess {
            receiver,
            name,
            optional_safe,
        } => format!(
            "{}{}{}",
            write_path(receiver),
            if *optional_safe { "?." } else { "." },
            name.value
        ),
        _ => "<expression>".to_string(),
    }
}

/// Keep the decimal point so the literal reads as the float the DSL
/// authored (the `expr_sql` convention).
fn float_literal(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value}.0")
    } else {
        format!("{value}")
    }
}

/// Render `text` as a Rust string literal.
pub(crate) fn rust_string_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

// ── Policy surface ─────────────────────────────────────────────────────

/// The actor policy flattened for server-side enforcement: BaseJump role →
/// capability → grant `when` sources (None = unconditional), plus the
/// `never_both` groups. Role mapping mirrors the IFML hydration convention
/// (`route_generator/hydration.rs`): an actor whose snake_case name equals
/// the account role claims it; otherwise `owner` falls to the first actor
/// and `member` to the last.
#[derive(Debug, Clone, Default)]
pub(crate) struct PolicySurface {
    pub roles: BTreeMap<String, BTreeMap<String, Vec<Option<String>>>>,
    pub never_both: Vec<Vec<String>>,
}

impl PolicySurface {
    /// Build from the graph readback. `None` when the graph carries no
    /// actors (no policy ingested — capability/SoD gates are skipped with
    /// a warning, the IFML warn-and-continue posture).
    pub async fn from_graph(db: &dyn GraphQuerier) -> Result<Option<Self>, GraphError> {
        let actors = db.get_actors().await?;
        if actors.is_empty() {
            return Ok(None);
        }
        let grants = db.get_grants().await?;
        let actor_names: Vec<String> = actors.iter().map(|a| a.name.clone()).collect();
        let mut roles: BTreeMap<String, BTreeMap<String, Vec<Option<String>>>> = BTreeMap::new();
        for role in ["owner", "member", "manager", "employee"] {
            let picked = basejump_role_actors(role, &actor_names);
            if picked.is_empty() {
                continue;
            }
            let mut permits: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();
            for actor in &picked {
                for permit in
                    codegraph_core::types::resolve_effective_permits(&actors, &grants, actor)
                {
                    let entry = permits.entry(permit.capability.clone()).or_default();
                    let when = permit.when.clone();
                    if !entry.contains(&when) {
                        entry.push(when);
                    }
                }
            }
            roles.insert(role.to_string(), permits);
        }
        let never_both = db
            .get_actor_policy()
            .await?
            .map(|policy| {
                policy
                    .never_both
                    .iter()
                    .map(|group| group.capabilities.clone())
                    .collect()
            })
            .unwrap_or_default();
        Ok(Some(Self { roles, never_both }))
    }

    /// The capabilities that conflict with `capabilities` under
    /// `never_both` (deterministic order, deduped).
    pub fn sod_conflicts(&self, capabilities: &[String]) -> Vec<String> {
        let mut conflicts = Vec::new();
        for group in &self.never_both {
            if group.iter().any(|c| capabilities.contains(c)) {
                for member in group {
                    if !capabilities.contains(member) && !conflicts.contains(member) {
                        conflicts.push(member.clone());
                    }
                }
            }
        }
        conflicts
    }
}

/// The actors mapped to one BaseJump account role (the hydration
/// convention).
fn basejump_role_actors(role: &str, actor_names: &[String]) -> Vec<String> {
    let snake_match = actor_names.iter().find(|a| to_snake_case(a) == role);
    match (role, snake_match, actor_names.first(), actor_names.last()) {
        (_, Some(a), _, _) => vec![a.clone()],
        ("owner", None, Some(first), _) => vec![first.clone()],
        ("member", None, _, Some(last)) => vec![last.clone()],
        _ => Vec::new(),
    }
}

// ── Authorization fn rendering ──────────────────────────────────────────

/// Render the per-op `authorize` fn encoding the policy for the route:
/// capability membership, `never_both` separation of duties, and per-grant
/// `when` guards evaluated over the loaded record.
///
/// `None` when no role holds any of the op's capabilities AND none even
/// conflicts — the route is emitted ungated-capability-free only when the
/// op declares no capabilities; a capability no grant mentions emits a
/// deny-all gate (the policy says nobody holds it), so this returns the
/// fn whenever the policy surface exists.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_authorize_fn(
    snake: &str,
    op_name: &str,
    service: &str,
    capabilities: &[String],
    condition: Option<&str>,
    response_type_path: &str,
    surface: Option<&PolicySurface>,
    fields: &HashMap<String, FieldInfo>,
    entity: &str,
    record_available: bool,
) -> Result<Option<String>, String> {
    render_authorize_fn_with_states(
        snake,
        op_name,
        service,
        capabilities,
        condition,
        response_type_path,
        surface,
        fields,
        entity,
        record_available,
        &[],
    )
}

/// [`render_authorize_fn`] with the entity's designed machine states bound
/// in `when` lowering (P0.5): `when (runStatus == AwaitingApproval)` treats
/// the bare state name as the enum value, not a record field.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_authorize_fn_with_states(
    snake: &str,
    op_name: &str,
    service: &str,
    capabilities: &[String],
    condition: Option<&str>,
    response_type_path: &str,
    surface: Option<&PolicySurface>,
    fields: &HashMap<String, FieldInfo>,
    entity: &str,
    record_available: bool,
    machine_states: &[String],
) -> Result<Option<String>, String> {
    render_authorize_fn_with_injected(
        snake,
        op_name,
        service,
        capabilities,
        condition,
        response_type_path,
        surface,
        fields,
        entity,
        record_available,
        machine_states,
        &[],
    )
}

/// [`render_authorize_fn_with_states`] with the op's injected-scope
/// `when` paths resolved (issue #663): the gate becomes `async`, takes the
/// `AppState` (the sibling query slot) and the session, loads every
/// injected scope through the repository plane's own query, then evaluates
/// the guard over the loaded records. `record_available` must hold — the
/// injected ids ride the loaded response — and is refused otherwise.
/// Empty `injected` renders byte-identically to
/// [`render_authorize_fn_with_states`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_authorize_fn_with_injected(
    snake: &str,
    op_name: &str,
    service: &str,
    capabilities: &[String],
    condition: Option<&str>,
    response_type_path: &str,
    surface: Option<&PolicySurface>,
    fields: &HashMap<String, FieldInfo>,
    entity: &str,
    record_available: bool,
    machine_states: &[String],
    injected: &[InjectedScope],
) -> Result<Option<String>, String> {
    // The enum-literal namespace is module-global upstream: the injected
    // machines' declared states bind as bare names exactly like the
    // op entity's own (`TimecardRepository.timecardStatus == Approved`
    // lowers `Approved` as the Timecard machine's literal).
    let mut all_states: Vec<String> = machine_states.to_vec();
    for scope in injected {
        if let Some(machine) = &scope.machine {
            for state in &machine.states {
                if !all_states.contains(state) {
                    all_states.push(state.clone());
                }
            }
        }
    }
    let lowered_condition = condition
        .map(|source| {
            let lowered =
                lower_when_scoped(source, op_name, entity, fields, &all_states, injected)?;
            Ok::<_, String>((lowered, source.to_string()))
        })
        .transpose()?;
    let condition_message = condition.map(|source| {
        rust_string_literal(&format!(
            "operation `{op_name}`: `when` condition not met ({source})"
        ))
    });
    if capabilities.is_empty() && lowered_condition.is_none() {
        return Ok(None);
    }
    let empty_roles = BTreeMap::new();
    let roles = match surface {
        Some(policy) => &policy.roles,
        None => &empty_roles,
    };
    // Does the rendered gate read the loaded record? The op-level `when`
    // always does; a per-grant `when` does whenever a held permit carries
    // one without an unconditional (None) override. (Over-approximates
    // only when a when-guarded permit also lands on a `never_both`
    // conflict — the conflict arm never reads the record — failing
    // closed there.)
    let needs_record = lowered_condition.is_some()
        || roles.values().any(|permits| {
            permits.iter().any(|(capability, whens)| {
                capabilities.contains(capability)
                    && !whens.is_empty()
                    && !whens.iter().any(Option::is_none)
                    && whens.iter().any(Option::is_some)
            })
        });
    if needs_record && !record_available {
        return Err(format!(
            "service op `{op_name}` carries `when` conditions, but its bound \
             entity `{entity}` has no record load (its design surface exposes \
             no `read`, so the query plane has no `find_by_id`) — generation \
             refuses to emit an unguarded route"
        ));
    }
    // An injected guard loads the related records off the response —
    // without a record load there is no honest id source (fail closed,
    // the same posture as the `when`-without-read refusal above).
    if !injected.is_empty() && !record_available {
        return Err(format!(
            "service op `{op_name}` carries an injected-scope `when` guard, \
             but its bound entity `{entity}` has no record load (its design \
             surface exposes no `read`) — the guard's id source is the \
             loaded record, so generation refuses"
        ));
    }
    let mut out = String::new();
    let mut body = String::new();
    if injected.is_empty() {
        // Without a record the gate is capability membership only: no
        // response parameter to evaluate `when`s over (there are none —
        // `needs_record` refused those above).
        let response_param = if record_available {
            format!("    response: &{response_type_path},\n")
        } else {
            String::new()
        };
        out.push_str(&format!(
            "/// Authorization gate for `{op_name}` (service `{service}`, issue #590).\n\
             ///\n\
             /// Encodes the actor policy at generation time: capability\n\
             /// membership{membership_note}\n\
             /// API-key sessions bypass the policy gates (RLS scopes still\n\
             /// apply) so contract suites and service callers keep working.\n\
             #[allow(clippy::result_large_err)]\n\
             fn service_op_{snake}_authorize(\n    \
                 role: &str,\n{response_param}\
             ) -> Result<(), String> {{\n",
            membership_note = if record_available {
                ", `never_both` separation-of-duties conflicts, and\n\
                 /// per-grant `when` conditions evaluated over the loaded record."
            } else {
                " and\n\
                 /// `never_both` separation-of-duties conflicts (no record load — \
                 the bound\n\
                 /// entity exposes no `read`, so `when` conditions cannot be \
                 evaluated; the\n\
                 /// op body is a 501 seam regardless)."
            },
            response_param = response_param,
        ));
    } else {
        out.push_str(&format!(
            "/// Authorization gate for `{op_name}` (service `{service}`, issue #590).\n\
             ///\n\
             /// Encodes the actor policy at generation time: capability\n\
             /// membership, `never_both` separation-of-duties conflicts, and\n\
             /// per-grant `when` conditions evaluated over the loaded record —\n\
             /// plus the injected-scope `when` guard (issue #663): every\n\
             /// injected scope loads through the repository plane's own query\n\
             /// (RLS-scoped by the session context) before the guard evaluates.\n\
             /// API-key sessions bypass the policy gates (RLS scopes still\n\
             /// apply) so contract suites and service callers keep working.\n\
             #[allow(clippy::result_large_err)]\n\
             async fn service_op_{snake}_authorize(\n    \
                 state: &AppState,\n    \
                 api_key_info: &ApiKeyInfo,\n    \
                 response: &{response_type_path},\n\
             ) -> Result<(), String> {{\n    \
                 let role = api_key_info.role.as_str();\n"
        ));
        for scope in injected {
            let message = condition_message.as_deref().unwrap_or("\"guard denied\"");
            // The own-record FK carries the injected id; an optional `refers`
            // with no related row cannot satisfy the guard — the denial IS
            // the guard's answer.
            if scope.fk_optional {
                body.push_str(&format!(
                    "    let Some({local}_id) = response.{fk} else {{\n        \
                         return Err({message}.to_string());\n    \
                     }};\n",
                    local = scope.local,
                    fk = scope.fk_field,
                    message = message,
                ));
            } else {
                body.push_str(&format!(
                    "    let {local}_id = response.{fk};\n",
                    local = scope.local,
                    fk = scope.fk_field,
                ));
            }
            let auditable_clause = if scope.auditable { " false," } else { "" };
            body.push_str(&format!(
                "    // The guard reads `{root}.*` — load the related record through\n    \
                     // the repository plane's own query (issue #663).\n    \
                     let {local} = state.{slot}_queries\n        \
                     .find_by_id({local}_id,{aud} api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n        \
                     .await\n        \
                     .map_err(|e| e.to_string())?;\n    \
                     let Some({local}) = {local} else {{\n        \
                         return Err({message}.to_string());\n    \
                     }};\n",
                root = scope.root,
                local = scope.local,
                slot = scope.slot,
                aud = auditable_clause,
                message = message,
            ));
        }
    }
    if let (Some((lowered, _)), Some(message)) = (&lowered_condition, &condition_message) {
        body.push_str(&format!(
            "    // op-level `when` transition guard — business semantics apply \
             to every session\n\
             if !({lowered}) {{\n                \
                 return Err({message}.to_string());\n            }}\n"
        ));
    }
    body.push_str(
        "    if role == \"api_key\" {\n                \
             return Ok(());\n            \
         }\n\
         match role {\n",
    );
    out.push_str(&body);
    let conflicts = surface
        .map(|policy| policy.sod_conflicts(capabilities))
        .unwrap_or_default();
    for (role, permits) in roles {
        let held: Vec<&String> = capabilities
            .iter()
            .filter(|c| permits.contains_key(*c))
            .collect();
        if held.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "        {role_quoted} => {{\n",
            role_quoted = rust_string_literal(role)
        ));
        let role_conflicts: Vec<&String> = conflicts
            .iter()
            .filter(|c| permits.contains_key(*c))
            .collect();
        if !role_conflicts.is_empty() {
            out.push_str(&format!(
                "            // separation of duties: `never_both` conflicts\n\
                 // with `{}`\n\
                 Err({}.to_string())\n        }}\n",
                role_conflicts
                    .iter()
                    .map(|c| c.as_str())
                    .collect::<Vec<_>>()
                    .join("`, `"),
                rust_string_literal(&format!(
                    "separation of duties: role `{role}` holds `{}`, which is \
                     `never_both` with `{}`",
                    role_conflicts
                        .iter()
                        .map(|c| c.as_str())
                        .collect::<Vec<_>>()
                        .join("`, `"),
                    capabilities.join("`, `")
                ))
            ));
            continue;
        }
        // Conditional permits over the held capabilities; an unconditional
        // permit clears the guard.
        let mut conditions: Vec<String> = Vec::new();
        let mut unconditional = false;
        for capability in &held {
            if let Some(whens) = permits.get(*capability) {
                if whens.iter().any(Option::is_none) {
                    unconditional = true;
                    break;
                }
                for when in whens.iter().flatten() {
                    let lowered = lower_when_scoped(
                        when,
                        capability,
                        entity,
                        fields,
                        machine_states,
                        injected,
                    )?;
                    conditions.push(lowered);
                }
            }
        }
        if !unconditional && !conditions.is_empty() {
            // Conditions are fully parenthesized by the lowering.
            let joined = conditions.join(" || ");
            let sources = held
                .iter()
                .filter_map(|c| permits.get(*c))
                .flat_map(|whens| whens.iter().flatten())
                .cloned()
                .collect::<Vec<_>>()
                .join("` / `");
            out.push_str(&format!(
                "            // `when` guard{plural} on {}\n\
                 if !({joined}) {{\n                \
                     return Err({}.to_string());\n            }}\n\
                 Ok(())\n        }}\n",
                sources,
                rust_string_literal(&format!(
                    "capability `{}`: `when` condition not met ({sources})",
                    held.iter()
                        .map(|c| c.as_str())
                        .collect::<Vec<_>>()
                        .join("`, `")
                )),
                plural = if conditions.len() == 1 { "" } else { "s" },
            ));
        } else {
            out.push_str("            Ok(())\n        }\n");
        }
    }
    out.push_str(&format!(
        "        _ => Err({}.to_string()),\n    }}\n}}\n",
        rust_string_literal(&format!(
            "capability `{}` required (no policy role holds it)",
            capabilities.join("`, `")
        ))
    ));
    Ok(Some(out))
}

// ── Graph resolution ───────────────────────────────────────────────────

/// The project-side render context a body-carrying route needs (issue #662):
/// the domain-types crate name its command/query calls spell, and the
/// Update-DTO wire-key casing its patch keys.
#[derive(Debug, Clone)]
pub struct BodyRenderContext {
    /// The generated domain-types crate (`domain_types`).
    pub domain_types_crate: String,
    /// The Update-DTO wire-key casing.
    pub dto_casing: DtoKeyCasing,
    /// The deployment topology is `workers` (issue #663): an injected-scope
    /// `when` guard refuses. The #590 action handlers run in per-domain
    /// worker crates whose `AppState` carries only its OWN domain's query
    /// slots, so a cross-domain injected load (`state.{other_domain}_…`)
    /// renders a field no worker crate has — E0599 at consumer build time,
    /// silently. Same-domain loads would compile, but the plane refuses
    /// both until the worker media/async story exists (#338's "guards stay
    /// doc-only under workers" conservatism). Default `false`.
    pub workers_topology: bool,
}

impl Default for BodyRenderContext {
    /// The plane's default names — the fallback for the read-side callers
    /// (IFML e2e planning, router registration) that never emit the body
    /// block itself.
    fn default() -> Self {
        Self {
            domain_types_crate: "domain_types".to_string(),
            dto_casing: DtoKeyCasing::default(),
            workers_topology: false,
        }
    }
}

impl BodyRenderContext {
    /// The context of one project configuration.
    #[must_use]
    pub fn from_project(project: &crate::ProjectConfig) -> Self {
        Self {
            domain_types_crate: project.identity.domain_types_crate.clone(),
            dto_casing: project.codegen.dto_key_casing,
            workers_topology: project.is_workers_topology(),
        }
    }
}

/// The declared `{Domain}Error` variants a body raise may name (DD7).
async fn raise_table(
    db: &dyn GraphQuerier,
    domain: &str,
) -> Result<crate::ddd::body_lower::RaiseTable, String> {
    let declared = crate::ddd::errors::declared_error_defs(db, domain)
        .await
        .map_err(|e| e.to_string())?;
    let mut table = crate::ddd::body_lower::RaiseTable::default();
    for error in declared {
        table.declared.insert(
            error.variant_name.clone(),
            crate::ddd::body_lower::DeclaredError {
                variant: error.variant_name,
                code: error.code,
                status: error.http_status.max(0) as u16,
            },
        );
    }
    Ok(table)
}

/// Resolve a class name (possibly `pkg::Name` or `pkg.Name`) to a schema
/// title: exact first, then the configured type suffix (the
/// `schema_for_class` convention).
async fn schema_title_for_class(
    db: &dyn GraphQuerier,
    class: &str,
    type_suffix: &str,
) -> Option<String> {
    let unqualified = short_class_name(class);
    if let Ok(Some(schema)) = db.get_schema(&unqualified).await {
        return Some(schema.title);
    }
    let suffixed = format!("{unqualified}{type_suffix}");
    if let Ok(Some(schema)) = db.get_schema(&suffixed).await {
        return Some(schema.title);
    }
    None
}

/// Resolve the service operations bound to one entity. Deterministic: models
/// in readback order, module-major, declaration order; ops binding the same
/// entity under one snake name dedup keeping the first.
pub async fn routes_for_entity(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    entity_title: &str,
    body: &BodyRenderContext,
) -> Result<Vec<ServiceOpRoute>, String> {
    let models = db.get_ddd_models().await.map_err(|e| e.to_string())?;
    // A model may carry aggregate child operations with no service at all
    // (issue #661) — the presence gate must see both planes.
    if models
        .iter()
        .all(|m| m.services.is_empty() && m.designs.iter().all(|d| d.child_operations.is_empty()))
    {
        return Ok(Vec::new());
    }
    let type_suffix = config.defaults.type_suffix.clone();
    let props = db
        .get_properties(entity_title)
        .await
        .map_err(|e| e.to_string())?;
    let fields = fields_for(&props);
    let surface = PolicySurface::from_graph(db)
        .await
        .map_err(|e| e.to_string())?;
    let Some(schema) = db
        .get_schema(entity_title)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(Vec::new());
    };
    let response_type = format!("{}Response", schema.rust_type_name);
    let module_name = schema.pg_table_name.clone();
    let entity_cfg = config
        .domains
        .get(domain)
        .and_then(|entry| entry.get_entity_config(entity_title));
    // The handler's own names, derived from the SAME inputs the handler
    // generator uses (`resolve_path_segment` + the path-segment→param-name
    // rule) so a body plan's command/query calls line up with the route.
    let id_param = super::super::api::router::param_name_from_path_segment(
        &super::super::api::api_model::resolve_path_segment(entity_cfg, &schema),
    );
    let scoped =
        (entity_cfg.and_then(|cfg| cfg.role.as_deref()) == Some("child")).then_some("parent_id");
    let auditable =
        super::super::api::handler::resolve_is_auditable(db, entity_title, config, domain)
            .await
            .map_err(|e| e.to_string())?;

    // The design surface gates the execution legs: delegation targets must
    // survive the #449 op-set mapping (delete → commands.delete, findById
    // → queries.find_by_id) to lower. The same surface carries the machine
    // (P0.5): `save`/`update` delegations bind a transition when the op
    // matches one, and the op-level `when` lowering binds declared states.
    let design_surface = DddDesignSurface::from_graph(db).await.ok();
    let surface_ops = design_surface
        .as_ref()
        .and_then(|s| s.operations_for(entity_title))
        .unwrap_or_default();
    let machine = design_surface
        .as_ref()
        .and_then(|s| s.machine_for(entity_title));
    let machine_states: Vec<String> = machine
        .as_ref()
        .map(|m| m.states().into_iter().map(str::to_string).collect())
        .unwrap_or_default();
    // The declared raise vocabulary a statement body lowers against (issue
    // #662). Read only when a bodied op exists — a model without bodies pays
    // no extra graph read.
    let raises = if models.iter().any(|model| {
        model
            .services
            .iter()
            .any(|service| service.operations.iter().any(|op| !op.bodies.is_empty()))
    }) {
        raise_table(db, domain).await?
    } else {
        crate::ddd::body_lower::RaiseTable::default()
    };

    let mut routes: Vec<ServiceOpRoute> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                if op.is_protected {
                    continue;
                }
                let Some(bound) = bind_entity(model, op, db, &type_suffix).await else {
                    continue;
                };
                if bound != entity_title || !seen.insert(op.name.clone()) {
                    continue;
                }
                let snake = to_snake_case(&op.name);
                // The route's record load rides the #449 invariant: the
                // entity's query plane carries `find_by_id` iff its design
                // surface exposes `read`. Capability clauses can bind
                // non-root entities (e.g. a PaySlip/`Coverage` noun), which
                // expose no read — those routes render without the record
                // load and with a response-less authorize fn (crewbase#71;
                // #595 follow-up). `when`-conditioned ops on such entities
                // are a generation error (fail closed).
                let record_load = surface_ops.iter().any(|o| o == "read");
                // Issue #663: resolve the op's injected-scope `when` paths
                // into the per-scope plans the rendered gate loads (schema,
                // domain, query slot, FK) plus the class fields the
                // comparison lowers over. Unresolvable shapes are named
                // generation errors — never a silently narrower guard.
                let injected =
                    resolve_injected_scopes(db, config, entity_title, op, &props, &design_surface)
                        .await?;
                // Issue #663, workers topology: the #590 action handlers
                // run in per-domain worker crates whose AppState carries
                // only its own domain's query slots — a cross-domain
                // injected load renders a field no worker crate has
                // (E0599 at consumer build, silently). The plane refuses
                // both scopes until the worker async-AppState story
                // exists (the #338 "guards stay doc-only under workers"
                // conservatism).
                if body.workers_topology && !injected.is_empty() {
                    return Err(format!(
                        "service op `{}` carries an injected-scope `when` guard, but the \
                         deployment topology is `workers`: the action handlers run in \
                         per-domain worker crates whose AppState carries only its own \
                         domain's query slots, so the injected load cannot address the \
                         `{}` aggregate — use the monolith topology for injected guards \
                         (a recorded #663 deferral, not a silent break)",
                        op.name,
                        injected
                            .first()
                            .map(|scope| scope.root.as_str())
                            .unwrap_or("injected")
                    ));
                }
                let injected_guard = injected_guard_facts(op, &injected);
                let authorize_fn = render_authorize_fn_with_injected(
                    &snake,
                    &op.name,
                    &service.name,
                    &op.capabilities,
                    op.condition.as_deref(),
                    &format!(
                        "crate::domain::{domain}::{module_name}::dto_response::{response_type}"
                    ),
                    surface.as_ref(),
                    &fields,
                    entity_title,
                    record_load,
                    &machine_states,
                    &injected,
                )?;
                let params = if op.bodies.is_empty() {
                    Vec::new()
                } else {
                    crate::ddd::body_lower::body_params(&op.name, &op.params)
                        .map_err(|refusal| refusal.0)?
                };
                let throws: Vec<String> = op
                    .throws
                    .iter()
                    .filter_map(|json| json.get("name").and_then(serde_json::Value::as_str))
                    .map(str::to_string)
                    .collect();
                let shape = RouteShape {
                    domain,
                    entity_title,
                    entity_name: &schema.rust_type_name,
                    entity_table: &module_name,
                    id_param: &id_param,
                    scoped,
                    auditable,
                    domain_types_crate: &body.domain_types_crate,
                    dto_casing: body.dto_casing,
                    fields: &fields,
                    surface_ops: &surface_ops,
                    record_load,
                    machine: machine.as_ref(),
                    raises: &raises,
                    params: &params,
                    throws: &throws,
                };
                let execution = execution_for(op, model, config, &service.name, &shape)?;
                routes.push(ServiceOpRoute {
                    service: service.name.clone(),
                    op_name: op.name.clone(),
                    fn_name: format!("service_op_{snake}"),
                    snake,
                    capabilities: op.capabilities.clone(),
                    authorize_fn,
                    execution,
                    record_load,
                    authorize_async: !injected.is_empty(),
                    injected_guard,
                });
            }
        }
    }

    // ── Aggregate child operations (issue #661) ─────────────────────────
    //
    // A `children <Root> { <op> => <feature>.<action> }` block is declared on
    // the ROOT's design but ACTS on the child: `add` appends to the root's
    // child collection, `remove` deletes one element, `save` updates one.
    // The route is therefore emitted into the CHILD entity's router — the
    // parent nests that router (`POST …/{parent}/{parent_id}/{child}/…`), so
    // the parent ownership load and the nested path are the SAME WS4 shape
    // the child CRUD plane already generates; nothing here invents a second
    // nesting. This pass collects the ops whose `childScope` resolves to the
    // entity being generated.
    for model in &models {
        for design in &model.designs {
            for op in &design.child_operations {
                let Some(scope) = op.child_scope.as_ref() else {
                    continue;
                };
                let Some(action) = ChildAction::from_scope(&scope.action) else {
                    crate::ddd::design::warn_once(
                        &format!("child-op-action:{}::{}", design.class, op.name),
                        &format!(
                            "child operation `{}` of `{}`: unknown child action `{}` (the \
                             closed set is add, remove, save); no route emitted",
                            op.name, design.class, scope.action
                        ),
                    );
                    continue;
                };
                // Resolution may be exact, `+type_suffix`, or a miss (the
                // class is not a schema in the graph).
                let Some(child_title) =
                    schema_title_for_class(db, &scope.class, &type_suffix).await
                else {
                    crate::ddd::design::warn_once(
                        &format!("child-op-class:{}::{}", design.class, op.name),
                        &format!(
                            "child operation `{}` of `{}`: child class `{}` resolves to no \
                             ingested schema; no route emitted",
                            op.name, design.class, scope.class
                        ),
                    );
                    continue;
                };
                if child_title != entity_title {
                    continue;
                }
                if !seen.insert(op.name.clone()) {
                    continue;
                }
                let Some(parent_title) = design.resolved_title.as_deref() else {
                    continue;
                };
                if op.is_protected {
                    crate::ddd::design::warn_once(
                        &format!("child-op-protected:{}::{}", design.class, op.name),
                        &format!(
                            "child operation `{}` of `{}` is `protected` — it emits no API \
                             surface (Sculptor visibility); IFML actions bound to it are \
                             dead triggers",
                            op.name, design.class
                        ),
                    );
                    continue;
                }
                if !schema.is_entity {
                    continue;
                }
                // The child's plane only nests under the parent when the
                // MODEL says so (config `role = "child"` + `parent` +
                // `parent_ref`): the parent ownership load, the two-uuid
                // path, and the scoped read all derive from it. Without it
                // there is no aggregated route to emit — warn (never a
                // top-level route that silently drops the parent anchor).
                if !is_nested_child_of(entity_cfg, parent_title, &design.class, &type_suffix) {
                    crate::ddd::design::warn_once(
                        &format!("child-op-nesting:{}::{}", design.class, op.name),
                        &format!(
                            "child operation `{}` of `{}`: child entity `{}` is not declared \
                             as a nested child of `{}` (domains.toml `role = \"child\"` + \
                             `parent = \"{}\"` + `parent_ref`); no route emitted",
                            op.name, design.class, entity_title, parent_title, parent_title
                        ),
                    );
                    continue;
                }
                let record_load = match action {
                    // `add` addresses the collection, not a child row.
                    ChildAction::Add => false,
                    ChildAction::Remove | ChildAction::Save => {
                        surface_ops.iter().any(|o| o == "read")
                    }
                };
                let execution = child_execution(op, action, scope, &surface_ops, record_load);
                // Issue #663: injected-scope guards are a SERVICE-op plane
                // lowering — a child op's `when` types against the CHILD
                // class as `self`, so an injected path there has no id
                // source (the child's own path is the route anchor). A
                // named generation error, never a silently narrower guard.
                if op.condition_paths.iter().any(|path| {
                    matches!(
                        path.scope,
                        codegraph_core::types::DddConditionScope::Injected { .. }
                    )
                }) {
                    return Err(format!(
                        "child operation `{}` of `{}` carries an injected-scope \
                         `when` path — the child plane lowers own-record guards \
                         only (the child row IS the guard's record); bind the \
                         cross-entity guard on a root service operation instead",
                        op.name, design.class
                    ));
                }
                // The policy surface (capability membership + the op-level
                // `when`, which types against the CHILD class as `self`).
                // `add` carries no loaded record, so a guard there was seamed
                // out above; the response-less authorize fn still gates the
                // capability.
                let authorize_fn = if !op.capabilities.is_empty() || op.condition.is_some() {
                    render_authorize_fn_with_states(
                        &to_snake_case(&op.name),
                        &op.name,
                        parent_title,
                        &op.capabilities,
                        if record_load {
                            op.condition.as_deref()
                        } else {
                            None
                        },
                        &format!(
                            "crate::domain::{domain}::{module_name}::dto_response::{response_type}"
                        ),
                        surface.as_ref(),
                        &fields,
                        entity_title,
                        record_load,
                        &machine_states,
                    )?
                } else {
                    None
                };
                let snake = to_snake_case(&op.name);
                routes.push(ServiceOpRoute {
                    // The declaration site: a child op's `service` names the
                    // aggregate ROOT design whose `children` block declared
                    // it (there is no `service` in the artifact).
                    service: short_class_name(&design.class),
                    op_name: op.name.clone(),
                    fn_name: format!("service_op_{snake}"),
                    snake,
                    capabilities: op.capabilities.clone(),
                    authorize_fn,
                    execution,
                    record_load,
                    authorize_async: false,
                    injected_guard: None,
                });
            }
        }
    }

    Ok(routes)
}

// ── Injected-scope `when` guards (issue #663) ──────────────────────────

/// Resolve an op's injected-scope `when` paths (issue #663) into the
/// per-scope plans the rendered gate loads: the injected class's schema
/// (exact → `+type_suffix`), its owning domain and query slot, the
/// auditable `find_by_id` arity, and the own-record FK carrying the id.
/// Every unresolvable shape is a named generation error — the closed
/// subset refuses, it never silently narrows. An op with no injected
/// paths resolves to an empty vec without touching the graph again.
#[allow(clippy::too_many_arguments)]
async fn resolve_injected_scopes(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    entity_title: &str,
    op: &codegraph_core::types::DddServiceOperation,
    props: &[PropertyNode],
    design_surface: &Option<DddDesignSurface>,
) -> Result<Vec<InjectedScope>, String> {
    let type_suffix = config.defaults.type_suffix.clone();
    // Distinct injected scopes, in the condition's declaration order.
    let mut requested: Vec<(String, String)> = Vec::new();
    for path in &op.condition_paths {
        if let codegraph_core::types::DddConditionScope::Injected { repository, class } =
            &path.scope
            && !requested.iter().any(|(r, c)| r == repository && c == class)
        {
            requested.push((repository.clone(), class.clone()));
        }
    }
    if requested.is_empty() {
        return Ok(Vec::new());
    }
    let mut scopes = Vec::with_capacity(requested.len());
    for (repository, class) in &requested {
        let Some(title) = schema_title_for_class(db, class, &type_suffix).await else {
            return Err(format!(
                "service op `{}`: the `when` guard's injected path `{repository}.…` names \
                 class `{class}`, which resolves to no schema in the graph — the injected \
                 repository's design must bind a modeled class",
                op.name
            ));
        };
        let Some(injected_schema) = db.get_schema(&title).await.map_err(|e| e.to_string())? else {
            return Err(format!(
                "service op `{}`: the injected class `{class}` resolved to `{title}`, \
                 which is not in the graph",
                op.name
            ));
        };
        let Some(injected_domain) = injected_schema.domain.clone() else {
            return Err(format!(
                "service op `{}`: the injected class `{class}` (`{title}`) carries no \
                 domain on its schema — the AppState query slot cannot be resolved",
                op.name
            ));
        };
        // A child-role injected entity exposes no plain `find_by_id` (its
        // plane is parent-scoped) — refuse with the modeling fix named.
        let injected_cfg = config
            .domains
            .get(&injected_domain)
            .and_then(|entry| entry.get_entity_config(&title));
        if injected_cfg.and_then(|cfg| cfg.role.as_deref()) == Some("child") {
            return Err(format!(
                "service op `{}`: the injected repository `{repository}` binds `{title}`, a \
                 child-role entity whose query plane is parent-scoped (`find_by_id_scoped`) — \
                 an injected `when` path cannot address it; bind the guard on the root \
                 aggregate's service operation instead",
                op.name
            ));
        }
        // The injected plane must expose `read` — the guard's load rides
        // the repository plane's own query (the #449 invariant).
        let surface_ops = design_surface
            .as_ref()
            .and_then(|s| s.operations_for(&title))
            .unwrap_or_default();
        if !surface_ops.iter().any(|o| o == "read") {
            return Err(format!(
                "service op `{}`: the injected repository `{repository}` binds `{title}`, \
                 whose design surface exposes no `read` — the guard's load has no query to \
                 ride; declare `findById` on the injected repository",
                op.name
            ));
        }
        // The id source: exactly ONE scalar entity-ref feature on the op's
        // entity targeting the injected class. Zero or ambiguous is a
        // named error — the guard has no honest record to load.
        let class_leaf = short_class_name(class);
        let fks: Vec<&PropertyNode> = props
            .iter()
            .filter(|p| {
                !p.is_array
                    && p.effective_kind()
                        == Some(codegraph_type_contracts::RefClassificationKind::EntityReference)
                    && p.ref_target.as_deref().is_some_and(|target| {
                        target == title
                            || target == class_leaf
                            || format!("{class_leaf}{type_suffix}") == target
                    })
            })
            .collect();
        let fk = match fks.as_slice() {
            [] => {
                return Err(format!(
                    "service op `{}`: the `when` guard reads `{repository}.…` (class \
                     `{class_leaf}`), but its bound entity `{entity_title}` carries no \
                     `{class_leaf}`-typed reference — the guard has no id source; add a \
                     `refers {class_leaf} timecard`-style feature to `{entity_title}`",
                    op.name
                ));
            }
            [only] => (only.rust_field_name.clone(), only.is_nullable),
            many => {
                let names: Vec<String> = many.iter().map(|p| p.rust_field_name.clone()).collect();
                return Err(format!(
                    "service op `{}`: `{entity_title}` carries more than one \
                     `{class_leaf}`-typed reference (`{}`) — the injected guard's id source is \
                     ambiguous; point the guard at exactly one reference (or a dedicated \
                     service parameter, a recorded follow-up)",
                    op.name,
                    names.join("`, `")
                ));
            }
        };
        let injected_fields = db.get_properties(&title).await.map_err(|e| e.to_string())?;
        let machine = design_surface.as_ref().and_then(|s| s.machine_for(&title));
        scopes.push(InjectedScope {
            root: repository.clone(),
            local: to_snake_case(repository),
            slot: format!("{injected_domain}_{}", injected_schema.pg_table_name),
            auditable: super::super::api::handler::resolve_is_auditable(
                db,
                &title,
                config,
                &injected_domain,
            )
            .await
            .map_err(|e| e.to_string())?,
            fk_field: fk.0,
            fk_optional: fk.1,
            fields: fields_for(&injected_fields),
            machine: machine.map(|m| InjectedMachine {
                states: m.states().into_iter().map(str::to_string).collect(),
                field: m.field,
                initial: m.initial,
            }),
        });
    }
    Ok(scopes)
}

/// The compact guard facts the hurl contract's planner needs (issue #663):
/// the THREE-verdict model — [`FreshFixtureGuard::Satisfied`] (every
/// comparison machine-decided and passing on a fresh row), [`FreshFixtureGuard::Denied`]
/// (an optional FK — the fresh row's FK is NULL and the gate denies before
/// evaluating — or a machine-decided comparison failing on the fresh row),
/// or [`FreshFixtureGuard::Unverifiable`] (any comparison the collector
/// does not fully understand). `None` when the op carries no injected
/// scopes.
fn injected_guard_facts(
    op: &codegraph_core::types::DddServiceOperation,
    scopes: &[InjectedScope],
) -> Option<InjectedGuard> {
    if scopes.is_empty() {
        return None;
    }
    let source = op.condition.as_deref()?;
    let verdict = |fresh: FreshFixtureGuard| {
        Some(InjectedGuard {
            condition: source.to_string(),
            fresh_fixture: fresh,
        })
    };
    // An optional FK denies BEFORE any comparison is evaluated: the create
    // body omits nullable props, so the fresh row's FK is NULL and the
    // rendered `let Some(..) else` fires. Provable 403 regardless of the
    // condition's content.
    if scopes.iter().any(|scope| scope.fk_optional) {
        return verdict(FreshFixtureGuard::Denied);
    }
    let parsed = phero_expr::parse(source);
    let Some(ast) = parsed.ast.filter(|_| parsed.errors.is_empty()) else {
        return verdict(FreshFixtureGuard::Unverifiable);
    };
    let mut comparisons = Vec::new();
    // Only comparisons under PLAIN AND from the root are collected (a
    // failing one there decides the whole guard); negation, `||`
    // composition and arithmetic set `fully_understood = false` and their
    // subtrees are not pushed — uncollected ⇒ Unverifiable (omit, never
    // guess).
    let mut fully_understood = true;
    collect_injected_comparisons(&ast, scopes, &mut comparisons, &mut fully_understood, true);
    // Every pushed comparison is machine-decidable (the collector filters);
    // a decided comparison FAILING on the fresh row means, under AND
    // composition (the only composition collected), the guard is false.
    let machine_decided: Vec<bool> = comparisons
        .iter()
        .map(|(scope_index, _, op, literal)| {
            let machine = scopes[*scope_index]
                .machine
                .as_ref()
                .expect("the collector only pushes machine-decidable comparisons");
            match op {
                BinOp::Eq => literal == &machine.initial,
                BinOp::Ne => literal != &machine.initial,
                _ => false,
            }
        })
        .collect();
    if machine_decided.iter().any(|passes| !passes) {
        return verdict(FreshFixtureGuard::Denied);
    }
    if !fully_understood || comparisons.is_empty() {
        return verdict(FreshFixtureGuard::Unverifiable);
    }
    verdict(FreshFixtureGuard::Satisfied)
}

/// The `(scope index, field, operator, literal)` triples of every injected
/// eq/ne comparison the planner can decide. ONLY comparisons under PLAIN
/// AND from the root are pushed, and ONLY machine-decidable ones (the
/// scope carries a machine and the field is its machine field): anything
/// else in a plain-AND context — ordering comparisons (`hours > 100`),
/// same-scope field-to-field, float/bool/date literals, non-machine
/// fields — is a conjunct that can fail on the fresh row while the
/// collected set reads Satisfied, so it sets `fully_understood = false`
/// and the planner's verdict becomes Unverifiable: the contract OMITS the
/// leg — a guess in either direction fails the suite just as hard.
/// Negation flips polarity and `||` may pass via the own-record side, so
/// their whole subtrees are undecided too. Pure own-record leaf
/// comparisons are skipped without marking: the planner's injected
/// verdict does not reason about them (the own-machine state is the
/// existing state-aware planner's input; a non-transition op's own-record
/// conjuncts are the documented pre-#663 surface).
#[allow(clippy::too_many_arguments)]
fn collect_injected_comparisons(
    expr: &Expr,
    scopes: &[InjectedScope],
    out: &mut Vec<(usize, String, BinOp, String)>,
    fully_understood: &mut bool,
    plain_and: bool,
) {
    // A decidable literal is a bare name (an enum member), a string, or an
    // integer — everything else (floats, bools, dates, arithmetic) leaves
    // the fresh value undecidable.
    let literal_side = |e: &Expr| -> Option<String> {
        match &e.kind {
            ExprKind::Name(name) => Some(name.clone()),
            ExprKind::String(text) => Some(text.clone()),
            ExprKind::Int(value) => Some(value.to_string()),
            _ => None,
        }
    };
    // Machine-decidable at COLLECT time: the scope carries a machine and
    // the field is its machine field. A non-machine Eq/Ne conjunct's fresh
    // value is fixture-seeded (required ints seed 42, strings seed
    // `contract-{table}`, codelists seed values[0]) — deciding it at the
    // fold would guess.
    let machine_decidable = |index: usize, field: &str| -> bool {
        scopes[index].machine.as_ref().is_some_and(|machine| {
            let snake = to_snake_case(&machine.field);
            let names = [machine.field.as_str(), snake.as_str()];
            names.contains(&field)
        })
    };
    match &expr.kind {
        ExprKind::Binary { op, lhs, rhs } => {
            let lhs_injected = injected_scope_of(lhs, scopes);
            let rhs_injected = injected_scope_of(rhs, scopes);
            let touches = lhs_injected.is_some() || rhs_injected.is_some();
            if plain_and && matches!(*op, BinOp::Eq | BinOp::Ne) && touches {
                // An injected-side Eq/Ne conjunct under plain AND: it can
                // decide the whole guard — understand it or mark undecided.
                // The operands of a comparison are values, not conjuncts —
                // nothing more to collect either way.
                if let (Some((index, field)), Some(literal)) = (lhs_injected, literal_side(rhs)) {
                    if machine_decidable(index, &field) {
                        out.push((index, field, *op, literal));
                    } else {
                        *fully_understood = false;
                    }
                    return;
                }
                if let (Some(literal), Some((index, field))) = (literal_side(lhs), rhs_injected) {
                    // Flip so the literal is compared against the field.
                    let flipped = match op {
                        BinOp::Eq => BinOp::Eq,
                        _ => BinOp::Ne,
                    };
                    if machine_decidable(index, &field) {
                        out.push((index, field, flipped, literal));
                    } else {
                        *fully_understood = false;
                    }
                    return;
                }
                // An injected comparison the collector cannot represent —
                // ordering, same-scope field-to-field, float/bool/date
                // literals, cross-scope pairs — is a dropped conjunct that
                // can fail on the fresh row while the collected set reads
                // Satisfied. Undecided, never dropped-silent.
                *fully_understood = false;
                return;
            }
            if plain_and && touches {
                // An ordering (or otherwise non-Eq/Ne) injected-side
                // conjunct under plain AND: the fresh value may be derivable
                // (required ints seed 42) but the planner does not model it
                // — undecided, never dropped-silent. The operands are
                // values, not conjuncts — nothing more to collect.
                *fully_understood = false;
                return;
            }
            if !touches && !matches!(*op, BinOp::And | BinOp::Or) {
                // A pure own-record LEAF comparison (its operands are
                // values — nothing injected anywhere in it): skip without
                // marking. AND/OR nodes always recurse: their operands ARE
                // the conjuncts.
                return;
            }
            match op {
                // `||`: the guard may pass via the own-record side — the
                // collected set no longer decides the whole condition, and
                // nothing under it is plain-AND from the root.
                BinOp::Or => *fully_understood = false,
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => *fully_understood = false,
                _ => {}
            }
            let child_and = plain_and && matches!(*op, BinOp::And);
            collect_injected_comparisons(lhs, scopes, out, fully_understood, child_and);
            collect_injected_comparisons(rhs, scopes, out, fully_understood, child_and);
        }
        ExprKind::Unary { op, expr } => {
            // Negation flips polarity: an un-negated collect would decide
            // the comparison upside down, and nothing under it is
            // plain-AND from the root.
            if matches!(op, UnOp::Not) {
                *fully_understood = false;
            }
            collect_injected_comparisons(expr, scopes, out, fully_understood, false);
        }
        _ => {}
    }
}

/// The injected scope `expr`'s FEATURE-ACCESS root names, if it is a bare
/// `Root.field` chain (the collect shape).
fn injected_scope_of(expr: &Expr, scopes: &[InjectedScope]) -> Option<(usize, String)> {
    let ExprKind::FeatureAccess {
        receiver,
        name,
        optional_safe: false,
    } = &expr.kind
    else {
        return None;
    };
    let ExprKind::Name(root) = &receiver.kind else {
        return None;
    };
    let index = scopes.iter().position(|scope| &scope.root == root)?;
    Some((index, name.value.clone()))
}

/// Whether the entity's config declares it a nested child of `parent_title`
/// (config `role = "child"`, `parent` naming the root, `parent_ref` naming
/// the back-ref column) — the WS4 nesting contract.
fn is_nested_child_of(
    entity_cfg: Option<&codegraph_config::EntityConfig>,
    parent_title: &str,
    parent_class: &str,
    type_suffix: &str,
) -> bool {
    let Some(cfg) = entity_cfg else {
        return false;
    };
    if cfg.role.as_deref() != Some("child") || cfg.parent_ref.is_none() {
        return false;
    }
    let Some(parent) = cfg.parent.as_deref() else {
        return false;
    };
    // The authored `parent` names the parent entity's title (the config
    // convention) but may use the bare class name — accept the class, its
    // `+type_suffix` spelling, and the resolved title.
    let parent_leaf = short_class_name(parent_class);
    parent == parent_title
        || parent == parent_leaf
        || format!("{parent}{type_suffix}") == parent_title
        || parent == format!("{parent_leaf}{type_suffix}")
}

/// The execution lowering for one aggregate child operation (issue #661).
///
/// A child action drives the CHILD entity's command plane:
///
/// - `add` → the child's create command (create-and-link into the parent's
///   child collection: the command writes the parent back-ref from the path);
/// - `save` → the child's update command over the parent-scoped record;
/// - `remove` → the child's delete command over the parent-scoped record.
///
/// Shapes that cannot be bound FAITHFULLY answer the 501 seam naming the
/// fix — never a silently broader write, never a patch of the parent row.
fn child_execution(
    op: &codegraph_core::types::DddServiceOperation,
    action: ChildAction,
    scope: &codegraph_core::types::DddChildRef,
    surface_ops: &[String],
    record_load: bool,
) -> ExecutionKind {
    let required = match action {
        ChildAction::Add => "create",
        ChildAction::Save => "update",
        ChildAction::Remove => "delete",
    };
    if !surface_ops.iter().any(|o| o == required) {
        return seam(format!(
            "child operation `{}` (action `{}`) needs the child's `{}` operation, but the \
             child's design exposes no `{}` — its repository must declare `{}` for the \
             action to lower",
            op.name,
            action.as_str(),
            required,
            required,
            match action {
                ChildAction::Add => "save",
                ChildAction::Save => "save",
                ChildAction::Remove => "delete",
            }
        ));
    }
    // A child action binds its input from the request BODY: zero parameters,
    // or exactly one object-shaped carrier (the child class or a DTO
    // boundary). A scalar/collection parameter has no honest binding — the
    // child id rides the route path, and everything else is a payload.
    if let Some(param) = op.params.first()
        && (op.params.len() > 1 || !is_body_carrier(param, &scope.class))
    {
        return seam(format!(
            "child operation `{}` declares parameter `{}` ({}), which has no executable \
             binding: a child action takes its payload from the request body — write \
             the parameter as the child class or a DTO boundary, and let the child id \
             ride the route path",
            op.name,
            param.name,
            body_carrier_hint(&param.type_json)
        ));
    }
    if op.condition.is_some() {
        if action == ChildAction::Add {
            return seam(format!(
                "child operation `{}` declares a `when` guard, but `add` runs before the \
                 child row exists — the guard types against a record that is not there \
                 (guard a `save`/`remove` action, or validate in the payload)",
                op.name
            ));
        }
        if !record_load {
            return seam(format!(
                "child operation `{}` declares a `when` guard, but the child entity exposes \
                 no `read` operation — the guard has no loaded record to evaluate",
                op.name
            ));
        }
    }
    ExecutionKind::ChildOp {
        action,
        feature: scope.feature.clone(),
        child_class: scope.class.clone(),
        read_back: surface_ops.iter().any(|o| o == "read"),
    }
}

/// Whether a child operation's parameter is a request-body carrier: the
/// child class itself (a domain class reference) or a DTO boundary.
fn is_body_carrier(param: &codegraph_core::types::DddParam, child_class: &str) -> bool {
    if param.type_json.get("dto").is_some() {
        return true;
    }
    let Some(kind) = param.type_json.get("type").and_then(|t| t.as_str()) else {
        return false;
    };
    if kind != "class" {
        return false;
    }
    param
        .type_json
        .get("value")
        .and_then(|v| v.get("name"))
        .and_then(|n| n.as_str())
        .is_some_and(|name| name == short_class_name(child_class))
}

/// A human-readable spelling of a parameter's declared type for a refusal
/// (`a DTO boundary` / `the class X` / a primitive).
fn body_carrier_hint(type_json: &serde_json::Value) -> String {
    if let Some(dto) = type_json.get("dto").and_then(|d| d.as_str()) {
        return format!("the DTO boundary `{dto}`");
    }
    match type_json.get("type").and_then(|t| t.as_str()) {
        Some("class") => {
            let name = type_json
                .get("value")
                .and_then(|v| v.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("<unnamed>");
            format!("the class `{name}`")
        }
        Some("primitive") => "a primitive".to_string(),
        Some(other) => format!("a `{other}`"),
        None => "an unresolved type".to_string(),
    }
}

/// The entity title an operation binds to, if any (resolution order in the
/// module doc).
async fn bind_entity(
    model: &codegraph_core::types::DddModelGraph,
    op: &codegraph_core::types::DddServiceOperation,
    db: &dyn GraphQuerier,
    type_suffix: &str,
) -> Option<String> {
    // 0. An authored transition binding names a machine on the DELEGATION
    //    target's designed class (issue #660), so that class is the entity
    //    the operation acts on — it wins over a capability binding, whose
    //    class only GATES the route. Without this an op that both binds a
    //    capability elsewhere and a transition would route to the wrong
    //    aggregate and be refused downstream.
    if op.transition.is_some()
        && let Some(target) = op.delegation_target.as_deref()
        && let Some(repo) = model.repositories.iter().find(|r| r.name == target)
        && let Some(title) = schema_title_for_class(db, &repo.design_class, type_suffix).await
    {
        return Some(title);
    }
    // 1. Capability class binding.
    if !op.capabilities.is_empty()
        && let Ok(capabilities) = db.get_capabilities().await
    {
        for capability in &op.capabilities {
            if let Some(node) = capabilities.iter().find(|c| &c.name == capability)
                && let Some(title) = schema_title_for_class(db, &node.class, type_suffix).await
            {
                return Some(title);
            }
        }
    }
    // 2. Delegation to a repository of this model.
    if let (Some(target), _) = (&op.delegation_target, &op.delegation_operation)
        && let Some(repo) = model.repositories.iter().find(|r| &r.name == target)
        && let Some(title) = schema_title_for_class(db, &repo.design_class, type_suffix).await
    {
        return Some(title);
    }
    // 3. The statement body's `=> <Repository>` receiver (issue #662): a
    //    bodied op declares no delegation, so the primary repository the
    //    body orchestrates is its entity binding.
    if !op.bodies.is_empty()
        && let Some(receiver) = op.body_receiver.as_deref()
        && let Some(repo) = model.repositories.iter().find(|r| r.name == receiver)
        && let Some(title) = schema_title_for_class(db, &repo.design_class, type_suffix).await
    {
        return Some(title);
    }
    // 3. Signature: first class-typed param, then the return type. The slot is
    //    a `DddTypeRef` (DD3) — the shared classifier reads the adjacently
    //    tagged domain `TypeRef` and reports a DTO boundary as "no domain
    //    class" rather than misreading it.
    let class_of = |json: Option<&serde_json::Value>| -> Option<String> {
        match crate::ddd::signature::signature_type_opt(json)?.as_domain()? {
            phero_ir::TypeRef::Class { package, name } if package.is_empty() => Some(name.clone()),
            phero_ir::TypeRef::Class { package, name } => Some(format!("{package}::{name}")),
            _ => None,
        }
    };
    for param in &op.params {
        if let Some(class) = class_of(Some(&param.type_json))
            && let Some(title) = schema_title_for_class(db, &class, type_suffix).await
        {
            return Some(title);
        }
    }
    if let Some(class) = class_of(op.return_type.as_ref())
        && let Some(title) = schema_title_for_class(db, &class, type_suffix).await
    {
        return Some(title);
    }
    None
}

/// The machine's transitions as wire `(from, to)` pairs, declaration order.
fn machine_transition_pairs(machine: &phero_ir::ddd::StateMachine) -> Vec<ServiceOpTransition> {
    machine
        .transitions
        .iter()
        .map(|t| ServiceOpTransition {
            from: StateName::new(t.from.clone()),
            to: StateName::new(t.to.clone()),
        })
        .collect()
}

/// The transition's trigger normalized for matching: the authored `via`, or
/// the bare-arrow derivation (the target state's lowerCamel, normalized to
/// snake_case here).
fn transition_trigger(t: &phero_ir::ddd::Transition) -> TriggerName {
    TriggerName::new(to_snake_case(t.trigger.as_deref().unwrap_or(&t.to)))
}

/// Name candidates an operation can match a trigger by: the exact op, the op
/// with the designing class's snake suffix stripped (`submit_pay_run` →
/// `submit`), and the op with the entity table suffix stripped.
fn trigger_candidates(op_name: &str, entity_class: &str, entity_table: &str) -> Vec<TriggerName> {
    let snake = to_snake_case(op_name);
    let mut candidates = vec![TriggerName::new(snake.clone())];
    for suffix in [
        to_snake_case(&short_class_name(entity_class)),
        to_snake_case(entity_table),
    ] {
        if !suffix.is_empty()
            && let Some(stripped) = snake.strip_suffix(&format!("_{suffix}"))
        {
            candidates.push(TriggerName::new(stripped));
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn short_class_name(class: &str) -> String {
    // phero-ir references are `pkg::Name` or `pkg.Name` (child scopes use the
    // dotted spelling) — compare the last segment under both qualifiers.
    let after_ns = class.rsplit("::").next().unwrap_or(class);
    after_ns.rsplit('.').next().unwrap_or(after_ns).to_string()
}

/// The state names an op-level `when` condition pins as its source: equality
/// comparisons between the machine field and a declared state (`runStatus ==
/// Draft`), OR-combined. Used when the op name does not match a trigger (the
/// `preparePayRun`/`Calculate` shape).
fn condition_source_states(
    source: &str,
    state_field_snake: &str,
    states: &[String],
) -> Vec<String> {
    let parsed = phero_expr::parse(source);
    let Some(ast) = parsed.ast.filter(|_| parsed.errors.is_empty()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect_condition_states(&ast, state_field_snake, states, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_condition_states(
    expr: &Expr,
    state_field_snake: &str,
    states: &[String],
    out: &mut Vec<String>,
) {
    match &expr.kind {
        ExprKind::Binary { op, lhs, rhs } => match op {
            BinOp::Eq => {
                let field_state = |a: &Expr, b: &Expr| -> Option<String> {
                    let ExprKind::Name(field) = &a.kind else {
                        return None;
                    };
                    if to_snake_case(field) != state_field_snake {
                        return None;
                    }
                    let ExprKind::Name(state) = &b.kind else {
                        return None;
                    };
                    states.iter().find(|s| *s == state).cloned()
                };
                if let Some(state) = field_state(lhs, rhs).or_else(|| field_state(rhs, lhs)) {
                    out.push(state);
                }
            }
            // Both `&&` and `||` union the source states: the runtime
            // current-state match is the authoritative selector.
            BinOp::And | BinOp::Or => {
                collect_condition_states(lhs, state_field_snake, states, out);
                collect_condition_states(rhs, state_field_snake, states, out);
            }
            _ => {}
        },
        ExprKind::Unary {
            op: UnOp::Not,
            expr,
        } => collect_condition_states(expr, state_field_snake, states, out),
        _ => {}
    }
}

/// The transitions a service op applies.
///
/// **Primary selector (issue #660): the authored `transition` binding.** When
/// the operation declares one, the machine's transition is resolved from the
/// binding alone — the trigger must be one the machine declares (an explicit
/// `via` name or a bare arrow's derived lowerCamel name), the optional
/// qualifier must name the bound machine's enum type, and a module carrying
/// more than one machine-bearing design requires that qualifier. Any failure
/// is a HARD generation error naming the op, the binding, and the machines
/// involved: an authored binding NEVER falls back to an inference.
///
/// **No-binding fallback** — the pre-#660 heuristics, kept ONLY because
/// consumer models still declare ops without a binding (the upstream syntax
/// postdates them): op-name → trigger (exact, then entity-suffix-stripped),
/// op-level `when` state equality → the unique outgoing transition(s), and
/// the capability-sharing arm (a `protected` internal op carrying the real
/// trigger, `preparePayRun` → `calculate`). Every op that resolves a
/// transition this way reports a WARN naming the missing binding
/// ([`transition_binding_warnings`]).
fn resolved_transitions(
    op: &codegraph_core::types::DddServiceOperation,
    model: &codegraph_core::types::DddModelGraph,
    machine: &phero_ir::ddd::StateMachine,
    entity_title: &str,
    entity_class: &str,
    entity_table: &str,
) -> Result<Vec<ServiceOpTransition>, String> {
    match &op.transition {
        Some(binding) => resolve_transition_binding(op, binding, model, machine, entity_title),
        None => Ok(heuristic_transitions(
            op,
            model,
            machine,
            entity_class,
            entity_table,
        )),
    }
}

/// The authored spelling of a transition binding for messages:
/// `SignOff`, or `ReleaseStatus.SignOff` when qualified.
fn binding_spelling(binding: &codegraph_core::types::DddTransitionRef) -> String {
    match &binding.machine {
        Some(qualifier) => format!("{qualifier}.{}", binding.trigger),
        None => binding.trigger.clone(),
    }
}

/// The machine's enum type name — the only machine qualifier an authored
/// binding can carry (Phero Lang machine declarations are field-named).
fn machine_enum_type_name(machine: &phero_ir::ddd::StateMachine) -> String {
    match &machine.enum_type {
        phero_ir::TypeRef::Enum { name, .. }
        | phero_ir::TypeRef::Class { name, .. }
        | phero_ir::TypeRef::Datatype { name, .. }
        | phero_ir::TypeRef::Interface { name, .. }
        | phero_ir::TypeRef::Vocabulary { name, .. } => name.clone(),
        phero_ir::TypeRef::Primitive(_) => String::new(),
    }
}

/// Every machine-bearing design of the entity's module, as
/// `(class, enum type)` — the ambiguity set an unqualified binding cannot
/// disambiguate (upstream E0352 requires the qualifier there).
fn module_machines(
    model: &codegraph_core::types::DddModelGraph,
    entity_title: &str,
) -> Vec<(String, String)> {
    let Some(design) = model
        .designs
        .iter()
        .find(|d| d.resolved_title.as_deref() == Some(entity_title))
    else {
        return Vec::new();
    };
    model
        .designs
        .iter()
        .filter(|d| {
            d.application == design.application
                && d.module == design.module
                && d.machine_json.is_some()
        })
        .filter_map(|d| {
            let machine: phero_ir::ddd::StateMachine =
                serde_json::from_value(d.machine_json.clone()?).ok()?;
            Some((d.class.clone(), machine_enum_type_name(&machine)))
        })
        .collect()
}

fn format_module_machines(machines: &[(String, String)]) -> String {
    machines
        .iter()
        .map(|(class, enum_type)| format!("`{class}`: `{enum_type}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The declared trigger vocabulary of a machine, in declaration order:
/// explicit `via` names and every bare arrow's derived lowerCamel name.
fn declared_triggers(machine: &phero_ir::ddd::StateMachine) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for transition in &machine.transitions {
        let name = match &transition.trigger {
            Some(trigger) => trigger.clone(),
            None => transition.to.to_lower_camel_case(),
        };
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Resolve one authored binding against the entity's machine (issue #660).
///
/// The returned transitions are EXACTLY the machine transitions the binding
/// names — one per `(from, to)` pair carrying that trigger, so a trigger
/// declared from several states still selects at runtime by the record's
/// current state, but a trigger the machine does not declare (or a
/// qualifier that names another machine) can never silently become a
/// different transition.
fn resolve_transition_binding(
    op: &codegraph_core::types::DddServiceOperation,
    binding: &codegraph_core::types::DddTransitionRef,
    model: &codegraph_core::types::DddModelGraph,
    machine: &phero_ir::ddd::StateMachine,
    entity_title: &str,
) -> Result<Vec<ServiceOpTransition>, String> {
    let machines = module_machines(model, entity_title);
    let spelling = binding_spelling(binding);
    let entity_machine = machine_enum_type_name(machine);
    match binding.machine.as_deref() {
        None if machines.len() > 1 => {
            return Err(format!(
                "service op `{}` declares `transition {spelling}`, but the module carries \
                 more than one machine-bearing design ({}); qualify the binding as \
                 `<EnumType>.<Trigger>` — the machine bound to `{entity_title}` is \
                 `{}: {entity_machine}`",
                op.name,
                format_module_machines(&machines),
                machine.field,
            ));
        }
        Some(qualifier) if qualifier != entity_machine => {
            let others = machines
                .iter()
                .filter(|(class, _)| class.as_str() != entity_title)
                .map(|(class, _)| class.as_str())
                .collect::<Vec<_>>();
            let tail = if others.is_empty() {
                String::new()
            } else {
                format!(
                    " (the module's machines are {})",
                    format_module_machines(&machines)
                )
            };
            return Err(format!(
                "service op `{}` declares `transition {spelling}`, but the machine bound to \
                 `{entity_title}` is `{}: {entity_machine}`, not `{qualifier}`{tail}",
                op.name, machine.field,
            ));
        }
        _ => {}
    }

    let mut explicit: Vec<&phero_ir::ddd::Transition> = Vec::new();
    let mut derived: Vec<&phero_ir::ddd::Transition> = Vec::new();
    for transition in &machine.transitions {
        match &transition.trigger {
            Some(trigger) if trigger == &binding.trigger => explicit.push(transition),
            None if transition.to.to_lower_camel_case() == binding.trigger => {
                derived.push(transition);
            }
            _ => {}
        }
    }
    let chosen: Vec<&phero_ir::ddd::Transition> = match (explicit.is_empty(), derived.is_empty()) {
        (false, true) => explicit,
        (true, false) => derived,
        (true, true) => {
            let declared = declared_triggers(machine);
            let vocabulary = if declared.is_empty() {
                "the machine declares no triggers".to_string()
            } else {
                format!("declared triggers: {}", declared.join(", "))
            };
            return Err(format!(
                "service op `{}` declares `transition {spelling}`, but the machine bound to \
                 `{entity_title}` (`{}: {entity_machine}`) declares no such trigger ({vocabulary})",
                op.name, machine.field,
            ));
        }
        (false, false) => {
            return Err(format!(
                "service op `{}` declares `transition {spelling}`, which is ambiguous: the \
                 machine bound to `{entity_title}` (`{}: {entity_machine}`) declares it both \
                 as an explicit `via` trigger and as a bare arrow's derived name",
                op.name, machine.field,
            ));
        }
    };
    Ok(pairs_of(&chosen))
}

fn pairs_of(transitions: &[&phero_ir::ddd::Transition]) -> Vec<ServiceOpTransition> {
    let mut pairs: Vec<ServiceOpTransition> = transitions
        .iter()
        .map(|t| ServiceOpTransition {
            from: StateName::new(t.from.clone()),
            to: StateName::new(t.to.clone()),
        })
        .collect();
    pairs.sort_by(|a, b| a.from.cmp(&b.from).then(a.to.cmp(&b.to)));
    pairs.dedup_by(|a, b| a.from == b.from && a.to == b.to);
    pairs
}

/// The pre-#660 inference (the no-binding fallback), kept ONLY for models
/// that declare no `transition` binding: op-name → trigger (exact, then
/// entity-suffix-stripped), then the op-level `when` source states, then
/// the capability-sharing `protected`-op arm. Every op that resolves a
/// transition here warns ([`transition_binding_warnings`]).
fn heuristic_transitions(
    op: &codegraph_core::types::DddServiceOperation,
    model: &codegraph_core::types::DddModelGraph,
    machine: &phero_ir::ddd::StateMachine,
    entity_class: &str,
    entity_table: &str,
) -> Vec<ServiceOpTransition> {
    let states: Vec<String> = machine.states().into_iter().map(str::to_string).collect();
    let state_field_snake = to_snake_case(&machine.field);

    let by_trigger = |op_name: &str| -> Vec<ServiceOpTransition> {
        let op_candidates = trigger_candidates(op_name, entity_class, entity_table);
        machine
            .transitions
            .iter()
            .filter(|t| op_candidates.contains(&transition_trigger(t)))
            .map(|t| ServiceOpTransition {
                from: StateName::new(t.from.clone()),
                to: StateName::new(t.to.clone()),
            })
            .collect()
    };

    let mut chosen = by_trigger(&op.name);
    if chosen.is_empty()
        && let Some(condition) = op.condition.as_deref()
    {
        let source_states = condition_source_states(condition, &state_field_snake, &states);
        chosen = machine
            .transitions
            .iter()
            .filter(|t| source_states.iter().any(|s| s == &t.from))
            .map(|t| ServiceOpTransition {
                from: StateName::new(t.from.clone()),
                to: StateName::new(t.to.clone()),
            })
            .collect();
    }
    if chosen.is_empty() {
        for other in model.services.iter().flat_map(|s| &s.operations) {
            if !other.is_protected {
                continue;
            }
            let shares_capability = other
                .capabilities
                .iter()
                .any(|c| op.capabilities.iter().any(|own| own == c));
            if !shares_capability {
                continue;
            }
            let matched = by_trigger(&other.name);
            if !matched.is_empty() {
                chosen = matched;
                break;
            }
        }
    }
    // `dedup_by` only removes adjacent duplicates; sort then dedup keeps
    // the runtime match arms stable and unique.
    chosen.sort_by(|a, b| a.from.cmp(&b.from).then(a.to.cmp(&b.to)));
    chosen.dedup_by(|a, b| a.from == b.from && a.to == b.to);
    chosen
}

/// The Rust expression producing the record's current machine state as a
/// `String` (the codelist/string comparison path).
fn current_state_expr(field: &FieldInfo) -> String {
    match (field.kind, field.optional) {
        (FieldKind::Codelist, false) => format!("response.{}.to_string()", field.rust_name),
        (FieldKind::Codelist, true) => format!(
            "response.{}.as_ref().map(|v| v.to_string()).unwrap_or_default()",
            field.rust_name
        ),
        (_, false) => format!("response.{}.clone()", field.rust_name),
        (_, true) => format!("response.{}.clone().unwrap_or_default()", field.rust_name),
    }
}

fn wire_key(rust_field: &str, casing: DtoKeyCasing) -> WireKey {
    WireKey::new(dto_wire_key(rust_field, casing))
}

/// The Update-DTO wire key of one entity field under the project's casing —
/// the key a `save`-payload op's patch writes (and the machine transition
/// arm patches).
pub(crate) fn dto_wire_key(rust_field: &str, casing: DtoKeyCasing) -> String {
    match casing {
        DtoKeyCasing::Snake => rust_field.to_string(),
        DtoKeyCasing::Camel => rust_field.to_lower_camel_case(),
    }
}

/// Whether the machine state field survives into the Update DTO. Workflow
/// status/approval fields and configured `immutable_fields` are excluded;
/// patching them through the update DTO would either 422 (`deny_unknown_
/// fields`) or be ignored, so those ops keep a seam until a dedicated
/// transition command lands.
fn state_field_is_dto_updatable(
    config: &DomainConfig,
    domain: &str,
    entity_title: &str,
    state_field_rust: &str,
) -> bool {
    let Some(ec) = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(entity_title))
    else {
        return true;
    };
    if ec
        .dto
        .immutable_fields
        .iter()
        .any(|f| f == state_field_rust)
    {
        return false;
    }
    if let Some(wf) = &ec.workflow
        && (wf.status_field == state_field_rust
            || wf.approval_status_field.as_deref() == Some(state_field_rust))
    {
        return false;
    }
    true
}

/// The render context one route needs from the project configuration and
/// the graph (issue #662: the body lowering renders command/query calls, so
/// it needs the same names the handler template has).
pub struct RouteShape<'a> {
    /// The route's domain.
    pub domain: &'a str,
    /// The bound entity's schema title.
    pub entity_title: &'a str,
    /// The bound entity's Rust type name (`PayRunType`).
    pub entity_name: &'a str,
    /// The bound entity's module name (pg table name).
    pub entity_table: &'a str,
    /// The handler's path-parameter binding (`pay_run_id`).
    pub id_param: &'a str,
    /// `Some("parent_id")` for a nested child route (the scoped record load).
    pub scoped: Option<&'a str>,
    /// The auditable flag the query/command calls carry.
    pub auditable: bool,
    /// The generated domain-types crate name (`domain_types`).
    pub domain_types_crate: &'a str,
    /// The Update-DTO wire-key casing.
    pub dto_casing: DtoKeyCasing,
    /// The entity's properties.
    pub fields: &'a HashMap<String, FieldInfo>,
    /// The design-mapped operation set (create/read/update/delete/list).
    pub surface_ops: &'a [String],
    /// Whether the route performs the record load.
    pub record_load: bool,
    /// The entity's designed machine, when any.
    pub machine: Option<&'a phero_ir::ddd::StateMachine>,
    /// The declared error vocabulary a body may raise (DD7).
    pub raises: &'a crate::ddd::body_lower::RaiseTable,
    /// The declared action-body parameters.
    pub params: &'a [crate::ddd::body_lower::BodyParam],
    /// The operation's declared `throws` set (authored error names).
    pub throws: &'a [String],
}

/// The execution lowering. A checked statement body takes precedence
/// (issue #662); otherwise repository `delete`/`findById` builtins wire to
/// the command/query planes, `save`/`update` wire to the update command with
/// an optional machine transition applied, and everything else seams.
fn seam(reason: impl Into<String>) -> ExecutionKind {
    ExecutionKind::Seam {
        reason: reason.into(),
    }
}

fn execution_for(
    op: &codegraph_core::types::DddServiceOperation,
    model: &codegraph_core::types::DddModelGraph,
    config: &DomainConfig,
    service: &str,
    shape: &RouteShape<'_>,
) -> Result<ExecutionKind, String> {
    // A checked body WINS over the delegation lowering (issue #662). The
    // upstream compiler attaches a body only to an operation that declares
    // one, so `bodies` non-empty is the author's explicit intent.
    if !op.bodies.is_empty() {
        return body_execution(op, model, config, service, shape);
    }
    let Some(target) = &op.delegation_target else {
        return Ok(seam(format!(
            "service op `{}` has no delegation target and no statement body — \
             abstract service operations have no executable lowering yet \
             (recorded follow-up)",
            op.name
        )));
    };
    let Some(target_repo) = model.repositories.iter().find(|r| &r.name == target) else {
        return Ok(seam(format!(
            "service op `{}` delegates to `{target}`, which is not a \
             repository — service-to-service and search delegation have \
             no executable lowering yet (recorded follow-up)",
            op.name
        )));
    };

    // Fail closed on cross-entity (child/aggregate) delegation. A public op
    // routing through a repository for a DIFFERENT design class would
    // otherwise patch the parent row and silently drop the child intent
    // (`addTimecardItem`, `punch`, `ingestHours`, assignment/deployment ops).
    // The diagnostic names the op, the target repository, and the target
    // class so the consumer can model a dedicated command.
    let entity_class = model
        .designs
        .iter()
        .find(|d| d.resolved_title.as_deref() == Some(shape.entity_title))
        .map(|d| d.class.as_str());
    if let Some(entity_class) = entity_class
        && target_repo.design_class != entity_class
    {
        // A binding names a transition on the DELEGATION target's machine;
        // when the route binds another aggregate that machine is not the one
        // the route can move — refuse rather than seam silently (issue
        // #660: an authored binding never degrades).
        if let Some(binding) = &op.transition {
            return Err(format!(
                "service op `{}` declares `transition {}` on repository `{target}` (class \
                 `{}`), but the operation binds entity `{}` (`{entity_class}`) — the \
                 binding's machine belongs to another aggregate",
                op.name,
                binding_spelling(binding),
                target_repo.design_class,
                shape.entity_title
            ));
        }
        return Ok(seam(format!(
            "service op `{}` delegates to repository `{}` for class `{}`, not the route \
             entity `{}` (`{entity_class}`) — a cross-aggregate delegation has no executable \
             lowering. Model the child mutation as a `children {header} {{ … => {feature}.<add|remove|save> }}` \
             block (pherolang child operations, whose ops carry `childScope`): those lower to \
             the nested child action route. Refusing to patch the parent row silently",
            op.name,
            target,
            target_repo.design_class,
            shape.entity_title,
            header = short_class_name(entity_class),
            feature = "<childFeature>",
        )));
    }

    let operation = op
        .delegation_operation
        .as_deref()
        .map(|o| o.to_ascii_lowercase())
        .unwrap_or_default();
    Ok(match operation.as_str() {
        "delete" if shape.surface_ops.iter().any(|o| o == "delete") => ExecutionKind::Delete,
        "findbyid" if shape.surface_ops.iter().any(|o| o == "read") => ExecutionKind::Read,
        "save" | "update" if shape.surface_ops.iter().any(|o| o == "update") => {
            if !shape.record_load {
                return Ok(seam(format!(
                    "service op `{}` delegates to repository `update`, which \
                     needs the loaded row, but `{}` exposes no read operation \
                     (no record load)",
                    op.name, shape.entity_title
                )));
            }
            let state = match shape.machine {
                Some(machine) => {
                    let field_snake = to_snake_case(&machine.field);
                    let entity_class = entity_class
                        .map(str::to_string)
                        .unwrap_or_else(|| shape.entity_title.to_string());
                    let Some(field) = shape
                        .fields
                        .get(&machine.field)
                        .or_else(|| shape.fields.get(&field_snake))
                    else {
                        // The machine binds a field the graph exposes no
                        // property for — a transition cannot be rendered.
                        let transitions = resolved_transitions(
                            op,
                            model,
                            machine,
                            shape.entity_title,
                            &entity_class,
                            shape.entity_table,
                        )?;
                        if transitions.is_empty() {
                            return Ok(ExecutionKind::Update { state: None });
                        }
                        return Ok(seam(format!(
                            "service op `{}` applies a machine transition, but the \
                             machine field `{}` is not a property of `{}`",
                            op.name, machine.field, shape.entity_title
                        )));
                    };
                    let transitions = resolved_transitions(
                        op,
                        model,
                        machine,
                        shape.entity_title,
                        &entity_class,
                        shape.entity_table,
                    )?;
                    if !state_field_is_dto_updatable(
                        config,
                        shape.domain,
                        shape.entity_title,
                        &field.rust_name,
                    ) {
                        // A state-only transition cannot ride the update DTO
                        // (workflow-managed/immutable field). Fail closed
                        // rather than silently no-op the action.
                        if !transitions.is_empty() {
                            return Ok(seam(format!(
                                "service op `{}` applies a machine transition on \
                                 `{}`, but the field is workflow-managed/immutable \
                                 and excluded from the update DTO (dedicated \
                                 transition lowering pending)",
                                op.name, field.rust_name
                            )));
                        }
                        None
                    } else {
                        Some(ServiceOpState {
                            current_expr: current_state_expr(field),
                            wire_key: wire_key(&field.rust_name, shape.dto_casing),
                            initial: StateName::new(machine.initial.clone()),
                            all: machine_transition_pairs(machine),
                            op: transitions,
                        })
                    }
                }
                None => {
                    // An authored binding on an entity with no machine has
                    // nothing to fire — never degrade it to a plain update
                    // (issue #660).
                    if let Some(binding) = &op.transition {
                        return Err(format!(
                            "service op `{}` declares `transition {}`, but the bound entity \
                             `{}` carries no lifecycle machine — the binding has nothing to fire",
                            op.name,
                            binding_spelling(binding),
                            shape.entity_title
                        ));
                    }
                    None
                }
            };
            ExecutionKind::Update { state }
        }
        other => seam(format!(
            "service op `{}` delegates to repository `{other}`, which has \
             no executable lowering yet (recorded follow-up)",
            op.name
        )),
    })
}

/// Lower a checked `.ddd` statement body (issue #662).
///
/// Every refusal is a HARD generation error naming the op and the construct
/// (the DD9 `query_refusals` posture): the body plane never emits a partially
/// interpreted handler, and never silently answers 501 for a body it cannot
/// honour.
fn body_execution(
    op: &codegraph_core::types::DddServiceOperation,
    model: &codegraph_core::types::DddModelGraph,
    config: &DomainConfig,
    service: &str,
    shape: &RouteShape<'_>,
) -> Result<ExecutionKind, String> {
    let Some(json) = op.bodies.get("stmt") else {
        let targets: Vec<&str> = op.bodies.keys().map(String::as_str).collect();
        return Err(format!(
            "service op `{}`: the operation carries body target(s) `{}`, but the \
             action plane lowers the `stmt` target only",
            op.name,
            targets.join("`, `")
        ));
    };
    let body = crate::ddd::body_lower::parse_body(&op.name, json).map_err(|refusal| refusal.0)?;

    // The body orchestrates ONE aggregate: the route's own. A receiver for
    // another design class is the cross-aggregate load the upstream checker
    // refuses, re-checked here against the graph (a hand-built or stale
    // artifact cannot smuggle one past the plane).
    let Some(receiver) = op.body_receiver.as_deref() else {
        return Err(format!(
            "service op `{}`: the operation carries a statement body but no \
             `body_receiver` (the `=> <Repository>` the body binds as `self`); \
             regenerate the graph",
            op.name
        ));
    };
    let entity_class = model
        .designs
        .iter()
        .find(|d| d.resolved_title.as_deref() == Some(shape.entity_title))
        .map(|d| d.class.as_str());
    if let Some(entity_class) = entity_class {
        match model.repositories.iter().find(|r| r.name == receiver) {
            Some(repository) if repository.design_class == entity_class => {}
            Some(repository) => {
                return Err(format!(
                    "service op `{}`: the body orchestrates repository `{receiver}` \
                     for class `{}`, not the route entity `{}` (`{entity_class}`) — \
                     a body loads one aggregate, the one the action addresses",
                    op.name, repository.design_class, shape.entity_title
                ));
            }
            None => {
                return Err(format!(
                    "service op `{}`: the body receiver `{receiver}` is not a \
                     repository of the model — the body has no aggregate to \
                     orchestrate",
                    op.name
                ));
            }
        }
    }

    // The inferred machine transition survives only when the body leaves the
    // state field alone (the lowering drops it otherwise — the TODO(#662)
    // boundary). No transitions selected ⇒ no patch at all: unlike the
    // deployment `save` leg, a body op never gets the body-driven target
    // whitelist (an action body must not move the machine arbitrarily).
    let transition = match shape.machine {
        Some(machine) => body_transition_state(op, model, config, machine, shape)?,
        None => {
            if let Some(binding) = &op.transition {
                return Err(format!(
                    "service op `{}` declares `transition {}`, but the bound entity `{}` \
                     carries no lifecycle machine — the binding has nothing to fire",
                    op.name,
                    binding_spelling(binding),
                    shape.entity_title
                ));
            }
            None
        }
    };
    // The declared `throws` vocabulary (authored names from the phero-ir
    // `ErrorRef` JSON) — a raise outside it is a refusal, even though the
    // upstream compiler already proves it.
    let throws: Vec<String> = op
        .throws
        .iter()
        .filter_map(|json| json.get("name").and_then(serde_json::Value::as_str))
        .map(str::to_string)
        .collect();
    let scope = crate::ddd::body_lower::BodyScope {
        op_name: &op.name,
        service,
        domain: shape.domain,
        entity_name: shape.entity_name,
        module_name: shape.entity_table,
        id_param: shape.id_param,
        scoped: shape.scoped,
        auditable: shape.auditable,
        domain_types_crate: shape.domain_types_crate,
        dto_casing: shape.dto_casing,
        fields: shape.fields,
        state_field: shape
            .machine
            .and_then(|machine| shape.fields.get(&machine.field))
            .map(|field| field.rust_name.as_str()),
        transition: transition.as_ref(),
        raises: shape.raises,
        params: shape.params,
        throws: &throws,
        receiver: Some(receiver),
        has_update: shape.surface_ops.iter().any(|o| o == "update"),
        record_load: shape.record_load,
    };
    let plan = crate::ddd::body_lower::lower_body(&scope, &body).map_err(|refusal| refusal.0)?;
    Ok(ExecutionKind::Body {
        plan: Box::new(plan),
    })
}

/// The machine transition a bodied op applies when its body leaves the state
/// field alone. `None` when the machine field is not a property, the field is
/// workflow-managed/immutable, or the op matches no declared transition (a
/// bodied op gets NO body-driven target whitelist). An authored binding that
/// cannot be resolved is a HARD error, never `None` (issue #660: a binding
/// never degrades silently).
fn body_transition_state(
    op: &codegraph_core::types::DddServiceOperation,
    model: &codegraph_core::types::DddModelGraph,
    config: &DomainConfig,
    machine: &phero_ir::ddd::StateMachine,
    shape: &RouteShape<'_>,
) -> Result<Option<ServiceOpState>, String> {
    let field_snake = to_snake_case(&machine.field);
    let Some(field) = shape
        .fields
        .get(&machine.field)
        .or_else(|| shape.fields.get(&field_snake))
    else {
        if let Some(binding) = &op.transition {
            return Err(format!(
                "service op `{}` declares `transition {}`, but the machine field `{}` is \
                 not a property of `{}` — the binding cannot be rendered",
                op.name,
                binding_spelling(binding),
                machine.field,
                shape.entity_title
            ));
        }
        return Ok(None);
    };
    if !state_field_is_dto_updatable(config, shape.domain, shape.entity_title, &field.rust_name) {
        if let Some(binding) = &op.transition {
            return Err(format!(
                "service op `{}` declares `transition {}`, but the machine field `{}` is \
                 workflow-managed/immutable and excluded from the update DTO — the binding \
                 cannot be rendered",
                op.name,
                binding_spelling(binding),
                field.rust_name
            ));
        }
        return Ok(None);
    }
    let entity_class = model
        .designs
        .iter()
        .find(|d| d.resolved_title.as_deref() == Some(shape.entity_title))
        .map(|d| d.class.clone())
        .unwrap_or_else(|| shape.entity_title.to_string());
    let transitions = resolved_transitions(
        op,
        model,
        machine,
        shape.entity_title,
        &entity_class,
        shape.entity_table,
    )?;
    if transitions.is_empty() {
        return Ok(None);
    }
    Ok(Some(ServiceOpState {
        current_expr: current_state_expr(field),
        wire_key: wire_key(&field.rust_name, shape.dto_casing),
        initial: StateName::new(machine.initial.clone()),
        all: machine_transition_pairs(machine),
        op: transitions,
    }))
}

/// The IFML dead-trigger scan (Ask 1): every `.ifml` action whose name
/// binds (ASCII-case-insensitive, the pherolang `check_action_bindings`
/// convention) a service op that emits no API surface — `protected`, or
/// entity-unbound — is a dead button. Returns `(action, op, reason)`.
pub async fn ifml_dead_triggers(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<(String, String, String)>, GraphError> {
    let mut triggers = Vec::new();
    let Ok(actions) = db.get_ifml_actions().await else {
        return Ok(triggers);
    };
    if actions.is_empty() {
        return Ok(triggers);
    }
    let models = db.get_ddd_models().await?;
    if models.iter().all(|m| m.services.is_empty()) {
        return Ok(triggers);
    }
    let type_suffix = config.defaults.type_suffix.clone();
    for action in &actions {
        let Some((service, op)) = models.iter().find_map(|model| {
            for service in &model.services {
                for op in &service.operations {
                    if op.name.eq_ignore_ascii_case(&action.name) {
                        return Some((service.name.clone(), op.clone()));
                    }
                }
            }
            None
        }) else {
            // Binds no service op at all: the upstream binding check owns
            // that class of defect.
            continue;
        };
        if op.is_protected {
            triggers.push((
                action.name.clone(),
                op.name.clone(),
                "the op is `protected` (Sculptor visibility)".to_string(),
            ));
            continue;
        }
        let bound = bind_entity_service_scoped(&models, &service, &op, db, &type_suffix).await;
        if bound.is_none() {
            triggers.push((
                action.name.clone(),
                op.name.clone(),
                "the op binds no entity (no capability class, repository                  delegation, or class-typed signature)"
                    .to_string(),
            ));
        }
    }
    Ok(triggers)
}

/// [`bind_entity`] scoped to one already-found service.
async fn bind_entity_service_scoped(
    models: &[codegraph_core::types::DddModelGraph],
    service_name: &str,
    op: &codegraph_core::types::DddServiceOperation,
    db: &dyn GraphQuerier,
    type_suffix: &str,
) -> Option<String> {
    let model = models
        .iter()
        .find(|m| m.services.iter().any(|s| s.name == service_name))?;
    bind_entity(model, op, db, type_suffix).await
}

/// Whether an operation consults the machine at all: a `save`/`update`
/// delegation (the shapes that patch the loaded row) or a statement body
/// (whose inferred transition rides the same lowering). Everything else —
/// delete/read delegations, abstract ops, seams — applies no transition, so
/// a missing binding on those is not a diagnostic.
fn consults_the_machine(op: &codegraph_core::types::DddServiceOperation) -> bool {
    if !op.bodies.is_empty() {
        return true;
    }
    matches!(
        op.delegation_operation
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("save") | Some("update")
    )
}

/// Authored transition bindings that cannot be resolved (issue #660): the
/// HARD half of the binding diagnostics.
///
/// Upstream already validates a binding against the target repository's
/// designed machine, so on a freshly compiled artifact this list is empty —
/// it exists for the hand-built/stale-graph class (the same posture as
/// [`body_refusals`] and DD9's `query_refusals`), where a binding would
/// otherwise be resolved against the wrong machine, or silently dropped
/// after the tree was half-written. The run path turns each entry into a
/// `ValidationPass` ERROR before generation.
///
/// Each entry names the op, the binding as authored, and the machines
/// involved. Empty = every binding resolves.
pub async fn transition_binding_errors(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<String>, GraphError> {
    let models = db.get_ddd_models().await?;
    if models.iter().all(|model| {
        model
            .services
            .iter()
            .all(|service| service.operations.iter().all(|op| op.transition.is_none()))
    }) {
        return Ok(Vec::new());
    }
    let surface = match DddDesignSurface::from_graph(db).await {
        Ok(surface) => surface,
        Err(_) => return Ok(Vec::new()),
    };
    let type_suffix = config.defaults.type_suffix.clone();
    let mut errors = Vec::new();
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                let Some(binding) = &op.transition else {
                    continue;
                };
                if op.is_protected || !consults_the_machine(op) {
                    continue;
                }
                let Some(title) = bind_entity(model, op, db, &type_suffix).await else {
                    // Unbound ops emit no route at all; the dead-surface
                    // warning owns that class.
                    continue;
                };
                let Some(machine) = surface.machine_for(&title) else {
                    errors.push(format!(
                        "service op `{}::{}` declares `transition {}`, but the bound entity \
                         `{title}` carries no lifecycle machine — the binding has nothing to fire",
                        service.name,
                        op.name,
                        binding_spelling(binding)
                    ));
                    continue;
                };
                if let Err(reason) =
                    resolve_transition_binding(op, binding, model, &machine, &title)
                {
                    // The resolver does not know the owning service; name
                    // it the same way the no-machine branch does.
                    errors.push(reason.replacen(
                        &format!("service op `{}`", op.name),
                        &format!("service op `{}::{}`", service.name, op.name),
                        1,
                    ));
                }
            }
        }
    }
    errors.sort();
    errors.dedup();
    Ok(errors)
}

/// One WARN per operation whose machine transition is INFERRED (issue
/// #660): the no-binding fallback stays for models that predate the
/// `transition` syntax, but every op it resolves must name the binding it
/// lacks, so the inference is visible in the run log instead of silently
/// re-selecting a transition after a rename.
///
/// Empty when every machine-bound op declares its binding (and when the
/// graph carries no machines).
pub async fn transition_binding_warnings(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<String>, GraphError> {
    let models = db.get_ddd_models().await?;
    // No machine ⇒ nothing can be inferred, so the walk (which resolves the
    // entity binding per op) is skipped entirely: the common model pays no
    // graph reads here.
    if !models
        .iter()
        .any(|model| model.designs.iter().any(|d| d.machine_json.is_some()))
    {
        return Ok(Vec::new());
    }
    let surface = match DddDesignSurface::from_graph(db).await {
        Ok(surface) => surface,
        Err(_) => return Ok(Vec::new()),
    };
    let type_suffix = config.defaults.type_suffix.clone();
    let mut warnings = Vec::new();
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                if op.transition.is_some() || op.is_protected || !consults_the_machine(op) {
                    continue;
                }
                let Some(title) = bind_entity(model, op, db, &type_suffix).await else {
                    continue;
                };
                let Some(machine) = surface.machine_for(&title) else {
                    continue;
                };
                let entity_class = model
                    .designs
                    .iter()
                    .find(|d| d.resolved_title.as_deref() == Some(title.as_str()))
                    .map(|d| d.class.clone())
                    .unwrap_or_else(|| title.clone());
                let table = db
                    .get_schema(&title)
                    .await
                    .ok()
                    .flatten()
                    .map(|s| s.pg_table_name)
                    .unwrap_or_else(|| to_snake_case(&title));
                let inferred = heuristic_transitions(op, model, &machine, &entity_class, &table);
                if inferred.is_empty() {
                    continue;
                }
                let pairs = inferred
                    .iter()
                    .map(|t| format!("{} -> {}", t.from, t.to))
                    .collect::<Vec<_>>()
                    .join(", ");
                warnings.push(format!(
                    "service op `{}::{}` applies the machine transition {pairs} of `{title}` \
                     by inference: the operation declares no `transition` binding; add \
                     `transition <Trigger>` (or `transition <EnumType>.<Trigger>` in a \
                     multi-machine module) to pin it",
                    service.name, op.name,
                ));
            }
        }
    }
    warnings.sort();
    warnings.dedup();
    Ok(warnings)
}

/// Every statically-unlowerable statement body (issue #662): the DD9
/// `query_refusals` posture, one plane over. A body construct outside the
/// documented subset is a HARD generation error — the run path turns these
/// into `ValidationPass` errors BEFORE any tree is written, because the
/// domain-generator phase collects its own errors non-fatally (a refusal
/// surfacing there would half-generate).
///
/// Each entry names the service op and the construct. Empty = every bodied
/// op lowers.
pub async fn body_refusals(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<String>, GraphError> {
    let models = db.get_ddd_models().await?;
    if models.iter().all(|model| {
        model
            .services
            .iter()
            .all(|service| service.operations.iter().all(|op| op.bodies.is_empty()))
    }) {
        return Ok(Vec::new());
    }
    let type_suffix = config.defaults.type_suffix.clone();
    let mut titles: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                if op.bodies.is_empty() || op.is_protected {
                    continue;
                }
                if let Some(title) = bind_entity(model, op, db, &type_suffix).await {
                    titles.insert(title);
                }
            }
        }
    }
    let mut refusals = Vec::new();
    for title in titles {
        let Ok(Some(schema)) = db.get_schema(&title).await else {
            continue;
        };
        let Some(domain) = schema.domain.clone() else {
            continue;
        };
        if !config.domains.contains_key(domain.as_str()) {
            continue;
        }
        if let Err(reason) =
            routes_for_entity(db, config, &domain, &title, &BodyRenderContext::default()).await
        {
            refusals.push(reason);
        }
    }
    refusals.sort();
    refusals.dedup();
    Ok(refusals)
}

/// Injected-scope `when` guards (issue #663): every guarded operation's
/// injected paths must resolve (schema, domain, query slot, exactly-one
/// FK id source) and the whole condition must lower. The same refusal
/// class the generation path raises — this pre-generation pass is what
/// makes it fail BEFORE any tree is written (the `body_refusals`
/// precedent).
pub async fn guard_refusals(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    workers_topology: bool,
) -> Result<Vec<String>, GraphError> {
    let models = db.get_ddd_models().await?;
    if models.iter().all(|model| {
        model.services.iter().all(|service| {
            service.operations.iter().all(|op| {
                !op.condition_paths.iter().any(|path| {
                    matches!(
                        path.scope,
                        codegraph_core::types::DddConditionScope::Injected { .. }
                    )
                })
            })
        })
    }) {
        return Ok(Vec::new());
    }
    let type_suffix = config.defaults.type_suffix.clone();
    let mut titles: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                let guarded = op.condition_paths.iter().any(|path| {
                    matches!(
                        path.scope,
                        codegraph_core::types::DddConditionScope::Injected { .. }
                    )
                });
                if !guarded || op.is_protected {
                    continue;
                }
                if let Some(title) = bind_entity(model, op, db, &type_suffix).await {
                    titles.insert(title);
                }
            }
        }
    }
    let mut refusals = Vec::new();
    for title in titles {
        let Ok(Some(schema)) = db.get_schema(&title).await else {
            continue;
        };
        let Some(domain) = schema.domain.clone() else {
            continue;
        };
        if !config.domains.contains_key(domain.as_str()) {
            continue;
        }
        let body = BodyRenderContext {
            workers_topology,
            ..Default::default()
        };
        if let Err(reason) = routes_for_entity(db, config, &domain, &title, &body).await {
            refusals.push(reason);
        }
    }
    refusals.sort();
    refusals.dedup();
    Ok(refusals)
}

/// The Ask 1 diagnostic: service operations that emit no API surface —
/// `protected` ops and ops that bind no entity. Returned strings are
/// human-readable warnings (empty = clean).
pub async fn dead_surface_warnings(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<String>, GraphError> {
    let models = db.get_ddd_models().await?;
    let mut warnings = Vec::new();
    let type_suffix = config.defaults.type_suffix.clone();
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                if op.is_protected {
                    warnings.push(format!(
                        "service op `{}::{}` is `protected` — it emits no API \
                         surface (Sculptor visibility); IFML actions bound to \
                         it are dead triggers",
                        service.name, op.name
                    ));
                    continue;
                }
                if bind_entity(model, op, db, &type_suffix).await.is_none() {
                    warnings.push(format!(
                        "service op `{}::{}` binds no entity (no capability \
                         class, repository delegation, or class-typed \
                         signature) — it emits no API surface",
                        service.name, op.name
                    ));
                }
            }
        }
        // Aggregate child operations (issue #661): a `children <Class>` op
        // with no mounted route is dead surface too — `protected` ops and
        // ops whose child class is not a schema in the graph (the nesting /
        // lowering misses warn at generation time, where the child's plane
        // is known).
        for design in &model.designs {
            for op in &design.child_operations {
                if op.is_protected {
                    warnings.push(format!(
                        "child operation `{}::{}` is `protected` — it emits no API \
                         surface (Sculptor visibility); IFML actions bound to it \
                         are dead triggers",
                        design.class, op.name
                    ));
                    continue;
                }
                if let Some(scope) = &op.child_scope
                    && schema_title_for_class(db, &scope.class, &type_suffix)
                        .await
                        .is_none()
                {
                    warnings.push(format!(
                        "child operation `{}::{}` names child class `{}`, which \
                         resolves to no ingested schema — it emits no API surface",
                        design.class, op.name, scope.class
                    ));
                }
            }
        }
    }
    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One `execution_for` call with the test fixture's route shape.
    fn lower_exec(
        model: &codegraph_core::types::DddModelGraph,
        config: &DomainConfig,
        surface_ops: &[String],
        op: &codegraph_core::types::DddServiceOperation,
        machine: Option<&phero_ir::ddd::StateMachine>,
        fields: &HashMap<String, FieldInfo>,
    ) -> ExecutionKind {
        try_lower_exec(model, config, surface_ops, op, machine, fields).expect("lowering")
    }

    fn try_lower_exec(
        model: &codegraph_core::types::DddModelGraph,
        config: &DomainConfig,
        surface_ops: &[String],
        op: &codegraph_core::types::DddServiceOperation,
        machine: Option<&phero_ir::ddd::StateMachine>,
        fields: &HashMap<String, FieldInfo>,
    ) -> Result<ExecutionKind, String> {
        let raises = crate::ddd::body_lower::RaiseTable::default();
        let shape = RouteShape {
            domain: "payroll",
            entity_title: "PayRunType",
            entity_name: "PayRunType",
            entity_table: "pay_run",
            id_param: "pay_run_id",
            scoped: None,
            auditable: false,
            domain_types_crate: "domain_types",
            dto_casing: crate::project_config::DtoKeyCasing::Snake,
            fields,
            surface_ops,
            record_load: true,
            machine,
            raises: &raises,
            params: &[],
            throws: &[],
        };
        execution_for(op, model, config, "PayRunService", &shape)
    }

    fn fields(pairs: &[(&str, FieldKind, bool)]) -> HashMap<String, FieldInfo> {
        let mut map = HashMap::new();
        for (name, kind, optional) in pairs {
            // DTO fields are snake_case; the authored (often camelCase)
            // name keys the lookup, mirroring `fields_for`.
            let rust_name = to_snake_case(name);
            let info = FieldInfo {
                rust_name,
                kind: *kind,
                optional: *optional,
            };
            map.insert(name.to_string(), info);
        }
        map
    }

    fn lower(source: &str, fields: &HashMap<String, FieldInfo>) -> Result<String, String> {
        lower_when_to_rust(source, "ApprovePayRun", "PayRunType", fields)
    }

    #[test]
    fn string_equality_lowering() {
        let f = fields(&[("status", FieldKind::String, false)]);
        assert_eq!(
            lower(r#"status == "AwaitingApproval""#, &f).unwrap(),
            r#"(response.status == "AwaitingApproval")"#
        );
    }

    #[test]
    fn optional_string_equality_lowering() {
        let f = fields(&[("status", FieldKind::String, true)]);
        assert_eq!(
            lower(r#"status != "Posted""#, &f).unwrap(),
            r#"(response.status.as_deref() != Some("Posted"))"#
        );
    }

    #[test]
    fn codelist_and_date_compare_via_display() {
        let f = fields(&[
            ("status", FieldKind::Codelist, false),
            ("filedOn", FieldKind::Date, true),
        ]);
        assert_eq!(
            lower(r#"status == "AwaitingApproval""#, &f).unwrap(),
            r#"(response.status.to_string() == "AwaitingApproval")"#
        );
        assert_eq!(
            lower(r#"filedOn == date("2026-01-15")"#, &f).unwrap(),
            r#"(response.filed_on.as_ref().map(|v| v.to_string()).as_deref() == Some("2026-01-15"))"#
        );
    }

    #[test]
    fn numeric_and_bool_orderings() {
        let f = fields(&[
            ("amount", FieldKind::Float, false),
            ("count", FieldKind::Integer, true),
            ("active", FieldKind::Bool, false),
        ]);
        assert_eq!(
            lower("amount >= 0.0", &f).unwrap(),
            "(response.amount >= 0.0)"
        );
        assert_eq!(
            lower("count < 5", &f).unwrap(),
            "(response.count.is_some_and(|v| v < 5))"
        );
        assert_eq!(
            lower("active == true", &f).unwrap(),
            "(response.active == true)"
        );
    }

    #[test]
    fn logical_and_null_forms() {
        let f = fields(&[
            ("status", FieldKind::String, false),
            ("note", FieldKind::String, true),
            ("amount", FieldKind::Float, false),
        ]);
        assert_eq!(
            lower(
                r#"status == "Draft" && amount > 0.0 && !(note == null)"#,
                &f
            )
            .unwrap(),
            "(((response.status == \"Draft\") && (response.amount > 0.0)) && (!((response.note.is_none()))))"
        );
        assert_eq!(
            lower("note != null", &f).unwrap(),
            "(!(response.note.is_none()))"
        );
        assert_eq!(
            lower("status == null", &f).unwrap(),
            "false" // required field: vacuously false
        );
    }

    #[test]
    fn refusals_name_the_capability() {
        let f = fields(&[("amount", FieldKind::Float, false)]);
        let err = lower("amount + 1.0 > 2.0", &f).unwrap_err();
        assert!(err.contains("capability `ApprovePayRun`"), "{err}");
        assert!(err.contains("arithmetic"), "{err}");
        let err = lower("total == 1.0", &f).unwrap_err();
        assert!(err.contains("is not a field of `PayRunType`"), "{err}");
        let err = lower("customer.name == \"x\"", &f).unwrap_err();
        assert!(err.contains("field access"), "{err}");
        let err = lower("date(\"15/01/2026\") == \"x\"", &f).unwrap_err();
        assert!(err.contains("not a calendar date"), "{err}");
    }

    #[test]
    fn literal_flips_ordering_to_keep_field_on_the_left() {
        let f = fields(&[("amount", FieldKind::Float, false)]);
        assert_eq!(
            lower("0.0 < amount", &f).unwrap(),
            "(response.amount > 0.0)"
        );
    }

    fn policy_fixture() -> PolicySurface {
        let mut roles = BTreeMap::new();
        // owner holds ApprovePayRun when (status == "AwaitingApproval")
        let mut owner = BTreeMap::new();
        owner.insert(
            "ApprovePayRun".to_string(),
            vec![Some(r#"status == "AwaitingApproval""#.to_string())],
        );
        roles.insert("owner".to_string(), owner);
        // manager holds both sides of the never_both pair
        let mut manager = BTreeMap::new();
        manager.insert("ApprovePayRun".to_string(), vec![None]);
        manager.insert("PreparePayRun".to_string(), vec![None]);
        roles.insert("manager".to_string(), manager);
        // member holds nothing relevant
        roles.insert("member".to_string(), BTreeMap::new());
        PolicySurface {
            roles,
            never_both: vec![vec![
                "PreparePayRun".to_string(),
                "ApprovePayRun".to_string(),
            ]],
        }
    }

    #[test]
    fn authorize_fn_encodes_capability_sod_and_when() {
        let f = fields(&[("status", FieldKind::String, false)]);
        let surface = policy_fixture();
        let rendered = render_authorize_fn(
            "approve",
            "approve",
            "PayRunService",
            &["ApprovePayRun".to_string()],
            None,
            "crate::domain::payroll::pay_run::dto_response::PayRunResponse",
            Some(&surface),
            &f,
            "PayRunType",
            true,
        )
        .unwrap()
        .expect("policy surface exists");
        // api_key bypass
        assert!(rendered.contains(r#"if role == "api_key""#), "{rendered}");
        // owner arm carries the when guard over the record
        assert!(
            rendered.contains(r#"if !((response.status == "AwaitingApproval"))"#),
            "{rendered}"
        );
        // manager arm is denied by separation of duties
        assert!(
            rendered.contains("separation of duties: role `manager` holds `PreparePayRun`"),
            "{rendered}"
        );
        // member falls to the deny-all arm
        assert!(
            rendered.contains("capability `ApprovePayRun` required"),
            "{rendered}"
        );
    }

    #[test]
    fn authorize_fn_emits_the_op_level_transition_guard() {
        let f = fields(&[("status", FieldKind::String, false)]);
        let surface = policy_fixture();
        let rendered = render_authorize_fn(
            "approve",
            "approve",
            "PayRunService",
            &["ApprovePayRun".to_string()],
            Some(r#"status == "AwaitingApproval""#),
            "crate::domain::payroll::pay_run::dto_response::PayRunResponse",
            Some(&surface),
            &f,
            "PayRunType",
            true,
        )
        .unwrap()
        .expect("policy surface exists");
        // The transition guard is business semantics: it sits BEFORE the
        // api_key bypass and applies to every session.
        let api_key_pos = rendered.find(r#"if role == "api_key""#).unwrap();
        let cond_pos = rendered
            .find(r#"if !((response.status == "AwaitingApproval"))"#)
            .unwrap();
        assert!(cond_pos < api_key_pos, "{rendered}");
        assert!(rendered.contains("`when` condition not met"), "{rendered}");
    }

    #[test]
    fn condition_only_op_emits_a_guard_without_a_policy() {
        let f = fields(&[("status", FieldKind::String, false)]);
        let rendered = render_authorize_fn(
            "submit",
            "submit",
            "TimeService",
            &[],
            Some(r#"status == "Draft""#),
            "crate::domain::time::timecard::dto_response::TimecardResponse",
            None,
            &f,
            "TimecardType",
            true,
        )
        .unwrap()
        .expect("a condition alone is enough to gate");
        assert!(
            rendered.contains(r#"if !((response.status == "Draft"))"#),
            "{rendered}"
        );
        // No role arms without a policy: everything but api_key denies.
        assert!(rendered.contains("match role"), "{rendered}");
        assert!(
            rendered.contains("capability ` ` required") || rendered.contains("required"),
            "{rendered}"
        );
    }

    #[test]
    fn authorize_fn_none_without_capabilities() {
        let surface = policy_fixture();
        let rendered = render_authorize_fn(
            "prepare",
            "prepare",
            "PayRunService",
            &[],
            None,
            "crate::domain::payroll::pay_run::dto_response::PayRunResponse",
            Some(&surface),
            &HashMap::new(),
            "PayRunType",
            true,
        )
        .unwrap();
        assert!(rendered.is_none());
    }

    // ── Non-root record loads (the #595 follow-up; crewbase#71) ──────────

    #[test]
    fn no_record_load_renders_a_response_less_gate() {
        let f = fields(&[("status", FieldKind::String, false)]);
        let surface = policy_fixture();
        // `manager`'s permit is unconditional (`vec![None]`), so the gate
        // is capability membership + sod only — no record needed.
        let rendered = render_authorize_fn(
            "issue",
            "issue",
            "PaySlipService",
            &["IssuePaySlip".to_string()],
            None,
            "crate::domain::payroll::pay_slip::dto_response::PaySlipResponse",
            Some(&surface),
            &f,
            "PaySlipType",
            false,
        )
        .unwrap()
        .expect("membership gate renders without a record");
        // The response-less signature: no `response` parameter.
        assert!(!rendered.contains("response: &"), "{rendered}");
        // Still carries the deny-all membership arm.
        assert!(
            rendered.contains("capability `IssuePaySlip` required"),
            "{rendered}"
        );
    }

    #[test]
    fn when_condition_without_a_record_is_a_generation_error() {
        let f = fields(&[("status", FieldKind::String, false)]);
        let surface = policy_fixture();
        // owner's permit carries `when (status == ...)` — unrenderable
        // without a record load; fail closed at generation time.
        let err = render_authorize_fn(
            "approve",
            "approve",
            "PaySlipService",
            &["ApprovePayRun".to_string()],
            None,
            "crate::domain::payroll::pay_slip::dto_response::PaySlipResponse",
            Some(&surface),
            &f,
            "PaySlipType",
            false,
        )
        .unwrap_err();
        assert!(err.contains("`when` conditions"), "{err}");
        assert!(err.contains("no record load"), "{err}");
    }

    #[test]
    fn sod_conflicts_are_deterministic_and_deduped() {
        let surface = policy_fixture();
        assert_eq!(
            surface.sod_conflicts(&["ApprovePayRun".to_string()]),
            vec!["PreparePayRun".to_string()]
        );
        assert!(surface.sod_conflicts(&["Unrelated".to_string()]).is_empty());
    }

    #[test]
    fn basejump_role_mapping_mirrors_the_hydration_convention() {
        let actors = vec![
            "PayrollManager".to_string(),
            "Owner".to_string(),
            "Member".to_string(),
        ];
        assert_eq!(
            basejump_role_actors("owner", &actors),
            vec!["Owner".to_string()]
        );
        assert_eq!(
            basejump_role_actors("member", &actors),
            vec!["Member".to_string()]
        );
        // No actor named Manager: the role maps to nobody.
        assert_eq!(
            basejump_role_actors("manager", &actors),
            Vec::<String>::new()
        );
        // Unnamed actors: owner falls to first, member to last.
        let anon = vec!["Auditor".to_string(), "Clerk".to_string()];
        assert_eq!(
            basejump_role_actors("owner", &anon),
            vec!["Auditor".to_string()]
        );
        assert_eq!(
            basejump_role_actors("member", &anon),
            vec!["Clerk".to_string()]
        );
    }

    fn empty_config() -> DomainConfig {
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

    #[test]
    fn execution_maps_repository_builtins_and_save() {
        let model = codegraph_core::types::DddModelGraph {
            source_path: "test.ddd".to_string(),
            application: codegraph_core::types::DddApplicationNode {
                name: "App".to_string(),
                base: None,
                source_path: "test.ddd".to_string(),
            },
            modules: Vec::new(),
            designs: Vec::new(),
            repositories: vec![codegraph_core::types::DddRepositoryNode {
                application: "App".to_string(),
                name: "PayRunRepository".to_string(),
                design_class: "PayRun".to_string(),
                operations: Vec::new(),
            }],
            services: Vec::new(),
            searches: Vec::new(),
        };
        let ops = [
            "delete".to_string(),
            "read".to_string(),
            "update".to_string(),
        ];
        let config = empty_config();
        let op = |target: Option<&str>, operation: Option<&str>| {
            codegraph_core::types::DddServiceOperation {
                name: "op".to_string(),
                return_type: None,
                return_multiplicity: None,
                params: Vec::new(),
                delegation_target: target.map(|t| t.to_string()),
                delegation_operation: operation.map(|o| o.to_string()),
                capabilities: Vec::new(),
                is_protected: false,
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
        };
        let lower = |op: &codegraph_core::types::DddServiceOperation, machine| {
            lower_exec(&model, &config, &ops, op, machine, &HashMap::new())
        };
        let delete = lower(&op(Some("PayRunRepository"), Some("delete")), None);
        assert!(matches!(delete, ExecutionKind::Delete));
        let read = lower(&op(Some("PayRunRepository"), Some("findById")), None);
        assert!(matches!(read, ExecutionKind::Read));
        // save delegates through the update command (P0)
        match lower(&op(Some("PayRunRepository"), Some("save")), None) {
            ExecutionKind::Update { state } => {
                assert!(state.is_none(), "no machine ⇒ no transition binding")
            }
            other => panic!("expected update lowering, got {other:?}"),
        }
        // non-repository delegation seams
        match lower(&op(Some("NotificationService"), Some("notify")), None) {
            ExecutionKind::Seam { reason } => assert!(reason.contains("not a repository")),
            other => panic!("expected seam, got {other:?}"),
        }
        // abstract op seams
        match lower(&op(None, None), None) {
            ExecutionKind::Seam { reason } => assert!(reason.contains("no delegation target")),
            other => panic!("expected seam, got {other:?}"),
        }
    }

    fn pay_run_machine() -> phero_ir::ddd::StateMachine {
        let transition = |from: &str, to: &str, trigger: &str| phero_ir::ddd::Transition {
            from: from.to_string(),
            to: to.to_string(),
            trigger: Some(trigger.to_string()),
            guard: None,
        };
        phero_ir::ddd::StateMachine {
            field: "runStatus".to_string(),
            enum_type: phero_ir::TypeRef::Enum {
                package: "payroll".to_string(),
                name: "PayRunStatus".to_string(),
            },
            initial: "Draft".to_string(),
            terminals: vec!["Reversed".to_string()],
            transitions: vec![
                transition("Draft", "Calculating", "Calculate"),
                transition("Calculating", "AwaitingApproval", "Submit"),
                transition("AwaitingApproval", "Approved", "Approve"),
                transition("Approved", "Posted", "Post"),
                transition("Posted", "Reversed", "Reverse"),
            ],
        }
    }

    fn pay_run_fields() -> HashMap<String, FieldInfo> {
        let mut fields = HashMap::new();
        fields.insert(
            "runStatus".to_string(),
            FieldInfo {
                rust_name: "run_status".to_string(),
                kind: FieldKind::Codelist,
                optional: false,
            },
        );
        fields.insert(
            "run_status".to_string(),
            FieldInfo {
                rust_name: "run_status".to_string(),
                kind: FieldKind::Codelist,
                optional: false,
            },
        );
        fields
    }

    fn service_op(
        name: &str,
        capabilities: &[&str],
        is_protected: bool,
        condition: Option<&str>,
    ) -> codegraph_core::types::DddServiceOperation {
        codegraph_core::types::DddServiceOperation {
            name: name.to_string(),
            return_type: None,
            return_multiplicity: None,
            params: Vec::new(),
            delegation_target: Some("PayRunRepository".to_string()),
            delegation_operation: Some("save".to_string()),
            capabilities: capabilities.iter().map(|c| c.to_string()).collect(),
            is_protected,
            condition: condition.map(str::to_string),
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

    fn pay_run_model(
        ops: Vec<codegraph_core::types::DddServiceOperation>,
    ) -> codegraph_core::types::DddModelGraph {
        codegraph_core::types::DddModelGraph {
            source_path: "test.ddd".to_string(),
            application: codegraph_core::types::DddApplicationNode {
                name: "App".to_string(),
                base: None,
                source_path: "test.ddd".to_string(),
            },
            modules: Vec::new(),
            designs: vec![codegraph_core::types::DddDesignNode {
                application: "App".to_string(),
                module: "payroll".to_string(),
                class: "PayRun".to_string(),
                resolved_title: Some("PayRunType".to_string()),
                stereotype: "entity".to_string(),
                is_abstract: false,
                flags: codegraph_core::types::DddDesignFlags::default(),
                machine_json: None,
                child_operations: Vec::new(),
                ordinal: 0,
            }],
            repositories: vec![codegraph_core::types::DddRepositoryNode {
                application: "App".to_string(),
                name: "PayRunRepository".to_string(),
                design_class: "PayRun".to_string(),
                operations: Vec::new(),
            }],
            services: vec![codegraph_core::types::DddServiceNode {
                application: "App".to_string(),
                module: "payroll".to_string(),
                name: "PayRunService".to_string(),
                description: None,
                dependencies: Vec::new(),
                decision_dependencies: Vec::new(),
                operations: ops,
                ordinal: 0,
            }],
            searches: Vec::new(),
        }
    }

    #[test]
    fn save_delegation_binds_the_op_trigger_transition() {
        let machine = pay_run_machine();
        let model = pay_run_model(vec![service_op(
            "submitPayRun",
            &["SubmitPayRun"],
            false,
            None,
        )]);
        let config = empty_config();
        let ops = ["delete", "read", "update"].map(str::to_string);
        let lowering = lower_exec(
            &model,
            &config,
            &ops,
            &model.services[0].operations[0],
            Some(&machine),
            &pay_run_fields(),
        );
        let state = match lowering {
            ExecutionKind::Update { state: Some(state) } => state,
            other => panic!("expected machine-bound update, got {other:?}"),
        };
        assert_eq!(state.wire_key.as_str(), "run_status");
        assert_eq!(state.current_expr, "response.run_status.to_string()");
        assert_eq!(state.initial.as_str(), "Draft");
        assert_eq!(
            state.op,
            vec![ServiceOpTransition {
                from: StateName::new("Calculating"),
                to: StateName::new("AwaitingApproval"),
            }]
        );
    }

    #[test]
    fn prepare_pay_run_binds_the_protected_internal_trigger() {
        // `preparePayRun` has no matching trigger; the protected `calculate`
        // shares its capability and matches `Calculate` (the crewbase shape).
        let machine = pay_run_machine();
        let model = pay_run_model(vec![
            service_op("preparePayRun", &["PreparePayRun"], false, None),
            service_op("calculate", &["PreparePayRun"], true, None),
        ]);
        let config = empty_config();
        let ops = ["delete", "read", "update"].map(str::to_string);
        let lowering = lower_exec(
            &model,
            &config,
            &ops,
            &model.services[0].operations[0],
            Some(&machine),
            &pay_run_fields(),
        );
        let state = match lowering {
            ExecutionKind::Update { state: Some(state) } => state,
            other => panic!("expected machine-bound update, got {other:?}"),
        };
        assert_eq!(
            state.op,
            vec![ServiceOpTransition {
                from: StateName::new("Draft"),
                to: StateName::new("Calculating"),
            }]
        );
    }

    #[test]
    fn machine_states_lower_in_when_conditions() {
        let fields = pay_run_fields();
        let states = vec![
            "Draft".to_string(),
            "AwaitingApproval".to_string(),
            "Approved".to_string(),
        ];
        assert_eq!(
            lower_when_to_rust_with_states(
                "runStatus == AwaitingApproval",
                "ApprovePayRun",
                "PayRunType",
                &fields,
                &states,
            )
            .unwrap(),
            r#"(response.run_status.to_string() == "AwaitingApproval")"#
        );
        let err = lower_when_to_rust_with_states(
            "runStatus == Shelved",
            "ApprovePayRun",
            "PayRunType",
            &fields,
            &states,
        )
        .unwrap_err();
        assert!(err.contains("nor a declared state"), "{err}");
        assert!(err.contains("Draft"), "{err}");
    }

    #[test]
    fn workflow_managed_state_field_keeps_the_update_but_seams_transitions() {
        let machine = pay_run_machine();
        let config = empty_config();
        let mut config = config;
        let entry = config.domains.get_mut("payroll").unwrap();
        entry.entity_config.insert(
            "PayRunType".to_string(),
            toml::from_str(
                r#"
workflow = { status_field = "run_status", initial_state = "Draft" }
"#,
            )
            .unwrap(),
        );
        let ops = ["read", "update"].map(str::to_string);
        // A non-transition delegation still updates the other body fields.
        let model = pay_run_model(vec![service_op("update", &[], false, None)]);
        let lowering = lower_exec(
            &model,
            &config,
            &ops,
            &model.services[0].operations[0],
            Some(&machine),
            &pay_run_fields(),
        );
        match lowering {
            ExecutionKind::Update { state } => assert!(
                state.is_none(),
                "workflow-managed state is not DTO-patchable"
            ),
            other => panic!("expected a plain update, got {other:?}"),
        }

        // A state transition cannot ride the update DTO — fail closed.
        let model = pay_run_model(vec![service_op("submitPayRun", &[], false, None)]);
        let lowering = lower_exec(
            &model,
            &config,
            &ops,
            &model.services[0].operations[0],
            Some(&machine),
            &pay_run_fields(),
        );
        match lowering {
            ExecutionKind::Seam { reason } => {
                assert!(reason.contains("workflow-managed"), "{reason}")
            }
            other => panic!("expected a fail-closed seam, got {other:?}"),
        }
    }

    fn lower_with_machine(
        op_name: &str,
        capabilities: &[&str],
        machine: &phero_ir::ddd::StateMachine,
        protected: Option<(&str, &[&str])>,
    ) -> ExecutionKind {
        let mut ops = vec![service_op(op_name, capabilities, false, None)];
        if let Some((name, caps)) = protected {
            ops.push(service_op(name, caps, true, None));
        }
        let model = pay_run_model(ops);
        let ops_surface = ["delete", "read", "update"].map(str::to_string);
        lower_exec(
            &model,
            &empty_config(),
            &ops_surface,
            &model.services[0].operations[0],
            Some(machine),
            &pay_run_fields(),
        )
    }

    #[test]
    fn self_transition_is_a_declared_noop_edge() {
        let mut machine = pay_run_machine();
        machine.transitions.push(phero_ir::ddd::Transition {
            from: "Draft".to_string(),
            to: "Draft".to_string(),
            trigger: Some("Refresh".to_string()),
            guard: None,
        });
        let lowering = lower_with_machine("refresh", &[], &machine, None);
        let state = match lowering {
            ExecutionKind::Update { state: Some(state) } => state,
            other => panic!("expected a machine-bound update, got {other:?}"),
        };
        assert_eq!(
            state.op,
            vec![ServiceOpTransition {
                from: StateName::new("Draft"),
                to: StateName::new("Draft"),
            }],
            "a self transition is legal and does not advance the state"
        );
    }

    #[test]
    fn body_driven_op_whitelists_every_transition_but_selects_none() {
        // `changeStatus` matches no trigger: the op has no transition of its
        // own, so the handler validates a body-supplied target against `all`
        // and an empty body is an idempotent read.
        let machine = pay_run_machine();
        let lowering = lower_with_machine("changeStatus", &[], &machine, None);
        let state = match lowering {
            ExecutionKind::Update { state: Some(state) } => state,
            other => panic!("expected a body-driven update, got {other:?}"),
        };
        assert!(state.op.is_empty(), "no transition is selected by name");
        assert_eq!(state.all.len(), machine.transitions.len());
        assert_eq!(state.initial.as_str(), "Draft");
    }

    // ── Issue #660: the authored binding is the primary selector ──

    /// A machine with a trigger the op name does NOT resemble plus one the
    /// name DOES resemble — so a binding that wins can only have won by
    /// being authored.
    fn pay_run_machine_with_abandon() -> phero_ir::ddd::StateMachine {
        let mut machine = pay_run_machine();
        machine.transitions.push(phero_ir::ddd::Transition {
            from: "Draft".to_string(),
            to: "Abandoned".to_string(),
            trigger: Some("Abandon".to_string()),
            guard: None,
        });
        machine.terminals.push("Abandoned".to_string());
        machine
    }

    /// The same machine carrying a bare arrow (`-> Posted`, no `via`): the
    /// binding must accept its derived lowerCamel trigger (`posted`).
    fn pay_run_machine_with_bare_arrow() -> phero_ir::ddd::StateMachine {
        let mut machine = pay_run_machine();
        machine.transitions.push(phero_ir::ddd::Transition {
            from: "Approved".to_string(),
            to: "Archived".to_string(),
            trigger: None,
            guard: None,
        });
        machine
    }

    /// A second machine-bearing design in the SAME module (the qualifier
    /// gap's ambiguity set).
    fn second_machine_design() -> codegraph_core::types::DddDesignNode {
        let mut machine = pay_run_machine();
        machine.field = "checkStatus".to_string();
        machine.enum_type = phero_ir::TypeRef::Enum {
            package: "payroll".to_string(),
            name: "PayRunCheckStatus".to_string(),
        };
        codegraph_core::types::DddDesignNode {
            application: "App".to_string(),
            module: "payroll".to_string(),
            class: "PayRunCheck".to_string(),
            resolved_title: Some("PayRunCheckType".to_string()),
            stereotype: "entity".to_string(),
            is_abstract: false,
            flags: codegraph_core::types::DddDesignFlags::default(),
            machine_json: Some(serde_json::to_value(machine).unwrap()),
            child_operations: Vec::new(),
            ordinal: 1,
        }
    }

    fn bound_op(
        name: &str,
        machine: Option<&str>,
        trigger: &str,
    ) -> codegraph_core::types::DddServiceOperation {
        let mut op = service_op(name, &[], false, None);
        op.transition = Some(codegraph_core::types::DddTransitionRef {
            trigger: trigger.to_string(),
            machine: machine.map(str::to_string),
        });
        op
    }

    fn machine_bound_update(lowering: ExecutionKind) -> ServiceOpState {
        match lowering {
            ExecutionKind::Update { state: Some(state) } => state,
            other => panic!("expected a machine-bound update, got {other:?}"),
        }
    }

    #[test]
    fn authored_binding_wins_over_a_name_resembling_trigger() {
        // `abandonPayRun` matches the `Abandon` trigger by name, but the
        // authored binding selects `Submit` — the binding is the whole
        // selector, so the heuristic never runs (issue #660).
        let machine = pay_run_machine_with_abandon();
        let model = pay_run_model(vec![bound_op("abandonPayRun", None, "Submit")]);
        let state = machine_bound_update(lower_exec(
            &model,
            &empty_config(),
            &["delete", "read", "update"].map(str::to_string),
            &model.services[0].operations[0],
            Some(&machine),
            &pay_run_fields(),
        ));
        assert_eq!(
            state.op,
            vec![ServiceOpTransition {
                from: StateName::new("Calculating"),
                to: StateName::new("AwaitingApproval"),
            }],
            "the authored trigger must win over the name heuristic"
        );
    }

    #[test]
    fn authored_binding_accepts_a_bare_arrows_derived_trigger() {
        // A bare arrow's trigger is the lowerCamel of the target state
        // upstream (`Archived` → `archived`); the binding resolves it.
        let machine = pay_run_machine_with_bare_arrow();
        let model = pay_run_model(vec![bound_op("storeRun", None, "archived")]);
        let state = machine_bound_update(lower_exec(
            &model,
            &empty_config(),
            &["delete", "read", "update"].map(str::to_string),
            &model.services[0].operations[0],
            Some(&machine),
            &pay_run_fields(),
        ));
        assert_eq!(
            state.op,
            vec![ServiceOpTransition {
                from: StateName::new("Approved"),
                to: StateName::new("Archived"),
            }]
        );
    }

    #[test]
    fn a_single_machine_module_takes_the_unqualified_binding() {
        // One machine-bearing design ⇒ the qualifier is optional and the
        // unqualified spelling resolves against the bound machine.
        let machine = pay_run_machine();
        let model = pay_run_model(vec![bound_op("prepareRun", None, "Calculate")]);
        let state = machine_bound_update(lower_exec(
            &model,
            &empty_config(),
            &["delete", "read", "update"].map(str::to_string),
            &model.services[0].operations[0],
            Some(&machine),
            &pay_run_fields(),
        ));
        assert_eq!(
            state.op,
            vec![ServiceOpTransition {
                from: StateName::new("Draft"),
                to: StateName::new("Calculating"),
            }]
        );
    }

    #[test]
    fn a_qualified_binding_resolves_in_a_multi_machine_module() {
        let machine = pay_run_machine();
        let mut model = pay_run_model(vec![bound_op(
            "prepareRun",
            Some("PayRunStatus"),
            "Calculate",
        )]);
        model.designs[0].machine_json = Some(serde_json::to_value(&machine).unwrap());
        model.designs.push(second_machine_design());
        let state = machine_bound_update(lower_exec(
            &model,
            &empty_config(),
            &["delete", "read", "update"].map(str::to_string),
            &model.services[0].operations[0],
            Some(&machine),
            &pay_run_fields(),
        ));
        assert_eq!(
            state.op,
            vec![ServiceOpTransition {
                from: StateName::new("Draft"),
                to: StateName::new("Calculating"),
            }]
        );
    }

    #[test]
    fn an_unqualified_binding_in_a_multi_machine_module_names_both_machines() {
        let machine = pay_run_machine();
        let mut model = pay_run_model(vec![bound_op("prepareRun", None, "Calculate")]);
        model.designs[0].machine_json = Some(serde_json::to_value(&machine).unwrap());
        model.designs.push(second_machine_design());
        let err = try_lower_exec(
            &model,
            &empty_config(),
            &["delete", "read", "update"].map(str::to_string),
            &model.services[0].operations[0],
            Some(&machine),
            &pay_run_fields(),
        )
        .expect_err("an unqualified binding in a multi-machine module must fail");
        assert!(err.contains("prepareRun"), "{err}");
        assert!(err.contains("PayRun"), "{err}");
        assert!(
            err.contains("PayRunCheck"),
            "the error must name both machines: {err}"
        );
        assert!(err.contains("PayRunStatus"), "{err}");
        assert!(err.contains("PayRunCheckStatus"), "{err}");
    }

    #[test]
    fn a_qualifier_naming_another_machine_is_refused() {
        let machine = pay_run_machine();
        let mut model = pay_run_model(vec![bound_op(
            "prepareRun",
            Some("PayRunCheckStatus"),
            "Calculate",
        )]);
        model.designs[0].machine_json = Some(serde_json::to_value(&machine).unwrap());
        model.designs.push(second_machine_design());
        let err = try_lower_exec(
            &model,
            &empty_config(),
            &["delete", "read", "update"].map(str::to_string),
            &model.services[0].operations[0],
            Some(&machine),
            &pay_run_fields(),
        )
        .expect_err("a qualifier naming another machine must fail");
        assert!(err.contains("prepareRun"), "{err}");
        assert!(err.contains("PayRunCheckStatus"), "{err}");
        assert!(err.contains("PayRunStatus"), "{err}");
    }

    #[test]
    fn an_unknown_trigger_names_the_declared_vocabulary() {
        let machine = pay_run_machine();
        let model = pay_run_model(vec![bound_op("prepareRun", None, "SignOff")]);
        let err = try_lower_exec(
            &model,
            &empty_config(),
            &["delete", "read", "update"].map(str::to_string),
            &model.services[0].operations[0],
            Some(&machine),
            &pay_run_fields(),
        )
        .expect_err("an unknown trigger must fail");
        assert!(err.contains("SignOff"), "{err}");
        assert!(
            err.contains("Calculate"),
            "the error must list the vocabulary: {err}"
        );
        assert!(err.contains("Submit"), "{err}");
    }

    #[test]
    fn a_binding_on_an_entity_without_a_machine_is_refused() {
        // Never degrade an authored binding to a plain update.
        let model = pay_run_model(vec![bound_op("prepareRun", None, "Calculate")]);
        let err = try_lower_exec(
            &model,
            &empty_config(),
            &["delete", "read", "update"].map(str::to_string),
            &model.services[0].operations[0],
            None,
            &pay_run_fields(),
        )
        .expect_err("a binding with no machine must fail");
        assert!(err.contains("prepareRun"), "{err}");
        assert!(err.contains("no lifecycle machine"), "{err}");
    }

    #[test]
    fn a_binding_on_a_cross_aggregate_delegation_is_refused() {
        // The binding's machine belongs to the delegation target's class;
        // a route bound to another aggregate must not seam silently.
        let machine = pay_run_machine();
        let mut model = pay_run_model(vec![bound_op("addPayRunItem", None, "Submit")]);
        model.repositories[0].design_class = "PayRunItem".to_string();
        let err = try_lower_exec(
            &model,
            &empty_config(),
            &["delete", "read", "update"].map(str::to_string),
            &model.services[0].operations[0],
            Some(&machine),
            &pay_run_fields(),
        )
        .expect_err("a binding across aggregates must fail");
        assert!(err.contains("addPayRunItem"), "{err}");
        assert!(err.contains("another aggregate"), "{err}");
        assert!(err.contains("PayRunItem"), "{err}");
    }

    #[test]
    fn capability_shared_op_without_a_trigger_stays_a_plain_update() {
        // Capability sharing is an explicit last-resort heuristic: the
        // protected op carries the trigger only when its NAME matches one.
        let machine = pay_run_machine();
        let lowering = lower_with_machine(
            "preparePayRun",
            &["PreparePayRun"],
            &machine,
            Some(("recalculate", &["PreparePayRun"])),
        );
        let state = match lowering {
            ExecutionKind::Update { state: Some(state) } => state,
            other => panic!("expected a plain body-driven update, got {other:?}"),
        };
        assert!(
            state.op.is_empty(),
            "no trigger match anywhere ⇒ no transition is invented"
        );
    }

    #[test]
    fn cross_entity_child_delegation_fails_closed() {
        let machine = pay_run_machine();
        let mut model = pay_run_model(vec![service_op("addPayRunItem", &[], false, None)]);
        // The op routes through a repository for a DIFFERENT design class —
        // the child/aggregate shape. Patching the parent row would silently
        // drop the child intent.
        model.repositories[0].design_class = "PayRunItem".to_string();
        let ops_surface = ["delete", "read", "update"].map(str::to_string);
        let lowering = lower_exec(
            &model,
            &empty_config(),
            &ops_surface,
            &model.services[0].operations[0],
            Some(&machine),
            &pay_run_fields(),
        );
        match lowering {
            ExecutionKind::Seam { reason } => {
                // The improved message names the construct that WOULD lower
                // it (issue #661): a `children <Root>` block whose ops carry
                // `childScope`.
                assert!(reason.contains("children PayRun"), "{reason}");
                assert!(reason.contains("childScope"), "{reason}");
                assert!(reason.contains("PayRunItem"), "{reason}");
                assert!(
                    reason.contains("Refusing to patch the parent row"),
                    "{reason}"
                );
            }
            other => panic!("expected a fail-closed child seam, got {other:?}"),
        }
    }

    #[test]
    fn no_record_load_route_fails_closed() {
        let machine = pay_run_machine();
        let model = pay_run_model(vec![service_op("submitPayRun", &[], false, None)]);
        let ops_surface = ["delete", "update"].map(str::to_string); // no read
        let shape_raises = crate::ddd::body_lower::RaiseTable::default();
        let shape = RouteShape {
            domain: "payroll",
            entity_title: "PayRunType",
            entity_name: "PayRunType",
            entity_table: "pay_run",
            id_param: "pay_run_id",
            scoped: None,
            auditable: false,
            domain_types_crate: "domain_types",
            dto_casing: crate::project_config::DtoKeyCasing::Snake,
            fields: &pay_run_fields(),
            surface_ops: &ops_surface,
            record_load: false,
            machine: Some(&machine),
            raises: &shape_raises,
            params: &[],
            throws: &[],
        };
        let lowering = execution_for(
            &model.services[0].operations[0],
            &model,
            &empty_config(),
            "PayRunService",
            &shape,
        )
        .expect("lowering");
        match lowering {
            ExecutionKind::Seam { reason } => {
                assert!(reason.contains("no record load"), "{reason}")
            }
            other => panic!("expected a no-record-load seam, got {other:?}"),
        }
    }

    // ── The pherolang TypeRef JSON shape (bug fix): the class-typed-signature
    //    binding path must read the ADJACENT-tagged shape phero-ir serializes
    //    (`{"type":"class","value":{"package":…,"name":…}}`), not a flat
    //    top-level `name` that can never match. ───────────────────────────

    fn op_with_type_json(
        return_json: serde_json::Value,
    ) -> codegraph_core::types::DddServiceOperation {
        codegraph_core::types::DddServiceOperation {
            name: "op".to_string(),
            return_type: Some(return_json),
            return_multiplicity: None,
            params: Vec::new(),
            delegation_target: None,
            delegation_operation: None,
            capabilities: Vec::new(),
            is_protected: false,
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

    fn class_type_json(package: &str, name: &str) -> serde_json::Value {
        // The phero-ir TypeRef wire shape (adjacently tagged, camelCase tags).
        serde_json::json!({
            "type": "class",
            "value": { "package": package, "name": name }
        })
    }

    #[tokio::test]
    async fn class_typed_signature_binds_the_real_phero_json_shape() {
        let engine = codegraph_core::mock::MockEngine::builder()
            .with_schema(codegraph_core::types::SchemaNode {
                namespace: None,
                schema_id: "payroll/json/PayRunType.json".to_string(),
                title: "PayRunType".to_string(),
                description: None,
                schema_type: "object".to_string(),
                classification: "entity_reference".to_string(),
                domain: Some("payroll".to_string()),
                rel_path: "payroll/json/PayRunType.json".to_string(),
                pg_type: "UUID".to_string(),
                rust_type: "PayRun".to_string(),
                sea_orm_type: "Uuid".to_string(),
                rust_type_name: "PayRun".to_string(),
                pg_table_name: "pay_run".to_string(),
                api_path_segment: "pay-run".to_string(),
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
            })
            .build();

        // The REAL pherolang shape: `{"type":"class","value":{"package":…,"name":…}}`.
        let bound = bind_entity(
            &pay_run_model(Vec::new()),
            &op_with_type_json(class_type_json("payroll", "PayRun")),
            &engine,
            "Type",
        )
        .await;
        assert_eq!(
            bound.as_deref(),
            Some("PayRunType"),
            "the class-typed signature path must bind through the nested pherolang shape"
        );

        // The qualified name form keeps the `pkg::Name` resolution chain.
        let bound = bind_entity(
            &pay_run_model(Vec::new()),
            &op_with_type_json(class_type_json("other.pkg", "PayRun")),
            &engine,
            "Type",
        )
        .await;
        assert_eq!(bound.as_deref(), Some("PayRunType"));

        // Non-class shapes bind nothing.
        let primitive = bind_entity(
            &pay_run_model(Vec::new()),
            &op_with_type_json(serde_json::json!({ "type": "primitive", "value": "string" })),
            &engine,
            "Type",
        )
        .await;
        assert_eq!(primitive, None);

        // Unknown classes bind nothing (no fabricated titles).
        let unknown = bind_entity(
            &pay_run_model(Vec::new()),
            &op_with_type_json(class_type_json("payroll", "NoSuchClass")),
            &engine,
            "Type",
        )
        .await;
        assert_eq!(unknown, None);

        // Param-typed signatures ride the same path.
        let mut param_op = op_with_type_json(serde_json::Value::Null);
        param_op.return_type = None;
        param_op.params = vec![codegraph_core::types::DddParam {
            name: "run".to_string(),
            type_json: class_type_json("payroll", "PayRun"),
            multiplicity: None,
        }];
        let bound = bind_entity(&pay_run_model(Vec::new()), &param_op, &engine, "Type").await;
        assert_eq!(
            bound.as_deref(),
            Some("PayRunType"),
            "param-typed signature"
        );
    }

    // ── Injected-scope guard facts (issue #663) ────────────────────────

    fn injected_scope(fk_optional: bool) -> InjectedScope {
        let mut fields = HashMap::new();
        fields.insert(
            "timecardStatus".to_string(),
            FieldInfo {
                rust_name: "timecard_status".to_string(),
                kind: FieldKind::Codelist,
                optional: false,
            },
        );
        fields.insert(
            "hours".to_string(),
            FieldInfo {
                rust_name: "hours".to_string(),
                kind: FieldKind::Integer,
                optional: false,
            },
        );
        InjectedScope {
            root: "TimecardRepository".to_string(),
            local: "timecard_repository".to_string(),
            slot: "guardpay_timecard".to_string(),
            auditable: false,
            fk_field: "timecard_id".to_string(),
            fk_optional,
            fields,
            machine: Some(InjectedMachine {
                field: "timecardStatus".to_string(),
                states: vec![
                    "Pending".to_string(),
                    "Approved".to_string(),
                    "Rejected".to_string(),
                ],
                initial: "Pending".to_string(),
            }),
        }
    }

    fn guard_op(condition: &str) -> codegraph_core::types::DddServiceOperation {
        codegraph_core::types::DddServiceOperation {
            name: "submit".to_string(),
            return_type: None,
            return_multiplicity: None,
            params: Vec::new(),
            delegation_target: None,
            delegation_operation: None,
            capabilities: Vec::new(),
            is_protected: false,
            condition: Some(condition.to_string()),
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

    #[test]
    fn fresh_fixture_verdicts_are_provable_never_guessed() {
        let scopes = vec![injected_scope(false)];
        // eq with the machine's initial state: the fresh row PASSES → the
        // contract keeps the normal legs.
        let facts = injected_guard_facts(
            &guard_op("payRunStatus == Draft && TimecardRepository.timecardStatus == Pending"),
            &scopes,
        )
        .expect("facts");
        assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Satisfied);
        // ne with a NON-initial state: `Pending != Approved` holds on the
        // fresh row → Satisfied.
        let facts = injected_guard_facts(
            &guard_op("payRunStatus == Draft && TimecardRepository.timecardStatus != Approved"),
            &scopes,
        )
        .expect("facts");
        assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Satisfied);
        // eq with a non-initial state: the fresh row (initial `Pending`)
        // FAILS the comparison → provable denial, pin 403.
        let facts = injected_guard_facts(
            &guard_op("payRunStatus == Draft && TimecardRepository.timecardStatus == Approved"),
            &scopes,
        )
        .expect("facts");
        assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Denied);
        // An OPTIONAL FK denies BEFORE any comparison is evaluated (the
        // create body omits nullable props → the fresh FK is NULL): 403
        // regardless of the comparison's content — including an
        // eq-initial comparison that would otherwise read `Satisfied`
        // (the exact reviewer case: planner 200 vs handler 403).
        let optional = vec![injected_scope(true)];
        let facts = injected_guard_facts(
            &guard_op("payRunStatus == Draft && TimecardRepository.timecardStatus == Pending"),
            &optional,
        )
        .expect("facts");
        assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Denied);
    }

    #[test]
    fn undecided_comparisons_omit_the_leg_never_guess() {
        let scopes = vec![injected_scope(false)];
        // A NON-machine injected field (`hours > 8`): the fresh value
        // (fixture-seeded 42) is derivable but the planner does not model
        // it — Unverifiable, and the contract OMITS the leg (a guessed
        // 403 would come back 200; a guessed 200 would come back 403 —
        // a wrong pin fails the suite in either direction).
        let facts = injected_guard_facts(
            &guard_op("payRunStatus == Draft && TimecardRepository.hours > 8"),
            &scopes,
        )
        .expect("facts");
        assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Unverifiable);
        // Negation flips polarity: an un-negated collect would decide the
        // comparison upside down — Unverifiable.
        let facts = injected_guard_facts(
            &guard_op("!(TimecardRepository.timecardStatus == Approved)"),
            &scopes,
        )
        .expect("facts");
        assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Unverifiable);
        // `||` composition: the guard may pass via the own-record side —
        // the collected set no longer decides the whole condition.
        let facts = injected_guard_facts(
            &guard_op("payRunStatus == Posted || TimecardRepository.timecardStatus == Approved"),
            &scopes,
        )
        .expect("facts");
        assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Unverifiable);
        // No injected scopes at all → no guard facts.
        assert!(injected_guard_facts(&guard_op("payRunStatus == Draft"), &[]).is_none());
    }

    #[test]
    fn mixed_machine_and_undecidable_conjuncts_never_misverdict() {
        // Round-2 review counterexamples: a machine comparison MIXED with
        // an undecidable one must not let the decided side guess the
        // verdict for the whole condition.
        let scopes = vec![injected_scope(false)];
        // Non-machine Eq/Ne conjunct: the fresh `hours` is fixture-seeded
        // (42) — `!= 8` PASSES on the fresh row, so a Denied fold would
        // pin 403 while the handler 200s (the reviewer's exact case).
        let facts = injected_guard_facts(
            &guard_op(
                "TimecardRepository.timecardStatus == Pending && TimecardRepository.hours != 8",
            ),
            &scopes,
        )
        .expect("facts");
        assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Unverifiable);
        // An ORDERING conjunct dropped beside a passing machine comparison:
        // fresh `hours` = 42 fails `> 100`, so a Satisfied fold would pin
        // 200 while the handler 403s.
        let facts = injected_guard_facts(
            &guard_op(
                "TimecardRepository.timecardStatus == Pending && TimecardRepository.hours > 100",
            ),
            &scopes,
        )
        .expect("facts");
        assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Unverifiable);
        // A failing machine conjunct beside an undecided sibling is still
        // PROVABLE: under plain AND the failing conjunct decides the whole
        // guard — Denied stands.
        let facts = injected_guard_facts(
            &guard_op(
                "TimecardRepository.timecardStatus == Approved && TimecardRepository.hours > 8",
            ),
            &scopes,
        )
        .expect("facts");
        assert_eq!(facts.fresh_fixture, FreshFixtureGuard::Denied);
    }

    #[test]
    fn cross_scope_field_comparisons_refuse() {
        let mut fields = HashMap::new();
        fields.insert(
            "status".to_string(),
            FieldInfo {
                rust_name: "status".to_string(),
                kind: FieldKind::String,
                optional: false,
            },
        );
        let mut own = scopes_for_test(&fields);
        let scopes = vec![injected_scope(false)];
        let _ = &mut own;
        // An own-record field vs an injected field: no honest root for
        // both sides — named refusal (the message covers two injected
        // scopes too).
        let err = lower_when_scoped(
            "status == TimecardRepository.timecardStatus",
            "SubmitPayRun",
            "PayRunType",
            &fields,
            &[],
            &scopes,
        )
        .unwrap_err();
        assert!(err.contains("across different records"), "{err}");
    }

    fn scopes_for_test(
        fields: &HashMap<String, FieldInfo>,
    ) -> (&str, &HashMap<String, FieldInfo>, &[InjectedScope]) {
        ("PayRunType", fields, &[])
    }
}
