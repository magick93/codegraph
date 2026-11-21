//! The projection-read plane (issue #768): the shape predicate that decides
//! which ops lower as projection reads, and the execution lowering they
//! carry. Split from `routes.rs` (#751).
use super::transitions::short_class_name;
use super::types::{ExecutionKind, ProjectionDerived, ProjectionReadPlan};

/// The #768 projection-read predicate: a PUBLIC abstract op whose return
/// slot is a projection boundary of the bound entity. `Some(collection)`
/// when the op lowers as a projection read (`collection` selects the
/// item-addressed vs collection-addressed route); `None` keeps the
/// pre-#768 lowering (the 501 seam).
pub(crate) fn projection_shape_candidate(
    op: &codegraph_core::types::DddServiceOperation,
    model: &codegraph_core::types::DddModelGraph,
    entity_title: &str,
    scoped: Option<&str>,
    boundaries: &crate::ddd::dto_boundaries::DtoBoundaryIndex,
) -> Option<bool> {
    // Abstract only: a body or a delegation has its own lowering; a child
    // anchor belongs to the child-op plane; a machine transition is a write.
    if !op.bodies.is_empty()
        || op.transition.is_some()
        || op.delegation_target.is_some()
        || scoped.is_some()
    {
        return None;
    }
    let boundary = crate::ddd::signature::signature_type_opt(op.return_type.as_ref())
        .and_then(|slot| slot.dto_name().map(str::to_string))?;
    let shape = boundaries.get(&boundary)?;
    // A read model is an OUTPUT boundary (the constructor plane emits
    // `from_row` for output shapes only).
    if shape.direction != "output" {
        return None;
    }
    // The boundary must derive from the ROUTE entity: the bind step proves
    // the op reaches this entity (through a class-typed parameter or the
    // boundary itself), but a `summarise(Book book)` returning a `Movie`
    // projection must never lower as a `Book` read.
    let entity_class = model
        .designs
        .iter()
        .find(|d| d.resolved_title.as_deref() == Some(entity_title))
        .map(|d| d.class.as_str());
    let source = crate::ddd::boundary_derive::shape_source_class(shape)?;
    match entity_class {
        Some(entity_class) if short_class_name(&source) == short_class_name(entity_class) => {}
        _ => return None,
    }
    let returns_many = op
        .return_multiplicity
        .as_ref()
        .map(|m| serde_json::from_value::<phero_ir::Multiplicity>(m.clone()).map(|m| m.is_many()))
        .transpose()
        .ok()
        .flatten()
        .unwrap_or(false);
    match op.params.len() {
        // Stateless: the record (item) or the collection is the read's input.
        0 => Some(returns_many),
        // Record-scoped: one class-typed parameter naming the bound entity —
        // documentation of the addressed record, item-addressed only.
        1 => {
            if returns_many {
                return None;
            }
            let entity_class = entity_class?;
            crate::ddd::signature::signature_type(&op.params[0].type_json)
                .class_name()
                .map(|class| short_class_name(class) == short_class_name(entity_class))
                .unwrap_or(false)
                .then_some(false)
        }
        _ => None,
    }
}

/// The walker-visible wrapper of [`projection_shape_candidate`] (issue #768
/// review): the pre-generation pass refuses exactly what the routes plane
/// would lower — one rule, two consumers.
pub fn projection_shape_candidate_public(
    op: &codegraph_core::types::DddServiceOperation,
    model: &codegraph_core::types::DddModelGraph,
    entity_title: &str,
    scoped: Option<&str>,
    boundaries: &crate::ddd::dto_boundaries::DtoBoundaryIndex,
) -> Option<bool> {
    projection_shape_candidate(op, model, entity_title, scoped, boundaries)
}

/// Lower one projection read (issue #768) into `ExecutionKind::ProjectionRead`.
///
/// Refusals are NAMED (the `body_refusals` posture): a collection read with
/// an op-level `when` guard (the authorize machinery evaluates over the
/// LOADED record; a collection has none) fails the run before any tree.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn projection_read_execution(
    op: &codegraph_core::types::DddServiceOperation,
    entity_title: &str,
    boundaries: &crate::ddd::dto_boundaries::DtoBoundaryIndex,
    collection: bool,
    dto_casing: crate::project_config::DtoKeyCasing,
    surface_ops: &[String],
) -> Result<ExecutionKind, String> {
    let boundary = crate::ddd::signature::signature_type_opt(op.return_type.as_ref())
        .and_then(|slot| slot.dto_name().map(str::to_string))
        .unwrap_or_default();
    let Some(shape) = boundaries.get(&boundary) else {
        return Err(format!(
            "service op `{}`: boundary `{boundary}` resolves to no modeled DTO shape",
            op.name
        ));
    };
    if collection && op.condition.is_some() {
        return Err(format!(
            "service op `{}` is a collection projection read carrying an op-level `when` \
             guard — the authorize machinery evaluates over the LOADED record and a \
             collection read loads none; drop the guard or return a single projection",
            op.name
        ));
    }
    // The #449/#590 gating invariant: every consumer of the op set must
    // respect the EMPTY set. The item read renders the record load
    // (`find_by_id`), the collection read renders `list_filtered` — an
    // entity whose design exposes neither op would emit a handler naming a
    // query method that does not exist (E0425/E0599 at consumer build,
    // silently). Both refuse by name, the save-delegation-seam posture.
    if collection {
        if !surface_ops.iter().any(|o| o == "list") {
            return Err(format!(
                "service op `{}` is a collection projection read, but `{}` exposes no \
                 list operation — the route loads the RLS-scoped collection it projects; \
                 give the design repository a `findAll` (or return a single projection)",
                op.name, entity_title
            ));
        }
    } else if !surface_ops.iter().any(|o| o == "read") {
        return Err(format!(
            "service op `{}` is an item-addressed projection read, but `{}` exposes no \
             read operation — the route loads the record it projects; give the design \
             repository a `findById` (or return a collection projection)",
            op.name, entity_title
        ));
    }
    let module = boundaries
        .module_of()
        .get(&boundary)
        .cloned()
        .unwrap_or_else(|| codegraph_naming::to_snake_case(&shape.module));
    let derived: Vec<ProjectionDerived> = shape
        .members
        .iter()
        .filter(|member| member.kind == "derive")
        .filter_map(|member| {
            let snake = codegraph_naming::to_snake_case(&member.name);
            let wire_key = match dto_casing {
                crate::project_config::DtoKeyCasing::Camel => {
                    super::transitions::dto_wire_key(&snake, dto_casing)
                }
                crate::project_config::DtoKeyCasing::Snake => snake,
            };
            member
                .expression
                .clone()
                .map(|expression| ProjectionDerived {
                    wire_key,
                    expression,
                })
        })
        .collect();
    let _ = entity_title;
    Ok(ExecutionKind::ProjectionRead {
        plan: ProjectionReadPlan {
            module,
            projection: boundary,
            collection,
            derived,
        },
    })
}
