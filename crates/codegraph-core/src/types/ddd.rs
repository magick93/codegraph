use serde::{Deserialize, Serialize};

/// The graph-native mirror of a rexlang `.ddd` design artifact
/// (`rex_ir::ddd::DddModel`, issue #449): one application with its modules,
/// class designs, repositories, application services, and search
/// definitions. codegraph-core takes no rex-* deps, so rex-ir signatures
/// (`TypeRef`, `Multiplicity`) cross this boundary as pre-serialized
/// `serde_json::Value` payloads (the `GrantEdge::expr_json` precedent) and
/// class references stay as-authored strings, resolved against the schema
/// graph at ingest (`resolved_title`/`entity_title`).
#[derive(Debug, Clone, PartialEq)]
pub struct DddModelGraph {
    /// The `.ddd` source file the model was parsed from.
    pub source_path: String,
    /// The designed application.
    pub application: DddApplicationNode,
    /// Modules in declaration order.
    pub modules: Vec<DddModuleNode>,
    /// Class designs across all modules, in declaration order.
    pub designs: Vec<DddDesignNode>,
    /// Repositories across all designs, in declaration order.
    pub repositories: Vec<DddRepositoryNode>,
    /// Application services across all modules, in declaration order.
    pub services: Vec<DddServiceNode>,
    /// Search definitions across all modules, in declaration order.
    pub searches: Vec<DddSearchNode>,
}

/// The designed application (rex-ir `Application` plus its source path).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddApplicationNode {
    /// Application name.
    pub name: String,
    /// The default domain package name for unqualified references, when the
    /// design declares one.
    pub base: Option<String>,
    /// The `.ddd` source file the application was parsed from.
    pub source_path: String,
}

/// A module of the application: a cohesive slice of services, designed
/// classes, and search definitions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddModuleNode {
    /// The owning application name.
    pub application: String,
    /// Module name.
    pub name: String,
    /// Declaration order within the application.
    pub ordinal: usize,
}

/// The design decision for one referenced `.mox` class: its stereotype,
/// flags, and the schema title the class resolved to at ingest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddDesignNode {
    /// The owning application name.
    pub application: String,
    /// The owning module name.
    pub module: String,
    /// The referenced class name exactly as authored in the design
    /// (unqualified or package-qualified).
    pub class: String,
    /// The schema title the class resolved to at ingest; `None` when the
    /// class is not (yet) in the schema graph.
    pub resolved_title: Option<String>,
    /// The DDD stereotype: `"entity" | "value" | "dto"`.
    pub stereotype: String,
    /// Whether the class is abstract.
    pub is_abstract: bool,
    /// The design flags.
    pub flags: DddDesignFlags,
    /// Declaration order within the module.
    pub ordinal: usize,
}

/// The design flags of a [`DddDesignNode`], all defaulting to `false`
/// (rex-ir `DesignFlags`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DddDesignFlags {
    /// Generate scaffolding for the class.
    pub scaffold: bool,
    /// Record an audit trail for the class's mutations.
    pub auditable: bool,
    /// Optimistic locking on the class's persistent state.
    pub optimistic_locking: bool,
    /// The class is never persisted.
    pub non_persistent: bool,
    /// Cache instances of the class.
    pub cache: bool,
}

/// The repository designed for a class (rex-ir `Repository`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddRepositoryNode {
    /// The owning application name.
    pub application: String,
    /// Repository name.
    pub name: String,
    /// The designing class, exactly as authored.
    pub design_class: String,
    /// Repository operations in declaration order.
    pub operations: Vec<DddRepositoryOperation>,
}

/// An operation of a [`DddRepositoryNode`]: exactly one of a built-in op
/// (whose signature the consumer knows) or a declared signature. Signature
/// payloads are rex-ir `TypeRef`/`Multiplicity` JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddRepositoryOperation {
    /// Operation name, unique within its repository.
    pub name: String,
    /// The built-in op when this is one: `"findById" | "findAll" |
    /// "findByExample" | "findByKeys" | "save" | "delete"`.
    pub builtin: Option<String>,
    /// The declared return type as rex-ir `TypeRef` JSON.
    pub return_type: Option<serde_json::Value>,
    /// The declared return cardinality as rex-ir `Multiplicity` JSON.
    pub return_multiplicity: Option<serde_json::Value>,
    /// Declared parameters in declaration order.
    pub params: Vec<DddParam>,
    /// Declaration order within the repository.
    pub ordinal: usize,
    /// Sculptor `protected` visibility: the operation stays off the public
    /// interface (it lowers onto the repository but maps no API operation).
    #[serde(default)]
    pub is_protected: bool,
}

