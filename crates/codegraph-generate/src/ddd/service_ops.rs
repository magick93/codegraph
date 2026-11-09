//! Service operations lower to API surface (issue #590).
//!
//! A `.ddd` service operation is the application surface, not
//! documentation: a public (non-`protected`) op with a resolvable entity
//! binding lowers to `POST /api/{version}/{domain}/{entity}/{id}/actions/
//! {op_snake}` — the exact wire shape the IFML action buttons already
//! emit (issue #559, `RenderActionButton`/`RemoteCommandKind::Op`). The
//! actor policy authored in `.actor` imports becomes runtime enforcement
//! at the route: capability membership, `never_both` separation-of-duties
//! conflicts, and per-grant `when` conditions evaluate server-side over
//! the loaded record.
//!
//! Entity binding resolution (first hit wins, deterministic):
//! 1. capability class binding — an op capability bound (`.actor`
//!    `capability X for Class`) to a schema title (exact → `+Type`);
//! 2. delegation to a repository — `op => SomeRepository.op` where the
//!    repository's design class resolves to a schema title;
//! 3. signature — a class-typed parameter or return type.
//!
//! Ops that bind no entity (or are `protected`) emit no route; they are
//! reported by [`dead_surface_warnings`] (the Ask 1 diagnostic).
//!
//! Execution lowering for v1: repository `delete` delegations wire to the
//! entity's command delete, `findById` delegations return the loaded
//! record; every other op answers 501 naming the gap (the functions-plane
//! seam precedent) — the enforcement is the deliverable; delegation
//! breadth is a recorded follow-up.

use std::collections::{BTreeMap, HashMap};

use codegraph_config::DomainConfig;
use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PropertyNode;
use codegraph_naming::to_snake_case;
use rex_expr::{BinOp, Expr, ExprKind, UnOp};

use crate::ddd::design::DddDesignSurface;

// ── Route model ────────────────────────────────────────────────────────

/// One service operation lowered to a route on one entity.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceOpRoute {
    /// The owning service name.
    pub service: String,
    /// Operation name as authored.
    pub op_name: String,
    /// Route path segment and handler-fn suffix (`to_snake_case(op_name)`).
    pub snake: String,
    /// Handler fn name (`service_op_{snake}`).
    pub fn_name: String,
    /// Actor capability names guarding the op, as authored. Empty = ungated.
    pub capabilities: Vec<String>,
    /// Pre-rendered authorization fn (the whole item, doc comment
    /// included). `None` = ungated: no capabilities, or no actor policy in
    /// the graph.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorize_fn: Option<String>,
    /// `true` when the op delegates to the bound repository's `delete`
    /// builtin and the entity's command plane exposes delete.
    pub is_delete: bool,
    /// `true` when the op delegates to the bound repository's `findById`
    /// builtin and the entity's op set exposes read.
    pub is_read: bool,
    /// `true` when the bound entity's query plane carries `find_by_id`
    /// (the #449 invariant: the design surface exposes `read`). When
    /// `false` — a capability bound to a non-root entity — the route
    /// renders without the record load, and the authorize fn (if any)
    /// takes no response parameter: capability membership only. The
    /// route body still 501s (no record ⇒ no `read`/`delete` lowering),
    /// so nothing flows past the seam.
    pub record_load: bool,
    /// The 501-seam reason for ops with no executable lowering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seam_reason: Option<String>,
}

// ── Field knowledge ────────────────────────────────────────────────────

/// How a record field compares in the Rust lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldKind {
    String,
    Bool,
    Integer,
    Float,
    Date,
    Codelist,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldInfo {
    pub(crate) rust_name: String,
    pub(crate) kind: FieldKind,
    pub(crate) optional: bool,
}

fn field_kind_for(prop: &PropertyNode) -> FieldKind {
    if prop.is_array {
        return FieldKind::Unsupported;
    }
    if matches!(
        prop.effective_kind(),
        Some(codegraph_type_contracts::RefClassificationKind::CodelistReference)
            | Some(codegraph_type_contracts::RefClassificationKind::CodelistCheck)
    ) {
        return FieldKind::Codelist;
    }
    match prop.rust_field_type.as_str() {
        "String" => FieldKind::String,
        "bool" => FieldKind::Bool,
        "chrono::NaiveDate" => FieldKind::Date,
        other => {
            let (numeric, integer) = numeric_class(other);
            if integer {
                FieldKind::Integer
            } else if numeric {
                FieldKind::Float
            } else {
                FieldKind::Unsupported
            }
        }
    }
}

/// `(numeric, integer)` — the `rules.rs` classification, minus the
/// collection unwrap (arrays are refused above).
fn numeric_class(rust_field_type: &str) -> (bool, bool) {
    let integer = matches!(
        rust_field_type,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "isize"
    );
    let numeric = integer || matches!(rust_field_type, "f32" | "f64");
    (numeric, integer)
}

fn fields_for(props: &[PropertyNode]) -> HashMap<String, FieldInfo> {
    let mut fields = HashMap::new();
    for p in props {
        let info = FieldInfo {
            rust_name: p.rust_field_name.clone(),
            kind: field_kind_for(p),
            optional: p.is_nullable,
        };
        fields.insert(p.name.clone(), info);
        // The authored name may be camelCase; the DTO field is snake_case.
        let snake = to_snake_case(&p.name);
        fields.entry(snake).or_insert_with(|| FieldInfo {
            rust_name: p.rust_field_name.clone(),
            kind: field_kind_for(p),
            optional: p.is_nullable,
        });
    }
    fields
}

// ── Rust lowering of `when` conditions ──────────────────────────────────

/// Lower a rexlang `when` condition source to a Rust boolean expression
/// over `response` (the loaded `{Entity}Response` record).
///
/// The documented closed subset mirrors `db::expr_sql` (the RLS lowering):
/// bare own-record field refs, string/number/boolean/date literals,
/// comparisons, `&& || !`, parentheses, and null comparisons. Codelist
/// fields compare via their `Display` (the codelist code); date fields
/// compare via their ISO `Display`. EVERYTHING else is refused — what
/// cannot be lowered faithfully is a hard generation error naming the
/// capability (the rexlang Cedar-backend philosophy).
pub(crate) fn lower_when_to_rust(
    source: &str,
    capability: &str,
    entity: &str,
    fields: &HashMap<String, FieldInfo>,
) -> Result<String, String> {
    let parsed = rex_expr::parse(source);
    match parsed.ast {
        Some(ast) if parsed.errors.is_empty() => lower(&ast, capability, entity, fields),
        _ => Err(format!(
            "capability `{capability}`: `when` condition is not valid \
             rexlang expression syntax: {source:?}"
        )),
    }
}

