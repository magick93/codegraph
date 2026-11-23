use serde::{Deserialize, Serialize};

/// The graph-native mirror of a pherolang `.ddd` design artifact
/// (`phero_ir::ddd::DddModel`, issue #449): one application with its modules,
/// class designs, repositories, application services, and search
/// definitions. codegraph-core takes no phero-* deps, so phero-ir signatures
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
    /// Modeled DTO boundaries (DD3, issue #667), module-major declaration
    /// order. Every `dto` design carries one — mirror, projection, and
    /// standalone alike — including the standalone form whose `Design::class`
    /// is `None` (it has no design node of its own). Empty for a model with
    /// no `dto` designs, so such a model behaves exactly as before.
    pub dto_shapes: Vec<DddDtoShapeNode>,
}

/// One modeled DTO boundary (phero-ir `DtoShape`, DD3 / pherolang#300, issue
/// #667): the transport shape a `DddTypeRef::Dto` signature slot references.
///
/// The shape carries its own members so generation never re-derives them:
/// a **mirror** (`dto User`) expands to every stored feature upstream, a
/// **projection** carries the authored select/derive members, and a
/// **standalone** DTO (`dto CreateBookRequest { … }`) has no `source` at all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddDtoShapeNode {
    /// The owning application name.
    pub application: String,
    /// The owning module name.
    pub module: String,
    /// The boundary name — the name a `DddTypeRef::Dto` references.
    pub name: String,
    /// The resolved domain class the boundary is built from, as phero-ir
    /// `TypeRef` JSON. `None` for a standalone DTO (codegraph-core takes no
    /// phero-* deps — the `type_json` discipline).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_json: Option<serde_json::Value>,
    /// The derived direction: `"input"` (the caller supplies the data) or
    /// `"output"` (the use case returns it — any derived member makes a
    /// boundary output-only, upstream).
    pub direction: String,
    /// Members in declaration order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<DddDtoMember>,
    /// Declaration order within the module (the owning design's ordinal —
    /// a standalone DTO has no design node but shares the design index
    /// space, so module-major readback is preserved).
    pub ordinal: usize,
}

/// One member of a [`DddDtoShapeNode`] (phero-ir `DtoMember`, DD3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddDtoMember {
    /// The member name as it appears on the boundary.
    pub name: String,
    /// How the member is populated: `"select"` (copied from the source
    /// feature named by [`Self::path`]), `"derive"` (computed from
    /// [`Self::expression`]), or `"declared"` (carried by the boundary
    /// alone).
    pub kind: String,
    /// The member's type as phero-ir `DddTypeRef` JSON — a domain type or
    /// another DTO boundary (`{"dto":"Name"}`).
    pub type_json: serde_json::Value,
    /// The declared cardinality as phero-ir `Multiplicity` JSON; `None` is
    /// single-valued.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiplicity_json: Option<serde_json::Value>,
    /// The selected source feature path (a single segment for a top-level
    /// feature, a dotted chain for a nested one). Empty for every
    /// non-`select` kind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<String>,
    /// The derivation's verbatim source text, for a `derive` member. The
    /// payload stays OPAQUE here (codegraph-core may not depend on
    /// phero-expr); generation re-parses it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
}

/// One declared error of the DD7 vocabulary (phero-ir `ErrorDecl`, issue
/// #665): the payload reuses the operation-parameter shape, so every field
/// type rides as phero-ir `TypeRef`/`Multiplicity` JSON (codegraph-core
/// takes no phero-* deps — the `type_json` discipline). An
/// application-level declaration rides [`DddApplicationNode::errors`]; a
/// module-level one rides [`DddModuleNode::errors`]. Error names are
/// application-wide unique upstream and may not shadow a prelude error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddErrorDecl {
    /// The error name exactly as declared.
    pub name: String,
    /// Human-readable description (the declaration's doc comment).
    /// Serde-defaulted: the pinned compiler does not capture docs yet.
    #[serde(default)]
    pub description: Option<String>,
    /// Payload fields in declaration order.
    #[serde(default)]
    pub fields: Vec<DddParam>,
}

