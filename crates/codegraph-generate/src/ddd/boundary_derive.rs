//! Projection boundaries as read models (issue #768).
//!
//! A `.ddd` projection (`dto BookSummary from Book { title; minutes =
//! playLength / 60; }`) is a READ model: it is DERIVED from a loaded record,
//! never written. #667 generated the shape (a real Rust type in the
//! domain-types crate); this module is the construction site that makes the
//! derived member COMPUTE — one
//! `impl {Projection} { pub fn from_row(&{Entity}Response) -> Self }` per
//! projection, emitted into the same boundary module as the type, so the
//! repository finder and the projection endpoint cannot drift from it.
//!
//! Member lowering (mirrors [`crate::ddd::boundary_map`]'s discipline: the
//! shared member typing, the DTO generator's own field universe, named total
//! refusals):
//!
//! - `select` members copy the matching response field, Option-aware; a
//!   required member over an optional column refuses (the constructor would
//!   have to conjure a value);
//! - `derive` members lower a BOUNDED arithmetic subset — `+ - * /` (and
//!   unary minus) over numeric stored-field refs and numeric literals with
//!   implicit parentheses. EVERY other construct (navigation, calls,
//!   collection algebra, strings, dates, codelists) is a NAMED generation
//!   error naming the boundary + member, the DD9 `query_refusals` posture.
//!   Integer/float division by a FIELD follows Rust semantics (a zero
//!   divisor panics — documented on the emitted constructor); a literal-zero
//!   divisor is refused at generation time.
//! - Type adaptation between a field's column type and the member's declared
//!   type is EXPLICIT: same-type verbatim, widening (`i32→i64`, `i32→f32`,
//!   `i32→f64`, `f32→f64`) through `From`, every narrowing/lossy direction
//!   (`i64→i32`, `i64→f64`, `f64→f32`, float→int) refused by name. The two
//!   planes share the payload mapping (`int`→`i32`, `long`→`i64`,
//!   `float`→`f32`, `double`→`f64`), so the fixture shapes agree by
//!   construction and the adaptation rules exist for authored
//!   cross-width members (`long total = a + b;` over `int` columns).
//!
//! The field universe is the DTO generator's OWN context
//! ([`crate::ddd::dto::build_dto_context`]) — the same fields the response
//! template renders — so the constructor cannot drift from the emitted
//! `{Entity}Response` struct.

use std::collections::BTreeMap;

use codegraph_config::DomainConfig;
use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::DddDtoShapeNode;

use crate::ddd::dto::{DtoContext, build_dto_context};
use crate::ddd::dto_boundaries::{resolve_member_type, wrap_multiplicity};

/// The numeric types the derive subset models, keyed by their Rust spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericTy {
    I32,
    I64,
    F32,
    F64,
}

impl NumericTy {
    /// Classify a rendered Rust type; `None` for every non-numeric type.
    #[must_use]
    pub fn of_rust(rust_type: &str) -> Option<Self> {
        match rust_type {
            "i32" => Some(Self::I32),
            "i64" => Some(Self::I64),
            "f32" => Some(Self::F32),
            "f64" => Some(Self::F64),
            _ => None,
        }
    }

    fn rust(self) -> &'static str {
        match self {
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }

    /// Whether `from` widens losslessly into `self` (the `From` impl exists).
    fn accepts(self, from: Self) -> bool {
        use NumericTy::*;
        matches!(
            (from, self),
            (I32, I32)
                | (I64, I64)
                | (F32, F32)
                | (F64, F64)
                | (I32, I64)
                | (I32, F32)
                | (I32, F64)
                | (F32, F64)
        )
    }

    /// The explicit adaptation expression for `from` → `self`.
    fn adapt(self, expr: &str, from: Self) -> String {
        if from == self {
            return expr.to_string();
        }
        format!("{}::from({expr})", self.rust())
    }

    /// Render an integer literal in this type's context (`60` / `60i64` /
    /// `60.0f32` / `60.0f64`).
    fn int_literal(self, value: i64) -> String {
        match self {
            Self::I32 | Self::I64 => {
                format!("{value}{}", if self == Self::I64 { "i64" } else { "" })
            }
            Self::F32 => format!("{value}.0f32"),
            Self::F64 => format!("{value}.0f64"),
        }
    }

    /// Render a float literal in this type's context (`1.5f32` / `1.5`).
    fn float_literal(self, value: f64) -> String {
        match self {
            Self::F32 => format!("{value}f32"),
            Self::F64 => format!("{value}"),
            Self::I32 | Self::I64 => unreachable!("float literal refused for int targets"),
        }
    }
}

