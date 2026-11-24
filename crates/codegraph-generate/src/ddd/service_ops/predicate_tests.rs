//! The #767 create-shape and #768 projection-read PREDICATE pins (issue
//! quality bar: the predicates decide which ops lower, so their acceptance
//! and refusal sets are pinned KAT-style here, beside the planner).
// A minimal `DddServiceOperation` for the predicate pins.
fn create_op(
    name: &str,
    params: Vec<codegraph_core::types::DddParam>,
    return_json: serde_json::Value,
) -> codegraph_core::types::DddServiceOperation {
    codegraph_core::types::DddServiceOperation {
        name: name.to_string(),
        params,
        return_type: Some(return_json),
        return_multiplicity: None,
        is_protected: false,
        capabilities: Vec::new(),
        condition: None,
        condition_paths: Vec::new(),
        bodies: std::collections::BTreeMap::new(),
        body_receiver: None,
        throws: Vec::new(),
        emits: Vec::new(),
        transition: None,
        delegation_target: None,
        delegation_operation: None,
        child_scope: None,
        invokes: Vec::new(),
        ordinal: 0,
    }
}

fn class_json(name: &str) -> serde_json::Value {
    serde_json::json!({"type": "class", "value": {"package": "nz.example.library", "name": name}})
}

fn dto_json(name: &str) -> serde_json::Value {
    serde_json::json!({ "dto": name })
}

fn dto_param(name: &str, boundary: &str, optional: bool) -> codegraph_core::types::DddParam {
    codegraph_core::types::DddParam {
        name: name.to_string(),
        type_json: dto_json(boundary),
        multiplicity: optional.then(|| serde_json::json!({"lower": 0, "upper": {"finite": 1}})),
    }
}

fn predicate_model() -> codegraph_core::types::DddModelGraph {
    codegraph_core::types::DddModelGraph {
        source_path: "design.ddd".to_string(),
        application: codegraph_core::types::DddApplicationNode {
            name: "Library".to_string(),
            base: Some("nz.example.library".to_string()),
            source_path: "design.ddd".to_string(),
            errors: Vec::new(),
        },
        modules: Vec::new(),
        designs: vec![codegraph_core::types::DddDesignNode {
            application: "Library".to_string(),
            module: "catalog".to_string(),
            class: "Book".to_string(),
            resolved_title: Some("BookType".to_string()),
            stereotype: "entity".to_string(),
            ordinal: 0,
            is_abstract: false,
            flags: Default::default(),
            machine_json: None,
            machine_ir_json: None,
            machine_provenance_json: None,
            aggregate_boundary: None,
            invariants: Vec::new(),
            child_operations: Vec::new(),
        }],
        repositories: Vec::new(),
        services: Vec::new(),
        searches: Vec::new(),
        dto_shapes: Vec::new(),
    }
}

fn create_surface() -> Vec<String> {
    vec!["create".to_string(), "read".to_string()]
}

#[test]
fn create_shaped_predicate_accepts_the_issue_shape() {
    let model = predicate_model();
    let op = create_op(
        "registerBook",
        vec![dto_param("input", "NewBookDraft", false)],
        class_json("Book"),
    );
    assert!(super::routes::create_shape_candidate(
        &op,
        &model,
        "BookType",
        &create_surface(),
        None
    ));
}