/// The designed application (phero-ir `Application` plus its source path).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddApplicationNode {
    /// Application name.
    pub name: String,
    /// The default domain package name for unqualified references, when the
    /// design declares one.
    pub base: Option<String>,
    /// The `.ddd` source file the application was parsed from.
    pub source_path: String,
    /// The application's declared error vocabulary (DD7, issue #665) —
    /// visible to every module of the application. Serde-defaulted for
    /// read-compat with graphs persisted before the field existed.
    #[serde(default)]
    pub errors: Vec<DddErrorDecl>,
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
    /// The module's private error vocabulary (DD7, issue #665) — visible
    /// only inside this module. Serde-defaulted for read-compat with
    /// graphs persisted before the field existed.
    #[serde(default)]
    pub errors: Vec<DddErrorDecl>,
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
    /// The class's lifecycle state machine (`machine <field>: <Enum>`), as
    /// phero-ir `StateMachine` JSON. Opaque to codegraph-core (which must not
    /// depend on phero-ir); consumers that need the structured form
    /// deserialize it. Serde-defaulted for read-compat with graphs
    /// persisted before the field existed.
    #[serde(default)]
    pub machine_json: Option<serde_json::Value>,
    /// Child-scoped operations declared by the module's `children <Class>`
    /// blocks (phero-ir `Design::child_operations`, issue #661) — root-side
    /// operations over the aggregate's child collection, each carrying a
    /// [`DddChildRef`] in [`DddServiceOperation::child_scope`]. Serde-
    /// defaulted and omitted when empty, so a model with no `children`
    /// block serializes exactly as before.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub child_operations: Vec<DddServiceOperation>,
    /// The authored aggregate boundary of the design (DD4, issue #666 /
    /// pherolang#301): the effective aggregate-root verdict plus the
    /// resolved membership of a non-root entity, mirrored as ONE typed
    /// blob. `None` when both are undecided (non-entity designs, an
    /// incompletely lowered domain union) — a design with no authored
    /// boundary serializes exactly as before. Serde-defaulted for
    /// read-compat with graphs persisted before the field existed; a warm
    /// graph cache persisted earlier serves none, hence the
    /// GRAPH_FORMAT_VERSION bump that cold-re-ingests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregate_boundary: Option<DddAggregateBoundary>,
    /// The design's invariants (DD5, issue #666 / pherolang#302), in
    /// declaration order — the verbatim expression source is the identity
    /// (there is no name/index upstream). Sparse: a design without
    /// invariants serializes exactly as before. Serde-defaulted for
    /// read-compat with graphs persisted before the field existed (same
    /// GRAPH_FORMAT_VERSION story as `aggregate_boundary`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<DddInvariant>,
    /// The compiled machine IR of a REFERENCED machine (issue #815 /
    /// pherolang#388): the canonical `xstate_ir::Machine` JSON the authored
    /// TypeScript/XState file parsed to, exactly as the pherolang artifact
    /// embeds it (`machineIr` — keys sorted recursively upstream, so the
    /// bytes are already deterministic). Absent on inline machines (the
    /// DSL mirror in [`Self::machine_json`] covers those) and machine-less
    /// designs. Opaque to codegraph-core (no xstate deps here); consumers
    /// that need the structured form deserialize into
    /// `xstate_core::Machine` (which re-exports `xstate_ir`).
    /// Serde-defaulted for read-compat; a warm graph cache persisted
    /// pre-#815 serves none — hence the GRAPH_FORMAT_VERSION 14 → 15
    /// cold-re-ingest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine_ir_json: Option<serde_json::Value>,
    /// The provenance of [`Self::machine_ir_json`] (issue #815 /
    /// pherolang#388): `{ path, machineId, contentSha256 }` (camelCase,
    /// the phero-ir serde spelling) — the authored file path as resolved
    /// from the declaring `.ddd` file, the machine id, and the SHA-256
    /// over the canonical IR JSON. `None` when the design carries no
    /// referenced machine. Same GRAPH_FORMAT_VERSION story as the IR.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine_provenance_json: Option<serde_json::Value>,
    /// Declaration order within the module.
    pub ordinal: usize,
}

