use serde::{Deserialize, Serialize};

/// The kind of a [`RuleNode`] (issue #264). Mirrors the two sigil `Rule`
/// flavors — sigil models the split as the `eligibility` bool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    /// `reporting rule Name from Input: expr` — computes a reported value.
    Reporting,
    /// `eligibility rule Name from Input: expr` — gates whether the input
    /// is eligible at all (endpoint-guard material).
    Eligibility,
}

impl RuleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RuleKind::Reporting => "reporting",
            RuleKind::Eligibility => "eligibility",
        }
    }

    /// Parse the persisted kind string ([`RuleKind::as_str`] output).
    pub fn parse_kind(value: &str) -> Option<Self> {
        match value {
            "reporting" => Some(RuleKind::Reporting),
            "eligibility" => Some(RuleKind::Eligibility),
            _ => None,
        }
    }
}

/// A rosetta rule (issue #264, computation plane).
///
/// ONE structured node per sigil `Rule` — the `FunctionNode` pattern of
/// typed common-shape fields plus a JSON payload for the rest. Rules do
/// NOT extend (the sigil `Rule` element has no parent ref — unlike
/// functions, there is no `super_rule`), so the family has no cycle
/// semantics.
///
/// Provenance: `properties["origin"] = "rosetta"` (the ROSETTA_ORIGIN
/// convention, mirroring `FunctionNode`/`RegulatoryNode`).
///
/// Edges:
/// - `RuleAppliesTo` (Rule → Schema) when the `from TypeCall` input
///   matches a schema of the bridged run — the input-type attachment.
/// - `[docReference ...]` metadata becomes `RegulatoryReference` edges
///   owned by `RegulatoryOwner::Rule`.
/// - `[ruleReference R]` entries on `rule source` classes become
///   `RuleReference` edges (Schema → Rule) when R resolves within the
///   run; unresolvable ones stay documented skips.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleNode {
    /// The rule name (namespace-unique in Rosetta; the node's natural
    /// key).
    pub name: String,
    pub domain: Option<String>,
    /// The sigil definition text (`<"definition">`), when authored.
    pub definition: Option<String>,
    pub kind: RuleKind,
    /// Bare title of the `from TypeCall` input, when written. `None` for
    /// the input-less degenerate form (its bare symbols have no receiver
    /// to resolve against — the generator emits a marker instead of a
    /// function).
    pub input_type: Option<String>,
    /// Canonical `Expr::to_json()` serialization of the rule body — the
    /// WP1.6 embedding contract (deterministic, serialization-stable,
    /// write-once); the transpiler consumes it at generation time.
    pub expr_json: String,
    /// Open-ended metadata: `origin`, `doc_references`. An object; never
    /// null.
    pub properties: serde_json::Value,
}

/// One rule-source binding read back from the graph (issue #264): a
/// `RuleReference` edge — the class data type (Schema title) of a `rule
/// source` binds `attribute` to the referenced `rule`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleRefRecord {
    /// Title of the rule-source class's data Schema.
    pub schema_title: String,
    /// The bound attribute name (`+ status [ruleReference R]`).
    pub attribute: String,
    /// The referenced rule name (resolved within the bridged run).
    pub rule: String,
    /// The rule source that expresses the binding.
    pub rule_source: String,
}