fn lower(
    expr: &Expr,
    capability: &str,
    entity: &str,
    fields: &HashMap<String, FieldInfo>,
) -> Result<String, String> {
    let reject = |construct: &str| {
        Err(format!(
            "capability `{capability}`: `when` condition uses {construct}, \
             which cannot be lowered to a Rust guard on `{entity}`; the \
             supported subset is own-record field refs, string/number/\
             boolean/date literals, comparisons, && || !, parentheses, and \
             null comparisons"
        ))
    };
    match &expr.kind {
        ExprKind::Int(value) => Ok(value.to_string()),
        ExprKind::Float(value) => Ok(float_literal(*value)),
        ExprKind::String(text) => Ok(rust_string_literal(text)),
        ExprKind::Bool(true) => Ok("true".to_string()),
        ExprKind::Bool(false) => Ok("false".to_string()),
        ExprKind::Null => reject("`null` (compare a field to null instead)"),
        ExprKind::Date { text, .. } => {
            // Format-validated here; the comparison renders the ISO text
            // (`NaiveDate`'s Display) at the use site.
            if crate::db::expr_sql::parse_date_text(text).is_some() {
                Ok(rust_string_literal(text))
            } else {
                Err(format!(
                    "capability `{capability}`: `when` condition uses the \
                     date literal {text:?}, which is not a calendar date \
                     `YYYY-MM-DD`"
                ))
            }
        }
        ExprKind::Name(name) => {
            let field = field_for(name, capability, entity, fields)?;
            Ok(field_ref(&field))
        }
        ExprKind::Unary { op, expr } => {
            let inner = lower(expr, capability, entity, fields)?;
            match op {
                UnOp::Not => Ok(format!("(!({inner}))")),
                UnOp::Neg => match &expr.kind {
                    ExprKind::Int(value) => Ok(format!("-{value}")),
                    ExprKind::Float(value) => Ok(format!("-{}", float_literal(*value))),
                    _ => reject("negation of a non-literal (arithmetic)"),
                },
            }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            if matches!(op, BinOp::Eq | BinOp::Ne)
                && (matches!(lhs.kind, ExprKind::Null) || matches!(rhs.kind, ExprKind::Null))
            {
                let both_null =
                    matches!(lhs.kind, ExprKind::Null) && matches!(rhs.kind, ExprKind::Null);
                if both_null {
                    return Ok(match op {
                        BinOp::Eq => "true".to_string(),
                        _ => "false".to_string(),
                    });
                }
                let field_expr_expr = if matches!(lhs.kind, ExprKind::Null) {
                    rhs
                } else {
                    lhs
                };
                // The non-null operand must be a bare field ref for the
                // Option check to be honest; a required field's null
                // comparison is vacuous and lowers to a literal.
                let ExprKind::Name(name) = &field_expr_expr.kind else {
                    return reject("a null comparison against a non-field");
                };
                let field = field_for(name, capability, entity, fields)?;
                return Ok(match (op, field.optional) {
                    (BinOp::Eq, true) => format!("(response.{}.is_none())", field.rust_name),
                    (BinOp::Ne, true) => {
                        format!("(!(response.{}.is_none()))", field.rust_name)
                    }
                    (BinOp::Eq, false) => "false".to_string(),
                    (BinOp::Ne, false) => "true".to_string(),
                    _ => unreachable!("only Eq/Ne enter this branch"),
                });
            }
            let rust_op = match op {
                BinOp::Eq => "==",
                BinOp::Ne => "!=",
                BinOp::Lt => "<",
                BinOp::Le => "<=",
                BinOp::Gt => ">",
                BinOp::Ge => ">=",
                BinOp::And => "&&",
                BinOp::Or => "||",
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    return reject(&format!("arithmetic (`{op}`)"));
                }
            };
            comparison(
                op,
                &lower(lhs, capability, entity, fields)?,
                &lower(rhs, capability, entity, fields)?,
                rust_op,
                capability,
                entity,
                fields,
                lhs,
                rhs,
            )
        }
        ExprKind::FeatureAccess {
            receiver,
            name,
            optional_safe,
        } => {
            if *optional_safe {
                return reject("`?.` (safe navigation)");
            }
            reject(&format!(
                "field access `{}`.{}, which is not an own-record field; \
                 only bare field references are supported",
                write_path(receiver),
                name.value
            ))
        }
        ExprKind::Coalesce { .. } => reject("`?:`"),
        ExprKind::Call { .. } | ExprKind::FunctionCall { .. } => reject("an operation call"),
        ExprKind::Algebra { kind, .. } => reject(&format!("collection algebra `{kind}`")),
        ExprKind::If { .. } => reject("`if`"),
        ExprKind::Let { .. } => reject("`let`"),
        ExprKind::Lambda { .. } => reject("a lambda"),
        ExprKind::ListLiteral(_) => reject("list literals"),
    }
}

/// Render a comparison. Field-vs-literal comparisons unwrap `Option`
/// fields honestly; field-vs-field comparisons require matching
/// requiredness (mixed requiredness cannot compare without a policy
/// decision the model does not make).
#[allow(clippy::too_many_arguments)]
fn comparison(
    op: &BinOp,
    lhs_text: &str,
    rhs_text: &str,
    rust_op: &str,
    capability: &str,
    entity: &str,
    fields: &HashMap<String, FieldInfo>,
    lhs: &Expr,
    rhs: &Expr,
) -> Result<String, String> {
    let is_ordering = matches!(op, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge);
    let lhs_field = operand_field(lhs, fields);
    let rhs_field = operand_field(rhs, fields);
    match (lhs_field, rhs_field) {
        (Some(field), None) => {
            let literal = strip_parens(rhs_text);
            field_vs_literal(&field, &literal, rust_op, is_ordering, capability, entity)
        }
        (None, Some(field)) => {
            let literal = strip_parens(lhs_text);
            // Flip the ordering operator so the field stays on the left.
            let flipped = match op {
                BinOp::Lt => ">",
                BinOp::Le => ">=",
                BinOp::Gt => "<",
                BinOp::Ge => "<=",
                BinOp::Eq => "==",
                BinOp::Ne => "!=",
                BinOp::And => "&&",
                BinOp::Or => "||",
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    unreachable!("arithmetic is rejected before comparison rendering")
                }
            };
            field_vs_literal(&field, &literal, flipped, is_ordering, capability, entity)
        }
        (Some(a), Some(b)) => {
            if is_ordering && (a.optional || b.optional || a.kind != b.kind) {
                return Err(format!(
                    "capability `{capability}`: ordering comparison between \
                     fields `{}` and `{}` cannot be lowered faithfully",
                    a.rust_name, b.rust_name
                ));
            }
            if a.optional != b.optional || a.kind != b.kind {
                return Err(format!(
                    "capability `{capability}`: field-to-field comparison \
                     between `{}` and `{}` mixes requiredness or types",
                    a.rust_name, b.rust_name
                ));
            }
            Ok(format!(
                "(response.{} {rust_op} response.{})",
                a.rust_name, b.rust_name
            ))
        }
        (None, None) => Ok(format!("({lhs_text} {rust_op} {rhs_text})")),
    }
}