/// The authored aggregate boundary of a [`DddDesignNode`] (DD4, issue
/// #666): one typed blob carrying BOTH the aggregate-root verdict and the
/// resolved membership, so the Grafeo node persists exactly one JSON prop
/// (`aggregate_json`) per concern. Both halves are optional the way the
/// upstream payload is: a root carries the verdict alone, a member carries
/// both, and an undecided design carries no blob at all.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DddAggregateBoundary {
    /// The effective aggregate-root verdict: `Some(true)` for a root,
    /// `Some(false)` for a member, `None` when unknown (the imported
    /// domain union did not fully lower).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_root: Option<bool>,
    /// The resolved aggregate membership of a **non-root** entity design,
    /// with the decision's origin. Absent on roots and non-entities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub membership: Option<DddAggregateMembership>,
}

/// The aggregate membership of a non-root entity design (phero-ir
/// `AggregateMembership`, issue #666): the resolved root class and how the
/// placement was decided. The root rides as phero-ir `TypeRef` JSON
/// (codegraph-core takes no phero-* deps — the `type_json` discipline).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DddAggregateMembership {
    /// The resolved aggregate root class, as phero-ir `TypeRef` JSON.
    pub root_json: serde_json::Value,
    /// Whether containment or an authored declaration placed the member.
    pub origin: DddAggregateOrigin,
}

/// How an entity design's aggregate boundary was decided (phero-ir
/// `AggregateOrigin`, issue #666).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DddAggregateOrigin {
    /// The design authored the boundary (`aggregate <Root>`).
    Authored,
    /// The containment closure decided the boundary.
    Derived,
}

/// One invariant of a [`DddDesignNode`] (DD5, issue #666): the verbatim
/// source text between `invariant` and `;`. Typing is proven upstream; the
/// consumer re-parses with the shared expression semantics. Declaration
/// order is the identity (there is no name/index upstream).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DddInvariant {
    /// The invariant expression, verbatim.
    pub expression: String,
}

/// The design flags of a [`DddDesignNode`], all defaulting to `false`
/// (phero-ir `DesignFlags`).
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

/// The repository designed for a class (phero-ir `Repository`).
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
/// payloads are phero-ir `TypeRef`/`Multiplicity` JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddRepositoryOperation {
    /// Operation name, unique within its repository.
    pub name: String,
    /// The built-in op when this is one: `"findById" | "findAll" |
    /// "findByExample" | "findByKeys" | "save" | "delete"`.
    pub builtin: Option<String>,
    /// The declared return type as phero-ir `TypeRef` JSON.
    pub return_type: Option<serde_json::Value>,
    /// The declared return cardinality as phero-ir `Multiplicity` JSON.
    pub return_multiplicity: Option<serde_json::Value>,
    /// Declared parameters in declaration order.
    pub params: Vec<DddParam>,
    /// Declaration order within the repository.
    pub ordinal: usize,
    /// Sculptor `protected` visibility: the operation stays off the public
    /// interface (it lowers onto the repository but maps no API operation).
    #[serde(default)]
    pub is_protected: bool,
    /// The typed read-only query shape (DD9, issue #659), as phero-ir
    /// `QuerySpec` JSON — predicate source text, resolved sort keys,
    /// pagination, and `readOnly`. Opaque to codegraph-core (which must not
    /// depend on phero-ir); `codegraph-generate` rehydrates it and lowers
    /// the predicate to parameterized SQL. `None` on every non-query
    /// operation. Serde-defaulted for read-compat with graphs persisted
    /// before the field existed.
    #[serde(default)]
    pub query_json: Option<serde_json::Value>,
    /// The resolved actor-capability names guarding the query (DD9's read
    /// authorization), in declaration order. Serde-defaulted for read-compat
    /// with graphs persisted before the field existed.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// One declared parameter: a phero-ir `OperationParam` with the signature
