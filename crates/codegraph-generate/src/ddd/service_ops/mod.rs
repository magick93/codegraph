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

mod execution;
mod policy;
mod projection;
pub mod render;
mod routes;
mod transitions;
mod types;
mod validation;
mod when_lower;

#[cfg(test)]
mod predicate_tests;
#[cfg(test)]
mod tests;

pub use execution::RouteShape;
pub use projection::projection_shape_candidate_public;
pub(crate) use routes::bind_entity;
pub use routes::{BodyRenderContext, routes_for_entity};
pub use types::{
    BoundaryPayloadPlan, ChildAction, ChildParentLoad, ExecutionKind, FreshFixtureGuard,
    InjectedGuard, ServiceOpRoute, ServiceOpState, ServiceOpTransition, StateName, TriggerName,
    WireKey,
};
pub use validation::{
    body_refusals, child_parent_load_refusals, dead_surface_warnings, guard_refusals,
    ifml_dead_triggers, throws_refusals, transition_binding_errors, transition_binding_warnings,
};
pub use when_lower::InjectedMachine;

pub(crate) use transitions::dto_wire_key;
pub(crate) use types::{FieldInfo, FieldKind, fields_for};
pub(crate) use when_lower::{lower_invariant, rust_string_literal};

#[cfg(test)]
pub(crate) use policy::{PolicySurface, render_authorize_fn};
#[cfg(test)]
pub(crate) use when_lower::{
    InjectedScope, lower_when_scoped, lower_when_to_rust, lower_when_to_rust_with_states,
};

// Names the `#[cfg(test)] tests` module glob-imports (`use super::*;`) that
// live private in their sibling submodules. Test-only: the gated imports keep
// non-test builds free of unused-import warnings.
#[cfg(test)]
use std::collections::{BTreeMap, HashMap};

#[cfg(test)]
use codegraph_config::DomainConfig;
#[cfg(test)]
use codegraph_naming::to_snake_case;

#[cfg(test)]
use execution::execution_for;
#[cfg(test)]
use policy::basejump_role_actors;
#[cfg(test)]
use routes::injected_guard_facts;