fn field_vs_literal(
    field: &FieldInfo,
    literal: &str,
    rust_op: &str,
    is_ordering: bool,
    capability: &str,
    entity: &str,
) -> Result<String, String> {
    let name = &field.rust_name;
    match field.kind {
        FieldKind::String => match field.optional {
            false => Ok(format!("(response.{name} {rust_op} {literal})")),
            true if is_ordering => Ok(format!(
                "(response.{name}.as_deref().is_some_and(|v| v {rust_op} {literal}))"
            )),
            true => Ok(format!(
                "(response.{name}.as_deref() {} Some({literal}))",
                eq_op(rust_op)
            )),
        },
        FieldKind::Bool => {
            if is_ordering {
                return Err(format!(
                    "capability `{capability}`: ordering comparison on bool \
                     field `{name}` of `{entity}`"
                ));
            }
            match field.optional {
                false => Ok(format!("(response.{name} {rust_op} {literal})")),
                true => Ok(format!(
                    "(response.{name} {} Some({literal}))",
                    eq_op(rust_op)
                )),
            }
        }
        FieldKind::Integer | FieldKind::Float => match field.optional {
            false => Ok(format!("(response.{name} {rust_op} {literal})")),
            true if is_ordering => Ok(format!(
                "(response.{name}.is_some_and(|v| v {rust_op} {literal}))"
            )),
            true => Ok(format!(
                "(response.{name} {} Some({literal}))",
                eq_op(rust_op)
            )),
        },
        FieldKind::Codelist | FieldKind::Date => {
            if is_ordering {
                return Err(format!(
                    "capability `{capability}`: ordering comparison on \
                     `{kind}` field `{name}` of `{entity}`",
                    kind = if field.kind == FieldKind::Codelist {
                        "codelist"
                    } else {
                        "date"
                    },
                ));
            }
            match field.optional {
                false => Ok(format!("(response.{name}.to_string() {rust_op} {literal})")),
                true => Ok(format!(
                    "(response.{name}.as_ref().map(|v| v.to_string()).as_deref() {} Some({literal}))",
                    eq_op(rust_op)
                )),
            }
        }
        FieldKind::Unsupported => Err(format!(
            "capability `{capability}`: `when` condition references `{name}`, \
             which is not a comparable scalar field of `{entity}`"
        )),
    }
}

fn eq_op(rust_op: &str) -> &'static str {
    match rust_op {
        "==" => "==",
        _ => "!=",
    }
}

/// The field an operand refers to, when it is a bare field ref.
fn operand_field(expr: &Expr, fields: &HashMap<String, FieldInfo>) -> Option<FieldInfo> {
    match &expr.kind {
        ExprKind::Name(name) => field_for(name, "", "", fields).ok(),
        _ => None,
    }
}

fn field_for(
    name: &str,
    capability: &str,
    entity: &str,
    fields: &HashMap<String, FieldInfo>,
) -> Result<FieldInfo, String> {
    fields.get(name).cloned().ok_or_else(|| {
        format!(
            "capability `{capability}`: `when` condition references \
                 `{name}`, which is not a field of `{entity}`"
        )
    })
}

/// Render one field reference (the bare-ref arm of `lower`).
fn field_ref(field: &FieldInfo) -> String {
    format!("response.{}", field.rust_name)
}

fn strip_parens(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

fn write_path(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Name(name) => name.clone(),
        ExprKind::FeatureAccess {
            receiver,
            name,
            optional_safe,
        } => format!(
            "{}{}{}",
            write_path(receiver),
            if *optional_safe { "?." } else { "." },
            name.value
        ),
        _ => "<expression>".to_string(),
    }
}

/// Keep the decimal point so the literal reads as the float the DSL
/// authored (the `expr_sql` convention).
fn float_literal(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value}.0")
    } else {
        format!("{value}")
    }
}

/// Render `text` as a Rust string literal.
fn rust_string_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

// ── Policy surface ─────────────────────────────────────────────────────

/// The actor policy flattened for server-side enforcement: BaseJump role →
/// capability → grant `when` sources (None = unconditional), plus the
/// `never_both` groups. Role mapping mirrors the IFML hydration convention
/// (`route_generator/hydration.rs`): an actor whose snake_case name equals
/// the account role claims it; otherwise `owner` falls to the first actor
/// and `member` to the last.
#[derive(Debug, Clone, Default)]
pub(crate) struct PolicySurface {
    pub roles: BTreeMap<String, BTreeMap<String, Vec<Option<String>>>>,
    pub never_both: Vec<Vec<String>>,
}

impl PolicySurface {
    /// Build from the graph readback. `None` when the graph carries no
    /// actors (no policy ingested — capability/SoD gates are skipped with
    /// a warning, the IFML warn-and-continue posture).
    pub async fn from_graph(db: &dyn GraphQuerier) -> Result<Option<Self>, GraphError> {
        let actors = db.get_actors().await?;
        if actors.is_empty() {
            return Ok(None);
        }
        let grants = db.get_grants().await?;
        let actor_names: Vec<String> = actors.iter().map(|a| a.name.clone()).collect();
        let mut roles: BTreeMap<String, BTreeMap<String, Vec<Option<String>>>> = BTreeMap::new();
        for role in ["owner", "member", "manager", "employee"] {
            let picked = basejump_role_actors(role, &actor_names);
            if picked.is_empty() {
                continue;
            }
            let mut permits: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();
            for actor in &picked {
                for permit in
                    codegraph_core::types::resolve_effective_permits(&actors, &grants, actor)
                {
                    let entry = permits.entry(permit.capability.clone()).or_default();
                    let when = permit.when.clone();
                    if !entry.contains(&when) {
                        entry.push(when);
                    }
                }
            }
            roles.insert(role.to_string(), permits);
        }
        let never_both = db
            .get_actor_policy()
            .await?
            .map(|policy| {
                policy
                    .never_both
                    .iter()
                    .map(|group| group.capabilities.clone())
                    .collect()
            })
            .unwrap_or_default();
        Ok(Some(Self { roles, never_both }))
    }