#[test]
fn create_shaped_predicate_refuses_every_non_create_shape() {
    let model = predicate_model();
    let surface = create_surface();
    let base = || {
        create_op(
            "registerBook",
            vec![dto_param("input", "NewBookDraft", false)],
            class_json("Book"),
        )
    };

    // No `create` operation on the entity.
    let no_create = create_op(
        "registerBook",
        vec![dto_param("input", "NewBookDraft", false)],
        class_json("Book"),
    );
    assert!(!super::routes::create_shape_candidate(
        &no_create,
        &model,
        "BookType",
        &["read".to_string()],
        None
    ));

    // A nested child entity's create is the child-op plane's.
    assert!(!super::routes::create_shape_candidate(
        &base(),
        &model,
        "BookType",
        &surface,
        Some("parent_id")
    ));

    // An op-level `when` guard has no record to evaluate (keeps its seam).
    let mut guarded = base();
    guarded.condition = Some("title == title".to_string());
    assert!(!super::routes::create_shape_candidate(
        &guarded, &model, "BookType", &surface, None
    ));

    // A statement body wins over the create shape.
    let mut bodied = base();
    bodied.bodies.insert("stmt".to_string(), String::new());
    assert!(!super::routes::create_shape_candidate(
        &bodied, &model, "BookType", &surface, None
    ));

    // Two parameters, a primitive parameter, a DTO return, a foreign return
    // and a non-save delegation all stay non-create.
    let mut two_params = base();
    two_params
        .params
        .push(dto_param("other", "NewBookDraft", false));
    assert!(!super::routes::create_shape_candidate(
        &two_params,
        &model,
        "BookType",
        &surface,
        None
    ));
    let primitive_param = create_op(
        "importBook",
        vec![codegraph_core::types::DddParam {
            name: "title".to_string(),
            type_json: serde_json::json!({"type": "primitive", "value": "string"}),
            multiplicity: None,
        }],
        class_json("Book"),
    );
    assert!(!super::routes::create_shape_candidate(
        &primitive_param,
        &model,
        "BookType",
        &surface,
        None
    ));
    let dto_return = create_op(
        "summarise",
        vec![dto_param("input", "NewBookDraft", false)],
        dto_json("BookSummary"),
    );
    assert!(!super::routes::create_shape_candidate(
        &dto_return,
        &model,
        "BookType",
        &surface,
        None
    ));
    let mut delete_delegation = base();
    delete_delegation.delegation_target = Some("BookRepository".to_string());
    delete_delegation.delegation_operation = Some("delete".to_string());
    assert!(!super::routes::create_shape_candidate(
        &delete_delegation,
        &model,
        "BookType",
        &surface,
        None
    ));
    // ... while the `save` delegation IS accepted (the widened arm).
    let mut save_delegation = base();
    save_delegation.delegation_target = Some("BookRepository".to_string());
    save_delegation.delegation_operation = Some("save".to_string());
    assert!(super::routes::create_shape_candidate(
        &save_delegation,
        &model,
        "BookType",
        &surface,
        None
    ));
}

// ── #768: the projection-read predicate ────────────────────────────────

fn projection_shape(source: &str) -> codegraph_core::types::DddDtoShapeNode {
    codegraph_core::types::DddDtoShapeNode {
        application: "Library".to_string(),
        module: "catalog".to_string(),
        name: "BookSummary".to_string(),
        source_json: Some(class_json(source)),
        direction: "output".to_string(),
        members: Vec::new(),
        ordinal: 1,
    }
}

fn boundaries_with(source: &str) -> crate::ddd::dto_boundaries::DtoBoundaryIndex {
    let mut model = predicate_model();
    model.dto_shapes = vec![projection_shape(source)];
    crate::ddd::dto_boundaries::DtoBoundaryIndex::from_models(&[model])
}

#[test]
fn projection_predicate_accepts_the_stateless_shapes() {
    let model = predicate_model();
    let boundaries = boundaries_with("Book");

    // Stateless single return → the item-addressed read.
    let single = create_op("summarise", vec![], dto_json("BookSummary"));
    assert_eq!(
        super::projection::projection_shape_candidate(
            &single,
            &model,
            "BookType",
            None,
            &boundaries
        ),
        Some(false)
    );

    // Stateless many return → the collection-addressed read.
    let mut many = create_op("allSummaries", vec![], dto_json("BookSummary"));
    many.return_multiplicity = Some(serde_json::json!({ "lower": 0, "upper": "unbounded" }));
    assert_eq!(
        super::projection::projection_shape_candidate(&many, &model, "BookType", None, &boundaries),
        Some(true)
    );
}

#[test]
fn projection_predicate_accepts_the_record_scoped_read() {
    let model = predicate_model();
    let boundaries = boundaries_with("Book");
    // One class-typed parameter naming the bound entity: the item read.
    let op = create_op(
        "summarise",
        vec![codegraph_core::types::DddParam {
            name: "book".to_string(),
            type_json: class_json("Book"),
            multiplicity: None,
        }],
        dto_json("BookSummary"),
    );
    assert_eq!(
        super::projection::projection_shape_candidate(&op, &model, "BookType", None, &boundaries),
        Some(false)
    );
}

