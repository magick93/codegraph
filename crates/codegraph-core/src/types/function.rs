use serde::{Deserialize, Serialize};

/// The kind of a function `[transform]` annotation (issue #263).
/// `[ingest X]` / `[enrich]` / `[projection Y]` — the reference targets a
/// rule-schema element or a `SerializationFormat` value, kept as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionTransformKind {
    Ingest,
    Enrich,
    Projection,
}

impl FunctionTransformKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FunctionTransformKind::Ingest => "ingest",
            FunctionTransformKind::Enrich => "enrich",
            FunctionTransformKind::Projection => "projection",
        }
    }

    /// Parse the persisted kind string ([`FunctionTransformKind::as_str`]
    /// output).
    pub fn parse_kind(value: &str) -> Option<Self> {
        match value {
            "ingest" => Some(FunctionTransformKind::Ingest),
            "enrich" => Some(FunctionTransformKind::Enrich),
            "projection" => Some(FunctionTransformKind::Projection),
            _ => None,
        }
    }
}

/// The dispatch head `(attr: Enum->VALUE)` of a dispatched function
/// (sigil `FunctionDispatch`): the attribute names one of the function's
/// inputs, and the pair selects the enum case this function computes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionDispatch {
    pub attribute: String,
    pub enumeration: String,
    pub value: String,
}

/// One typed function input/output (sigil `Attribute`, cardinality
/// collapsed to the two flags codegen needs). `type_ref` is the bare type
/// title (last segment of the written, resolution-checked reference).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionInput {
    pub name: String,
    pub type_ref: String,
    /// Cardinality max is unbounded or > 1.
    pub is_array: bool,
    /// Cardinality min is 0.
    pub is_optional: bool,
}

/// One `alias name: expr` declaration (sigil `Shortcut`). `expr_json`
/// carries the canonical `Expr::to_json()` payload — the WP1.6 embedding
/// contract (deterministic, serialization-stable, write-once); the
/// transpiler consumes it at generation time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionAlias {
    pub name: String,
    pub expr_json: String,
}

/// One `set|add <root> (-> feature)*: expr` operation (sigil
/// `Operation`). `assign_root` is the function's output or an alias name;
/// `path` is the `-> feature` chain verbatim; `expr_json` is the canonical
/// expression payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionOperation {
    /// `true` for `add`, `false` for `set`.
    pub is_add: bool,
    pub assign_root: String,
    pub path: Vec<String>,
    pub expr_json: String,
}

/// One function post-condition (sigil `Condition` in `post_conditions`):
/// it may reference the output, unlike non-post conditions. The name is
/// the authored `condition Name:` or the bridge-synthesized
/// `<Function>_post_<idx>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionPostCondition {
    pub name: String,
    pub definition: Option<String>,
    pub expr_json: String,
}

/// One `[transform]` annotation on a function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionTransform {
    pub kind: FunctionTransformKind,
    pub reference: Option<String>,
}

/// A rosetta function (issue #263, computation plane).
///
/// ONE structured node per sigil `Function` — the `ConditionNode`
/// precedent of typed common-shape fields instead of an opaque payload:
/// dispatch head, typed inputs/output, aliases, operations, and
/// post-conditions each get their own typed field; only the genuinely
/// open-ended metadata (annotations, doc references, non-post conditions,
/// origin provenance) persists inside the `properties` JSON object.
///
/// Provenance: `properties["origin"] = "rosetta"` (the ROSETTA_ORIGIN
/// convention, mirroring the `RegulatoryNode` payload).
///
/// Edges: `FunctionExtends` (Function → Function) when `extends` resolves
/// within the bridged run; `[docReference ...]` metadata becomes
/// `RegulatoryReference` edges owned by `RegulatoryOwner::Function`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionNode {
    /// The function name (namespace-unique in Rosetta; the node's natural
    /// key).
    pub name: String,
    pub domain: Option<String>,
    /// The sigil definition text (`<"definition">`), when authored.
    pub definition: Option<String>,
    /// The `(attr: Enum->VALUE)` dispatch head, when the function is a
    /// `FunctionDispatch`.
    pub dispatch: Option<FunctionDispatch>,
    /// Typed inputs in declaration order.
    pub inputs: Vec<FunctionInput>,
    /// The typed output, when the function declares one.
    pub output: Option<FunctionInput>,
    /// `alias` declarations (sigil `Shortcut`), in declaration order.
    pub aliases: Vec<FunctionAlias>,
    /// `set`/`add` operations, in declaration order.
    pub operations: Vec<FunctionOperation>,
    /// Post-conditions (they see the output), in declaration order.
    pub post_conditions: Vec<FunctionPostCondition>,
    /// The resolved parent function name (sigil `extends`), written by the
    /// bridge. Dangling references cannot occur through the normal path —
    /// sigil resolution rejects unknown names — but the referenced node
    /// may live outside the bridged run (e.g. a builtin file).
    pub extends: Option<String>,
    /// `[ingest X]` / `[enrich]` / `[projection Y]` annotations.
    pub transform_annotations: Vec<FunctionTransform>,
    /// Open-ended metadata: `origin`, `annotations`, `doc_references`,
    /// non-post `conditions` (recorded; structured uplift deferred),
    /// `super_function` qualification. An object; never null.
    pub properties: serde_json::Value,
}