    /// The capabilities that conflict with `capabilities` under
    /// `never_both` (deterministic order, deduped).
    pub fn sod_conflicts(&self, capabilities: &[String]) -> Vec<String> {
        let mut conflicts = Vec::new();
        for group in &self.never_both {
            if group.iter().any(|c| capabilities.contains(c)) {
                for member in group {
                    if !capabilities.contains(member) && !conflicts.contains(member) {
                        conflicts.push(member.clone());
                    }
                }
            }
        }
        conflicts
    }
}

/// The actors mapped to one BaseJump account role (the hydration
/// convention).
fn basejump_role_actors(role: &str, actor_names: &[String]) -> Vec<String> {
    let snake_match = actor_names.iter().find(|a| to_snake_case(a) == role);
    match (role, snake_match, actor_names.first(), actor_names.last()) {
        (_, Some(a), _, _) => vec![a.clone()],
        ("owner", None, Some(first), _) => vec![first.clone()],
        ("member", None, _, Some(last)) => vec![last.clone()],
        _ => Vec::new(),
    }
}

// ── Authorization fn rendering ──────────────────────────────────────────

/// Render the per-op `authorize` fn encoding the policy for the route:
/// capability membership, `never_both` separation of duties, and per-grant
/// `when` guards evaluated over the loaded record.
///
/// `None` when no role holds any of the op's capabilities AND none even
/// conflicts — the route is emitted ungated-capability-free only when the
/// op declares no capabilities; a capability no grant mentions emits a
/// deny-all gate (the policy says nobody holds it), so this returns the
/// fn whenever the policy surface exists.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_authorize_fn(
    snake: &str,
    op_name: &str,
    service: &str,
    capabilities: &[String],
    condition: Option<&str>,
    response_type_path: &str,
    surface: Option<&PolicySurface>,
    fields: &HashMap<String, FieldInfo>,
    entity: &str,
    record_available: bool,
) -> Result<Option<String>, String> {
    let lowered_condition = condition
        .map(|source| {
            let lowered = lower_when_to_rust(source, op_name, entity, fields)?;
            Ok::<_, String>((lowered, source.to_string()))
        })
        .transpose()?;
    let condition_message = condition.map(|source| {
        rust_string_literal(&format!(
            "operation `{op_name}`: `when` condition not met ({source})"
        ))
    });
    if capabilities.is_empty() && lowered_condition.is_none() {
        return Ok(None);
    }
    let empty_roles = BTreeMap::new();
    let roles = match surface {
        Some(policy) => &policy.roles,
        None => &empty_roles,
    };
    // Does the rendered gate read the loaded record? The op-level `when`
    // always does; a per-grant `when` does whenever a held permit carries
    // one without an unconditional (None) override. (Over-approximates
    // only when a when-guarded permit also lands on a `never_both`
    // conflict — the conflict arm never reads the record — failing
    // closed there.)
    let needs_record = lowered_condition.is_some()
        || roles.values().any(|permits| {
            permits.iter().any(|(capability, whens)| {
                capabilities.contains(capability)
                    && !whens.is_empty()
                    && !whens.iter().any(Option::is_none)
                    && whens.iter().any(Option::is_some)
            })
        });
    if needs_record && !record_available {
        return Err(format!(
            "service op `{op_name}` carries `when` conditions, but its bound \
             entity `{entity}` has no record load (its design surface exposes \
             no `read`, so the query plane has no `find_by_id`) — generation \
             refuses to emit an unguarded route"
        ));
    }
    let mut out = String::new();
    // Without a record the gate is capability membership only: no
    // response parameter to evaluate `when`s over (there are none —
    // `needs_record` refused those above).
    let response_param = if record_available {
        format!("    response: &{response_type_path},\n")
    } else {
        String::new()
    };
    out.push_str(&format!(
        "/// Authorization gate for `{op_name}` (service `{service}`, issue #590).\n\
         ///\n\
         /// Encodes the actor policy at generation time: capability\n\
         /// membership{membership_note}\n\
         /// API-key sessions bypass the policy gates (RLS scopes still\n\
         /// apply) so contract suites and service callers keep working.\n\
         #[allow(clippy::result_large_err)]\n\
         fn service_op_{snake}_authorize(\n    \
             role: &str,\n{response_param}\
         ) -> Result<(), String> {{\n",
        membership_note = if record_available {
            ", `never_both` separation-of-duties conflicts, and\n\
             /// per-grant `when` conditions evaluated over the loaded record."
        } else {
            " and\n\
             /// `never_both` separation-of-duties conflicts (no record load — \
             the bound\n\
             /// entity exposes no `read`, so `when` conditions cannot be \
             evaluated; the\n\
             /// op body is a 501 seam regardless)."
        },
        response_param = response_param,
    ));
    if let (Some((lowered, _)), Some(message)) = (&lowered_condition, &condition_message) {
        out.push_str(&format!(
            "    // op-level `when` transition guard — business semantics apply \
             to every session\n\
             if !({lowered}) {{\n                \
                 return Err({message});\n            }}\n"
        ));
    }
    out.push_str(
        "    if role == \"api_key\" {\n                \
             return Ok(());\n            \
         }\n\
         match role {\n",
    );
    let conflicts = surface
        .map(|policy| policy.sod_conflicts(capabilities))
        .unwrap_or_default();
    for (role, permits) in roles {
        let held: Vec<&String> = capabilities
            .iter()
            .filter(|c| permits.contains_key(*c))
            .collect();
        if held.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "        {role_quoted} => {{\n",
            role_quoted = rust_string_literal(role)
        ));
        let role_conflicts: Vec<&String> = conflicts
            .iter()
            .filter(|c| permits.contains_key(*c))
            .collect();
        if !role_conflicts.is_empty() {
            out.push_str(&format!(
                "            // separation of duties: `never_both` conflicts\n\
                 // with `{}`\n\
                 Err({}.to_string())\n        }}\n",
                role_conflicts
                    .iter()
                    .map(|c| c.as_str())
                    .collect::<Vec<_>>()
                    .join("`, `"),
                rust_string_literal(&format!(
                    "separation of duties: role `{role}` holds `{}`, which is \
                     `never_both` with `{}`",
                    role_conflicts
                        .iter()
                        .map(|c| c.as_str())
                        .collect::<Vec<_>>()
                        .join("`, `"),
                    capabilities.join("`, `")
                ))
            ));
            continue;
        }
        // Conditional permits over the held capabilities; an unconditional
        // permit clears the guard.
        let mut conditions: Vec<String> = Vec::new();
        let mut unconditional = false;
        for capability in &held {
            if let Some(whens) = permits.get(*capability) {
                if whens.iter().any(Option::is_none) {
                    unconditional = true;
                    break;
                }
                for when in whens.iter().flatten() {
                    let lowered = lower_when_to_rust(when, capability, entity, fields)?;
                    conditions.push(lowered);
                }
            }
        }
        if !unconditional && !conditions.is_empty() {
            // Conditions are fully parenthesized by the lowering.
            let joined = conditions.join(" || ");
            let sources = held
                .iter()
                .filter_map(|c| permits.get(*c))
                .flat_map(|whens| whens.iter().flatten())
                .cloned()
                .collect::<Vec<_>>()
                .join("` / `");
            out.push_str(&format!(
                "            // `when` guard{plural} on {}\n\
                 if !({joined}) {{\n                \
                     return Err({}.to_string());\n            }}\n\
                 Ok(())\n        }}\n",
                sources,
                rust_string_literal(&format!(
                    "capability `{}`: `when` condition not met ({sources})",
                    held.iter()
                        .map(|c| c.as_str())
                        .collect::<Vec<_>>()
                        .join("`, `")
                )),
                plural = if conditions.len() == 1 { "" } else { "s" },
            ));
        } else {
            out.push_str("            Ok(())\n        }\n");
        }
    }
    out.push_str(&format!(
        "        _ => Err({}.to_string()),\n    }}\n}}\n",
        rust_string_literal(&format!(
            "capability `{}` required (no policy role holds it)",
            capabilities.join("`, `")
        ))
    ));
    Ok(Some(out))
}