/// A resolved response field the constructor may read.
#[derive(Debug, Clone)]
pub struct ProjectionField {
    /// The response field name (snake_case, `play_length`).
    pub name: String,
    /// The response-rendered Rust type (`i32`, `Option<String>`,
    /// `Vec<uuid::Uuid>`, `uuid::Uuid`, …).
    pub rendered_type: String,
    /// The type WITHOUT the `Option` wrapper (the unification unit).
    pub inner_type: String,
    /// Whether the rendered type is `Option<…>`.
    pub optional: bool,
    /// The base primitive type when the field is a numeric column (`i32`),
    /// else `None`.
    pub numeric: Option<NumericTy>,
    /// Whether the base type is `Copy` (direct assignment; non-Copy takes
    /// `.clone()` to keep the constructor total over a borrowed record).
    pub copy: bool,
}

/// The resolved source of a projection: the designing entity as the
/// generation-order entry that owns it.
#[derive(Debug, Clone)]
pub struct ProjectionSource {
    /// The entity's schema title (`Book`).
    pub schema_title: String,
    /// The domain the generation order assigned the title to.
    pub domain: String,
}

/// The pre-rendered `from_row` construction of one projection.
#[derive(Debug, Clone)]
pub struct ProjectionPlan {
    /// The source entity's Response struct name (`BookResponse`).
    pub entity_response: String,
    /// The Response's module path within the domain-types crate:
    /// `(domain, module)` — the import spells
    /// `use crate::{domain}::{module}::dto_response::{Entity}Response;`.
    pub source_module: (String, String),
    /// Field assignments, one per boundary member, already indented for a
    /// struct literal body.
    pub construct: String,
    /// Response fields the constructor reads (drift-checkable).
    pub reads: Vec<String>,
    /// `true` when every read field is `Copy` (no `.clone()` anywhere).
    pub all_copy: bool,
}

/// Plan the `from_row` construction of one projection boundary.
///
/// # Errors
/// A named message (the caller prefixes the boundary) for every member the
/// subset cannot express — a select with no matching response field, a
/// optionality/`Vec` unification failure, a derive expression outside the
/// arithmetic subset, a non-numeric derive member, an Option field under
/// arithmetic, and every lossy adaptation.
pub async fn plan_projection_from_row(
    db: &dyn codegraph_core::traits::GraphQuerier,
    config: &codegraph_config::DomainConfig,
    shape: &DddDtoShapeNode,
    shapes: &BTreeMap<String, DddDtoShapeNode>,
    module_of: &BTreeMap<String, String>,
    source: &ProjectionSource,
) -> Result<ProjectionPlan, String> {
    let ctx = build_dto_context(db, &source.schema_title, &source.domain, config)
        .await
        .map_err(|e| {
            format!(
                "cannot resolve the `{}` response DTO context: {e}",
                source.schema_title
            )
        })?;
    let fields = response_fields(&ctx);
    let mut lines: Vec<String> = Vec::new();
    let mut reads: Vec<String> = Vec::new();
    let mut all_copy = true;

    for member in &shape.members {
        let name = codegraph_naming::to_snake_case(&member.name);
        let resolved = resolve_member_type(
            &shape.name,
            member,
            db,
            config,
            module_of
                .get(&shape.name)
                .map(String::as_str)
                .unwrap_or_default(),
            shapes,
            module_of,
        )
        .await
        .map_err(|e| format!("member `{}`: {e}", member.name))?;
        let (member_type, member_optional) =
            wrap_multiplicity(&resolved.rust_type, member.multiplicity_json.as_ref())
                .map_err(|reason| format!("member `{}`: {reason}", member.name))?;

        match member.kind.as_str() {
            "select" => {
                let Some(field) = fields.get(&name) else {
                    let accepted: Vec<&str> = fields.keys().map(String::as_str).collect();
                    return Err(format!(
                        "select member `{}` matches no field of `{Entity}Response` \
                         (accepted: {}); rename the member or drop it",
                        member.name,
                        accepted.join(", "),
                        Entity = ctx.entity_name,
                    ));
                };
                let copy = select_copy(&shape.name, member, &member_type, member_optional, field)?;
                if !copy {
                    all_copy = false;
                }
                reads.push(field.name.clone());
                lines.push(format!("            {}: {},", name, copy_expr(field)));
            }
            "derive" => {
                let Some(expression) = member.expression.as_deref() else {
                    return Err(format!(
                        "member `{}` is a derivation with no expression source; the artifact \
                         is malformed",
                        member.name
                    ));
                };
                let member_numeric = NumericTy::of_rust(&member_type).ok_or_else(|| {
                    format!(
                        "derive member `{}` types `{member_type}` — derived members must be \
                         numeric (`int`/`long`/`float`/`double`); the read path computes \
                         arithmetic only",
                        member.name
                    )
                })?;
                if member_optional {
                    return Err(format!(
                        "derive member `{}` is optional (`0..1`) — a computed member is \
                         always present; declare it required",
                        member.name
                    ));
                }
                let expr = lower_derive(&member.name, expression, member_numeric, &fields)?;
                for field in fields_used(expression, &fields) {
                    reads.push(field);
                }
                lines.push(format!("            {name}: {expr},"));
            }
            other => {
                return Err(format!(
                    "member `{}` has kind `{other}` — projections lower `select` and \
                     `derive` members only",
                    member.name
                ));
            }
        }
    }

    let construct = lines.join("\n");
    reads.sort();
    reads.dedup();
    Ok(ProjectionPlan {
        entity_response: format!("{}Response", ctx.entity_name),
        source_module: (source.domain.clone(), ctx.module_name.clone()),
        construct,
        reads,
        all_copy,
    })
}

