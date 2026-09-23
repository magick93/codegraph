use serde::{Deserialize, Serialize};

/// The kind of a [`RegulatoryNode`] (regulatory reference plane, issue
/// #265). Mirrors the sigil semantic elements that carry regulatory
/// reference metadata (`sigil_model::{Report, Body, Corpus, Segment,
/// ExternalRuleSource, Schema, MetaType}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegulatoryKind {
    /// `report Body (Corpus)* (Segment "ref")* in <timing> from T ...` —
    /// anonymous in sigil ([`sigil_model::SemanticElement::name`] returns
    /// `""`); the bridge synthesizes a deterministic name (see
    /// `ROSETTA_REPORT_NAME_PREFIX` doc there).
    Report,
    /// `body <type> <name> <"...">?` (`sigil_model::Body`).
    Body,
    /// `corpus <type> [Body] ["display"] <name> <"...">?`
    /// (`sigil_model::Corpus`).
    Corpus,
    /// `segment <name>` (`sigil_model::Segment`).
    Segment,
    /// `rule source <name> (extends S)? {...}` (`sigil_model::ExternalRuleSource`).
    RuleSource,
    /// `schema <name> <format> <"...">?` (`sigil_model::Schema`,
    /// main-branch metamodel).
    RuleSchema,
    /// `metaType <name> <type>` (`sigil_model::MetaType`).
    MetaType,
}

impl RegulatoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RegulatoryKind::Report => "report",
            RegulatoryKind::Body => "body",
            RegulatoryKind::Corpus => "corpus",
            RegulatoryKind::Segment => "segment",
            RegulatoryKind::RuleSource => "rule_source",
            RegulatoryKind::RuleSchema => "rule_schema",
            RegulatoryKind::MetaType => "meta_type",
        }
    }

    /// Parse the persisted kind string ([`RegulatoryKind::as_str`]
    /// output).
    pub fn parse_kind(value: &str) -> Option<Self> {
        match value {
            "report" => Some(RegulatoryKind::Report),
            "body" => Some(RegulatoryKind::Body),
            "corpus" => Some(RegulatoryKind::Corpus),
            "segment" => Some(RegulatoryKind::Segment),
            "rule_source" => Some(RegulatoryKind::RuleSource),
            "rule_schema" => Some(RegulatoryKind::RuleSchema),
            "meta_type" => Some(RegulatoryKind::MetaType),
            _ => None,
        }
    }
}

/// A regulatory reference metadata node (issue #265).
///
/// ONE parameterized node kind (the `ConditionNode` precedent) instead of
/// seven near-identical structs: the kinds share only `name` / `label` /
/// `domain` / `definition`; every other field is kind-specific and
/// disjoint (`body_type` vs `corpus_type` vs `format` vs rule-source
/// classes vs the report's regulatory ref). Those payloads persist as the
/// serialized `properties` map (one `properties_json` column — the
/// `PolicyNode.kind_json` shape) so backends need a single node type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegulatoryNode {
    /// Element name. Reports synthesize one (`Report {body} {corpora}
    /// ({timing})`) because sigil reports are anonymous.
    pub name: String,
    pub kind: RegulatoryKind,
    /// Display name (`Corpus.displayName` — the only display field in the
    /// family).
    pub label: Option<String>,
    /// The sigil definition text (`<"definition">`), when authored.
    pub definition: Option<String>,
    pub domain: Option<String>,
    /// Kind-specific payload as a JSON object (e.g. Body `body_type`,
    /// Corpus `corpus_type`/`parent_body`, RuleSource `super_source` +
    /// `classes`, RuleSchema `format`, MetaType `type_ref`, Report
    /// `regulatory`/`timing`/`input_type`/`report_type`/
    /// `eligibility_rules`/`rule_source`).
    pub properties: serde_json::Value,
}

/// The owner end of a regulatory reference edge (issue #265). The sigil
/// model attaches regulatory references from three node families; each
/// matches by its natural key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegulatoryOwner {
    /// A Schema node, matched by title (a `Data` type carrying
    /// `[docReference ...]` metadata).
    Schema(String),
    /// A ConditionNode, matched by name (conditions carry
    /// `doc_references` too).
    Condition(String),
    /// Another RegulatoryNode (a report referencing its body/corpora/
    /// segments/rule source), matched by name + kind.
    Regulatory { name: String, kind: RegulatoryKind },
    /// A FunctionNode carrying `[docReference ...]` metadata (issue #263),
    /// matched by name.
    Function(String),
}

/// Which regulatory edge links an owner to a regulatory node. Grounded in
/// the sigil model — the DSL has no `appliedTo`/`derivesFrom` keywords;
/// these are the associations it actually expresses:
///
/// - [`RegulatoryEdgeKind::Reference`] — `[docReference ...]` metadata on
///   types/attributes/conditions, and a report's positional regulatory
///   reference (`report Body Corpus (Segment "ref")*`).
/// - [`RegulatoryEdgeKind::RuleSource`] — the report's `with source S`.
/// - [`RegulatoryEdgeKind::CorpusInBody`] — a corpus' parent body.
/// - [`RegulatoryEdgeKind::DerivesFrom`] — a rule source's `extends S`
///   (`super_source`) or a metaType's refined type (`type_ref`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegulatoryEdgeKind {
    Reference,
    RuleSource,
    CorpusInBody,
    DerivesFrom,
}

impl RegulatoryEdgeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RegulatoryEdgeKind::Reference => "RegulatoryReference",
            RegulatoryEdgeKind::RuleSource => "HasRuleSource",
            RegulatoryEdgeKind::CorpusInBody => "CorpusInBody",
            RegulatoryEdgeKind::DerivesFrom => "DerivesFrom",
        }
    }
}

/// One regulatory reference edge read back from the graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegulatoryRefRecord {
    /// Owner element name (Schema title, Condition name, or regulatory
    /// node name).
    pub owner: String,
    /// Owner node label (`Schema`, `Condition`, or `Regulatory`).
    pub owner_label: String,
    /// Target regulatory node name.
    pub target: String,
    pub target_kind: RegulatoryKind,
    pub edge_kind: RegulatoryEdgeKind,
    /// The segment reference text (`(Segment "ref")`) or docReference
    /// `provision`, when present.
    pub ref_path: Option<String>,
}