// ── Graph resolution ───────────────────────────────────────────────────

/// Resolve a class name (possibly `pkg::Name`) to a schema title: exact
/// first, then the configured type suffix (the `schema_for_class`
/// convention).
async fn schema_title_for_class(
    db: &dyn GraphQuerier,
    class: &str,
    type_suffix: &str,
) -> Option<String> {
    let unqualified = class.rsplit("::").next().unwrap_or(class).to_string();
    if let Ok(Some(schema)) = db.get_schema(&unqualified).await {
        return Some(schema.title);
    }
    let suffixed = format!("{unqualified}{type_suffix}");
    if let Ok(Some(schema)) = db.get_schema(&suffixed).await {
        return Some(schema.title);
    }
    None
}

/// Resolve the service operations bound to one entity (root entities
/// only — the caller gates on role). Deterministic: models in readback
/// order, module-major, declaration order; ops binding the same entity
/// under one snake name dedup keeping the first.
pub async fn routes_for_entity(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    entity_title: &str,
) -> Result<Vec<ServiceOpRoute>, String> {
    let models = db.get_ddd_models().await.map_err(|e| e.to_string())?;
    if models.iter().all(|m| m.services.is_empty()) {
        return Ok(Vec::new());
    }
    let type_suffix = config.defaults.type_suffix.clone();
    let props = db
        .get_properties(entity_title)
        .await
        .map_err(|e| e.to_string())?;
    let fields = fields_for(&props);
    let surface = PolicySurface::from_graph(db)
        .await
        .map_err(|e| e.to_string())?;
    let Some(schema) = db
        .get_schema(entity_title)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(Vec::new());
    };
    let response_type = format!("{}Response", schema.rust_type_name);
    let module_name = schema.pg_table_name.clone();

    // The design surface gates the execution legs: delegation targets must
    // survive the #449 op-set mapping (delete → commands.delete, findById
    // → queries.find_by_id) to lower.
    let surface_ops = DddDesignSurface::from_graph(db)
        .await
        .ok()
        .and_then(|s| s.operations_for(entity_title))
        .unwrap_or_default();

    let mut routes: Vec<ServiceOpRoute> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                if op.is_protected {
                    continue;
                }
                let Some(bound) = bind_entity(model, op, db, &type_suffix).await else {
                    continue;
                };
                if bound != entity_title || !seen.insert(op.name.clone()) {
                    continue;
                }
                let snake = to_snake_case(&op.name);
                // The route's record load rides the #449 invariant: the
                // entity's query plane carries `find_by_id` iff its design
                // surface exposes `read`. Capability clauses can bind
                // non-root entities (e.g. a PaySlip/`Coverage` noun), which
                // expose no read — those routes render without the record
                // load and with a response-less authorize fn (crewbase#71;
                // #595 follow-up). `when`-conditioned ops on such entities
                // are a generation error (fail closed).
                let record_load = surface_ops.iter().any(|o| o == "read");
                let authorize_fn = render_authorize_fn(
                    &snake,
                    &op.name,
                    &service.name,
                    &op.capabilities,
                    op.condition.as_deref(),
                    &format!(
                        "crate::domain::{domain}::{module_name}::dto_response::{response_type}"
                    ),
                    surface.as_ref(),
                    &fields,
                    entity_title,
                    record_load,
                )?;
                let (is_delete, is_read, seam_reason) = execution_for(op, model, &surface_ops);
                routes.push(ServiceOpRoute {
                    service: service.name.clone(),
                    op_name: op.name.clone(),
                    fn_name: format!("service_op_{snake}"),
                    snake,
                    capabilities: op.capabilities.clone(),
                    authorize_fn,
                    is_delete,
                    is_read,
                    record_load,
                    seam_reason,
                });
            }
        }
    }
    Ok(routes)
}