/// The response field universe, exactly as `domain_types/dto_response.tera`
/// renders it.
fn response_fields(ctx: &DtoContext) -> BTreeMap<String, ProjectionField> {
    let mut out = BTreeMap::new();
    for field in &ctx.fields {
        let base = if field.is_entity_ref {
            "uuid::Uuid".to_string()
        } else {
            field.rust_type.clone()
        };
        let inner = if field.is_array {
            format!("Vec<{base}>")
        } else {
            base.clone()
        };
        let optional = !field.is_required;
        let rendered = if optional {
            format!("Option<{inner}>")
        } else {
            inner.clone()
        };
        let numeric = (!field.is_array && !field.is_entity_ref)
            .then(|| NumericTy::of_rust(&base))
            .flatten();
        let copy = !field.is_array && is_copy_rust(&base);
        out.insert(
            field.name.clone(),
            ProjectionField {
                name: field.name.clone(),
                rendered_type: rendered,
                inner_type: inner,
                optional,
                numeric,
                copy,
            },
        );
    }
    out
}

/// Rust primitive/`Uuid` types that are `Copy`.
fn is_copy_rust(rust_type: &str) -> bool {
    matches!(
        rust_type,
        "i16" | "i32" | "i64" | "f32" | "f64" | "u32" | "u64" | "bool" | "uuid::Uuid"
    )
}

/// The assignment expression copying one response field into the projection.
fn copy_expr(field: &ProjectionField) -> String {
    if field.copy && !field.optional {
        format!("r.{}", field.name)
    } else {
        format!("r.{}.clone()", field.name)
    }
}

/// Unify one select member with its response field, returning whether the
/// assignment is a plain `Copy` move (vs a `.clone()`).
fn select_copy(
    boundary: &str,
    member: &codegraph_core::types::DddDtoMember,
    member_type: &str,
    member_optional: bool,
    field: &ProjectionField,
) -> Result<bool, String> {
    let refuse = |why: String| -> Result<bool, String> {
        Err(format!(
            "select member `{}` types `{member_type}`, but `{Entity}Response.{}` types `{}` — \
             {why}",
            member.name,
            field.name,
            field.rendered_type,
            Entity = boundary,
        ))
    };
    let member_vec = member_type.starts_with("Vec<");
    if member_vec != field.inner_type.starts_with("Vec<") {
        return refuse(format!(
            "a {many} member cannot copy a {other} column (multiplicity must agree)",
            many = if member_vec { "collection" } else { "single" },
            other = if member_vec { "single" } else { "collection" },
        ));
    }
    let member_inner = strip_option(member_type).to_string();
    let field_inner = strip_option(&field.inner_type).to_string();
    if member_inner != field_inner {
        return refuse(
            "the member and the column must unify exactly (the primitive mapping is \
             shared with the DTO plane: `int`→`i32`, `long`→`i64`, `float`→`f32`, \
             `double`→`f64`)"
                .to_string(),
        );
    }
    if !member_optional && field.optional {
        // A required member over an optional column: the constructor would
        // have to conjure a value the row may not carry.
        return refuse(
            "a required member cannot copy an optional column; declare the member \
             optional (`0..1`) or the feature required"
                .to_string(),
        );
    }
    let copy = member_inner == member_type && is_copy_rust(&member_inner);
    Ok(copy)
}

fn strip_option(ty: &str) -> &str {
    ty.strip_prefix("Option<")
        .and_then(|inner| inner.strip_suffix('>'))
        .unwrap_or(ty)
}

/// The response fields a derive expression names (for the reads list).
fn fields_used(source: &str, fields: &BTreeMap<String, ProjectionField>) -> Vec<String> {
    let parsed = phero_expr::parse(source);
    let Some(ast) = parsed.ast else {
        return Vec::new();
    };
    if !parsed.errors.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    walk_names(&ast, &mut |name| {
        let snake = codegraph_naming::to_snake_case(name);
        if fields.contains_key(&snake) {
            out.push(snake);
        }
    });
    out.sort();
    out.dedup();
    out
}

