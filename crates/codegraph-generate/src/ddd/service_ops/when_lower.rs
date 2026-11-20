use std::collections::HashMap;

use codegraph_naming::to_snake_case;
use phero_expr::{BinOp, Expr, ExprKind, UnOp};

use super::types::{FieldInfo, FieldKind};

// ── Rust lowering of `when` conditions ──────────────────────────────────

/// The refusal wording of one lowering context. Every message the lowering
/// emits is assembled from these six pieces, so the #590 `when` wording and
/// the #666 invariant wording share ONE message set and can never drift
/// apart in shape — only in the named pieces.
pub(crate) struct LowerWording<'w> {
    /// The full refusal prefix, without the trailing colon-space:
    /// `` capability `ApprovePayRun` `` /
    /// `` invariant 0 of `GrantApplicationType` in domain `grants` ``.
    pub subject: &'w str,
    /// The condition noun (`` `when` condition `` / "invariant expression").
    pub noun: &'w str,
    /// The tail of the deep-chain refusal (after "which cannot be lowered — ").
    pub chain_tail: &'w str,
    /// The tail of the non-own-record field-access refusal (after "which is
    /// not an own-record field; ").
    pub own_record_tail: &'w str,
    /// The Rust construct the lowering targets ("Rust guard" / "Rust
    /// predicate").
    pub target: &'w str,
    /// The supported-subset sentence tail of the general refusal.
    pub subset: &'w str,
}

impl<'w> LowerWording<'w> {
    /// The service-op `when` wording. Every existing refusal text is byte
    /// for byte the pre-refactor string — the `ddd::service_ops` tests and
    /// the `service_ops_tests` pins hold the exact texts.
    fn when(subject: &'w str) -> Self {
        Self {
            subject,
            noun: "`when` condition",
            chain_tail: "a dotted chain under an injected repository is one \
                         segment deep at most",
            own_record_tail: "only bare field references and one segment under \
                              an injected repository are supported",
            target: "Rust guard",
            subset: "own-record field refs, one segment under an injected \
                     repository, string/number/boolean/date literals, \
                     comparisons, && || !, parentheses, and null comparisons",
        }
    }

    /// The DD5 invariant wording (issue #666 slice 2): invariants read the
    /// record they guard — no injected scopes — so the tails name the
    /// own-record subset alone.
    fn invariant(subject: &'w str) -> Self {
        Self {
            subject,
            noun: "invariant expression",
            chain_tail: "navigation beyond the record's own scalar features \
                         cannot be lowered",
            own_record_tail: "only bare own-record field references are supported",
            target: "Rust predicate",
            subset: "own-record scalar field refs, string/number/boolean/date \
                     literals, comparisons, && || !, parentheses, and null \
                     comparisons",
        }
    }
}

/// Lower a pherolang `when` condition source to a Rust boolean expression
/// over `response` (the loaded `{Entity}Response` record).
///
/// The documented closed subset mirrors `db::expr_sql` (the RLS lowering):
/// bare own-record field refs, string/number/boolean/date literals,
/// comparisons, `&& || !`, parentheses, and null comparisons. Codelist
/// fields compare via their `Display` (the codelist code); date fields
/// compare via their ISO `Display`. EVERYTHING else is refused — what
/// cannot be lowered faithfully is a hard generation error naming the
/// capability (the pherolang Cedar-backend philosophy).
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
/// enum-valued bare names (the pherolang `check_condition` convention): a
/// `when (runStatus == AwaitingApproval)` lowers the bare `AwaitingApproval`
/// to the string literal the codelist/string comparison path consumes.
/// Unknown names fail closed with a diagnostic naming both the fields and
/// the declared states.
#[cfg(test)]
pub(crate) fn lower_when_to_rust_with_states(
    source: &str,
    capability: &str,
    entity: &str,
    fields: &HashMap<String, FieldInfo>,
    states: &[String],
) -> Result<String, String> {
    lower_when_scoped(source, capability, entity, fields, states, &[])
}