/// The entity title an operation binds to, if any (resolution order in the
/// module doc).
async fn bind_entity(
    model: &codegraph_core::types::DddModelGraph,
    op: &codegraph_core::types::DddServiceOperation,
    db: &dyn GraphQuerier,
    type_suffix: &str,
) -> Option<String> {
    // 1. Capability class binding.
    if !op.capabilities.is_empty()
        && let Ok(capabilities) = db.get_capabilities().await
    {
        for capability in &op.capabilities {
            if let Some(node) = capabilities.iter().find(|c| &c.name == capability)
                && let Some(title) = schema_title_for_class(db, &node.class, type_suffix).await
            {
                return Some(title);
            }
        }
    }
    // 2. Delegation to a repository of this model.
    if let (Some(target), _) = (&op.delegation_target, &op.delegation_operation)
        && let Some(repo) = model.repositories.iter().find(|r| &r.name == target)
        && let Some(title) = schema_title_for_class(db, &repo.design_class, type_suffix).await
    {
        return Some(title);
    }
    // 3. Signature: first class-typed param, then the return type.
    let class_of = |json: Option<&serde_json::Value>| -> Option<String> {
        let value = json?;
        let name = value.get("name")?.as_str()?;
        (value.get("type")?.as_str()? == "class").then(|| name.to_string())
    };
    for param in &op.params {
        if let Some(class) = class_of(Some(&param.type_json))
            && let Some(title) = schema_title_for_class(db, &class, type_suffix).await
        {
            return Some(title);
        }
    }
    if let Some(class) = class_of(op.return_type.as_ref())
        && let Some(title) = schema_title_for_class(db, &class, type_suffix).await
    {
        return Some(title);
    }
    None
}

/// `(is_delete, is_read, seam_reason)` — the execution lowering. Repository
/// delegations to builtins wire only when the design op set exposes the
/// counterpart (the #449 gating invariant); everything else seams.
fn execution_for(
    op: &codegraph_core::types::DddServiceOperation,
    model: &codegraph_core::types::DddModelGraph,
    surface_ops: &[String],
) -> (bool, bool, Option<String>) {
    let Some(target) = &op.delegation_target else {
        return (
            false,
            false,
            Some(format!(
                "service op `{}` has no delegation target — abstract service \
                 bodies have no executable lowering yet (recorded follow-up)",
                op.name
            )),
        );
    };
    let is_repository = model.repositories.iter().any(|r| &r.name == target);
    if !is_repository {
        return (
            false,
            false,
            Some(format!(
                "service op `{}` delegates to `{target}`, which is not a \
                 repository — service-to-service and search delegation have \
                 no executable lowering yet (recorded follow-up)",
                op.name
            )),
        );
    }
    let operation = op
        .delegation_operation
        .as_deref()
        .map(|o| o.to_ascii_lowercase())
        .unwrap_or_default();
    match operation.as_str() {
        "delete" if surface_ops.iter().any(|o| o == "delete") => (true, false, None),
        "findbyid" if surface_ops.iter().any(|o| o == "read") => (false, true, None),
        other => (
            false,
            false,
            Some(format!(
                "service op `{}` delegates to repository `{other}`, which has \
                 no executable lowering yet (recorded follow-up)",
                op.name
            )),
        ),
    }
}

