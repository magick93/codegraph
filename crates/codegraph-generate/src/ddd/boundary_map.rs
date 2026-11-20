//! A modeled input boundary as the entity's request DTO (issue #767).
//!
//! #667 landed the modeled DTO boundary as an action-body type: a LONE
//! boundary parameter IS the endpoint body (`BodyParam.direct_body`). What it
//! could not do is reach the ENTITY's generated `Create{Entity}Request` /
//! `Update{Entity}Request`, so a modeled create payload had no path into the
//! create command. This module is that path: it maps the boundary's members
//! onto the request DTO's fields by **snake_case identity** and pre-renders the
//! construction the handler emits.
//!
//! The field universe is the DTO generator's OWN context ([`build_dto_context`])
//! — the same `fields` the `dto_create` / `dto_update` templates loop over — so
//! the mapping cannot drift from the emitted struct: a workflow-excluded,
//! immutable, media-owned or derived field is excluded here for exactly the
//! reason it is excluded there.
//!
//! Refusals are NAMED and total (the DD9 `query_refusals` posture), never a
//! silently dropped field:
//!
//! - a member with no matching request field (a typo, a workflow-managed
//!   column, an immutable update column, a VO child) names the boundary, the
//!   member, and the accepted field names;
//! - a member whose Rust type does not unify with the field's names BOTH
//!   spellings and the multiplicity/primitive alignment rule;
//! - a REQUIRED request field the boundary does not carry names the field —
//!   the mapping never guesses a value for a non-defaultable column.
//!
//! Optional request fields the boundary omits are defaulted (`None`), which is
//! the same wire meaning an omitted key has (the DTOs are
//! `skip_serializing_if = "Option::is_none"`); the closure is documented in
//! the generated op's doc comment.

use std::collections::BTreeMap;

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::DddDtoShapeNode;

use crate::ddd::body_params::{BodyParam, param_extraction_validation};
use crate::ddd::dto::build_dto_context;
use crate::ddd::dto_boundaries::{MemberTypeResolution, wrap_multiplicity};

/// The binding the extracted boundary parameter takes in the handler body.
/// Deliberately NOT the authored parameter name: that name may be camelCase,
/// and the mapping reads Rust struct fields.
const BOUNDARY_BINDING: &str = "payload";

/// Which generated request DTO the boundary fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestDtoKind {
    /// `Create{Entity}Request` — required fields are plain, optional `Option`.
    Create,
    /// `Update{Entity}Request` — every field is `Option<T>` and immutable
    /// fields are absent.
    Update,
}

impl RequestDtoKind {
    /// The struct name the caller renders (`Create`/`Update` prefix).
    #[must_use]
    pub fn dto_prefix(self) -> &'static str {
        match self {
            Self::Create => "Create",
            Self::Update => "Update",
        }
    }

    /// The noun the refusals use ("create"/"update").
    fn noun(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
        }
    }
}

/// The pre-rendered construction of one request DTO from a modeled boundary.
#[derive(Debug, Clone)]
pub struct BoundaryRequestPlan {
    /// The handler-side extraction of the lone boundary parameter, bound to
    /// [`BOUNDARY_BINDING`] (`param_extraction_with` — the #667 shape).
    pub extraction: String,
    /// Field assignments, one per line, already indented for a struct literal.
    pub construct: String,
    /// Request fields the boundary does not carry and the plan defaults to
    /// `None` (documented in the emitted op, never silent).
    pub defaulted: Vec<String>,
    /// Required ENTITY-reference members (a stored FK the boundary carries as
    /// a `uuid::Uuid`). A generated contract cannot seed an arbitrary target
    /// row, so the hurl planner omits the leg for such a payload instead of
    /// posting the nil uuid (a guaranteed 23503 → 500) — issue #767 review.
    pub required_class_refs: Vec<String>,
}

/// The entity a boundary fills the request DTO of, plus the graph/config
/// handles its resolution needs (grouped to keep the mapping's arity small).
#[derive(Clone, Copy)]
pub struct BoundaryTarget<'a> {
    pub db: &'a dyn GraphQuerier,
    pub config: &'a DomainConfig,
    pub domain: &'a str,
    pub entity_title: &'a str,
}

/// The boundary shape set the mapping resolves member types against (the
/// generator's own index, borrowed for the pass).
#[derive(Clone, Copy)]
pub struct BoundaryShapeSet<'a> {
    /// Every modeled boundary by name.
    pub shapes: &'a BTreeMap<String, DddDtoShapeNode>,
    /// Each boundary's `.ddd` module file stem.
    pub module_of: &'a BTreeMap<String, String>,
}

