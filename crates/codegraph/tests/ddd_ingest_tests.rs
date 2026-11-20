//! DDD design-plane ingest tests (issue #449 Phase 2): the phero-ir to
//! `DddModelGraph` conversion (module-major ordinals, every payload),
//! class title resolution (exact, then +type_suffix, then miss), the
//! end-to-end `ingest_ddd_files` run over a fixture dir (mox import,
//! `import schema`, and `import sigil` with the sibling candidate pool),
//! hard-error semantics, and the graph-cache inputs group.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use codegraph_core::mock::MockEngine;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{
    DddAggregateBoundary, DddAggregateMembership, DddAggregateOrigin, DddApplicationNode,
    DddDesignFlags, DddDesignNode, DddDocumentField, DddInvariant, DddModelGraph, DddModuleNode,
    DddPagination, DddRepositoryNode, DddSearchField, SchemaNode,
};
use phero_ir::ddd::{
    AggregateMembership, AggregateOrigin, Application, BuiltinRepositoryOp, DddModel, DddParam,
    DddTypeRef, Delegation, Design, DesignFlags, DocumentField, ErrorDecl, ErrorRef, Invariant,
    Module, Pagination, RankingStrategy, Repository, RepositoryOperation, SearchDef, SearchField,
    SearchFilter, SearchSort, Service, ServiceOperation, Stereotype,
};
use phero_ir::{Multiplicity, PrimitiveType, TypeRef, Upper};

// ── phero-ir fixture ─────────────────────────────────────────────────────

fn class_ref(package: &str, name: &str) -> TypeRef {
    TypeRef::Class {
        package: package.to_string(),
        name: name.to_string(),
    }
}

/// A `DddParam` (DD3: the signature slot is a `DddTypeRef`).
fn param(name: &str, type_: TypeRef, multiplicity: Option<Multiplicity>) -> DddParam {
    DddParam::new(name, DddTypeRef::domain(type_)).with_multiplicity(multiplicity)
}