fn walk_names(ast: &phero_expr::Expr, f: &mut impl FnMut(&str)) {
    use phero_expr::ExprKind::*;
    match &ast.kind {
        Name(name) => f(name),
        Binary { lhs, rhs, .. } => {
            walk_names(lhs, f);
            walk_names(rhs, f);
        }
        Unary { expr, .. } => walk_names(expr, f),
        _ => {}
    }
}

/// Lower one derive expression through the bounded arithmetic subset, with
/// every leaf adapted to the member's numeric type.
///
/// Every construct outside `+ - * /`, unary minus, numeric field refs,
/// numeric literals, and parentheses is a NAMED refusal.
fn lower_derive(
    member: &str,
    source: &str,
    target: NumericTy,
    fields: &BTreeMap<String, ProjectionField>,
) -> Result<String, String> {
    let ast = match phero_expr::parse(source) {
        // The `expr_sql` convention: a recovered AST with zero errors.
        parsed if parsed.errors.is_empty() => parsed.ast,
        parsed => {
            let reason = parsed
                .errors
                .first()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "no expression".to_string());
            return Err(format!(
                "derive member `{member}`: the expression `{source}` does not parse \
                 ({reason}); the derive subset parses arithmetic only"
            ));
        }
    };
    let Some(ast) = ast else {
        return Err(format!(
            "derive member `{member}`: the expression `{source}` parses to no expression"
        ));
    };
    let body = lower_arith(member, &ast, target, fields)?;
    // BinOp nodes self-parenthesize (`(a / b)`), and a unary renders
    // `-({inner})` — both are already unambiguous as a struct-literal field.
    Ok(body)
}

fn lower_arith(
    member: &str,
    ast: &phero_expr::Expr,
    target: NumericTy,
    fields: &BTreeMap<String, ProjectionField>,
) -> Result<String, String> {
    use phero_expr::{BinOp, ExprKind, UnOp};
    let refuse = |construct: &str| -> Result<String, String> {
        Err(format!(
            "derive member `{member}`: the expression uses {construct}, which the \
             read-path derive subset does not support (subset: `+ - * /` over \
             numeric stored fields and numeric literals, parentheses)"
        ))
    };
    match &ast.kind {
        ExprKind::Int(value) => {
            if matches!(target, NumericTy::I32 | NumericTy::I64) && *value > i32::MAX as i64 {
                return Err(format!(
                    "derive member `{member}`: integer literal `{value}` overflows `i32`; \
                     declare the member `long`"
                ));
            }
            Ok(target.int_literal(*value))
        }
        ExprKind::Float(value) => match target {
            NumericTy::F32 | NumericTy::F64 => Ok(target.float_literal(*value)),
            NumericTy::I32 | NumericTy::I64 => Err(format!(
                "derive member `{member}`: the float literal would truncate into the \
                 integer member type `{}` — declare the member `float`/`double`",
                target.rust()
            )),
        },
        ExprKind::Name(name) => {
            let snake = codegraph_naming::to_snake_case(name);
            let field = fields.get(&snake).ok_or_else(|| {
                let accepted: Vec<&str> = fields.keys().map(String::as_str).collect();
                format!(
                    "derive member `{member}`: `{name}` names no field of the response \
                     (accepted: {})",
                    accepted.join(", ")
                )
            })?;
            let Some(numeric) = field.numeric else {
                return Err(format!(
                    "derive member `{member}`: `{name}` types `{}` — arithmetic reads \
                     numeric stored fields only (`int`/`long`/`float`/`double`)",
                    field.rendered_type
                ));
            };
            if field.optional {
                return Err(format!(
                    "derive member `{member}`: `{name}` is an OPTIONAL column — \
                     arithmetic over `Option` is not supported; declare the member over \
                     a required numeric feature"
                ));
            }
            if !target.accepts(numeric) {
                return Err(format!(
                    "derive member `{member}`: `{name}` types `{}` and the member types \
                     `{}` — the lossy {} → {} adaptation is refused; declare the member \
                     `{}` (or narrower)",
                    field.rendered_type,
                    target.rust(),
                    numeric.rust(),
                    target.rust(),
                    numeric.rust(),
                ));
            }
            Ok(target.adapt(&format!("r.{snake}"), numeric))
        }
        ExprKind::Binary {
            op: bin_op,
            lhs,
            rhs,
        } => match bin_op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                let op = match bin_op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    _ => "/",
                };
                let lhs_text = lower_arith(member, lhs, target, fields)?;
                let rhs_text = lower_arith(member, rhs, target, fields)?;
                if *bin_op == BinOp::Div && is_provable_zero(rhs) {
                    return Err(format!(
                        "derive member `{member}`: division by the literal `0` always \
                         fails — fix the expression"
                    ));
                }
                Ok(format!("({lhs_text} {op} {rhs_text})"))
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                refuse("a comparison")
            }
            BinOp::And | BinOp::Or => refuse("a boolean operator"),
        },
        ExprKind::Unary { op: un_op, expr } => match un_op {
            UnOp::Neg => {
                let inner = lower_arith(member, expr, target, fields)?;
                Ok(format!("-({inner})"))
            }
            UnOp::Not => refuse("a boolean negation"),
        },
        ExprKind::String(_) | ExprKind::Bool(_) | ExprKind::Null | ExprKind::Date { .. } => {
            refuse("a non-numeric literal")
        }
        ExprKind::FeatureAccess { name, .. } => {
            let _ = name;
            refuse(
                "feature navigation — the derive subset reads own-record numeric fields \
                 (bare names) only",
            )
        }
        ExprKind::Coalesce { .. } => refuse("a `?:` coalesce"),
        ExprKind::Call { .. } | ExprKind::FunctionCall { .. } | ExprKind::Algebra { .. } => {
            refuse("a call or collection algebra")
        }
        ExprKind::If { .. } | ExprKind::Let { .. } | ExprKind::Lambda { .. } => {
            refuse("a control-flow construct (if/let/lambda)")
        }
        ExprKind::ListLiteral(_) => refuse("a list literal"),
    }
}