/// One request-DTO field the boundary may fill.
struct RequestField {
    /// The Rust type of the generated struct field, verbatim.
    rust_type: String,
    /// The type WITHOUT the `Option` wrapper (the `Update` comparison unit).
    inner_type: String,
    /// `true` when the generated field is `Option<…>`.
    optional: bool,
}

/// Plan the construction of `Create{Entity}Request` / `Update{Entity}Request`
/// from a modeled input boundary.
///
/// # Errors
/// A named message (the caller prefixes the operation) for every unmappable
/// member and every uncovered required field.
pub async fn plan_boundary_request(
    target: BoundaryTarget<'_>,
    boundary: &DddDtoShapeNode,
    set: BoundaryShapeSet<'_>,
    kind: RequestDtoKind,
    param: &BodyParam,
) -> Result<BoundaryRequestPlan, String> {
    let ctx = build_dto_context(target.db, target.entity_title, target.domain, target.config)
        .await
        .map_err(|e| format!("cannot resolve the request DTO context: {e}"))?;

    let fields = request_fields(&ctx, kind);
    let mut covered: Vec<String> = Vec::new();
    let mut required_class_refs: Vec<String> = Vec::new();
    let mut lines: Vec<String> = Vec::new();

    for member in &boundary.members {
        if member.kind == "derive" {
            // The boundary generator marks any derived member output-only; an
            // input boundary cannot reach here, but a hand-built artifact can.
            return Err(format!(
                "boundary `{}` member `{}` is a DERIVED member — an output-only shape \
                 cannot be a request body; declare a plain member",
                boundary.name, member.name
            ));
        }
        let name = codegraph_naming::to_snake_case(&member.name);
        let Some(field) = fields.get(&name) else {
            let accepted: Vec<&str> = fields.keys().map(String::as_str).collect();
            return Err(format!(
                "boundary `{}` member `{}` matches no `{}{}Request` field the \
                 {noun} plane accepts (accepted: {}); rename the member or remove it \
                 from the boundary",
                boundary.name,
                member.name,
                kind.dto_prefix(),
                ctx.entity_name,
                accepted.join(", "),
                noun = kind.noun(),
            ));
        };
        let resolved = resolve_member(target, boundary, member, set).await?;
        let (member_type, member_optional) =
            wrap_multiplicity(&resolved.rust_type, member.multiplicity_json.as_ref()).map_err(
                |reason| {
                    format!(
                        "boundary `{}` member `{}`: {reason}",
                        boundary.name, member.name
                    )
                },
            )?;

        match kind {
            RequestDtoKind::Create => {
                if member_type == field.rust_type {
                    lines.push(format!("        {name}: {BOUNDARY_BINDING}.{name},"));
                } else if !member_optional && field.rust_type == format!("Option<{member_type}>") {
                    // An optional COLUMN of a many-valued member: the DTO keeps
                    // `Option<Vec<T>>` while the boundary's `[0..*]` is a plain
                    // `Vec<T>` — an absent list and an empty one are the same
                    // wire meaning, so the mapping wraps it.
                    lines.push(format!("        {name}: Some({BOUNDARY_BINDING}.{name}),"));
                } else {
                    return Err(type_mismatch(
                        boundary,
                        member,
                        &member_type,
                        &name,
                        field,
                        kind,
                        &ctx.entity_name,
                    ));
                }
            }
            RequestDtoKind::Update => {
                // The Update DTO wraps EVERY accepted field in `Option`, so the
                // comparison unit is the member WITHOUT its own `Option` (a
                // many-valued member keeps its `Vec<…>`).
                let member_inner = if member_optional {
                    member_type
                        .strip_prefix("Option<")
                        .and_then(|inner| inner.strip_suffix('>'))
                        .unwrap_or(&member_type)
                        .to_string()
                } else {
                    member_type.clone()
                };
                if member_inner != field.inner_type {
                    return Err(type_mismatch(
                        boundary,
                        member,
                        &member_inner,
                        &name,
                        field,
                        kind,
                        &ctx.entity_name,
                    ));
                }
                if member_optional {
                    lines.push(format!("        {name}: {BOUNDARY_BINDING}.{name},"));
                } else {
                    lines.push(format!("        {name}: Some({BOUNDARY_BINDING}.{name}),"));
                }
            }
        }
        if is_required_class_ref(member) {
            required_class_refs.push(name.clone());
        }
        covered.push(name);
    }

    // Every request field the boundary does not carry: refuse a required one
    // (the create command has no value for it), default an optional one.
    let mut defaulted = Vec::new();
    for (name, field) in &fields {
        if covered.contains(name) {
            continue;
        }
        let required = kind == RequestDtoKind::Create && !field.optional;
        if required {
            return Err(format!(
                "boundary `{}` does not carry the REQUIRED `{}{}Request` field `{name}` \
                 (`{}`) — the {noun} command would have no value for it; add the \
                 member to the boundary or model the column optional",
                boundary.name,
                kind.dto_prefix(),
                ctx.entity_name,
                field.rust_type,
                noun = kind.noun(),
            ));
        }
        defaulted.push(name.clone());
        lines.push(format!("        {name}: None,"));
    }
    defaulted.sort();

    Ok(BoundaryRequestPlan {
        extraction: param_extraction_validation(param, BOUNDARY_BINDING),
        construct: if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        },
        defaulted,
        required_class_refs,
    })
}