/// [`lower_when_to_rust_with_states`] over a scope table that may carry
/// injected repositories (issue #663): an injected path
/// (`TimecardRepository.timecardStatus`) lowers over the loaded related
/// record the rendered gate binds. The closed subset is unchanged — a
/// dotted chain deeper than one segment under an injected root, or a root
/// that names no injected scope, is refused with the construct named.
pub(crate) fn lower_when_scoped(
    source: &str,
    capability: &str,
    entity: &str,
    fields: &HashMap<String, FieldInfo>,
    states: &[String],
    injected: &[InjectedScope],
) -> Result<String, String> {
    let subject = format!("capability `{capability}`");
    parse_and_lower(
        source,
        &LowerWording::when(&subject),
        entity,
        fields,
        states,
        injected,
        "response",
    )
}

/// Lower one DD5 design invariant (issue #666 slice 2) over `record` — the
/// predicate fn's parameter, `input` — through the same own-record subset
/// the `when` guards use, with the design's machine states bound as bare
/// names. No injected scopes: an invariant reads the record it guards, so
/// dotted navigation (a cross-aggregate read) is the own-record refusal.
///
/// The upstream phero-driver pre-types the expression as boolean over the
/// designed class; the boolean-shape check here re-proves it on the
/// re-parsed AST so a stale/hand-built graph refuses instead of emitting a
/// non-boolean predicate body.
pub(crate) fn lower_invariant(
    source: &str,
    subject: &str,
    entity: &str,
    record: &str,
    fields: &HashMap<String, FieldInfo>,
    states: &[String],
) -> Result<String, String> {
    let parsed = phero_expr::parse(source);
    let Some(ast) = parsed.ast.filter(|_| parsed.errors.is_empty()) else {
        return Err(format!(
            "{subject}: invariant expression is not valid pherolang \
             expression syntax: {source:?}"
        ));
    };
    // Lower FIRST — a construct outside the closed subset names itself
    // (`a lambda`, `if`, `let`, an operation call); the boolean-shape
    // proof then catches the lowerable-but-non-predicate roots (a bare
    // field, a literal), re-proving the upstream phero-driver typing for
    // a stale/hand-built graph.
    let lowered = parse_lowered(
        &ast,
        &LowerWording::invariant(subject),
        entity,
        fields,
        states,
        &[],
        record,
    )?;
    if !is_boolean_shaped(&ast) {
        return Err(format!(
            "{subject}: invariant expression must lower to a boolean \
             predicate, but `{source}` has a non-boolean root; an invariant \
             is a predicate over the record"
        ));
    }
    Ok(lowered)
}

/// Whether the expression's root is boolean-shaped — the shape proof behind
/// [`lower_invariant`]. Comparisons, the logical operators, `!`, and boolean
/// literals are predicates; everything else (a bare field, a literal) is
/// not. Arithmetic roots cannot reach here (they are refused first).
fn is_boolean_shaped(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Bool(_) => true,
        ExprKind::Unary { op: UnOp::Not, .. } => true,
        ExprKind::Binary { op, .. } => {
            !matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div)
        }
        _ => false,
    }
}

/// Parse `source` and lower the AST, or refuse naming the syntax failure.
fn parse_and_lower(
    source: &str,
    wording: &LowerWording<'_>,
    entity: &str,
    fields: &HashMap<String, FieldInfo>,
    states: &[String],
    injected: &[InjectedScope],
    record: &str,
) -> Result<String, String> {
    let parsed = phero_expr::parse(source);
    match parsed.ast {
        Some(ast) if parsed.errors.is_empty() => {
            parse_lowered(&ast, wording, entity, fields, states, injected, record)
        }
        _ => Err(format!(
            "{subject}: {noun} is not valid pherolang expression syntax: \
             {source:?}",
            subject = wording.subject,
            noun = wording.noun,
        )),
    }
}