#[test]
fn projection_predicate_refuses_every_non_read_shape() {
    let model = predicate_model();
    let boundaries = boundaries_with("Book");
    let base = || create_op("summarise", vec![], dto_json("BookSummary"));

    // A boundary deriving from ANOTHER class is not a read of this entity.
    let foreign = boundaries_with("Movie");
    assert_eq!(
        super::projection::projection_shape_candidate(&base(), &model, "BookType", None, &foreign),
        None
    );
    // A statement body has its own lowering.
    let mut bodied = base();
    bodied.bodies.insert("stmt".to_string(), "{}".to_string());
    assert_eq!(
        super::projection::projection_shape_candidate(
            &bodied,
            &model,
            "BookType",
            None,
            &boundaries
        ),
        None
    );
    // A delegation has its own lowering.
    let mut delegated = base();
    delegated.delegation_target = Some("BookRepository".to_string());
    assert_eq!(
        super::projection::projection_shape_candidate(
            &delegated,
            &model,
            "BookType",
            None,
            &boundaries
        ),
        None
    );
    // A machine transition is a write.
    let mut transition = base();
    transition.transition = Some(codegraph_core::types::DddTransitionRef {
        trigger: "submit".to_string(),
        machine: None,
    });
    assert_eq!(
        super::projection::projection_shape_candidate(
            &transition,
            &model,
            "BookType",
            None,
            &boundaries
        ),
        None
    );
    // A child anchor belongs to the child-op plane.
    assert_eq!(
        super::projection::projection_shape_candidate(
            &base(),
            &model,
            "BookType",
            Some("parent_id"),
            &boundaries
        ),
        None
    );
    // A class-typed parameter that names another entity does not bind.
    let mut foreign_param = base();
    foreign_param.params = vec![codegraph_core::types::DddParam {
        name: "other".to_string(),
        type_json: class_json("Movie"),
        multiplicity: None,
    }];
    assert_eq!(
        super::projection::projection_shape_candidate(
            &foreign_param,
            &model,
            "BookType",
            None,
            &boundaries
        ),
        None
    );
    // A collection return with a class-typed parameter is neither shape.
    let mut many_scoped = base();
    many_scoped.return_multiplicity = Some(serde_json::json!({ "lower": 0, "upper": "unbounded" }));
    many_scoped.params = vec![codegraph_core::types::DddParam {
        name: "book".to_string(),
        type_json: class_json("Book"),
        multiplicity: None,
    }];
    assert_eq!(
        super::projection::projection_shape_candidate(
            &many_scoped,
            &model,
            "BookType",
            None,
            &boundaries
        ),
        None
    );
}

// ── #768 review: the execution-plane op-set requirement ────────────────

#[tokio::test]
async fn item_projection_read_requires_the_read_operation() {
    let boundaries = boundaries_with("Book");
    let op = create_op("summarise", vec![], dto_json("BookSummary"));
    // The design exposes NO read: the route would render a record load the
    // query plane never emits — refused by name (#449/#590 gating).
    let error = super::projection::projection_read_execution(
        &op,
        "BookType",
        &boundaries,
        false,
        crate::project_config::DtoKeyCasing::Snake,
        &["create".to_string(), "update".to_string()],
    )
    .await
    .expect_err("an entity without read must refuse the item projection read");
    assert!(error.contains("exposes no"), "{error}");
    assert!(error.contains("findById"), "{error}");

    // With `read` on the surface the op lowers.
    super::projection::projection_read_execution(
        &op,
        "BookType",
        &boundaries,
        false,
        crate::project_config::DtoKeyCasing::Snake,
        &["read".to_string()],
    )
    .await
    .expect("a read-capable entity lowers the item projection read");
}

#[tokio::test]
async fn collection_projection_read_requires_the_list_operation() {
    let boundaries = boundaries_with("Book");
    let mut op = create_op("allSummaries", vec![], dto_json("BookSummary"));
    op.return_multiplicity = Some(serde_json::json!({ "lower": 0, "upper": "unbounded" }));
    let error = super::projection::projection_read_execution(
        &op,
        "BookType",
        &boundaries,
        true,
        crate::project_config::DtoKeyCasing::Snake,
        &["read".to_string()],
    )
    .await
    .expect_err("an entity without list must refuse the collection projection read");
    assert!(error.contains("exposes no"), "{error}");
    assert!(error.contains("findAll"), "{error}");

    super::projection::projection_read_execution(
        &op,
        "BookType",
        &boundaries,
        true,
        crate::project_config::DtoKeyCasing::Snake,
        &["list".to_string()],
    )
    .await
    .expect("a list-capable entity lowers the collection projection read");
}

#[test]
fn an_input_direction_boundary_is_never_a_projection_read() {
    let model = predicate_model();
    // The mirror-as-request shape (#767): a legal INPUT boundary is not a
    // read model — the constructor plane emits no from_row for it.
    let input_boundaries = {
        let mut model = predicate_model();
        model.dto_shapes = vec![{
            let mut s = projection_shape("Book");
            s.direction = "input".to_string();
            s
        }];
        crate::ddd::dto_boundaries::DtoBoundaryIndex::from_models(&[model])
    };
    let op = create_op("summarise", vec![], dto_json("BookSummary"));
    assert_eq!(
        super::projection::projection_shape_candidate(
            &op,
            &model,
            "BookType",
            None,
            &input_boundaries
        ),
        None
    );
}
