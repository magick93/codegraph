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
//! Execution lowering: repository `delete` delegations wire to the
//! entity's command delete, `findById` delegations return the loaded
//! record, and `save`/`update` delegations patch the loaded row through the
//! update command (P0). Where the entity carries a `.ddd` machine, the op
//! binds a transition (by trigger name, op-level state guard, or the
//! protected internal op sharing its capability) and the target state is
//! resolved from the record's current state — a no-transition state change
//! fails closed with 409. Ops with no executable lowering (abstract bodies,
//! service-to-service/search/finder delegation) answer 501 naming the gap.

use std::collections::{BTreeMap, HashMap};

use codegraph_config::DomainConfig;
use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PropertyNode;
use codegraph_naming::to_snake_case;
use heck::ToLowerCamelCase;
use rex_expr::{BinOp, Expr, ExprKind, UnOp};

use crate::ddd::design::DddDesignSurface;
use crate::project_config::DtoKeyCasing;

// ── Route model ────────────────────────────────────────────────────────

/// A machine state name (declaration spelling, e.g. `AwaitingApproval`).
///
/// A newtype keeps state names from being confused with trigger names,
/// wire keys, or arbitrary prose at call sites.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct StateName(String);

impl StateName {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for StateName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A normalized machine trigger name (snake_case, e.g. `submit`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct TriggerName(String);

impl TriggerName {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TriggerName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// An Update-DTO wire key carrying the machine state field.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct WireKey(String);

impl WireKey {
    #[must_use]
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for WireKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One machine transition reduced to the wire-relevant `(from, to)` pair.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ServiceOpTransition {
    pub from: StateName,
    pub to: StateName,
}

/// The machine-field binding an executable save/update delegation carries
/// (P0.5/P0): the state field on the response, its DTO wire key, the
/// machine's initial state, and the transitions this operation may apply.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceOpState {
    /// Rust expression producing the record's current state as a `String`
    /// (codelist/string aware, optional-aware).
    pub current_expr: String,
    /// The Update-DTO key carrying the state field (DTO casing applied).
    pub wire_key: WireKey,
    /// The machine's declared initial state — the hurl contract's starting
    /// point when simulating the action sequence.
    pub initial: StateName,
    /// Every machine transition — the body-driven target whitelist when the
    /// op has no name/guard/capability binding.
    pub all: Vec<ServiceOpTransition>,
    /// The transitions this op applies, selected at runtime by the current
    /// state. Empty = the op performs no state transition of its own.
    pub op: Vec<ServiceOpTransition>,
}

/// The executable lowering for one service op.
///
/// Replaces the former `is_delete`/`is_read`/`is_update` boolean pack (which
/// could represent invalid combinations) with an exhaustive tagged enum.
/// `#[non_exhaustive]` lets new lowerings land without breaking external
/// matches — internal matches must stay exhaustive.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ExecutionKind {
    /// Delegates to the bound repository's `delete` builtin.
    Delete,
    /// Delegates to `findById` and returns the loaded record.
    Read,
    /// Delegates to `save`/`update` and patches the loaded row. `state` is
    /// present when the entity carries a machine (transition binding or
    /// body-driven target whitelist).
    Update {
        #[serde(skip_serializing_if = "Option::is_none")]
        state: Option<ServiceOpState>,
    },
    /// No executable lowering; the route answers 501 naming the gap.
    Seam { reason: String },
}

impl ExecutionKind {
    /// Whether the route answers 501 for an unlowered op.
    #[must_use]
    pub fn is_seam(&self) -> bool {
        matches!(self, Self::Seam { .. })
    }
}

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
    /// The executable lowering (delete / read / update / 501 seam).
    pub execution: ExecutionKind,
    /// `true` when the bound entity's query plane carries `find_by_id`
    /// (the #449 invariant: the design surface exposes `read`). When
    /// `false` — a capability bound to a non-root entity — the route
    /// renders without the record load, and the authorize fn (if any)
    /// takes no response parameter: capability membership only. The
    /// route body still 501s (no record ⇒ no `read`/`delete` lowering),
    /// so nothing flows past the seam.
    pub record_load: bool,
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
#[cfg(test)]
pub(crate) fn lower_when_to_rust(
    source: &str,
    capability: &str,
    entity: &str,
    fields: &HashMap<String, FieldInfo>,
) -> Result<String, String> {
    lower_when_to_rust_with_states(source, capability, entity, fields, &[])
}

/// [`lower_when_to_rust`] with the entity's designed machine states bound as
/// enum-valued bare names (the rexlang `check_condition` convention): a
/// `when (runStatus == AwaitingApproval)` lowers the bare `AwaitingApproval`
/// to the string literal the codelist/string comparison path consumes.
/// Unknown names fail closed with a diagnostic naming both the fields and
/// the declared states.
pub(crate) fn lower_when_to_rust_with_states(
    source: &str,
    capability: &str,
    entity: &str,
    fields: &HashMap<String, FieldInfo>,
    states: &[String],
) -> Result<String, String> {
    let parsed = rex_expr::parse(source);
    match parsed.ast {
        Some(ast) if parsed.errors.is_empty() => lower(&ast, capability, entity, fields, states),
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
    states: &[String],
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
            if let Some(field) = fields.get(name) {
                return Ok(field_ref(field));
            }
            // A bare name that is a declared machine state lowers to the
            // enum value's wire spelling: the codelist/string comparison
            // path then renders `response.<field>.to_string() == "<state>"`.
            if states.iter().any(|state| state == name) {
                return Ok(rust_string_literal(name));
            }
            let states_note = if states.is_empty() {
                String::new()
            } else {
                format!(
                    " nor a declared state of its machine ({})",
                    states.join(", ")
                )
            };
            Err(format!(
                "capability `{capability}`: `when` condition references \
                 `{name}`, which is not a field of `{entity}`{states_note}"
            ))
        }
        ExprKind::Unary { op, expr } => {
            let inner = lower(expr, capability, entity, fields, states)?;
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
                &lower(lhs, capability, entity, fields, states)?,
                &lower(rhs, capability, entity, fields, states)?,
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
#[cfg(test)]
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
    render_authorize_fn_with_states(
        snake,
        op_name,
        service,
        capabilities,
        condition,
        response_type_path,
        surface,
        fields,
        entity,
        record_available,
        &[],
    )
}

/// [`render_authorize_fn`] with the entity's designed machine states bound
/// in `when` lowering (P0.5): `when (runStatus == AwaitingApproval)` treats
/// the bare state name as the enum value, not a record field.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_authorize_fn_with_states(
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
    machine_states: &[String],
) -> Result<Option<String>, String> {
    let lowered_condition = condition
        .map(|source| {
            let lowered =
                lower_when_to_rust_with_states(source, op_name, entity, fields, machine_states)?;
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
                 return Err({message}.to_string());\n            }}\n"
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
                    let lowered = lower_when_to_rust_with_states(
                        when,
                        capability,
                        entity,
                        fields,
                        machine_states,
                    )?;
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
    dto_casing: DtoKeyCasing,
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
    // → queries.find_by_id) to lower. The same surface carries the machine
    // (P0.5): `save`/`update` delegations bind a transition when the op
    // matches one, and the op-level `when` lowering binds declared states.
    let design_surface = DddDesignSurface::from_graph(db).await.ok();
    let surface_ops = design_surface
        .as_ref()
        .and_then(|s| s.operations_for(entity_title))
        .unwrap_or_default();
    let machine = design_surface
        .as_ref()
        .and_then(|s| s.machine_for(entity_title));
    let machine_states: Vec<String> = machine
        .as_ref()
        .map(|m| m.states().into_iter().map(str::to_string).collect())
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
                let authorize_fn = render_authorize_fn_with_states(
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
                    &machine_states,
                )?;
                let execution = execution_for(
                    op,
                    model,
                    &surface_ops,
                    record_load,
                    config,
                    domain,
                    entity_title,
                    &module_name,
                    machine.as_ref(),
                    &fields,
                    dto_casing,
                );
                routes.push(ServiceOpRoute {
                    service: service.name.clone(),
                    op_name: op.name.clone(),
                    fn_name: format!("service_op_{snake}"),
                    snake,
                    capabilities: op.capabilities.clone(),
                    authorize_fn,
                    execution,
                    record_load,
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

/// The machine's transitions as wire `(from, to)` pairs, declaration order.
fn machine_transition_pairs(machine: &rex_ir::ddd::StateMachine) -> Vec<ServiceOpTransition> {
    machine
        .transitions
        .iter()
        .map(|t| ServiceOpTransition {
            from: StateName::new(t.from.clone()),
            to: StateName::new(t.to.clone()),
        })
        .collect()
}

/// The transition's trigger normalized for matching: the authored `via`, or
/// the bare-arrow derivation (the target state's lowerCamel, normalized to
/// snake_case here).
fn transition_trigger(t: &rex_ir::ddd::Transition) -> TriggerName {
    TriggerName::new(to_snake_case(t.trigger.as_deref().unwrap_or(&t.to)))
}

/// Name candidates an operation can match a trigger by: the exact op, the op
/// with the designing class's snake suffix stripped (`submit_pay_run` →
/// `submit`), and the op with the entity table suffix stripped.
fn trigger_candidates(op_name: &str, entity_class: &str, entity_table: &str) -> Vec<TriggerName> {
    let snake = to_snake_case(op_name);
    let mut candidates = vec![TriggerName::new(snake.clone())];
    for suffix in [
        to_snake_case(&short_class_name(entity_class)),
        to_snake_case(entity_table),
    ] {
        if !suffix.is_empty()
            && let Some(stripped) = snake.strip_suffix(&format!("_{suffix}"))
        {
            candidates.push(TriggerName::new(stripped));
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn short_class_name(class: &str) -> String {
    class.rsplit("::").next().unwrap_or(class).to_string()
}

/// The state names an op-level `when` condition pins as its source: equality
/// comparisons between the machine field and a declared state (`runStatus ==
/// Draft`), OR-combined. Used when the op name does not match a trigger (the
/// `preparePayRun`/`Calculate` shape).
fn condition_source_states(
    source: &str,
    state_field_snake: &str,
    states: &[String],
) -> Vec<String> {
    let parsed = rex_expr::parse(source);
    let Some(ast) = parsed.ast.filter(|_| parsed.errors.is_empty()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect_condition_states(&ast, state_field_snake, states, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_condition_states(
    expr: &Expr,
    state_field_snake: &str,
    states: &[String],
    out: &mut Vec<String>,
) {
    match &expr.kind {
        ExprKind::Binary { op, lhs, rhs } => match op {
            BinOp::Eq => {
                let field_state = |a: &Expr, b: &Expr| -> Option<String> {
                    let ExprKind::Name(field) = &a.kind else {
                        return None;
                    };
                    if to_snake_case(field) != state_field_snake {
                        return None;
                    }
                    let ExprKind::Name(state) = &b.kind else {
                        return None;
                    };
                    states.iter().find(|s| *s == state).cloned()
                };
                if let Some(state) = field_state(lhs, rhs).or_else(|| field_state(rhs, lhs)) {
                    out.push(state);
                }
            }
            // Both `&&` and `||` union the source states: the runtime
            // current-state match is the authoritative selector.
            BinOp::And | BinOp::Or => {
                collect_condition_states(lhs, state_field_snake, states, out);
                collect_condition_states(rhs, state_field_snake, states, out);
            }
            _ => {}
        },
        ExprKind::Unary {
            op: UnOp::Not,
            expr,
        } => collect_condition_states(expr, state_field_snake, states, out),
        _ => {}
    }
}

/// The transitions a service op applies, in resolution order:
/// 1. op-name → trigger (exact, then entity-suffix-stripped);
/// 2. op-level `when` state equality → the unique outgoing transition(s);
/// 3. capability fallback: a `protected` internal op (the machine's real
///    trigger carrier) sharing a capability with the public op
///    (`preparePayRun` → `calculate`).
fn op_transitions(
    op: &codegraph_core::types::DddServiceOperation,
    model: &codegraph_core::types::DddModelGraph,
    machine: &rex_ir::ddd::StateMachine,
    entity_class: &str,
    entity_table: &str,
) -> Vec<ServiceOpTransition> {
    let states: Vec<String> = machine.states().into_iter().map(str::to_string).collect();
    let state_field_snake = to_snake_case(&machine.field);

    let by_trigger = |op_name: &str| -> Vec<ServiceOpTransition> {
        let op_candidates = trigger_candidates(op_name, entity_class, entity_table);
        machine
            .transitions
            .iter()
            .filter(|t| op_candidates.contains(&transition_trigger(t)))
            .map(|t| ServiceOpTransition {
                from: StateName::new(t.from.clone()),
                to: StateName::new(t.to.clone()),
            })
            .collect()
    };

    let mut chosen = by_trigger(&op.name);
    if chosen.is_empty()
        && let Some(condition) = op.condition.as_deref()
    {
        let source_states = condition_source_states(condition, &state_field_snake, &states);
        chosen = machine
            .transitions
            .iter()
            .filter(|t| source_states.iter().any(|s| s == &t.from))
            .map(|t| ServiceOpTransition {
                from: StateName::new(t.from.clone()),
                to: StateName::new(t.to.clone()),
            })
            .collect();
    }
    if chosen.is_empty() {
        for other in model.services.iter().flat_map(|s| &s.operations) {
            if !other.is_protected {
                continue;
            }
            let shares_capability = other
                .capabilities
                .iter()
                .any(|c| op.capabilities.iter().any(|own| own == c));
            if !shares_capability {
                continue;
            }
            let matched = by_trigger(&other.name);
            if !matched.is_empty() {
                chosen = matched;
                break;
            }
        }
    }
    // `dedup_by` only removes adjacent duplicates; sort then dedup keeps
    // the runtime match arms stable and unique.
    chosen.sort_by(|a, b| a.from.cmp(&b.from).then(a.to.cmp(&b.to)));
    chosen.dedup_by(|a, b| a.from == b.from && a.to == b.to);
    chosen
}

/// The Rust expression producing the record's current machine state as a
/// `String` (the codelist/string comparison path).
fn current_state_expr(field: &FieldInfo) -> String {
    match (field.kind, field.optional) {
        (FieldKind::Codelist, false) => format!("response.{}.to_string()", field.rust_name),
        (FieldKind::Codelist, true) => format!(
            "response.{}.as_ref().map(|v| v.to_string()).unwrap_or_default()",
            field.rust_name
        ),
        (_, false) => format!("response.{}.clone()", field.rust_name),
        (_, true) => format!("response.{}.clone().unwrap_or_default()", field.rust_name),
    }
}

fn wire_key(rust_field: &str, casing: DtoKeyCasing) -> WireKey {
    WireKey::new(match casing {
        DtoKeyCasing::Snake => rust_field.to_string(),
        DtoKeyCasing::Camel => rust_field.to_lower_camel_case(),
    })
}

/// Whether the machine state field survives into the Update DTO. Workflow
/// status/approval fields and configured `immutable_fields` are excluded;
/// patching them through the update DTO would either 422 (`deny_unknown_
/// fields`) or be ignored, so those ops keep a seam until a dedicated
/// transition command lands.
fn state_field_is_dto_updatable(
    config: &DomainConfig,
    domain: &str,
    entity_title: &str,
    state_field_rust: &str,
) -> bool {
    let Some(ec) = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(entity_title))
    else {
        return true;
    };
    if ec
        .dto
        .immutable_fields
        .iter()
        .any(|f| f == state_field_rust)
    {
        return false;
    }
    if let Some(wf) = &ec.workflow
        && (wf.status_field == state_field_rust
            || wf.approval_status_field.as_deref() == Some(state_field_rust))
    {
        return false;
    }
    true
}

/// The execution lowering. Repository `delete`/`findById` builtins wire to
/// the command/query planes; `save`/`update` wire to the update command with
/// an optional machine transition applied; everything else seams.
fn seam(reason: impl Into<String>) -> ExecutionKind {
    ExecutionKind::Seam {
        reason: reason.into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn execution_for(
    op: &codegraph_core::types::DddServiceOperation,
    model: &codegraph_core::types::DddModelGraph,
    surface_ops: &[String],
    record_load: bool,
    config: &DomainConfig,
    domain: &str,
    entity_title: &str,
    entity_table: &str,
    machine: Option<&rex_ir::ddd::StateMachine>,
    fields: &HashMap<String, FieldInfo>,
    dto_casing: DtoKeyCasing,
) -> ExecutionKind {
    let Some(target) = &op.delegation_target else {
        return seam(format!(
            "service op `{}` has no delegation target — abstract service \
             bodies have no executable lowering yet (recorded follow-up)",
            op.name
        ));
    };
    let Some(target_repo) = model.repositories.iter().find(|r| &r.name == target) else {
        return seam(format!(
            "service op `{}` delegates to `{target}`, which is not a \
             repository — service-to-service and search delegation have \
             no executable lowering yet (recorded follow-up)",
            op.name
        ));
    };

    // Fail closed on cross-entity (child/aggregate) delegation. A public op
    // routing through a repository for a DIFFERENT design class would
    // otherwise patch the parent row and silently drop the child intent
    // (`addTimecardItem`, `punch`, `ingestHours`, assignment/deployment ops).
    // The diagnostic names the op, the target repository, and the target
    // class so the consumer can model a dedicated command.
    let entity_class = model
        .designs
        .iter()
        .find(|d| d.resolved_title.as_deref() == Some(entity_title))
        .map(|d| d.class.as_str());
    if let Some(entity_class) = entity_class
        && target_repo.design_class != entity_class
    {
        return seam(format!(
            "service op `{}` delegates to repository `{target}` for class `{}`, \
             not the route entity `{entity_title}` (`{entity_class}`) — \
             child/aggregate mutation has no executable lowering yet; \
             refusing to patch the parent row silently",
            op.name, target_repo.design_class
        ));
    }

    let operation = op
        .delegation_operation
        .as_deref()
        .map(|o| o.to_ascii_lowercase())
        .unwrap_or_default();
    match operation.as_str() {
        "delete" if surface_ops.iter().any(|o| o == "delete") => ExecutionKind::Delete,
        "findbyid" if surface_ops.iter().any(|o| o == "read") => ExecutionKind::Read,
        "save" | "update" if surface_ops.iter().any(|o| o == "update") => {
            if !record_load {
                return seam(format!(
                    "service op `{}` delegates to repository `update`, which \
                     needs the loaded row, but `{entity_title}` exposes no \
                     read operation (no record load)",
                    op.name
                ));
            }
            let state = match machine {
                Some(machine) => {
                    let field_snake = to_snake_case(&machine.field);
                    let entity_class = entity_class
                        .map(str::to_string)
                        .unwrap_or_else(|| entity_title.to_string());
                    let Some(field) = fields
                        .get(&machine.field)
                        .or_else(|| fields.get(&field_snake))
                    else {
                        // The machine binds a field the graph exposes no
                        // property for — a transition cannot be rendered.
                        let transitions =
                            op_transitions(op, model, machine, &entity_class, entity_table);
                        if transitions.is_empty() {
                            return ExecutionKind::Update { state: None };
                        }
                        return seam(format!(
                            "service op `{}` applies a machine transition, but the \
                             machine field `{}` is not a property of `{entity_title}`",
                            op.name, machine.field
                        ));
                    };
                    let transitions =
                        op_transitions(op, model, machine, &entity_class, entity_table);
                    if !state_field_is_dto_updatable(config, domain, entity_title, &field.rust_name)
                    {
                        // A state-only transition cannot ride the update DTO
                        // (workflow-managed/immutable field). Fail closed
                        // rather than silently no-op the action.
                        if !transitions.is_empty() {
                            return seam(format!(
                                "service op `{}` applies a machine transition on \
                                 `{}`, but the field is workflow-managed/immutable \
                                 and excluded from the update DTO (dedicated \
                                 transition lowering pending)",
                                op.name, field.rust_name
                            ));
                        }
                        None
                    } else {
                        Some(ServiceOpState {
                            current_expr: current_state_expr(field),
                            wire_key: wire_key(&field.rust_name, dto_casing),
                            initial: StateName::new(machine.initial.clone()),
                            all: machine_transition_pairs(machine),
                            op: transitions,
                        })
                    }
                }
                None => None,
            };
            ExecutionKind::Update { state }
        }
        other => seam(format!(
            "service op `{}` delegates to repository `{other}`, which has \
             no executable lowering yet (recorded follow-up)",
            op.name
        )),
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

    fn empty_config() -> DomainConfig {
        toml::from_str(
            r#"
[domains.payroll]
label = "Payroll"
schema_dir = "schemas/payroll"
postgres_schema = "payroll"
"#,
        )
        .unwrap()
    }

    #[test]
    fn execution_maps_repository_builtins_and_save() {
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
        let ops = [
            "delete".to_string(),
            "read".to_string(),
            "update".to_string(),
        ];
        let config = empty_config();
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
        let lower = |op: &codegraph_core::types::DddServiceOperation, machine| {
            execution_for(
                op,
                &model,
                &ops,
                true,
                &config,
                "payroll",
                "PayRunType",
                "pay_run",
                machine,
                &HashMap::new(),
                crate::project_config::DtoKeyCasing::Snake,
            )
        };
        let delete = lower(&op(Some("PayRunRepository"), Some("delete")), None);
        assert!(matches!(delete, ExecutionKind::Delete));
        let read = lower(&op(Some("PayRunRepository"), Some("findById")), None);
        assert!(matches!(read, ExecutionKind::Read));
        // save delegates through the update command (P0)
        match lower(&op(Some("PayRunRepository"), Some("save")), None) {
            ExecutionKind::Update { state } => {
                assert!(state.is_none(), "no machine ⇒ no transition binding")
            }
            other => panic!("expected update lowering, got {other:?}"),
        }
        // non-repository delegation seams
        match lower(&op(Some("NotificationService"), Some("notify")), None) {
            ExecutionKind::Seam { reason } => assert!(reason.contains("not a repository")),
            other => panic!("expected seam, got {other:?}"),
        }
        // abstract op seams
        match lower(&op(None, None), None) {
            ExecutionKind::Seam { reason } => assert!(reason.contains("no delegation target")),
            other => panic!("expected seam, got {other:?}"),
        }
    }

    fn pay_run_machine() -> rex_ir::ddd::StateMachine {
        let transition = |from: &str, to: &str, trigger: &str| rex_ir::ddd::Transition {
            from: from.to_string(),
            to: to.to_string(),
            trigger: Some(trigger.to_string()),
            guard: None,
        };
        rex_ir::ddd::StateMachine {
            field: "runStatus".to_string(),
            enum_type: rex_ir::TypeRef::Enum {
                package: "payroll".to_string(),
                name: "PayRunStatus".to_string(),
            },
            initial: "Draft".to_string(),
            terminals: vec!["Reversed".to_string()],
            transitions: vec![
                transition("Draft", "Calculating", "Calculate"),
                transition("Calculating", "AwaitingApproval", "Submit"),
                transition("AwaitingApproval", "Approved", "Approve"),
                transition("Approved", "Posted", "Post"),
                transition("Posted", "Reversed", "Reverse"),
            ],
        }
    }

    fn pay_run_fields() -> HashMap<String, FieldInfo> {
        let mut fields = HashMap::new();
        fields.insert(
            "runStatus".to_string(),
            FieldInfo {
                rust_name: "run_status".to_string(),
                kind: FieldKind::Codelist,
                optional: false,
            },
        );
        fields.insert(
            "run_status".to_string(),
            FieldInfo {
                rust_name: "run_status".to_string(),
                kind: FieldKind::Codelist,
                optional: false,
            },
        );
        fields
    }

    fn service_op(
        name: &str,
        capabilities: &[&str],
        is_protected: bool,
        condition: Option<&str>,
    ) -> codegraph_core::types::DddServiceOperation {
        codegraph_core::types::DddServiceOperation {
            name: name.to_string(),
            return_type: None,
            return_multiplicity: None,
            params: Vec::new(),
            delegation_target: Some("PayRunRepository".to_string()),
            delegation_operation: Some("save".to_string()),
            capabilities: capabilities.iter().map(|c| c.to_string()).collect(),
            is_protected,
            condition: condition.map(str::to_string),
            emits: Vec::new(),
            invokes: Vec::new(),
            ordinal: 0,
        }
    }

    fn pay_run_model(
        ops: Vec<codegraph_core::types::DddServiceOperation>,
    ) -> codegraph_core::types::DddModelGraph {
        codegraph_core::types::DddModelGraph {
            source_path: "test.ddd".to_string(),
            application: codegraph_core::types::DddApplicationNode {
                name: "App".to_string(),
                base: None,
                source_path: "test.ddd".to_string(),
            },
            modules: Vec::new(),
            designs: vec![codegraph_core::types::DddDesignNode {
                application: "App".to_string(),
                module: "payroll".to_string(),
                class: "PayRun".to_string(),
                resolved_title: Some("PayRunType".to_string()),
                stereotype: "entity".to_string(),
                is_abstract: false,
                flags: codegraph_core::types::DddDesignFlags::default(),
                machine_json: None,
                ordinal: 0,
            }],
            repositories: vec![codegraph_core::types::DddRepositoryNode {
                application: "App".to_string(),
                name: "PayRunRepository".to_string(),
                design_class: "PayRun".to_string(),
                operations: Vec::new(),
            }],
            services: vec![codegraph_core::types::DddServiceNode {
                application: "App".to_string(),
                module: "payroll".to_string(),
                name: "PayRunService".to_string(),
                description: None,
                dependencies: Vec::new(),
                decision_dependencies: Vec::new(),
                operations: ops,
                ordinal: 0,
            }],
            searches: Vec::new(),
        }
    }

    #[test]
    fn save_delegation_binds_the_op_trigger_transition() {
        let machine = pay_run_machine();
        let model = pay_run_model(vec![service_op(
            "submitPayRun",
            &["SubmitPayRun"],
            false,
            None,
        )]);
        let config = empty_config();
        let ops = ["delete", "read", "update"].map(str::to_string);
        let lowering = execution_for(
            &model.services[0].operations[0],
            &model,
            &ops,
            true,
            &config,
            "payroll",
            "PayRunType",
            "pay_run",
            Some(&machine),
            &pay_run_fields(),
            crate::project_config::DtoKeyCasing::Snake,
        );
        let state = match lowering {
            ExecutionKind::Update { state: Some(state) } => state,
            other => panic!("expected machine-bound update, got {other:?}"),
        };
        assert_eq!(state.wire_key.as_str(), "run_status");
        assert_eq!(state.current_expr, "response.run_status.to_string()");
        assert_eq!(state.initial.as_str(), "Draft");
        assert_eq!(
            state.op,
            vec![ServiceOpTransition {
                from: StateName::new("Calculating"),
                to: StateName::new("AwaitingApproval"),
            }]
        );
    }

    #[test]
    fn prepare_pay_run_binds_the_protected_internal_trigger() {
        // `preparePayRun` has no matching trigger; the protected `calculate`
        // shares its capability and matches `Calculate` (the crewbase shape).
        let machine = pay_run_machine();
        let model = pay_run_model(vec![
            service_op("preparePayRun", &["PreparePayRun"], false, None),
            service_op("calculate", &["PreparePayRun"], true, None),
        ]);
        let config = empty_config();
        let ops = ["delete", "read", "update"].map(str::to_string);
        let lowering = execution_for(
            &model.services[0].operations[0],
            &model,
            &ops,
            true,
            &config,
            "payroll",
            "PayRunType",
            "pay_run",
            Some(&machine),
            &pay_run_fields(),
            crate::project_config::DtoKeyCasing::Snake,
        );
        let state = match lowering {
            ExecutionKind::Update { state: Some(state) } => state,
            other => panic!("expected machine-bound update, got {other:?}"),
        };
        assert_eq!(
            state.op,
            vec![ServiceOpTransition {
                from: StateName::new("Draft"),
                to: StateName::new("Calculating"),
            }]
        );
    }

    #[test]
    fn machine_states_lower_in_when_conditions() {
        let fields = pay_run_fields();
        let states = vec![
            "Draft".to_string(),
            "AwaitingApproval".to_string(),
            "Approved".to_string(),
        ];
        assert_eq!(
            lower_when_to_rust_with_states(
                "runStatus == AwaitingApproval",
                "ApprovePayRun",
                "PayRunType",
                &fields,
                &states,
            )
            .unwrap(),
            r#"(response.run_status.to_string() == "AwaitingApproval")"#
        );
        let err = lower_when_to_rust_with_states(
            "runStatus == Shelved",
            "ApprovePayRun",
            "PayRunType",
            &fields,
            &states,
        )
        .unwrap_err();
        assert!(err.contains("nor a declared state"), "{err}");
        assert!(err.contains("Draft"), "{err}");
    }

    #[test]
    fn workflow_managed_state_field_keeps_the_update_but_seams_transitions() {
        let machine = pay_run_machine();
        let config = empty_config();
        let mut config = config;
        let entry = config.domains.get_mut("payroll").unwrap();
        entry.entity_config.insert(
            "PayRunType".to_string(),
            toml::from_str(
                r#"
workflow = { status_field = "run_status", initial_state = "Draft" }
"#,
            )
            .unwrap(),
        );
        let ops = ["read", "update"].map(str::to_string);
        // A non-transition delegation still updates the other body fields.
        let model = pay_run_model(vec![service_op("update", &[], false, None)]);
        let lowering = execution_for(
            &model.services[0].operations[0],
            &model,
            &ops,
            true,
            &config,
            "payroll",
            "PayRunType",
            "pay_run",
            Some(&machine),
            &pay_run_fields(),
            crate::project_config::DtoKeyCasing::Snake,
        );
        match lowering {
            ExecutionKind::Update { state } => assert!(
                state.is_none(),
                "workflow-managed state is not DTO-patchable"
            ),
            other => panic!("expected a plain update, got {other:?}"),
        }

        // A state transition cannot ride the update DTO — fail closed.
        let model = pay_run_model(vec![service_op("submitPayRun", &[], false, None)]);
        let lowering = execution_for(
            &model.services[0].operations[0],
            &model,
            &ops,
            true,
            &config,
            "payroll",
            "PayRunType",
            "pay_run",
            Some(&machine),
            &pay_run_fields(),
            crate::project_config::DtoKeyCasing::Snake,
        );
        match lowering {
            ExecutionKind::Seam { reason } => {
                assert!(reason.contains("workflow-managed"), "{reason}")
            }
            other => panic!("expected a fail-closed seam, got {other:?}"),
        }
    }

    fn lower_with_machine(
        op_name: &str,
        capabilities: &[&str],
        machine: &rex_ir::ddd::StateMachine,
        protected: Option<(&str, &[&str])>,
    ) -> ExecutionKind {
        let mut ops = vec![service_op(op_name, capabilities, false, None)];
        if let Some((name, caps)) = protected {
            ops.push(service_op(name, caps, true, None));
        }
        let model = pay_run_model(ops);
        let ops_surface = ["delete", "read", "update"].map(str::to_string);
        execution_for(
            &model.services[0].operations[0],
            &model,
            &ops_surface,
            true,
            &empty_config(),
            "payroll",
            "PayRunType",
            "pay_run",
            Some(machine),
            &pay_run_fields(),
            crate::project_config::DtoKeyCasing::Snake,
        )
    }

    #[test]
    fn self_transition_is_a_declared_noop_edge() {
        let mut machine = pay_run_machine();
        machine.transitions.push(rex_ir::ddd::Transition {
            from: "Draft".to_string(),
            to: "Draft".to_string(),
            trigger: Some("Refresh".to_string()),
            guard: None,
        });
        let lowering = lower_with_machine("refresh", &[], &machine, None);
        let state = match lowering {
            ExecutionKind::Update { state: Some(state) } => state,
            other => panic!("expected a machine-bound update, got {other:?}"),
        };
        assert_eq!(
            state.op,
            vec![ServiceOpTransition {
                from: StateName::new("Draft"),
                to: StateName::new("Draft"),
            }],
            "a self transition is legal and does not advance the state"
        );
    }

    #[test]
    fn body_driven_op_whitelists_every_transition_but_selects_none() {
        // `changeStatus` matches no trigger: the op has no transition of its
        // own, so the handler validates a body-supplied target against `all`
        // and an empty body is an idempotent read.
        let machine = pay_run_machine();
        let lowering = lower_with_machine("changeStatus", &[], &machine, None);
        let state = match lowering {
            ExecutionKind::Update { state: Some(state) } => state,
            other => panic!("expected a body-driven update, got {other:?}"),
        };
        assert!(state.op.is_empty(), "no transition is selected by name");
        assert_eq!(state.all.len(), machine.transitions.len());
        assert_eq!(state.initial.as_str(), "Draft");
    }

    #[test]
    fn capability_shared_op_without_a_trigger_stays_a_plain_update() {
        // Capability sharing is an explicit last-resort heuristic: the
        // protected op carries the trigger only when its NAME matches one.
        let machine = pay_run_machine();
        let lowering = lower_with_machine(
            "preparePayRun",
            &["PreparePayRun"],
            &machine,
            Some(("recalculate", &["PreparePayRun"])),
        );
        let state = match lowering {
            ExecutionKind::Update { state: Some(state) } => state,
            other => panic!("expected a plain body-driven update, got {other:?}"),
        };
        assert!(
            state.op.is_empty(),
            "no trigger match anywhere ⇒ no transition is invented"
        );
    }

    #[test]
    fn cross_entity_child_delegation_fails_closed() {
        let machine = pay_run_machine();
        let mut model = pay_run_model(vec![service_op("addPayRunItem", &[], false, None)]);
        // The op routes through a repository for a DIFFERENT design class —
        // the child/aggregate shape. Patching the parent row would silently
        // drop the child intent.
        model.repositories[0].design_class = "PayRunItem".to_string();
        let ops_surface = ["delete", "read", "update"].map(str::to_string);
        let lowering = execution_for(
            &model.services[0].operations[0],
            &model,
            &ops_surface,
            true,
            &empty_config(),
            "payroll",
            "PayRunType",
            "pay_run",
            Some(&machine),
            &pay_run_fields(),
            crate::project_config::DtoKeyCasing::Snake,
        );
        match lowering {
            ExecutionKind::Seam { reason } => {
                assert!(reason.contains("child/aggregate"), "{reason}");
                assert!(reason.contains("PayRunItem"), "{reason}");
            }
            other => panic!("expected a fail-closed child seam, got {other:?}"),
        }
    }

    #[test]
    fn no_record_load_route_fails_closed() {
        let machine = pay_run_machine();
        let model = pay_run_model(vec![service_op("submitPayRun", &[], false, None)]);
        let ops_surface = ["delete", "update"].map(str::to_string); // no read
        let lowering = execution_for(
            &model.services[0].operations[0],
            &model,
            &ops_surface,
            false,
            &empty_config(),
            "payroll",
            "PayRunType",
            "pay_run",
            Some(&machine),
            &pay_run_fields(),
            crate::project_config::DtoKeyCasing::Snake,
        );
        match lowering {
            ExecutionKind::Seam { reason } => {
                assert!(reason.contains("no record load"), "{reason}")
            }
            other => panic!("expected a no-record-load seam, got {other:?}"),
        }
    }
}