/// One injected scope a `when` lowering may reference (issue #663),
/// resolved at generation time from the op's `condition_paths`.
#[derive(Debug, Clone)]
pub(crate) struct InjectedScope {
    /// The injection root exactly as authored — the declared dependency
    /// name (`TimecardRepository`), which is the condition path's first
    /// segment.
    pub root: String,
    /// The Rust local the rendered gate binds (`to_snake_case(root)`).
    pub local: String,
    /// The AppState query slot prefix (`{domain}_{module}`) of the injected
    /// entity's query plane.
    pub slot: String,
    /// The injected plane's auditable flag — `find_by_id`'s
    /// include-deleted arity.
    pub auditable: bool,
    /// The own-record FK field carrying the injected id (the response's
    /// Rust field name, e.g. `timecard_id`).
    pub fk_field: String,
    /// Whether the FK is `Option` (an optional `refers`).
    pub fk_optional: bool,
    /// The injected class's fields — the comparison domain over the loaded
    /// record.
    pub fields: HashMap<String, FieldInfo>,
    /// The injected class's machine when the class carries one — the hurl
    /// planner's fresh-fixture reasoning and (via its states) the guard's
    /// enum-literal namespace.
    pub machine: Option<InjectedMachine>,
}

/// One injected class's designed machine (issue #663).
#[derive(Debug, Clone)]
pub struct InjectedMachine {
    /// The machine's field as authored (`timecardStatus`).
    pub field: String,
    /// The machine's initial state (`Pending`) — a fresh fixture row sits
    /// here.
    pub initial: String,
    /// Every declared state — the enum literals the guard may compare
    /// against (the module's enum namespace is global upstream).
    pub states: Vec<String>,
}

/// Resolve one operand to its scope root and field: `None` for a
/// non-field operand (literal), the own record (`record`) for a bare
/// entity field, the scope's local for a one-segment injected path.
fn resolve_operand(
    wording: &LowerWording<'_>,
    scopes: &(&str, &HashMap<String, FieldInfo>, &[InjectedScope]),
    expr: &Expr,
    record: &str,
) -> Result<Option<(String, FieldInfo)>, String> {
    let (_entity, fields, injected) = scopes;
    match &expr.kind {
        ExprKind::Name(name) => match fields.get(name) {
            Some(field) => Ok(Some((record.to_string(), field.clone()))),
            None => {
                // A bare name that is not a field is a literal operand (a
                // declared machine state, most often) — `lower` already
                // validated or lowered it; the comparison rendering treats
                // it as the literal side, exactly as the pre-#663
                // `operand_field` did.
                Ok(None)
            }
        },
        ExprKind::FeatureAccess {
            receiver,
            name,
            optional_safe,
        } => {
            if *optional_safe {
                return Err(format!(
                    "{subject}: {noun} uses `?.` (safe navigation), which \
                     cannot be lowered",
                    subject = wording.subject,
                    noun = wording.noun,
                ));
            }
            let ExprKind::Name(root) = &receiver.kind else {
                return Err(format!(
                    "{subject}: {noun} uses field access `{}`, which cannot \
                     be lowered — {chain_tail}",
                    write_path(expr),
                    subject = wording.subject,
                    noun = wording.noun,
                    chain_tail = wording.chain_tail,
                ));
            };
            let Some(scope) = injected.iter().find(|scope| &scope.root == root) else {
                return Err(format!(
                    "{subject}: {noun} uses field access `{root}`.{}, which \
                     is not an own-record field; {own_record_tail}",
                    name.value,
                    subject = wording.subject,
                    noun = wording.noun,
                    own_record_tail = wording.own_record_tail,
                ));
            };
            match scope
                .fields
                .get(&name.value)
                .or_else(|| scope.fields.get(&to_snake_case(&name.value)))
            {
                Some(field) => Ok(Some((scope.local.clone(), field.clone()))),
                None => Err(format!(
                    "{subject}: {noun} references `{root}`.{}, which is not \
                     a field of the injected repository's class",
                    name.value,
                    subject = wording.subject,
                    noun = wording.noun,
                )),
            }
        }
        _ => Ok(None),
    }
}

fn parse_lowered(
    expr: &Expr,
    wording: &LowerWording<'_>,
    entity: &str,
    fields: &HashMap<String, FieldInfo>,
    states: &[String],
    injected: &[InjectedScope],
    record: &str,
) -> Result<String, String> {
    let scopes = (entity, fields, injected);
    lower(expr, wording, &scopes, states, record)
}

