use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One typed facet of an ingested mox vocabulary: a named primitive value
/// every entry carries (e.g. `alpha3: string`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoxFacet {
    pub name: String,
    /// The facet's primitive type, e.g. `"string"` or `"int"`.
    pub type_: String,
}

/// One vendored entry of an ingested mox vocabulary: the unique key plus its
/// facet values (JSON-encoded rex primitive values).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoxEntry {
    pub key: String,
    pub facets: BTreeMap<String, serde_json::Value>,
}

/// A rexlang vocabulary definition ingested from a `.mox` domain source: a
/// fixed, versioned set of enumerated keys vendored from an external
/// authority, with typed facets per key. Entries are embedded so the graph
/// stays self-contained (no snapshot files needed downstream).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoxVocabularyNode {
    pub name: String,
    /// The dotted mox package the vocabulary was declared in.
    pub package: String,
    /// External source identifier, e.g. `"iso:4217"`.
    pub source: String,
    /// The snapshot version inlined at compile time, when known.
    pub version: Option<String>,
    /// Name of the facet acting as the unique entry key (e.g. `"alpha3"`).
    pub key_facet: String,
    /// Declared facets in source order; the key facet is included.
    pub facets: Vec<MoxFacet>,
    /// The vendored entries, in snapshot order.
    pub entries: Vec<MoxEntry>,
}

/// A parameter of a [`MoxOperationNode`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoxParam {
    pub name: String,
    pub type_: String,
}

/// An operation declared on a mox class. Bodies are verbatim code strings
/// keyed by target name (`"rust"`, `"expr"`, ...); codegraph persists them as
/// text and never parses or executes them (issue #218 non-goal).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoxOperationNode {
    pub name: String,
    /// The mox class the operation belongs to (matched to schema-ingested
    /// entities by name).
    pub class: String,
    pub package: String,
    pub description: Option<String>,
    pub return_type: String,
    pub params: Vec<MoxParam>,
    /// Verbatim per-target bodies, e.g. `"expr" -> "res.books.len()"`.
    pub bodies: BTreeMap<String, String>,
}

/// A derived (computed, not stored) feature of a mox class. Carried as a
/// dedicated node — never as a stored column — so column-emitting generators
/// (DDL, SeaORM entity, cornucopia queries) structurally cannot see it; only
/// opt-in consumers (the DTO generator) query it. This is the #195 decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoxDerivedFeatureNode {
    pub name: String,
    pub class: String,
    pub package: String,
    /// The feature's resolved type, e.g. `"string"`.
    pub type_ref: String,
    /// The neutral `expr` body text, verbatim from the source.
    pub expr: Option<String>,
}

/// A mox package name carrier: the target of VocabularyInPackage edges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoxPackageNode {
    pub name: String,
}

/// The full mox domain model handed to `GraphIngestor::ingest_mox_domain`:
/// packages, vocabularies (with facets and vendored entries), class
/// operations, derived features, and the (class → schema title) links
/// resolved at ingest time by name matching.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MoxDomainModel {
    pub packages: Vec<MoxPackageNode>,
    pub vocabularies: Vec<MoxVocabularyNode>,
    pub operations: Vec<MoxOperationNode>,
    pub derived_features: Vec<MoxDerivedFeatureNode>,
    /// `(class, schema_title)` pairs linking mox classes to schema-ingested
    /// entities whose names match (title or suffix-stripped entity name).
    pub class_links: Vec<(String, String)>,
}