fn is_provable_zero(ast: &phero_expr::Expr) -> bool {
    constant_value(ast).is_some_and(|value| value == 0.0)
}

/// Constant-fold an expression over numeric literals (`2 - 2`, `-(0)`,
/// `(1 * 0)`) — the provable-zero divisor check. Arithmetic overflow or any
/// non-literal operand yields `None` (not provably constant).
fn constant_value(ast: &phero_expr::Expr) -> Option<f64> {
    use phero_expr::{BinOp, ExprKind, UnOp};
    match &ast.kind {
        ExprKind::Int(value) => Some(*value as f64),
        ExprKind::Float(value) => Some(*value),
        ExprKind::Unary {
            op: UnOp::Neg,
            expr,
        } => constant_value(expr).map(|v| -v),
        ExprKind::Binary { op, lhs, rhs } => {
            let (l, r) = (constant_value(lhs), constant_value(rhs));
            let (l, r) = (l?, r?);
            match op {
                BinOp::Add => Some(l + r),
                BinOp::Sub => Some(l - r),
                BinOp::Mul => Some(l * r),
                BinOp::Div if r != 0.0 => Some(l / r),
                _ => None,
            }
        }
        _ => None,
    }
}

// ── source resolution (shared with the boundary generator) ─────────────

/// The authored source class name of a boundary (`nz.example.library.Book` →
/// `Book`), or `None` for a shape with no source (standalone).
pub(crate) fn shape_source_class(shape: &DddDtoShapeNode) -> Option<String> {
    let source = shape.source_json.as_ref()?;
    let value = source.get("value")?;
    value
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// The generation-order class→source index the projection constructors
/// resolve through (issue #768): one rule with the entity plane, so the
/// constructor's `use` path and the emitted Response cannot disagree. A
/// class may be authored without the configured `+type_suffix`; both
/// spellings resolve to the same entry (the `wire_alias_refs` chain).
pub(crate) fn projection_source_index(
    config: &DomainConfig,
    generation_order: &[crate::GenerationEntry],
) -> BTreeMap<String, ProjectionSource> {
    let mut out = BTreeMap::new();
    for entry in generation_order {
        let bare = config.defaults.strip_suffix(&entry.schema_title);
        for class in [entry.schema_title.clone(), bare] {
            out.entry(class).or_insert_with(|| ProjectionSource {
                schema_title: entry.schema_title.clone(),
                domain: entry.domain.clone(),
            });
        }
    }
    out
}

/// The pre-generation refusals walker (the `body_refusals` posture, issue
/// #768): plan every projection boundary's `from_row` constructor. Every
/// planning failure — a select that matches no response field, a derive
/// expression outside the arithmetic subset, a lossy adaptation — is a
/// named refusal so the run fails BEFORE any tree is written.
pub async fn projection_refusals(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<String>, GraphError> {
    let models = db.get_ddd_models().await?;
    let shapes: Vec<&DddDtoShapeNode> = models
        .iter()
        .flat_map(|model| model.dto_shapes.iter())
        .collect();
    if shapes.is_empty() {
        return Ok(Vec::new());
    }
    let generation_order = crate::ordering::compute_generation_order(db, config)
        .await
        .map_err(|e| GraphError::Query(e.to_string()))?;
    let sources = projection_source_index(config, &generation_order);
    let shapes_by_name: BTreeMap<String, DddDtoShapeNode> = shapes
        .iter()
        .map(|s| (s.name.clone(), (*s).clone()))
        .collect();
    let module_of: BTreeMap<String, String> = shapes
        .iter()
        .map(|s| (s.name.clone(), codegraph_naming::to_snake_case(&s.module)))
        .collect();
    let mut refusals = Vec::new();
    for shape in &shapes {
        let Some(class) = shape_source_class(shape) else {
            continue;
        };
        if shape.direction != "output" {
            continue;
        }
        let Some(source) = sources.get(&class) else {
            // The generator WARNs and skips this shape; the read-model planes
            // refuse when an op or finder binds it. Quiet here.
            continue;
        };
        if let Err(reason) =
            plan_projection_from_row(db, config, shape, &shapes_by_name, &module_of, source).await
        {
            refusals.push(format!("modeled DTO boundary `{}`: {reason}", shape.name));
        }
    }

    // Issue #768 (review): the projection READ endpoints consume the op-set
    // plane — the item read renders the record load (`find_by_id`), the
    // collection read renders `list_filtered`. An entity whose design
    // exposes neither op would emit a handler naming a query method that
    // does not exist (E0425/E0599 at consumer build, silently). The same
    // requirement rides the pre-generation pass, the #662 posture.
    let type_suffix = config.defaults.type_suffix.clone();
    let boundaries = crate::ddd::dto_boundaries::DtoBoundaryIndex::from_models(&models);
    let surface = crate::ddd::design::DddDesignSurface::from_models(&models);
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                if op.is_protected
                    || !op.bodies.is_empty()
                    || op.transition.is_some()
                    || op.delegation_target.is_some()
                {
                    continue;
                }
                // The BOUND entity is the routes plane's resolution —
                // bind_entity (one rule, two consumers), so a
                // capability-bound op returning a projection of another
                // class skips exactly as the routes plane's 501 seam does.
                let Some(bound_title) =
                    crate::ddd::service_ops::bind_entity(model, op, db, &type_suffix).await
                else {
                    continue;
                };
                // The candidate predicate is the routes plane's OWN — one
                // rule, so the walker refuses exactly what would lower.
                let Some(collection) = crate::ddd::service_ops::projection_shape_candidate_public(
                    op,
                    model,
                    &bound_title,
                    None,
                    &boundaries,
                ) else {
                    continue;
                };
                let surface_ops = surface.operations_for(&bound_title).unwrap_or_default();
                let required = if collection { "list" } else { "read" };
                if !surface_ops.iter().any(|o| o == required) {
                    refusals.push(format!(
                        "service op `{}` is {} projection read, but `{bound_title}` exposes \
                         no `{required}` operation — the route loads what it projects; give \
                         the design repository a `{}` (or return {})",
                        op.name,
                        if collection {
                            "the collection"
                        } else {
                            "an item-addressed"
                        },
                        if collection { "findAll" } else { "findById" },
                        if collection {
                            "a single projection"
                        } else {
                            "a collection projection"
                        },
                    ));
                }
            }
        }
    }
    Ok(refusals)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, ty: &str, required: bool) -> ProjectionField {
        let inner = ty.to_string();
        let optional = !required;
        ProjectionField {
            name: name.to_string(),
            rendered_type: if optional {
                format!("Option<{inner}>")
            } else {
                inner.clone()
            },
            inner_type: inner,
            optional,
            numeric: NumericTy::of_rust(ty),
            copy: is_copy_rust(ty),
        }
    }

    /// Parse an expression through the production convention (a recovered
    /// AST with zero errors), refusing anything else.
    fn parse_expr(source: &str) -> phero_expr::Expr {
        let parsed = phero_expr::parse(source);
        assert!(parsed.errors.is_empty(), "{source:?}: {:?}", parsed.errors);
        parsed.ast.expect("ast")
    }

    fn fields() -> BTreeMap<String, ProjectionField> {
        BTreeMap::from([
            ("title".to_string(), field("title", "String", true)),
            ("play_length".to_string(), field("play_length", "i32", true)),
            ("rating".to_string(), field("rating", "f64", false)),
            ("price".to_string(), field("price", "f32", true)),
        ])
    }

    // ── derive arithmetic ──────────────────────────────────────────────

    #[test]
    fn int_division_lowers_verbatim_when_types_agree() {
        let fields = fields();
        let ast = parse_expr("playLength / 60");
        let out = lower_arith("minutes", &ast, NumericTy::I32, &fields).unwrap();
        assert_eq!(out, "(r.play_length / 60)");
    }

    #[test]
    fn widening_field_adaptation_is_explicit_from() {
        let fields = fields();
        let ast = parse_expr("playLength + playLength");
        let out = lower_arith("total", &ast, NumericTy::I64, &fields).unwrap();
        assert_eq!(out, "(i64::from(r.play_length) + i64::from(r.play_length))");
    }

    #[test]
    fn f32_fields_widen_into_f64_members() {
        let fields = fields();
        let ast = parse_expr("price * 2");
        let out = lower_arith("m", &ast, NumericTy::F64, &fields).unwrap();
        assert_eq!(out, "(f64::from(r.price) * 2.0f64)");
    }

    #[test]
    fn literals_adapt_to_the_member_type() {
        let fields = fields();
        let ast = parse_expr("60");
        assert_eq!(
            lower_arith("m", &ast, NumericTy::I64, &fields).unwrap(),
            "60i64"
        );
        let ast = parse_expr("2");
        assert_eq!(
            lower_arith("m", &ast, NumericTy::F32, &fields).unwrap(),
            "2.0f32"
        );
    }

    #[test]
    fn unary_minus_lowers() {
        let fields = fields();
        let ast = parse_expr("-playLength");
        let out = lower_arith("m", &ast, NumericTy::I32, &fields).unwrap();
        assert_eq!(out, "-(r.play_length)");
    }

    #[test]
    fn parens_preserve_grouping() {
        let fields = fields();
        let ast = parse_expr("(playLength + 1) * 2");
        let out = lower_arith("m", &ast, NumericTy::I32, &fields).unwrap();
        assert_eq!(out, "((r.play_length + 1) * 2)");
    }

    // ── refusals (named, total) ────────────────────────────────────────

    #[test]
    fn navigation_refuses_by_name() {
        let fields = fields();
        let ast = parse_expr("book.playLength");
        let error = lower_arith("m", &ast, NumericTy::I32, &fields).unwrap_err();
        assert!(error.contains("feature navigation"), "{error}");
        assert!(error.contains("own-record numeric fields"), "{error}");
    }

    #[test]
    fn unknown_field_refuses_and_names_the_accepted_set() {
        let fields = fields();
        let ast = parse_expr("typo + 1");
        let error = lower_arith("m", &ast, NumericTy::I32, &fields).unwrap_err();
        assert!(error.contains("`typo` names no field"), "{error}");
        assert!(error.contains("play_length"), "{error}");
    }

    #[test]
    fn string_field_arithmetic_refuses() {
        let fields = fields();
        let ast = parse_expr("title + 1");
        let error = lower_arith("m", &ast, NumericTy::I32, &fields).unwrap_err();
        assert!(error.contains("numeric stored fields only"), "{error}");
        assert!(error.contains("String"), "{error}");
    }

    #[test]
    fn optional_field_arithmetic_refuses() {
        let fields = fields();
        let ast = parse_expr("rating + 1.0");
        let error = lower_arith("m", &ast, NumericTy::F64, &fields).unwrap_err();
        assert!(error.contains("OPTIONAL"), "{error}");
    }

    #[test]
    fn lossy_adaptation_refuses_by_name() {
        // The lossy directions: an f64 field into an f32 member, and any
        // float into an int member.
        let mut fields = fields();
        fields.insert("rating".to_string(), field("rating", "f64", true));
        let ast = parse_expr("rating + 1.0");
        let error = lower_arith("m", &ast, NumericTy::F32, &fields).unwrap_err();
        assert!(error.contains("lossy"), "{error}");
        let error = lower_arith("m", &ast, NumericTy::I32, &fields).unwrap_err();
        assert!(
            error.contains("lossy") || error.contains("numeric stored fields only"),
            "{error}"
        );
    }

    #[test]
    fn float_literal_into_int_member_refuses() {
        let fields = fields();
        let ast = parse_expr("playLength + 1.5");
        let error = lower_arith("m", &ast, NumericTy::I32, &fields).unwrap_err();
        assert!(error.contains("truncate"), "{error}");
    }

    #[test]
    fn division_by_zero_literal_refuses() {
        let fields = fields();
        let ast = parse_expr("playLength / 0");
        let error = lower_arith("m", &ast, NumericTy::I32, &fields).unwrap_err();
        assert!(error.contains("division by the literal `0`"), "{error}");
        let ast = parse_expr("playLength / (2 - 2)");
        let error = lower_arith("m", &ast, NumericTy::I32, &fields).unwrap_err();
        assert!(error.contains("division by the literal `0`"), "{error}");
    }

    #[test]
    fn comparisons_and_booleans_refuse() {
        let fields = fields();
        for source in ["playLength > 5", "true", "!(playLength == 1)"] {
            let ast = parse_expr(source);
            let error = lower_arith("m", &ast, NumericTy::I32, &fields).unwrap_err();
            assert!(
                error.contains("comparison")
                    || error.contains("literal")
                    || error.contains("negation"),
                "{source}: {error}"
            );
        }
    }

    #[test]
    fn coalesce_and_calls_refuse() {
        let fields = fields();
        for source in ["rating ?: 0", "len(title)"] {
            let ast = parse_expr(source);
            let error = lower_arith("m", &ast, NumericTy::I32, &fields).unwrap_err();
            assert!(
                error.contains("does not support") || error.contains("parse"),
                "{source}: {error}"
            );
        }
    }

    // ── numeric classification + adaptation table ──────────────────────

    #[test]
    fn adaptation_accepts_only_lossless_widenings() {
        use NumericTy::*;
        assert!(I64.accepts(I32));
        assert!(F32.accepts(I32));
        assert!(F64.accepts(I32));
        assert!(F64.accepts(F32));
        // Lossy directions refuse.
        assert!(!I32.accepts(I64));
        assert!(!F32.accepts(F64));
        assert!(!F64.accepts(I64));
        assert!(!I32.accepts(F32));
        assert!(!I64.accepts(F64));
    }

    #[test]
    fn rust_spelling_classification() {
        assert_eq!(NumericTy::of_rust("i32"), Some(NumericTy::I32));
        assert_eq!(NumericTy::of_rust("uuid::Uuid"), None);
        assert_eq!(NumericTy::of_rust("String"), None);
    }

    // ── response field universe ────────────────────────────────────────

    #[test]
    fn optional_columns_render_option_and_lose_numeric_rating() {
        let rating = &fields()["rating"];
        assert_eq!(rating.rendered_type, "Option<f64>");
        assert!(rating.optional);
        assert_eq!(rating.numeric, Some(NumericTy::F64));
    }

    #[test]
    fn copy_types_assign_directly_and_strings_clone() {
        assert_eq!(copy_expr(&fields()["play_length"]), "r.play_length");
        assert_eq!(copy_expr(&fields()["title"]), "r.title.clone()");
    }

    // ── select unification ─────────────────────────────────────────────

    #[test]
    fn required_select_over_required_column_unifies() {
        let f = fields();
        let m = codegraph_core::types::DddDtoMember {
            name: "Title".to_string(),
            kind: "select".to_string(),
            type_json: serde_json::json!({ "type": "primitive", "value": "String" }),
            multiplicity_json: None,
            path: Vec::new(),
            expression: None,
        };
        // `String` is not `Copy`: the assignment is a `.clone()` (Ok(false)).
        assert!(!select_copy("B", &m, "String", false, &f["title"]).unwrap());
        // A numeric member over a numeric column IS a plain copy.
        assert!(select_copy("B", &m, "i32", false, &f["play_length"]).unwrap());
    }

    #[test]
    fn required_select_over_optional_column_refuses() {
        let f = fields();
        let m = codegraph_core::types::DddDtoMember {
            name: "Rating".to_string(),
            kind: "select".to_string(),
            type_json: serde_json::json!({ "type": "primitive", "value": "Double" }),
            multiplicity_json: None,
            path: Vec::new(),
            expression: None,
        };
        let error = select_copy("B", &m, "f64", false, &f["rating"]).unwrap_err();
        assert!(error.contains("optional column"), "{error}");
    }

    #[test]
    fn vec_select_over_scalar_column_refuses() {
        let f = fields();
        let m = codegraph_core::types::DddDtoMember {
            name: "Title".to_string(),
            kind: "select".to_string(),
            type_json: serde_json::json!({ "type": "primitive", "value": "String" }),
            multiplicity_json: Some(serde_json::json!({ "lower": 0, "upper": { "finite": 255 } })),
            path: Vec::new(),
            expression: None,
        };
        let error = select_copy("B", &m, "Vec<String>", false, &f["title"]).unwrap_err();
        assert!(error.contains("multiplicity must agree"), "{error}");
    }

    #[test]
    fn type_disagreement_names_both_spellings() {
        let f = fields();
        let m = codegraph_core::types::DddDtoMember {
            name: "Title".to_string(),
            kind: "select".to_string(),
            type_json: serde_json::json!({ "type": "primitive", "value": "Int" }),
            multiplicity_json: None,
            path: Vec::new(),
            expression: None,
        };
        let error = select_copy("B", &m, "i32", false, &f["title"]).unwrap_err();
        assert!(error.contains("i32"), "{error}");
        assert!(error.contains("String"), "{error}");
    }
}