/// types as JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddParam {
    /// Parameter name.
    pub name: String,
    /// The parameter type as phero-ir `TypeRef` JSON.
    pub type_json: serde_json::Value,
    /// The parameter cardinality as phero-ir `Multiplicity` JSON.
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
    /// Declared `inject decision` dependencies: decision-service names
    /// (`phero_ir::ddd::Service::decision_dependencies`, pherolang #78's
    /// `.dmn` surface) this service pools as decision dependencies, as
    /// authored. Serde-defaulted for read-compat with graphs persisted
    /// before the field existed.
    #[serde(default)]
    pub decision_dependencies: Vec<String>,
    /// Service operations in declaration order.
    pub operations: Vec<DddServiceOperation>,
    /// Declaration order within the module.
    pub ordinal: usize,
}

/// An operation of a [`DddServiceNode`]. Signature payloads are phero-ir
/// `TypeRef`/`Multiplicity` JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddServiceOperation {
    /// Operation name, unique within its service.
    pub name: String,
    /// The resolved return type as phero-ir `TypeRef` JSON; `None` for
    /// delegation operations whose signature is copied from the target.
    pub return_type: Option<serde_json::Value>,
    /// The declared return cardinality as phero-ir `Multiplicity` JSON.
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
    /// The `when (...)` guard condition's source text, verbatim between
    /// the parentheses (issue #590): the route-level transition guard,
    /// lowered over the loaded record. Serde-defaulted for read-compat
    /// with graphs persisted before the field existed.
    #[serde(default)]
    pub condition: Option<String>,
    /// Event names the `emits` clause declares (pherolang #114's emission
    /// bindings): loosely-coupled references into the events plane, not
    /// validated at the wire layer. Serde-defaulted for read-compat.
    #[serde(default)]
    pub emits: Vec<String>,
    /// Decision-service names the `invokes` clause declares (pherolang #78's
    /// `.dmn` surface): loosely-coupled references into the decision
    /// plane, not validated at the wire layer. Serde-defaulted for
    /// read-compat.
    #[serde(default)]
    pub invokes: Vec<String>,
    /// The resolved error vocabulary the operation's `throws` clause
    /// declares (DD7, issue pherolang#304), as phero-ir `ErrorRef` JSON.
    /// Serde-defaulted for read-compat; a raise outside this set cannot
    /// occur upstream (the driver typechecks it).
    #[serde(default)]
    pub throws: Vec<serde_json::Value>,
    /// Checked executable bodies keyed by target name (issue pherolang#178):
    /// the `"stmt"` entry holds the canonical `phero_expr::stmt::StmtBody`
    /// JSON. The payload stays OPAQUE here (codegraph-core may not depend on
    /// phero-expr); generation re-parses and lowers it. Serde-defaulted for
    /// read-compat — an operation without a body omits the map.
    #[serde(default)]
    pub bodies: std::collections::BTreeMap<String, String>,
    /// The `=> <Repository>` receiver a statement body binds as `self`
    /// (the primary repository the body orchestrates). `None` for every
    /// operation without a body receiver.
    #[serde(default)]
    pub body_receiver: Option<String>,
    /// The explicit transition binding of a delegated operation
    /// (`name => Repository.save transition <Trigger>`, pherolang#176,
    /// issue #660). When present it is the PRIMARY selector for the
    /// operation's machine transition; the name/`when`/capability
    /// heuristics are the no-binding fallback only. Serde-defaulted and
    /// omitted when absent, so a model declaring no binding serializes
    /// exactly as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<DddTransitionRef>,
    /// The child-scoped target of a `children <Class>` operation (phero-ir
    /// `ChildRef`, issue #661): the root's child feature, the resolved child
    /// class, and the validated child action (`add`/`remove`/`save`).
    /// Serde-defaulted and omitted when absent, so every ordinary service
    /// operation serializes exactly as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_scope: Option<DddChildRef>,
    /// The resolved field paths of the `when` condition (phero-ir
    /// `condition_paths`, issue pherolang#180 / #663): injected-scope paths
    /// always (the repository→class binding is design knowledge), own-record
    /// paths only in a module with several machine-bearing designs.
    /// Serde-defaulted and omitted when empty, so every pre-#663 operation
    /// serializes exactly as before. A warm graph cache persisted before
    /// this field existed serves none — hence the GRAPH_FORMAT_VERSION
    /// bump that cold-re-ingests.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub condition_paths: Vec<DddConditionPath>,
    /// Declaration order within the service.
    pub ordinal: usize,
}