/// `true` for an entity-reference member the caller cannot omit (a required
/// stored FK — the contract would have to create the target row).
fn is_required_class_ref(member: &codegraph_core::types::DddDtoMember) -> bool {
    if member
        .multiplicity_json
        .as_ref()
        .and_then(|json| serde_json::from_value::<phero_ir::Multiplicity>(json.clone()).ok())
        .is_some_and(|m| m.lower == 0)
    {
        return false;
    }
    serde_json::from_value::<phero_ir::TypeRef>(member.type_json.clone())
        .is_ok_and(|t| matches!(t, phero_ir::TypeRef::Class { .. }))
}

/// Render the named refusal for a member/field type disagreement — BOTH
/// spellings, so the modeler sees which side to change.
fn type_mismatch(
    boundary: &DddDtoShapeNode,
    member: &codegraph_core::types::DddDtoMember,
    member_type: &str,
    field_name: &str,
    field: &RequestField,
    kind: RequestDtoKind,
    entity_name: &str,
) -> String {
    format!(
        "boundary `{}` member `{}` types `{member_type}`, but `{prefix}{entity_name}Request.{field_name}` \
         types `{}` — the mapped member must unify with the request field (multiplicity \
         and the primitive mapping are shared with the DTO plane: `int`→`i32`, \
         `long`→`i64`, `float`→`f32`, `double`→`f64`, an optional member ↔ an `Option` \
         field)",
        boundary.name,
        member.name,
        field.rust_type,
        prefix = kind.dto_prefix(),
    )
}

/// Resolve one boundary member through the SHARED member-type derivation (the
/// boundary generator's own — one rule, two consumers).
async fn resolve_member(
    target: BoundaryTarget<'_>,
    boundary: &DddDtoShapeNode,
    member: &codegraph_core::types::DddDtoMember,
    set: BoundaryShapeSet<'_>,
) -> Result<MemberTypeResolution, String> {
    let module_file = set
        .module_of
        .get(&boundary.name)
        .cloned()
        .unwrap_or_default();
    crate::ddd::dto_boundaries::resolve_member_type(
        &boundary.name,
        member,
        target.db,
        target.config,
        &module_file,
        set.shapes,
        set.module_of,
    )
    .await
    .map_err(|e| e.to_string())
}

/// The request DTO's field universe, exactly as the `dto_create` /
/// `dto_update` templates build it.
fn request_fields(
    ctx: &crate::ddd::dto::DtoContext,
    kind: RequestDtoKind,
) -> BTreeMap<String, RequestField> {
    let mut out = BTreeMap::new();
    for field in &ctx.fields {
        if field.name == "id" {
            continue;
        }
        if ctx.workflow_excluded_fields.contains(&field.name) {
            continue;
        }
        if kind == RequestDtoKind::Update && ctx.immutable_fields.contains(&field.name) {
            continue;
        }
        let base = if field.is_entity_ref {
            "uuid::Uuid".to_string()
        } else {
            field.rust_type.clone()
        };
        let inner = if field.is_array {
            format!("Vec<{base}>")
        } else {
            base
        };
        // Create: a required field is plain, an optional one is `Option<…>`.
        // Update: EVERY field is `Option<…>`.
        let optional = kind == RequestDtoKind::Update || !field.is_required;
        let rust_type = if optional {
            format!("Option<{inner}>")
        } else {
            inner.clone()
        };
        out.insert(
            field.name.clone(),
            RequestField {
                rust_type,
                inner_type: inner,
                optional,
            },
        );
    }
    for child in &ctx.child_dtos {
        if child.is_media {
            continue;
        }
        let base = format!("Create{}Request", child.struct_name);
        let inner = if child.is_array {
            format!("Vec<{base}>")
        } else {
            base
        };
        let optional = kind == RequestDtoKind::Update || !child.is_array;
        let rust_type = if optional {
            format!("Option<{inner}>")
        } else {
            inner.clone()
        };
        out.insert(
            child.field_name.clone(),
            RequestField {
                rust_type,
                inner_type: inner,
                optional,
            },
        );
    }
    out
}
