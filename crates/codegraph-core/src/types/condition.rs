use serde::{Deserialize, Serialize};

/// The kind of a [`ConditionNode`] (constraint plane, issue #261).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConditionKind {
    /// A named sigil condition (`condition Name: expr` on a rosetta type).
    /// `expr_json` carries the canonical `Expr::to_json()` payload (the
    /// WP1.6 embedding contract: deterministic, serialization-stable,
    /// write-once — `Expr` is Serialize-only).
    Condition,
    /// Bridge-derived choice exclusivity: ONE node per `choice` data type,
    /// synthesized by the rosetta bridge. one_of is NOT a sigil Expr kind —
    /// sigil models choices as `(0..1)` attributes, so the bridge derives
    /// the option set from the attribute target titles and leaves
    /// `expr_json` empty. Transpilation is #262.
    OneOf,
}

/// A constraint attached to a schema type (issue #261).
///
/// Owned by the Schema it constrains via a `HasCondition` edge
/// (Schema → Condition); `owner_title` is denormalized on the node so
/// backends can serve `get_conditions_for_schema` without a traversal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConditionNode {
    /// The condition name: the sigil `condition Name:` when present, else
    /// synthesized `<Type>_condition_<idx>`; choices derive
    /// `<Type>_one_of`.
    pub name: String,
    /// Title of the owning Schema node.
    pub owner_title: String,
    pub kind: ConditionKind,
    /// Canonical `Expr::to_json()` serialization. `None` for bridge-derived
    /// one_of nodes (see [`ConditionKind::OneOf`]).
    pub expr_json: Option<String>,
    /// one_of option target titles (populated for `kind == OneOf` only).
    pub options: Vec<String>,
    /// The sigil definition text (`<"definition">`), when authored.
    pub definition: Option<String>,
    pub domain: Option<String>,
}