/// Two modules: `lending` (entity/value/dto designs — the entity carries
/// all five flags and a repository with the four builtins plus a declared
/// finder — and a service with a declared op and a delegation), and
/// `discovery` (one more entity design plus a fully-loaded search).
///
/// Built with the upstream constructors (`Design::new`, `Service::new`,
/// `ServiceOperation::new`, `RepositoryOperation::builtin/declared`,
/// `SearchDef::new`) so a new DD-epic field cannot silently unset a
/// semantic here; the few fields without setters are assigned directly.
fn sample_model() -> DddModel {
    let mut book = Design::new("nz.example.library::Book", Stereotype::Entity);
    book.flags = DesignFlags {
        scaffold: true,
        auditable: true,
        optimistic_locking: true,
        non_persistent: false,
        cache: true,
    };
    book.repository = Some(Repository {
        name: "BookRepository".to_string(),
        operations: vec![
            RepositoryOperation::builtin("findById", BuiltinRepositoryOp::FindById),
            RepositoryOperation::builtin("findAll", BuiltinRepositoryOp::FindAll),
            RepositoryOperation::builtin("save", BuiltinRepositoryOp::Save),
            RepositoryOperation::builtin("delete", BuiltinRepositoryOp::Delete),
            RepositoryOperation::declared(
                "byTitle",
                Some(DddTypeRef::domain(TypeRef::Primitive(
                    PrimitiveType::String,
                ))),
                vec![param(
                    "title",
                    TypeRef::Primitive(PrimitiveType::String),
                    Some(Multiplicity::MANY),
                )],
            )
            .with_return_multiplicity(Some(Multiplicity {
                lower: 0,
                upper: Upper::Finite(5),
            })),
            // DD9 (issue #659): the typed query block rides the graph as
            // phero-ir `QuerySpec` JSON, plus the resolved read capability.
            RepositoryOperation::declared(
                "recent",
                Some(DddTypeRef::domain(class_ref("nz.example.library", "Book"))),
                vec![param(
                    "minimum",
                    TypeRef::Primitive(PrimitiveType::Int),
                    None,
                )],
            )
            .with_return_multiplicity(Some(Multiplicity::MANY))
            .with_query(Some(
                phero_ir::ddd::QuerySpec::new("self.title != null && self.title != title")
                    .with_sort(vec![phero_ir::ddd::QuerySortKey::new(
                        "title",
                        TypeRef::Primitive(PrimitiveType::String),
                        phero_ir::ddd::SortDirection::Desc,
                    )])
                    .with_pagination(Some(
                        phero_ir::ddd::QueryPagination::new()
                            .with_limit(Some(20))
                            .with_max(Some(100)),
                    )),
            ))
            .with_capabilities(vec![phero_ir::ddd::CapabilityRef::new("ReadBooks")]),
        ],
    });
    // DD4/DD5 (issue #666): the authored aggregate-root verdict and the
    // design's invariants ride the phero-ir `Design` payload; built with
    // the upstream constructors' field assignments so the conversion must
    // mirror them (not invent defaults).
    book.is_aggregate_root = Some(true);
    book.invariants = vec![
        Invariant::new("self.title != null"),
        Invariant::new("self.copies >= 0"),
    ];

    let money = Design::new("Money", Stereotype::Value);
    let mut summary = Design::new("shop::reporting::LoanSummary", Stereotype::Dto);
    summary.is_abstract = true;
    summary.flags = DesignFlags {
        non_persistent: true,
        ..DesignFlags::default()
    };

    let mut chapter = Design::new("nz.example.library.Chapter", Stereotype::Entity);
    // DD4: an authored aggregate member — the root + the decision origin
    // ride `aggregate`; the root verdict itself is `false`.
    chapter.is_aggregate_root = Some(false);
    chapter.aggregate = Some(AggregateMembership {
        root: class_ref("nz.example.library", "Book"),
        origin: AggregateOrigin::Authored,
    });

    let mut borrow = ServiceOperation::new("borrow");
    borrow.return_type = Some(DddTypeRef::domain(TypeRef::Primitive(
        PrimitiveType::Boolean,
    )));
    borrow.params = vec![param("book", class_ref("nz.example.library", "Book"), None)];
    borrow.capabilities = vec!["BorrowBooks".to_string()];

    let mut renew = ServiceOperation::new("renew");
    renew.delegation = Some(Delegation {
        target: "LoanRepository".to_string(),
        target_ref: None,
        operation: "save".to_string(),
    });
    renew.condition = Some(r#"status == "OnLoan""#.to_string());
    // Issue #662: the checked `.ddd` statement body rides the graph as the
    // opaque canonical `StmtBody` payload, alongside the `throws` set and
    // the `=> <Repository>` body receiver.
    renew.bodies.insert(
        "stmt".to_string(),
        r#"{"stmts":[{"kind":"guard","expr":"run.total > 0","error":{"variant":"Conflict","args":["\"total\""]}}]}"#
            .to_string(),
    );
    renew.body_receiver = Some("LoanRepository".to_string());
    renew.throws = vec![ErrorRef::prelude("Conflict")];

    let mut lending = Module::new("lending");
    lending.designs = vec![book, money, summary];
    lending.services = vec![Service {
        name: "LoanService".to_string(),
        description: Some("Coordinates lending.".to_string()),
        operations: vec![borrow, renew],
        dependencies: vec![
            "LoanRepository".to_string(),
            "NotificationService".to_string(),
        ],
        dependency_refs: Vec::new(),
        decision_dependencies: Vec::new(),
        decision_dependency_refs: Vec::new(),
        decision_dependency_pins: Vec::new(),
    }];

    let mut book_search = SearchDef::new("BookSearch", "nz.example.library::Book");
    book_search.description = Some("Full-text catalogue search".to_string());
    book_search.text = vec![
        SearchField::new("title")
            .with_boost(2.5)
            .with_analyzer("standard"),
        SearchField::new("synopsis"),
    ];
    book_search.filters = vec![SearchFilter::new("category")];
    book_search.sort = vec![SearchSort::new("title")];
    book_search.document = vec![DocumentField::new("label", r#"title + " - " + synopsis"#)];
    book_search.ranking = Some(RankingStrategy::Custom("recency".to_string()));
    book_search.analyzer = Some("english".to_string());
    book_search.pagination = Some(Pagination {
        limit: Some(20),
        max_limit: Some(100),
        cursor: true,
    });
    book_search.capabilities = vec!["SearchBooks".to_string()];

    let mut discovery = Module::new("discovery");
    discovery.designs = vec![chapter];
    discovery.searches = vec![book_search];

    // DD7 (issue #665): the declared error vocabulary, both scopes. A
    // module-private declaration and an application-wide one, each with a
    // payload field.
    lending.errors = vec![ErrorDecl {
        name: "LineLocked".to_string(),
        description: None,
        fields: vec![DddParam {
            name: "reason".to_string(),
            type_: DddTypeRef::Domain(TypeRef::Primitive(PrimitiveType::String)),
            multiplicity: None,
        }],
    }];
    let mut model = DddModel::new()
        .application(Application {
            name: "Library".to_string(),
            base: Some("nz.example.library".to_string()),
        })
        .module(lending)
        .module(discovery);
    model.errors = vec![ErrorDecl {
        name: "PaymentDeclined".to_string(),
        description: None,
        fields: vec![DddParam {
            name: "reason".to_string(),
            type_: DddTypeRef::Domain(TypeRef::Enum {
                package: "nz.example.library".to_string(),
                name: "DeclineReason".to_string(),
            }),
            multiplicity: None,
        }],
    }];
    model
}

#[test]
fn standalone_dto_boundaries_ingest_as_shapes_not_design_nodes() {
    // DD3 (issue #667): a `dto` design with no `.mox` source has `class: None`
    // and is described entirely by `dtoShape`. It is a transport boundary, not
    // a domain class, so it never becomes a design node — but its SHAPE is the
    // consumable surface and must ride the graph. The skip must not shift the
    // class-backed design's ordinal off its declaration index.
    let mut module = Module::new("reporting");
    module.designs = vec![
        Design::new("Book", Stereotype::Entity),
        Design::standalone_dto("CreateBookRequest"),
    ];
    let model = DddModel::new().module(module);

    let graph =
        codegraph::ingest::ddd_ingest::ddd_model_graph_from_phero(&model, "design.ddd", &|_| None);

    assert_eq!(graph.designs.len(), 1, "only class-backed designs map");
    assert_eq!(graph.designs[0].class, "Book");
    assert_eq!(graph.designs[0].ordinal, 0);
    assert_eq!(graph.designs[0].stereotype, "entity");
    assert_eq!(graph.dto_shapes.len(), 1, "the boundary shape still lands");
    assert_eq!(graph.dto_shapes[0].name, "CreateBookRequest");
    assert_eq!(graph.dto_shapes[0].module, "reporting");
    assert_eq!(graph.dto_shapes[0].direction, "output");
    assert!(
        graph.dto_shapes[0].source_json.is_none(),
        "a standalone boundary has no source class"
    );
    assert_eq!(
        graph.dto_shapes[0].ordinal, 1,
        "the shape keeps the owning design's declaration ordinal"
    );
}

#[test]
fn dto_shape_members_and_direction_mirror_the_artifact() {
    // DD3 (issue #667): a projection's selected and derived members ride the
    // graph verbatim (kinds, paths, expressions, resolved types), and the
    // derived direction follows upstream (any derive ⇒ output).
    let shape = phero_ir::ddd::DtoShape::new("BookSummary", phero_ir::ddd::DtoDirection::Output)
        .with_source(Some(TypeRef::Class {
            package: "nz.example.library".to_string(),
            name: "Book".to_string(),
        }))
        .with_members(vec![
            phero_ir::ddd::DtoMember::new(
                "title",
                phero_ir::ddd::DtoMemberKind::Select,
                phero_ir::ddd::DddTypeRef::domain(TypeRef::Primitive(PrimitiveType::String)),
            )
            .with_path(vec!["title".to_string()]),
            phero_ir::ddd::DtoMember::new(
                "minutes",
                phero_ir::ddd::DtoMemberKind::Derive,
                phero_ir::ddd::DddTypeRef::domain(TypeRef::Primitive(PrimitiveType::Int)),
            )
            .with_expression(Some("playLength / 60".to_string())),
        ]);
    let mut module = Module::new("reporting");
    module.designs = vec![
        Design::new("Book", Stereotype::Entity),
        Design::new("Book", Stereotype::Dto).with_dto_shape(Some(shape)),
    ];
    let model = DddModel::new().module(module);

    let graph =
        codegraph::ingest::ddd_ingest::ddd_model_graph_from_phero(&model, "design.ddd", &|_| {
            Some("BookType".to_string())
        });

    assert_eq!(graph.dto_shapes.len(), 1);
    let shape = &graph.dto_shapes[0];
    assert_eq!(shape.name, "BookSummary");
    assert_eq!(shape.direction, "output");
    assert_eq!(
        shape.source_json.as_ref().and_then(|s| s.get("type")),
        Some(&serde_json::json!("class"))
    );
    assert_eq!(shape.members.len(), 2);
    assert_eq!(shape.members[0].name, "title");
    assert_eq!(shape.members[0].kind, "select");
    assert_eq!(shape.members[0].path, vec!["title".to_string()]);
    assert_eq!(shape.members[1].name, "minutes");
    assert_eq!(shape.members[1].kind, "derive");
    assert_eq!(
        shape.members[1].expression.as_deref(),
        Some("playLength / 60")
    );
    assert!(
        shape.members[1].multiplicity_json.is_none(),
        "a single-valued member carries no multiplicity payload"
    );
}

/// Book resolves via the +`Type` suffix (the mox bridge convention), Money
/// is exact, everything else is unresolved.
fn sample_resolver() -> impl Fn(&str) -> Option<String> {
    |name: &str| match name {
        "Book" => Some("BookType".to_string()),
        "Money" => Some("Money".to_string()),
        _ => None,
    }
}

fn type_json(value: &TypeRef) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
}

fn mult_json(value: &Multiplicity) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
}

// ── conversion ─────────────────────────────────────────────────────────

#[test]
fn conversion_maps_module_major_order_and_every_payload() {
    let model = sample_model();
    let graph = codegraph::ingest::ddd_ingest::ddd_model_graph_from_phero(
        &model,
        "design.ddd",
        &sample_resolver(),
    );

    assert_eq!(graph.source_path, "design.ddd");
    // DD7 (issue #665): the application-scope vocabulary mirrors with its
    // payload fields.
    assert_eq!(graph.application.errors.len(), 1);
    assert_eq!(graph.application.errors[0].name, "PaymentDeclined");
    assert_eq!(graph.application.errors[0].fields.len(), 1);
    assert_eq!(graph.application.errors[0].fields[0].name, "reason");

    // Modules in declaration order with their own ordinals.
    let lending_errors = graph
        .modules
        .iter()
        .find(|m| m.name == "lending")
        .map(|m| m.errors.clone())
        .unwrap();
    assert_eq!(lending_errors.len(), 1);
    assert_eq!(lending_errors[0].name, "LineLocked");
    assert_eq!(lending_errors[0].fields.len(), 1);
    assert!(
        graph
            .modules
            .iter()
            .all(|m| m.name == "discovery" || m.errors.len() == 1),
        "only lending declares module-scope errors"
    );

    // Designs: module-major (lending's three, then discovery's one), with
    // per-module ordinals and resolved titles from the resolver.
    assert_eq!(graph.designs.len(), 4);
    let book = &graph.designs[0];
    assert_eq!(book.module, "lending");
    assert_eq!(book.class, "nz.example.library::Book");
    assert_eq!(book.resolved_title.as_deref(), Some("BookType"));
    assert_eq!(book.stereotype, "entity");
    assert!(!book.is_abstract);
    assert!(book.flags.scaffold && book.flags.auditable && book.flags.optimistic_locking);
    assert!(!book.flags.non_persistent && book.flags.cache);
    assert_eq!(book.ordinal, 0);

    let money = &graph.designs[1];
    assert_eq!(money.stereotype, "value");
    assert_eq!(money.resolved_title.as_deref(), Some("Money"));
    assert_eq!(money.flags, DddDesignFlags::default());
    assert_eq!(money.ordinal, 1);

    let summary = &graph.designs[2];
    assert_eq!(summary.stereotype, "dto");
    assert_eq!(summary.class, "shop::reporting::LoanSummary");
    assert_eq!(summary.resolved_title, None, "resolver miss → None");
    assert!(summary.is_abstract);
    assert!(summary.flags.non_persistent);
    assert_eq!(summary.ordinal, 2);

    let chapter = &graph.designs[3];
    assert_eq!(chapter.module, "discovery");
    assert_eq!(chapter.class, "nz.example.library.Chapter");
    assert_eq!(chapter.ordinal, 0, "ordinals restart per module");

    // DD4/DD5 (issue #666): the authored aggregate boundary rides as ONE
    // typed blob (`aggregate_boundary`) — the root verdict and the member
    // membership keep their authored semantics, the membership's root
    // crossing as phero-ir `TypeRef` JSON.
    assert_eq!(
        book.aggregate_boundary,
        Some(DddAggregateBoundary {
            is_root: Some(true),
            membership: None,
        }),
        "an authored root carries the verdict, never a membership"
    );
    assert_eq!(
        chapter.aggregate_boundary,
        Some(DddAggregateBoundary {
            is_root: Some(false),
            membership: Some(DddAggregateMembership {
                root_json: type_json(&class_ref("nz.example.library", "Book")),
                origin: DddAggregateOrigin::Authored,
            }),
        }),
    );
    // The invariants survive conversion verbatim, in declaration order.
    assert_eq!(
        book.invariants,
        vec![
            DddInvariant {
                expression: "self.title != null".to_string(),
            },
            DddInvariant {
                expression: "self.copies >= 0".to_string(),
            },
        ]
    );
    // Non-entity and undecided designs carry no boundary blob and no
    // invariants (the byte-identical discipline: nothing new serializes).
    assert_eq!(money.aggregate_boundary, None);
    assert!(money.invariants.is_empty());
    assert_eq!(summary.aggregate_boundary, None);
    assert!(summary.invariants.is_empty());
    assert_eq!(chapter.invariants, Vec::<DddInvariant>::new());

    // Repositories: one, in module-major position, with builtin labels and
    // the declared finder's full signature JSON.
    assert_eq!(graph.repositories.len(), 1);
    let repo = &graph.repositories[0];
    assert_eq!(
        repo,
        &DddRepositoryNode {
            application: "Library".to_string(),
            name: "BookRepository".to_string(),
            design_class: "nz.example.library::Book".to_string(),
            operations: vec![
                phero_ir_builtin("findById", BuiltinRepositoryOp::FindById, 0),
                phero_ir_builtin("findAll", BuiltinRepositoryOp::FindAll, 1),
                phero_ir_builtin("save", BuiltinRepositoryOp::Save, 2),
                phero_ir_builtin("delete", BuiltinRepositoryOp::Delete, 3),
                codegraph_core::types::DddRepositoryOperation {
                    name: "byTitle".to_string(),
                    builtin: None,
                    return_type: Some(type_json(&TypeRef::Primitive(PrimitiveType::String))),
                    return_multiplicity: Some(mult_json(&Multiplicity {
                        lower: 0,
                        upper: Upper::Finite(5)
                    })),
                    params: vec![codegraph_core::types::DddParam {
                        name: "title".to_string(),
                        type_json: type_json(&TypeRef::Primitive(PrimitiveType::String)),
                        multiplicity: Some(mult_json(&Multiplicity::MANY)),
                    }],
                    ordinal: 4,
                    is_protected: false,
                    query_json: None,
                    capabilities: Vec::new(),
                },
                codegraph_core::types::DddRepositoryOperation {
                    name: "recent".to_string(),
                    builtin: None,
                    return_type: Some(type_json(&class_ref("nz.example.library", "Book"))),
                    return_multiplicity: Some(mult_json(&Multiplicity::MANY)),
                    params: vec![codegraph_core::types::DddParam {
                        name: "minimum".to_string(),
                        type_json: type_json(&TypeRef::Primitive(PrimitiveType::Int)),
                        multiplicity: None,
                    }],
                    ordinal: 5,
                    is_protected: false,
                    query_json: Some(serde_json::json!({
                        "predicate": "self.title != null && self.title != title",
                        "sort": [{
                            "field": "title",
                            "fieldType": {"type": "primitive", "value": "string"},
                            "direction": "desc",
                        }],
                        "pagination": {"limit": 20, "max": 100, "cursor": false},
                        "readOnly": true,
                    })),
                    capabilities: vec!["ReadBooks".to_string()],
                },
            ],
        }
    );

    // Services: module-major, delegation split into target/operation,
    // capabilities verbatim.
    assert_eq!(graph.services.len(), 1);
    let service = &graph.services[0];
    assert_eq!(service.module, "lending");
    assert_eq!(service.ordinal, 0);
    assert_eq!(
        service.dependencies,
        ["LoanRepository", "NotificationService"]
    );
    let borrow = &service.operations[0];
    assert_eq!(
        borrow.return_type,
        Some(type_json(&TypeRef::Primitive(PrimitiveType::Boolean)))
    );
    assert_eq!(
        borrow.params,
        vec![codegraph_core::types::DddParam {
            name: "book".to_string(),
            type_json: type_json(&class_ref("nz.example.library", "Book")),
            multiplicity: None,
        }]
    );
    let renew = &service.operations[1];
    assert_eq!(renew.delegation_target.as_deref(), Some("LoanRepository"));
    assert_eq!(renew.delegation_operation.as_deref(), Some("save"));
    assert!(renew.return_type.is_none());
    assert_eq!(renew.ordinal, 1);
    // The op-level `when (...)` guard survives conversion verbatim (issue
    // #590): it is the route-level transition guard.
    assert_eq!(renew.condition.as_deref(), Some(r#"status == "OnLoan""#));
    assert_eq!(borrow.condition, None);
    // Issue #662: the body, its receiver, and the `throws` vocabulary survive
    // conversion (the body payload stays OPAQUE — generation re-parses it).
    assert_eq!(renew.body_receiver.as_deref(), Some("LoanRepository"));
    assert_eq!(
        renew.bodies.get("stmt").map(String::as_str),
        Some(
            r#"{"stmts":[{"kind":"guard","expr":"run.total > 0","error":{"variant":"Conflict","args":["\"total\""]}}]}"#
        )
    );
    assert_eq!(
        renew.throws,
        vec![serde_json::to_value(ErrorRef::prelude("Conflict")).unwrap()]
    );
    assert!(borrow.bodies.is_empty(), "a body-less op omits the map");
    assert_eq!(borrow.body_receiver, None);
    assert!(borrow.throws.is_empty());

    // Searches: the discovery module's search carries every knob.
    assert_eq!(graph.searches.len(), 1);
    let search = &graph.searches[0];
    assert_eq!(search.module, "discovery");
    assert_eq!(search.ordinal, 0);
    assert_eq!(search.entity_class, "nz.example.library::Book");
    assert_eq!(search.entity_title.as_deref(), Some("BookType"));
    assert_eq!(
        search.text,
        vec![
            DddSearchField {
                property: "title".to_string(),
                boost: Some(2.5),
                analyzer: Some("standard".to_string()),
            },
            DddSearchField {
                property: "synopsis".to_string(),
                boost: None,
                analyzer: None,
            },
        ]
    );
    assert_eq!(search.filters, vec!["category".to_string()]);
    assert_eq!(search.sorts, vec!["title".to_string()]);
    assert_eq!(
        search.document,
        vec![DddDocumentField {
            name: "label".to_string(),
            expr: r#"title + " - " + synopsis"#.to_string(),
        }]
    );
    assert_eq!(search.ranking.as_deref(), Some("recency"));
    assert_eq!(search.analyzer.as_deref(), Some("english"));
    assert_eq!(
        search.pagination,
        Some(DddPagination {
            limit: Some(20),
            max_limit: Some(100),
            cursor: true,
        })
    );
    assert_eq!(search.capabilities, vec!["SearchBooks".to_string()]);
}

/// The graph mirror round-trips the DD4/DD5 payloads through the ingest
/// contract (`ingest_ddd_model` → `get_ddd_models`, MockEngine): a warm
/// mirror that lost either blob would silently degrade the enforcement
/// plane later slices build (issue #666).
#[tokio::test]
async fn authored_aggregate_boundary_and_invariants_round_trip_through_the_mock() {
    let root = DddDesignNode {
        application: "Library".to_string(),
        module: "lending".to_string(),
        class: "nz.example.library::Book".to_string(),
        resolved_title: Some("BookType".to_string()),
        stereotype: "entity".to_string(),
        is_abstract: false,
        flags: DddDesignFlags::default(),
        machine_json: None,
        child_operations: Vec::new(),
        aggregate_boundary: Some(DddAggregateBoundary {
            is_root: Some(true),
            membership: None,
        }),
        invariants: vec![
            DddInvariant {
                expression: "self.title != null".to_string(),
            },
            DddInvariant {
                expression: "self.copies >= 0".to_string(),
            },
        ],
        ordinal: 0,
    };
    let member = DddDesignNode {
        application: "Library".to_string(),
        module: "lending".to_string(),
        class: "nz.example.library.Chapter".to_string(),
        resolved_title: None,
        stereotype: "entity".to_string(),
        is_abstract: false,
        flags: DddDesignFlags::default(),
        machine_json: None,
        child_operations: Vec::new(),
        aggregate_boundary: Some(DddAggregateBoundary {
            is_root: Some(false),
            membership: Some(DddAggregateMembership {
                root_json: serde_json::json!({
                    "type": "class",
                    "value": {"package": "nz.example.library", "name": "Book"}
                }),
                origin: DddAggregateOrigin::Derived,
            }),
        }),
        invariants: Vec::new(),
        ordinal: 1,
    };
    let graph = DddModelGraph {
        source_path: "design.ddd".to_string(),
        application: DddApplicationNode {
            name: "Library".to_string(),
            base: Some("nz.example.library".to_string()),
            source_path: "design.ddd".to_string(),
            errors: Vec::new(),
        },
        modules: vec![DddModuleNode {
            application: "Library".to_string(),
            name: "lending".to_string(),
            ordinal: 0,
            errors: Vec::new(),
        }],
        designs: vec![root, member],
        repositories: Vec::new(),
        services: Vec::new(),
        searches: Vec::new(),
        dto_shapes: Vec::new(),
    };

    let mock = MockEngine::builder().build();
    mock.ingest_ddd_model(&graph).await.unwrap();

    let loaded = mock.get_ddd_models().await.unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(
        loaded[0], graph,
        "the boundary blob and the invariants round-trip losslessly"
    );
}

fn phero_ir_builtin(
    name: &str,
    builtin: BuiltinRepositoryOp,
    ordinal: usize,
) -> codegraph_core::types::DddRepositoryOperation {
    let label = match builtin {
        BuiltinRepositoryOp::FindById => "findById",
        BuiltinRepositoryOp::FindAll => "findAll",
        BuiltinRepositoryOp::FindByExample => "findByExample",
        BuiltinRepositoryOp::FindByKeys => "findByKeys",
        BuiltinRepositoryOp::Save => "save",
        BuiltinRepositoryOp::Delete => "delete",
    };
    codegraph_core::types::DddRepositoryOperation {
        name: name.to_string(),
        builtin: Some(label.to_string()),
        return_type: None,
        return_multiplicity: None,
        params: Vec::new(),
        ordinal,
        is_protected: false,
        query_json: None,
        capabilities: Vec::new(),
    }
}

#[test]
fn title_resolution_receives_the_short_name_and_hits_exact_suffix_and_miss() {
    let seen = std::cell::RefCell::new(Vec::new());
    let titles: HashSet<String> = ["BookType", "Money"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let resolver = |name: &str| -> Option<String> {
        seen.borrow_mut().push(name.to_string());
        codegraph::ingest::ddd_ingest::resolve_with_suffix(name, "Type", &titles)
    };

    let model = sample_model();
    let graph =
        codegraph::ingest::ddd_ingest::ddd_model_graph_from_phero(&model, "d.ddd", &resolver);

    // The resolver saw SHORT names: the `::`-qualified and dot-qualified
    // references were reduced to their last segment.
    assert_eq!(
        *seen.borrow(),
        vec![
            "Book".to_string(),
            "Money".to_string(),
            "LoanSummary".to_string(),
            "Chapter".to_string(),
            "Book".to_string(),
        ]
    );
    // Exact hit (Money), +type_suffix hit (Book → BookType), misses → None.
    assert_eq!(graph.designs[0].resolved_title.as_deref(), Some("BookType"));
    assert_eq!(graph.designs[1].resolved_title.as_deref(), Some("Money"));
    assert_eq!(graph.designs[2].resolved_title, None);
    assert_eq!(graph.searches[0].entity_title.as_deref(), Some("BookType"));
}

// ── end-to-end ingest ──────────────────────────────────────────────────

const LIB_MOX: &str = concat!(
    "package shop\n",
    "\n",
    "import schema \"x.json\" as Widget\n",
    "import sigil \"y.rosetta\"\n",
    "\n",
    "class Product {\n",
    "    String title\n",
    "    refers Widget widget\n",
    "}\n",
    "\n",
    "class Money {\n",
    "    readonly int cents\n",
    "}\n",
);

/// Imports a namespace that only exists in the SIBLING pool file
/// (`z.rosetta`, never named by any import): a clean compile proves the
/// candidate-pool discovery ran.
const Y_ROSETTA: &str = concat!(
    "namespace shop.notes\n",
    "version \"1.0.0\"\n",
    "\n",
    "import shop.other.*\n",
    "\n",
    "type Note:\n",
    "    label string (1..1)\n",
    "    tag shop.other.Tag (1..1)\n",
);

const Z_ROSETTA: &str = concat!(
    "namespace shop.other\n",
    "version \"1.0.0\"\n",
    "\n",
    "type Tag:\n",
    "    name string (1..1)\n",
);

const X_JSON: &str = r#"{"title": "Widget"}"#;

const DESIGN_DDD: &str = concat!(
    "import \"lib.mox\"\n",
    "\n",
    "application Shop {\n",
    "    base shop\n",
    "\n",
    "    module catalogue {\n",
    "        entity Product repository ProductRepository {\n",
    "            findById;\n",
    "            findAll;\n",
    "            save;\n",
    "            delete;\n",
    "            Money pricedAbove(int cents);\n",
    "        }\n",
    "        value Money nonPersistent\n",
    "        search ProductSearch {\n",
    "            entity Product\n",
    "            text {\n",
    "                title boost 3\n",
    "            }\n",
    "            filters {\n",
    "                title\n",
    "            }\n",
    "            sort {\n",
    "                title\n",
    "            }\n",
    "            document {\n",
    "                label = title;\n",
    "            }\n",
    "            ranking custom \"recency\"\n",
    "            pagination {\n",
    "                limit 10\n",
    "                max 50\n",
    "                cursor\n",
    "            }\n",
    "            capability SearchProducts\n",
    "        }\n",
    "        service ProductService {\n",
    "            inject ProductRepository;\n",
    "            find => ProductRepository.findById;\n",
    "        }\n",
    "    }\n",
    "}\n",
);

const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.shop]
label = "Shop"
schema_dir = "shop"
postgres_schema = "shop"
"#;

fn entity_schema(title: &str) -> SchemaNode {
    SchemaNode {
        namespace: None,
        schema_id: format!("shop/json/{title}.json"),
        title: title.to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("shop".to_string()),
        rel_path: format!("shop/json/{title}.json"),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: title.strip_suffix("Type").unwrap_or(title).to_string(),
        pg_table_name: title.strip_suffix("Type").unwrap_or(title).to_lowercase(),
        api_path_segment: title.strip_suffix("Type").unwrap_or(title).to_lowercase(),
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
    }
}

/// Writes the fixture dir and returns (design.ddd path, domains.toml path).
fn write_fixture(dir: &Path) -> (PathBuf, PathBuf) {
    std::fs::write(dir.join("lib.mox"), LIB_MOX).unwrap();
    std::fs::write(dir.join("x.json"), X_JSON).unwrap();
    std::fs::write(dir.join("y.rosetta"), Y_ROSETTA).unwrap();
    std::fs::write(dir.join("z.rosetta"), Z_ROSETTA).unwrap();
    let design = dir.join("design.ddd");
    std::fs::write(&design, DESIGN_DDD).unwrap();
    let domains = dir.join("domains.toml");
    std::fs::write(&domains, DOMAINS_TOML).unwrap();
    (design, domains)
}

#[tokio::test]
async fn ingest_ddd_file_end_to_end_with_mox_schema_and_sigil_imports() {
    let dir = tempfile::tempdir().unwrap();
    let (design, domains_path) = write_fixture(dir.path());
    let domain_config = codegraph_config::config::parse_domain_config(&domains_path).unwrap();

    // The schema graph carries the mox-bridge titles: Product resolves via
    // the +Type suffix, Money exactly.
    let mock = MockEngine::builder()
        .with_schema(entity_schema("ProductType"))
        .with_schema(entity_schema("Money"))
        .build();

    let stats =
        codegraph::ingest::ddd_ingest::ingest_ddd_files(&mock, &mock, &[design], &domain_config)
            .await
            .unwrap();

    assert_eq!(stats.files, 1);
    assert_eq!(stats.designs, 2);
    assert_eq!(stats.repositories, 1);
    assert_eq!(stats.services, 1);
    assert_eq!(stats.searches, 1);
    assert_eq!(stats.unresolved_classes, 0, "both classes resolve");

    let models = mock.get_ddd_models().await.unwrap();
    assert_eq!(models.len(), 1);
    let graph = &models[0];
    assert_eq!(graph.application.name, "Shop");
    assert_eq!(graph.application.base.as_deref(), Some("shop"));
    assert_eq!(graph.designs.len(), 2);
    assert_eq!(graph.searches.len(), 1);

    // Title resolution against the ingested schema titles: exact first,
    // then +type_suffix.
    let product = graph
        .designs
        .iter()
        .find(|d| d.class == "Product")
        .expect("Product design");
    assert_eq!(product.resolved_title.as_deref(), Some("ProductType"));
    let money = graph
        .designs
        .iter()
        .find(|d| d.class == "Money")
        .expect("Money design");
    assert_eq!(money.resolved_title.as_deref(), Some("Money"));
    assert_eq!(
        graph.searches[0].entity_class, "Product",
        "as-authored reference"
    );
    assert_eq!(
        graph.searches[0].entity_title.as_deref(),
        Some("ProductType")
    );

    // The repository landed with the four builtins plus the declared op.
    assert_eq!(graph.repositories.len(), 1);
    let ops = &graph.repositories[0].operations;
    assert_eq!(ops.len(), 5);
    assert_eq!(ops[0].builtin.as_deref(), Some("findById"));
    assert_eq!(ops[4].name, "pricedAbove");
    assert!(ops[4].builtin.is_none());
    assert_eq!(
        ops[4].params[0].name, "cents",
        "the declared signature rides the graph"
    );
}

#[tokio::test]
async fn unknown_design_class_is_a_hard_error_with_the_diagnostic_text() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("lib.mox"), LIB_MOX).unwrap();
    std::fs::write(dir.path().join("x.json"), X_JSON).unwrap();
    std::fs::write(dir.path().join("y.rosetta"), Y_ROSETTA).unwrap();
    std::fs::write(dir.path().join("z.rosetta"), Z_ROSETTA).unwrap();
    let design = dir.path().join("bad.ddd");
    std::fs::write(
        &design,
        concat!(
            "import \"lib.mox\"\n",
            "\n",
            "application Shop {\n",
            "    base shop\n",
            "    module m {\n",
            "        entity Ghost\n",
            "    }\n",
            "}\n",
        ),
    )
    .unwrap();
    let domains_path = dir.path().join("domains.toml");
    std::fs::write(&domains_path, DOMAINS_TOML).unwrap();
    let domain_config = codegraph_config::config::parse_domain_config(&domains_path).unwrap();

    let mock = MockEngine::new();
    let err =
        codegraph::ingest::ddd_ingest::ingest_ddd_files(&mock, &mock, &[design], &domain_config)
            .await
            .unwrap_err();

    match &err {
        codegraph::error::Error::DddModel { file, reason } => {
            assert!(file.ends_with("bad.ddd"), "{file}");
            assert!(
                reason.contains("Ghost"),
                "the rendered diagnostic names the unknown class: {reason}"
            );
        }
        other => panic!("expected Error::DddModel, got {other:?}"),
    }
}

#[tokio::test]
async fn missing_design_file_is_a_hard_error() {
    let dir = tempfile::tempdir().unwrap();
    let domains_path = dir.path().join("domains.toml");
    std::fs::write(&domains_path, DOMAINS_TOML).unwrap();
    let domain_config = codegraph_config::config::parse_domain_config(&domains_path).unwrap();

    let mock = MockEngine::new();
    let err = codegraph::ingest::ddd_ingest::ingest_ddd_files(
        &mock,
        &mock,
        &[dir.path().join("absent.ddd")],
        &domain_config,
    )
    .await
    .unwrap_err();

    match &err {
        codegraph::error::Error::DddModel { file, reason } => {
            assert!(file.ends_with("absent.ddd"), "{file}");
            assert!(reason.contains("could not be read"), "{reason}");
        }
        other => panic!("expected Error::DddModel, got {other:?}"),
    }
}

// ── graph-cache inputs ─────────────────────────────────────────────────

#[test]
fn ddd_files_are_part_of_the_graph_cache_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let domains_path = dir.path().join("domains.toml");
    std::fs::write(&domains_path, DOMAINS_TOML).unwrap();
    let design = dir.path().join("design.ddd");
    std::fs::write(&design, DESIGN_DDD).unwrap();

    let inputs = codegraph::artifact::collect_run_inputs(
        None,
        None,
        &domains_path,
        &[],
        &[],
        std::slice::from_ref(&design),
        &[],
        &[],
        &[],
        &[],
    )
    .unwrap();
    assert!(
        inputs
            .iter()
            .any(|f| f.path == design.display().to_string() && f.bytes == DESIGN_DDD.as_bytes()),
        "the .ddd file group rides the inputs list: {:?}",
        inputs.iter().map(|f| &f.path).collect::<Vec<_>>()
    );

    let before = codegraph::artifact::inputs_hash(&inputs);
    std::fs::write(
        &design,
        DESIGN_DDD.replace("ProductSearch", "RenamedSearch"),
    )
    .unwrap();
    let after = codegraph::artifact::inputs_hash(
        &codegraph::artifact::collect_run_inputs(
            None,
            None,
            &domains_path,
            &[],
            &[],
            std::slice::from_ref(&design),
            &[],
            &[],
            &[],
            &[],
        )
        .unwrap(),
    );
    assert_ne!(
        before, after,
        "editing a .ddd file must bust the persisted-graph cache"
    );

    // A run without ddd files is unaffected (the group is empty).
    let without = codegraph::artifact::collect_run_inputs(
        None,
        None,
        &domains_path,
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
    )
    .unwrap();
    assert!(
        !without.iter().any(|f| f.path.contains(".ddd")),
        "no ddd entries without ddd files"
    );
}