/// The IFML dead-trigger scan (Ask 1): every `.ifml` action whose name
/// binds (ASCII-case-insensitive, the rexlang `check_action_bindings`
/// convention) a service op that emits no API surface — `protected`, or
/// entity-unbound — is a dead button. Returns `(action, op, reason)`.
pub async fn ifml_dead_triggers(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<(String, String, String)>, GraphError> {
    let mut triggers = Vec::new();
    let Ok(actions) = db.get_ifml_actions().await else {
        return Ok(triggers);
    };
    if actions.is_empty() {
        return Ok(triggers);
    }
    let models = db.get_ddd_models().await?;
    if models.iter().all(|m| m.services.is_empty()) {
        return Ok(triggers);
    }
    let type_suffix = config.defaults.type_suffix.clone();
    for action in &actions {
        let Some((service, op)) = models.iter().find_map(|model| {
            for service in &model.services {
                for op in &service.operations {
                    if op.name.eq_ignore_ascii_case(&action.name) {
                        return Some((service.name.clone(), op.clone()));
                    }
                }
            }
            None
        }) else {
            // Binds no service op at all: the upstream binding check owns
            // that class of defect.
            continue;
        };
        if op.is_protected {
            triggers.push((
                action.name.clone(),
                op.name.clone(),
                "the op is `protected` (Sculptor visibility)".to_string(),
            ));
            continue;
        }
        let bound = bind_entity_service_scoped(&models, &service, &op, db, &type_suffix).await;
        if bound.is_none() {
            triggers.push((
                action.name.clone(),
                op.name.clone(),
                "the op binds no entity (no capability class, repository                  delegation, or class-typed signature)"
                    .to_string(),
            ));
        }
    }
    Ok(triggers)
}

/// [`bind_entity`] scoped to one already-found service.
async fn bind_entity_service_scoped(
    models: &[codegraph_core::types::DddModelGraph],
    service_name: &str,
    op: &codegraph_core::types::DddServiceOperation,
    db: &dyn GraphQuerier,
    type_suffix: &str,
) -> Option<String> {
    let model = models
        .iter()
        .find(|m| m.services.iter().any(|s| s.name == service_name))?;
    bind_entity(model, op, db, type_suffix).await
}

/// The Ask 1 diagnostic: service operations that emit no API surface —
/// `protected` ops and ops that bind no entity. Returned strings are
/// human-readable warnings (empty = clean).
pub async fn dead_surface_warnings(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<String>, GraphError> {
    let models = db.get_ddd_models().await?;
    let mut warnings = Vec::new();
    let type_suffix = config.defaults.type_suffix.clone();
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                if op.is_protected {
                    warnings.push(format!(
                        "service op `{}::{}` is `protected` — it emits no API \
                         surface (Sculptor visibility); IFML actions bound to \
                         it are dead triggers",
                        service.name, op.name
                    ));
                    continue;
                }
                if bind_entity(model, op, db, &type_suffix).await.is_none() {
                    warnings.push(format!(
                        "service op `{}::{}` binds no entity (no capability \
                         class, repository delegation, or class-typed \
                         signature) — it emits no API surface",
                        service.name, op.name
                    ));
                }
            }
        }
    }
    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, FieldKind, bool)]) -> HashMap<String, FieldInfo> {
        let mut map = HashMap::new();
        for (name, kind, optional) in pairs {
            // DTO fields are snake_case; the authored (often camelCase)
            // name keys the lookup, mirroring `fields_for`.
            let rust_name = to_snake_case(name);
            let info = FieldInfo {
                rust_name,
                kind: *kind,
                optional: *optional,
            };
            map.insert(name.to_string(), info);
        }
        map
    }

    fn lower(source: &str, fields: &HashMap<String, FieldInfo>) -> Result<String, String> {
        lower_when_to_rust(source, "ApprovePayRun", "PayRunType", fields)
    }

    #[test]
    fn string_equality_lowering() {
        let f = fields(&[("status", FieldKind::String, false)]);
        assert_eq!(
            lower(r#"status == "AwaitingApproval""#, &f).unwrap(),
            r#"(response.status == "AwaitingApproval")"#
        );
    }

    #[test]
    fn optional_string_equality_lowering() {
        let f = fields(&[("status", FieldKind::String, true)]);
        assert_eq!(
            lower(r#"status != "Posted""#, &f).unwrap(),
            r#"(response.status.as_deref() != Some("Posted"))"#
        );
    }

    #[test]
    fn codelist_and_date_compare_via_display() {
        let f = fields(&[
            ("status", FieldKind::Codelist, false),
            ("filedOn", FieldKind::Date, true),
        ]);
        assert_eq!(
            lower(r#"status == "AwaitingApproval""#, &f).unwrap(),
            r#"(response.status.to_string() == "AwaitingApproval")"#
        );
        assert_eq!(
            lower(r#"filedOn == date("2026-01-15")"#, &f).unwrap(),
            r#"(response.filed_on.as_ref().map(|v| v.to_string()).as_deref() == Some("2026-01-15"))"#
        );
    }

    #[test]
    fn numeric_and_bool_orderings() {
        let f = fields(&[
            ("amount", FieldKind::Float, false),
            ("count", FieldKind::Integer, true),
            ("active", FieldKind::Bool, false),
        ]);
        assert_eq!(
            lower("amount >= 0.0", &f).unwrap(),
            "(response.amount >= 0.0)"
        );
        assert_eq!(
            lower("count < 5", &f).unwrap(),
            "(response.count.is_some_and(|v| v < 5))"
        );
        assert_eq!(
            lower("active == true", &f).unwrap(),
            "(response.active == true)"
        );
    }

    #[test]
    fn logical_and_null_forms() {
        let f = fields(&[
            ("status", FieldKind::String, false),
            ("note", FieldKind::String, true),
            ("amount", FieldKind::Float, false),
        ]);
        assert_eq!(
            lower(
                r#"status == "Draft" && amount > 0.0 && !(note == null)"#,
                &f
            )
            .unwrap(),
            "(((response.status == \"Draft\") && (response.amount > 0.0)) && (!((response.note.is_none()))))"
        );
        assert_eq!(
            lower("note != null", &f).unwrap(),
            "(!(response.note.is_none()))"
        );
        assert_eq!(
            lower("status == null", &f).unwrap(),
            "false" // required field: vacuously false
        );
    }

    #[test]
    fn refusals_name_the_capability() {
        let f = fields(&[("amount", FieldKind::Float, false)]);
        let err = lower("amount + 1.0 > 2.0", &f).unwrap_err();
        assert!(err.contains("capability `ApprovePayRun`"), "{err}");
        assert!(err.contains("arithmetic"), "{err}");
        let err = lower("total == 1.0", &f).unwrap_err();
        assert!(err.contains("is not a field of `PayRunType`"), "{err}");
        let err = lower("customer.name == \"x\"", &f).unwrap_err();
        assert!(err.contains("field access"), "{err}");
        let err = lower("date(\"15/01/2026\") == \"x\"", &f).unwrap_err();
        assert!(err.contains("not a calendar date"), "{err}");
    }

    #[test]
    fn literal_flips_ordering_to_keep_field_on_the_left() {
        let f = fields(&[("amount", FieldKind::Float, false)]);
        assert_eq!(
            lower("0.0 < amount", &f).unwrap(),
            "(response.amount > 0.0)"
        );
    }

    fn policy_fixture() -> PolicySurface {
        let mut roles = BTreeMap::new();
        // owner holds ApprovePayRun when (status == "AwaitingApproval")
        let mut owner = BTreeMap::new();
        owner.insert(
            "ApprovePayRun".to_string(),
            vec![Some(r#"status == "AwaitingApproval""#.to_string())],
        );
        roles.insert("owner".to_string(), owner);
        // manager holds both sides of the never_both pair
        let mut manager = BTreeMap::new();
        manager.insert("ApprovePayRun".to_string(), vec![None]);
        manager.insert("PreparePayRun".to_string(), vec![None]);
        roles.insert("manager".to_string(), manager);
        // member holds nothing relevant
        roles.insert("member".to_string(), BTreeMap::new());
        PolicySurface {
            roles,
            never_both: vec![vec![
                "PreparePayRun".to_string(),
                "ApprovePayRun".to_string(),
            ]],
        }
    }

    #[test]
    fn authorize_fn_encodes_capability_sod_and_when() {
        let f = fields(&[("status", FieldKind::String, false)]);
        let surface = policy_fixture();
        let rendered = render_authorize_fn(
            "approve",
            "approve",
            "PayRunService",
            &["ApprovePayRun".to_string()],
            None,
            "crate::domain::payroll::pay_run::dto_response::PayRunResponse",
            Some(&surface),
            &f,
            "PayRunType",
            true,
        )
        .unwrap()
        .expect("policy surface exists");
        // api_key bypass
        assert!(rendered.contains(r#"if role == "api_key""#), "{rendered}");
        // owner arm carries the when guard over the record
        assert!(
            rendered.contains(r#"if !((response.status == "AwaitingApproval"))"#),
            "{rendered}"
        );
        // manager arm is denied by separation of duties
        assert!(
            rendered.contains("separation of duties: role `manager` holds `PreparePayRun`"),
            "{rendered}"
        );
        // member falls to the deny-all arm
        assert!(
            rendered.contains("capability `ApprovePayRun` required"),
            "{rendered}"
        );
    }

    #[test]
    fn authorize_fn_emits_the_op_level_transition_guard() {
        let f = fields(&[("status", FieldKind::String, false)]);
        let surface = policy_fixture();
        let rendered = render_authorize_fn(
            "approve",
            "approve",
            "PayRunService",
            &["ApprovePayRun".to_string()],
            Some(r#"status == "AwaitingApproval""#),
            "crate::domain::payroll::pay_run::dto_response::PayRunResponse",
            Some(&surface),
            &f,
            "PayRunType",
            true,
        )
        .unwrap()
        .expect("policy surface exists");
        // The transition guard is business semantics: it sits BEFORE the
        // api_key bypass and applies to every session.
        let api_key_pos = rendered.find(r#"if role == "api_key""#).unwrap();
        let cond_pos = rendered
            .find(r#"if !((response.status == "AwaitingApproval"))"#)
            .unwrap();
        assert!(cond_pos < api_key_pos, "{rendered}");
        assert!(rendered.contains("`when` condition not met"), "{rendered}");
    }

    #[test]
    fn condition_only_op_emits_a_guard_without_a_policy() {
        let f = fields(&[("status", FieldKind::String, false)]);
        let rendered = render_authorize_fn(
            "submit",
            "submit",
            "TimeService",
            &[],
            Some(r#"status == "Draft""#),
            "crate::domain::time::timecard::dto_response::TimecardResponse",
            None,
            &f,
            "TimecardType",
            true,
        )
        .unwrap()
        .expect("a condition alone is enough to gate");
        assert!(
            rendered.contains(r#"if !((response.status == "Draft"))"#),
            "{rendered}"
        );
        // No role arms without a policy: everything but api_key denies.
        assert!(rendered.contains("match role"), "{rendered}");
        assert!(
            rendered.contains("capability ` ` required") || rendered.contains("required"),
            "{rendered}"
        );
    }

    #[test]
    fn authorize_fn_none_without_capabilities() {
        let surface = policy_fixture();
        let rendered = render_authorize_fn(
            "prepare",
            "prepare",
            "PayRunService",
            &[],
            None,
            "crate::domain::payroll::pay_run::dto_response::PayRunResponse",
            Some(&surface),
            &HashMap::new(),
            "PayRunType",
            true,
        )
        .unwrap();
        assert!(rendered.is_none());
    }

    // ── Non-root record loads (the #595 follow-up; crewbase#71) ──────────

    #[test]
    fn no_record_load_renders_a_response_less_gate() {
        let f = fields(&[("status", FieldKind::String, false)]);
        let surface = policy_fixture();
        // `manager`'s permit is unconditional (`vec![None]`), so the gate
        // is capability membership + sod only — no record needed.
        let rendered = render_authorize_fn(
            "issue",
            "issue",
            "PaySlipService",
            &["IssuePaySlip".to_string()],
            None,
            "crate::domain::payroll::pay_slip::dto_response::PaySlipResponse",
            Some(&surface),
            &f,
            "PaySlipType",
            false,
        )
        .unwrap()
        .expect("membership gate renders without a record");
        // The response-less signature: no `response` parameter.
        assert!(!rendered.contains("response: &"), "{rendered}");
        // Still carries the deny-all membership arm.
        assert!(
            rendered.contains("capability `IssuePaySlip` required"),
            "{rendered}"
        );
    }

    #[test]
    fn when_condition_without_a_record_is_a_generation_error() {
        let f = fields(&[("status", FieldKind::String, false)]);
        let surface = policy_fixture();
        // owner's permit carries `when (status == ...)` — unrenderable
        // without a record load; fail closed at generation time.
        let err = render_authorize_fn(
            "approve",
            "approve",
            "PaySlipService",
            &["ApprovePayRun".to_string()],
            None,
            "crate::domain::payroll::pay_slip::dto_response::PaySlipResponse",
            Some(&surface),
            &f,
            "PaySlipType",
            false,
        )
        .unwrap_err();
        assert!(err.contains("`when` conditions"), "{err}");
        assert!(err.contains("no record load"), "{err}");
    }

    #[test]
    fn sod_conflicts_are_deterministic_and_deduped() {
        let surface = policy_fixture();
        assert_eq!(
            surface.sod_conflicts(&["ApprovePayRun".to_string()]),
            vec!["PreparePayRun".to_string()]
        );
        assert!(surface.sod_conflicts(&["Unrelated".to_string()]).is_empty());
    }

    #[test]
    fn basejump_role_mapping_mirrors_the_hydration_convention() {
        let actors = vec![
            "PayrollManager".to_string(),
            "Owner".to_string(),
            "Member".to_string(),
        ];
        assert_eq!(
            basejump_role_actors("owner", &actors),
            vec!["Owner".to_string()]
        );
        assert_eq!(
            basejump_role_actors("member", &actors),
            vec!["Member".to_string()]
        );
        // No actor named Manager: the role maps to nobody.
        assert_eq!(
            basejump_role_actors("manager", &actors),
            Vec::<String>::new()
        );
        // Unnamed actors: owner falls to first, member to last.
        let anon = vec!["Auditor".to_string(), "Clerk".to_string()];
        assert_eq!(
            basejump_role_actors("owner", &anon),
            vec!["Auditor".to_string()]
        );
        assert_eq!(
            basejump_role_actors("member", &anon),
            vec!["Clerk".to_string()]
        );
    }

    #[test]
    fn execution_maps_repository_builtins_only() {
        let model = codegraph_core::types::DddModelGraph {
            source_path: "test.ddd".to_string(),
            application: codegraph_core::types::DddApplicationNode {
                name: "App".to_string(),
                base: None,
                source_path: "test.ddd".to_string(),
            },
            modules: Vec::new(),
            designs: Vec::new(),
            repositories: vec![codegraph_core::types::DddRepositoryNode {
                application: "App".to_string(),
                name: "PayRunRepository".to_string(),
                design_class: "PayRun".to_string(),
                operations: Vec::new(),
            }],
            services: Vec::new(),
            searches: Vec::new(),
        };
        let ops = ["delete".to_string(), "read".to_string()];
        let op = |target: Option<&str>, operation: Option<&str>| {
            codegraph_core::types::DddServiceOperation {
                name: "op".to_string(),
                return_type: None,
                return_multiplicity: None,
                params: Vec::new(),
                delegation_target: target.map(|t| t.to_string()),
                delegation_operation: operation.map(|o| o.to_string()),
                capabilities: Vec::new(),
                is_protected: false,
                condition: None,
                emits: Vec::new(),
                invokes: Vec::new(),
                ordinal: 0,
            }
        };
        let (del, read, seam) =
            execution_for(&op(Some("PayRunRepository"), Some("delete")), &model, &ops);
        assert!(del && !read && seam.is_none());
        let (del, read, seam) = execution_for(
            &op(Some("PayRunRepository"), Some("findById")),
            &model,
            &ops,
        );
        assert!(!del && read && seam.is_none());
        // save delegation seams (no payload lowering yet)
        let (_, _, seam) = execution_for(&op(Some("PayRunRepository"), Some("save")), &model, &ops);
        assert!(seam.is_some_and(|s| s.contains("no executable lowering")));
        // non-repository delegation seams
        let (_, _, seam) = execution_for(
            &op(Some("NotificationService"), Some("notify")),
            &model,
            &ops,
        );
        assert!(seam.is_some_and(|s| s.contains("not a repository")));
        // abstract op seams
        let (_, _, seam) = execution_for(&op(None, None), &model, &ops);
        assert!(seam.is_some_and(|s| s.contains("no delegation target")));
    }
}