/// The resolved scope of one [`DddConditionPath`] (phero-ir
/// `ConditionScope`, issue pherolang#180 / #663): a field of the op's own
/// record, or a field reached through an injected repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DddConditionScope {
    /// A field of the op's bound entity.
    OwnRecord,
    /// A field reached through an injected repository: the injection name
    /// exactly as declared (`inject TimecardRepository;` — the condition
    /// path's root) and the class the repository design binds,
    /// package-qualified.
    Injected {
        /// The injection name exactly as declared.
        repository: String,
        /// The class the repository design binds (package-qualified).
        class: String,
    },
}

/// One resolved field path of an operation's `when` condition (phero-ir
/// `ConditionPath`, issue pherolang#180 / #663): where the path resolves,
/// the field the dotted remainder names, and the field's declared type.
/// The lowering contract consumers build on, so they never re-parse the
/// condition source string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DddConditionPath {
    /// Where the path resolves.
    pub scope: DddConditionScope,
    /// The field path — one segment (`timecardStatus`) or a dotted chain
    /// (`holder.name`) under the scope's root.
    pub field: String,
    /// The field's declared type as phero-ir `TypeRef` JSON. The payload
    /// stays OPAQUE here (codegraph-core may not depend on phero-ir);
    /// generation rehydrates it when it needs the slot type.
    pub field_type: serde_json::Value,
}

/// The child-scoped target of a `children <Class>` operation (phero-ir
/// `ChildRef`, issue #661).
///
/// The action vocabulary is CLOSED and validated upstream: `add` (append an
/// element to a many-valued feature), `remove` (unlink one), `save` (persist
/// one — the only action legal on a single-valued feature).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DddChildRef {
    /// The root class's child feature name, exactly as authored (`lines`).
    pub feature: String,
    /// The child class the feature resolves to, package-qualified
    /// (`nz.example.payroll.PayRunLineType`).
    pub class: String,
    /// The validated child action: `"add"`, `"remove"`, or `"save"`.
    pub action: String,
}

/// An authored `transition <Trigger>` binding (phero-ir `TransitionRef`,
/// issue #660): the machine-bound transition a `save` delegation fires.
///
/// The machine qualifier names the machine's **enum type** (`PayRunStatus`),
/// never the machine field — Phero Lang machine declarations are
/// field-named, so an enum-type qualifier is the only machine
/// qualification the surface has. `None` when the source wrote the
/// unqualified spelling; a module with more than one machine-bearing design
/// requires it (upstream E0352).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DddTransitionRef {
    /// The trigger name as authored — an explicit `via` name or a bare
    /// arrow's derived name (the lowerCamel of the target state).
    pub trigger: String,
    /// The machine's enum type name, when the source qualified the
    /// binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
}

/// A search definition designed over one entity (phero-ir `SearchDef`): the
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

/// A full-text field of a [`DddSearchNode`] (phero-ir `SearchField`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddSearchField {
    /// The entity property whose text is indexed.
    pub property: String,
    /// Relevance boost multiplier; ordered comparison only (phero-ir wraps
    /// the `f32` in an ordered newtype).
    pub boost: Option<f32>,
    /// Analyzer override for this field.
    pub analyzer: Option<String>,
}

/// A computed projection entry of a search document (phero-ir
/// `DocumentField`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddDocumentField {
    /// The entry's name in the search document.
    pub name: String,
    /// The verbatim phero-expr source computing the value.
    pub expr: String,
}

/// The pagination knobs of a [`DddSearchNode`] (phero-ir `Pagination`).
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