/// One declared parameter: a rex-ir `OperationParam` with the signature
/// types as JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddParam {
    /// Parameter name.
    pub name: String,
    /// The parameter type as rex-ir `TypeRef` JSON.
    pub type_json: serde_json::Value,
    /// The parameter cardinality as rex-ir `Multiplicity` JSON.
    pub multiplicity: Option<serde_json::Value>,
}

/// An application service: stateless use-case orchestration over
/// repositories and other services.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddServiceNode {
    /// The owning application name.
    pub application: String,
    /// The owning module name.
    pub module: String,
    /// Service name.
    pub name: String,
    /// Human-readable description of the service's responsibility.
    pub description: Option<String>,
    /// Declared `inject` dependencies: names of repositories and other
    /// services this service delegates to, as authored.
    pub dependencies: Vec<String>,
    /// Service operations in declaration order.
    pub operations: Vec<DddServiceOperation>,
    /// Declaration order within the module.
    pub ordinal: usize,
}

/// An operation of a [`DddServiceNode`]. Signature payloads are rex-ir
/// `TypeRef`/`Multiplicity` JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddServiceOperation {
    /// Operation name, unique within its service.
    pub name: String,
    /// The resolved return type as rex-ir `TypeRef` JSON; `None` for
    /// delegation operations whose signature is copied from the target.
    pub return_type: Option<serde_json::Value>,
    /// The declared return cardinality as rex-ir `Multiplicity` JSON.
    pub return_multiplicity: Option<serde_json::Value>,
    /// Parameters in declaration order.
    pub params: Vec<DddParam>,
    /// The injected dependency receiving the call, when this operation
    /// forwards to a dependency's operation.
    pub delegation_target: Option<String>,
    /// The operation invoked on [`DddServiceOperation::delegation_target`].
    pub delegation_operation: Option<String>,
    /// Actor capability names guarding this operation, as authored.
    pub capabilities: Vec<String>,
    /// Sculptor `protected` visibility: the operation stays off the public
    /// interface.
    #[serde(default)]
    pub is_protected: bool,
    /// Declaration order within the service.
    pub ordinal: usize,
}

/// A search definition designed over one entity (rex-ir `SearchDef`): the
/// indexed full-text fields, the structured filter and sort keys, the
/// computed document projection, and the ranking/analyzer/pagination knobs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddSearchNode {
    /// The owning application name.
    pub application: String,
    /// The owning module name.
    pub module: String,
    /// Search name, unique within its module.
    pub name: String,
    /// Human-readable description of what the search finds.
    pub description: Option<String>,
    /// The referenced entity class exactly as authored.
    pub entity_class: String,
    /// The schema title the entity resolved to at ingest; `None` when the
    /// class is not (yet) in the schema graph.
    pub entity_title: Option<String>,
    /// Full-text fields in declaration order.
    pub text: Vec<DddSearchField>,
    /// Structured filter keys: entity property names in declaration order.
    pub filters: Vec<String>,
    /// Structured sort keys: entity property names in declaration order.
    pub sorts: Vec<String>,
    /// Computed projection entries of the search document, in declaration
    /// order.
    pub document: Vec<DddDocumentField>,
    /// The relevance ranking strategy: `"bm25" | "tfIdf" | "exact"` or a
    /// custom strategy name.
    pub ranking: Option<String>,
    /// The default analyzer applied to the indexed text.
    pub analyzer: Option<String>,
    /// Pagination knobs.
    pub pagination: Option<DddPagination>,
    /// Actor capability names guarding the search, as authored.
    pub capabilities: Vec<String>,
    /// Declaration order within the module.
    pub ordinal: usize,
}

/// A full-text field of a [`DddSearchNode`] (rex-ir `SearchField`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddSearchField {
    /// The entity property whose text is indexed.
    pub property: String,
    /// Relevance boost multiplier; ordered comparison only (rex-ir wraps
    /// the `f32` in an ordered newtype).
    pub boost: Option<f32>,
    /// Analyzer override for this field.
    pub analyzer: Option<String>,
}

/// A computed projection entry of a search document (rex-ir
/// `DocumentField`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddDocumentField {
    /// The entry's name in the search document.
    pub name: String,
    /// The verbatim rex-expr source computing the value.
    pub expr: String,
}

/// The pagination knobs of a [`DddSearchNode`] (rex-ir `Pagination`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DddPagination {
    /// The default page size.
    pub limit: Option<u32>,
    /// The maximum page size a query may request.
    pub max_limit: Option<u32>,
    /// The search pages by cursor (keyset) rather than by offset.
    pub cursor: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_design_flags_are_all_false() {
        let flags = DddDesignFlags::default();
        assert!(!flags.scaffold);
        assert!(!flags.auditable);
        assert!(!flags.optimistic_locking);
        assert!(!flags.non_persistent);
        assert!(!flags.cache);
        assert_eq!(flags, DddDesignFlags::default());
    }
}