fn lower(
    expr: &Expr,
    wording: &LowerWording<'_>,
    scopes: &(&str, &HashMap<String, FieldInfo>, &[InjectedScope]),
    states: &[String],
    record: &str,
) -> Result<String, String> {
    let (entity, fields, _injected) = scopes;
    let reject = |construct: &str| {
        Err(format!(
            "{subject}: {noun} uses {construct}, which cannot be lowered to \
             a {target} on `{entity}`; the supported subset is {subset}",
            subject = wording.subject,
            noun = wording.noun,
            target = wording.target,
            entity = entity,
            subset = wording.subset,
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
                    "{subject}: {noun} uses the date literal {text:?}, which \
                     is not a calendar date `YYYY-MM-DD`",
                    subject = wording.subject,
                    noun = wording.noun,
                ))
            }
        }
        ExprKind::Name(name) => {
            if let Some(field) = fields.get(name) {
                return Ok(field_ref(record, field));
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
                "{subject}: {noun} references `{name}`, which is not a field \
                 of `{entity}`{states_note}",
                subject = wording.subject,
                noun = wording.noun,
            ))
        }
        ExprKind::Unary { op, expr } => {
            let inner = lower(expr, wording, scopes, states, record)?;
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
                // The non-null operand must be a field ref for the
                // Option check to be honest; a required field's null
                // comparison is vacuous and lowers to a literal.
                let (root, field) = resolve_operand(wording, scopes, field_expr_expr, record)?
                    .ok_or_else(|| {
                        format!(
                            "{subject}: {noun} uses a null comparison against \
                             a non-field",
                            subject = wording.subject,
                            noun = wording.noun,
                        )
                    })?;
                return Ok(match (op, field.optional) {
                    (BinOp::Eq, true) => format!("({root}.{}.is_none())", field.rust_name),
                    (BinOp::Ne, true) => {
                        format!("(!({root}.{}.is_none()))", field.rust_name)
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
                &lower(lhs, wording, scopes, states, record)?,
                &lower(rhs, wording, scopes, states, record)?,
                rust_op,
                wording,
                scopes,
                lhs,
                rhs,
                record,
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
            // A dotted path lowers ONLY through an injected scope; every
            // other receiver is the own-record dotted-navigation refusal.
            match resolve_operand(wording, scopes, expr, record) {
                Ok(Some((root, field))) => Ok(field_ref(&root, &field)),
                Ok(None) => reject(&format!(
                    "field access on a non-field receiver (`{}`.{})",
                    write_path(receiver),
                    name.value
                )),
                Err(message) => Err(message),
            }
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
/// decision the model does not make). Cross-scope comparisons (an own
/// field against an injected field) are refused: no root is honest for
/// both sides.
#[allow(clippy::too_many_arguments)]
fn comparison(
    op: &BinOp,
    lhs_text: &str,
    rhs_text: &str,
    rust_op: &str,
    wording: &LowerWording<'_>,
    scopes: &(&str, &HashMap<String, FieldInfo>, &[InjectedScope]),
    lhs: &Expr,
    rhs: &Expr,
    record: &str,
) -> Result<String, String> {
    let is_ordering = matches!(op, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge);
    let lhs_field = resolve_operand(wording, scopes, lhs, record)?;
    let rhs_field = resolve_operand(wording, scopes, rhs, record)?;
    match (lhs_field, rhs_field) {
        (Some((root, field)), None) => {
            let literal = strip_parens(rhs_text);
            field_vs_literal(
                &root,
                &field,
                &literal,
                rust_op,
                is_ordering,
                wording,
                scopes.0,
            )
        }
        (None, Some((root, field))) => {
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
            field_vs_literal(
                &root,
                &field,
                &literal,
                flipped,
                is_ordering,
                wording,
                scopes.0,
            )
        }
        (Some((lhs_root, a)), Some((rhs_root, b))) => {
            if lhs_root != rhs_root {
                return Err(format!(
                    "{subject}: comparison between `{}` and `{}` across \
                     different records (an own-record field vs an injected \
                     field, or two injected scopes) cannot be lowered \
                     faithfully — compare each side against a literal instead",
                    a.rust_name,
                    b.rust_name,
                    subject = wording.subject,
                ));
            }
            if is_ordering && (a.optional || b.optional || a.kind != b.kind) {
                return Err(format!(
                    "{subject}: ordering comparison between fields `{}` and \
                     `{}` cannot be lowered faithfully",
                    a.rust_name,
                    b.rust_name,
                    subject = wording.subject,
                ));
            }
            if a.optional != b.optional || a.kind != b.kind {
                return Err(format!(
                    "{subject}: field-to-field comparison between `{}` and \
                     `{}` mixes requiredness or types",
                    a.rust_name,
                    b.rust_name,
                    subject = wording.subject,
                ));
            }
            Ok(format!(
                "({lhs_root}.{} {rust_op} {rhs_root}.{})",
                a.rust_name, b.rust_name
            ))
        }
        (None, None) => Ok(format!("({lhs_text} {rust_op} {rhs_text})")),
    }
}

fn field_vs_literal(
    root: &str,
    field: &FieldInfo,
    literal: &str,
    rust_op: &str,
    is_ordering: bool,
    wording: &LowerWording<'_>,
    entity: &str,
) -> Result<String, String> {
    let name = &field.rust_name;
    let root = root.to_string();
    match field.kind {
        FieldKind::String => match field.optional {
            false => Ok(format!("({root}.{name} {rust_op} {literal})")),
            true if is_ordering => Ok(format!(
                "({root}.{name}.as_deref().is_some_and(|v| v {rust_op} {literal}))"
            )),
            true => Ok(format!(
                "({root}.{name}.as_deref() {} Some({literal}))",
                eq_op(rust_op)
            )),
        },
        FieldKind::Bool => {
            if is_ordering {
                return Err(format!(
                    "{subject}: ordering comparison on bool field `{name}` of \
                     `{entity}`",
                    subject = wording.subject,
                ));
            }
            match field.optional {
                false => Ok(format!("({root}.{name} {rust_op} {literal})")),
                true => Ok(format!(
                    "({root}.{name} {} Some({literal}))",
                    eq_op(rust_op)
                )),
            }
        }
        FieldKind::Integer | FieldKind::Float => match field.optional {
            false => Ok(format!("({root}.{name} {rust_op} {literal})")),
            true if is_ordering => Ok(format!(
                "({root}.{name}.is_some_and(|v| v {rust_op} {literal}))"
            )),
            true => Ok(format!(
                "({root}.{name} {} Some({literal}))",
                eq_op(rust_op)
            )),
        },
        FieldKind::Codelist | FieldKind::Date => {
            if is_ordering {
                return Err(format!(
                    "{subject}: ordering comparison on `{kind}` field \
                     `{name}` of `{entity}`",
                    kind = if field.kind == FieldKind::Codelist {
                        "codelist"
                    } else {
                        "date"
                    },
                    subject = wording.subject,
                ));
            }
            match field.optional {
                false => Ok(format!("({root}.{name}.to_string() {rust_op} {literal})")),
                true => Ok(format!(
                    "({root}.{name}.as_ref().map(|v| v.to_string()).as_deref() {} Some({literal}))",
                    eq_op(rust_op)
                )),
            }
        }
        FieldKind::Unsupported => Err(format!(
            "{subject}: {noun} references `{name}`, which is not a \
             comparable scalar field of `{entity}`",
            subject = wording.subject,
            noun = wording.noun,
        )),
    }
}

fn eq_op(rust_op: &str) -> &'static str {
    match rust_op {
        "==" => "==",
        _ => "!=",
    }
}

/// Render one field reference over its scope root (`response` for the own
/// record, the injected scope's local otherwise).
fn field_ref(root: &str, field: &FieldInfo) -> String {
    format!("{root}.{}", field.rust_name)
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
pub(crate) fn rust_string_literal(text: &str) -> String {
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
