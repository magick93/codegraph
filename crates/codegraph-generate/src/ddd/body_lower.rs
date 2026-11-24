//! Executable `.ddd` statement bodies lower to the service-op action handler
//! (issue #662, consuming pherolang issue pherolang#178).
//!
//! A `.ddd` service operation may carry a checked procedural body — `let`,
//! `let … else error …`, `assign`, `guard … else error …`, `if`/`else`, and
//! `return` ([`phero_expr::stmt::StmtBody`], stored by the driver as the
//! canonical JSON under the operation's `"stmt"` body target). pherolang
//! typechecks and flow-checks it upstream against the operation's
//! parameters, the designed aggregate, and the synthetic repository classes;
//! this module turns the checked body into Rust **inside the generated action
//! handler**, replacing the 501 seam the pre-#662 plane answered with.
//!
//! # The closed lowering subset
//!
//! The route already loaded the addressed aggregate (`{Entity}Response`), so
//! the body orchestrates *that* record:
//!
//! - `let x = self.findById("current") else error …` — binds the route's
//!   loaded aggregate. The action route is addressed by id, so the only
//!   accepted argument is the documented `"current"` convention or the
//!   route's own id parameter name; anything else is a [`Refusal`] (the
//!   plane cannot honour a second aggregate load).
//! - `let x = <feature path>` / `<literal>` / `<param>` — a plain local.
//! - `let x = <optional feature path> else error …` — `let … else` with the
//!   declared raise on the absent arm.
//! - `guard <bool> else error …` — an early `return Err(…)` of the DECLARED
//!   error (a real 4xx, see [`RaiseTable`]).
//! - `x.field = <value>` — a typed assignment on the aggregate local, which
//!   records the DTO wire key; the patch reaches the command plane through
//!   `self.save(x)` (an assignment that no save persists is a refusal, never
//!   a silently dropped write).
//! - `let y = self.save(x)` — the entity's update command with the
//!   accumulated patch, bound to the RELOADED record.
//! - `if`/`else` — plain Rust blocks over the same machinery (branch locals
//!   stay branch-local).
//! - `return <value>` — the route's `{ "data": … }` envelope, typed by the
//!   operation's declared output.
//!
//! EVERY construct outside that subset is a [`Refusal`] naming the op and
//! the construct. That is the DD9 `query_refusals` posture: a body the plane
//! cannot lower faithfully fails generation BEFORE any tree is written,
//! rather than answering a 501 mid-life or — worse — persisting a partial
//! interpretation. `bodies` empty keeps today's behaviour (the delegation
//! lowering, or the 501 seam naming the gap).
//!
//! # Machine interaction (boundary, `TODO(#662)`)
//!
//! A bodied op takes PRECEDENCE over the `save`→Update delegation lowering.
//! When the op also binds a machine transition and the body does NOT assign
//! the machine state field, the transition patch is applied first and the
//! body's saves flush it — the #590 P0 semantics preserved. When the body
//! itself assigns the state field, the body owns the state and the inferred
//! transition is dropped (a comment names the boundary; unifying body
//! assignments with the designed machine is `TODO(#662)`).

use std::collections::{BTreeMap, HashMap};

use phero_expr::stmt::{BodyRaise, Stmt, StmtBody};
use phero_expr::{BinOp, Expr, ExprKind, UnOp};

use crate::ddd::service_ops::{
    FieldInfo, FieldKind, ServiceOpState, dto_wire_key, rust_string_literal,
};
use crate::project_config::DtoKeyCasing;

/// The declared error vocabulary one body lowers against (DD7).
///
/// The prelude's `NotFound(resource, key)` and `Conflict(detail)` map onto
/// `AppError`'s own 404/409 constructors; every DECLARED error maps onto its
/// generated `{Domain}Error` variant, so the status and code are the model's
/// own (a validation error declared `422` answers 422 with its own code).
/// A raise naming anything else is a refusal — the emitted mapping is total
/// over the declared vocabulary, never a silent fallback.
#[derive(Debug, Clone, Default)]
pub struct RaiseTable {
    /// Declared errors: authored name → the generated variant it maps to.
    pub declared: BTreeMap<String, DeclaredError>,
}

/// One declared, graph-seeded error the `{Domain}Error` plane carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredError {
    /// The generated PascalCase enum variant.
    pub variant: String,
    /// The model's error code (`PAYMENT_DECLINED`, …).
    pub code: String,
    /// The model's HTTP status.
    pub status: u16,
    /// The declared payload field NAMES in declaration order (DD7, issue
    /// #665) — raise arguments align with them positionally (the arity is
    /// the model's own and enforced), and each surfaces as the error
    /// envelope's `details` entry naming its field.
    pub fields: Vec<DeclaredErrorField>,
}

/// One payload field name of a [`DeclaredError`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredErrorField {
    pub name: String,
}

impl RaiseTable {
    /// Resolve one raise name: the prelude's two, then the declared set.
    fn resolve(&self, name: &str) -> Option<RaiseTarget> {
        match name {
            "NotFound" => Some(RaiseTarget::NotFound),
            "Conflict" => Some(RaiseTarget::Conflict),
            other => self.declared.get(other).cloned().map(RaiseTarget::Declared),
        }
    }

    /// The documented (status, declared-name) pair one `throws` entry
    /// lowers to (DD7, issue #665), or `None` when the entry names no
    /// prelude error and no declared variant — the caller turns that into
    /// a named refusal (the closed-subset posture).
    pub fn resolve_entry(&self, name: &str) -> Option<(u16, String)> {
        self.resolve(name)
            .map(|target| (target.status(), target.name().to_string()))
    }
}

/// How one raise lowers.
#[derive(Debug, Clone)]
enum RaiseTarget {
    NotFound,
    Conflict,
    Declared(DeclaredError),
}

impl RaiseTarget {
    fn status(&self) -> u16 {
        match self {
            Self::NotFound => 404,
            Self::Conflict => 409,
            Self::Declared(error) => error.status,
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::NotFound => "NotFound",
            Self::Conflict => "Conflict",
            Self::Declared(error) => &error.variant,
        }
    }
}

// The action-body parameter plane lives in its own module (issue #751: this
// file is over the 2,000-line budget; the #667 payload work must SHRINK it,
// never grow it). Re-exported so every existing path keeps working.
pub(crate) use crate::ddd::body_params::param_extraction_with;
pub(crate) use crate::ddd::body_params::payload_field_rust_type;
pub use crate::ddd::body_params::{BodyParam, Refusal, body_params};

/// One declared raise a body can answer with (docs + hurl planning).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BodyRaiseDoc {
    /// The HTTP status the raise answers with.
    pub status: u16,
    /// The declared error name (the prelude's or the model's).
    pub name: String,
}

/// One documented 4xx response of a bodied action (utoipa/docs).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BodyRaiseResponse {
    /// The HTTP status the raise answers with.
    pub status: u16,
    /// Every declared error documented under it, comma-separated.
    pub errors: String,
}

/// The lowered executable region of one action handler.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BodyPlan {
    /// Pre-rendered Rust statements (consumed verbatim by `handler.tera`).
    pub block: String,
    /// Action-body parameters, in declaration order.
    pub params: Vec<BodyParam>,
    /// The declared raises this body carries, deduped by (status, name).
    pub raises: Vec<BodyRaiseDoc>,
    /// The machine transition applied when the body does not manage the
    /// state field (the #590 P0 patch arm).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<ServiceOpState>,
    /// The machine state field the BODY assigns (`TODO(#662)` boundary).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_manages_state: Option<String>,
    /// `true` when the body persists through `self.save(..)`.
    pub saves: bool,
    /// `true` when any path may answer a declared 4xx — the hurl and e2e
    /// planners refuse to guess an expected status then.
    pub may_refuse: bool,
    /// The declared raises as documented responses (one per status).
    pub responses: Vec<BodyRaiseResponse>,
}

impl BodyPlan {
    /// Group the declared raises into documented responses: one entry per
    /// status, naming every error documented under it.
    fn response_docs(raises: &[BodyRaiseDoc]) -> Vec<BodyRaiseResponse> {
        let mut out: Vec<BodyRaiseResponse> = Vec::new();
        for raise in raises {
            match out.iter_mut().find(|entry| entry.status == raise.status) {
                Some(entry) => {
                    if !entry.errors.is_empty() {
                        entry.errors.push_str(", ");
                    }
                    entry.errors.push_str(&raise.name);
                }
                None => out.push(BodyRaiseResponse {
                    status: raise.status,
                    errors: raise.name.clone(),
                }),
            }
        }
        out
    }
}

/// Everything one body is lowered against (the handler's own scope names).
pub struct BodyScope<'a> {
    /// The service op name as authored.
    pub op_name: &'a str,
    /// The owning service name (diagnostics only).
    pub service: &'a str,
    /// The route's domain (module path of the command/query planes).
    pub domain: &'a str,
    /// The entity's Rust type name (`PayRunType`).
    pub entity_name: &'a str,
    /// The entity's module name (pg table name, `pay_run`).
    pub module_name: &'a str,
    /// The handler's path-parameter binding (`pay_run_id`).
    pub id_param: &'a str,
    /// Scoped (child) routes load through `find_by_id_scoped(<id>, parent_id)`.
    pub scoped: Option<&'a str>,
    /// The auditable flag the query/command calls carry.
    pub auditable: bool,
    /// The generated domain-types crate name (`domain_types`).
    pub domain_types_crate: &'a str,
    /// The Update-DTO wire-key casing.
    pub dto_casing: DtoKeyCasing,
    /// The entity's properties, keyed by authored and snake_case name.
    pub fields: &'a HashMap<String, FieldInfo>,
    /// The machine state field's Rust name, when the entity carries one.
    pub state_field: Option<&'a str>,
    /// The inferred machine transition (applied when the body leaves the
    /// state field alone).
    pub transition: Option<&'a ServiceOpState>,
    /// The declared raise vocabulary.
    pub raises: &'a RaiseTable,
    /// The operation's declared `throws` set (the authored error names). A
    /// raise outside it cannot occur upstream; re-checked here so a
    /// hand-built or stale artifact cannot smuggle one past the plane.
    pub throws: &'a [String],
    /// The declared action-body parameters.
    pub params: &'a [BodyParam],
    /// The `body_receiver` repository name (diagnostics).
    pub receiver: Option<&'a str>,
    /// Whether the bound entity exposes an `update` operation (the command
    /// plane a body's assignments persist through).
    pub has_update: bool,
    /// Whether the route performs the record load the body binds.
    pub record_load: bool,
}

/// A body-local binding.
#[derive(Debug, Clone)]
enum Binding {
    /// The route's loaded aggregate (`{Entity}Response`).
    Record,
    /// A scalar local (parameter or `let`).
    Scalar(ValKind),
}

/// How a value renders in comparisons and envelopes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ValKind {
    /// The aggregate record.
    Record,
    /// A `Copy` scalar (`i32`, `bool`, `Uuid`, …).
    Copy,
    /// A `String`-like owned scalar.
    Str,
    /// An owned scalar outside the comparison subset (date, decimal,
    /// codelist).
    Owned(String),
    /// `Option<Copy>`.
    OptCopy,
    /// `Option<String>`.
    OptStr,
    /// `Option<owned>` outside the subset.
    OptOwned,
    /// A collection (`Vec<T>`) — no statement form reads it.
    Collection,
}

impl ValKind {
    /// The kind of a declared Rust type text (params and `let` bindings).
    pub(crate) fn of_rust_type(rust_type: &str) -> Self {
        let trimmed = rust_type.trim();
        if let Some(inner) = trimmed
            .strip_prefix("Option<")
            .and_then(|rest| rest.strip_suffix('>'))
        {
            return match Self::of_rust_type(inner) {
                Self::Copy => Self::OptCopy,
                Self::Str => Self::OptStr,
                Self::Collection => Self::Collection,
                _ => Self::OptOwned,
            };
        }
        if trimmed.starts_with("Vec<") {
            return Self::Collection;
        }
        match trimmed {
            "bool" => Self::Copy,
            "String" | "&str" | "str" => Self::Str,
            "uuid::Uuid" | "Uuid" | "i8" | "i16" | "i32" | "i64" | "i128" | "u8" | "u16"
            | "u32" | "u64" | "u128" | "usize" | "isize" | "f32" | "f64" => Self::Copy,
            other => Self::Owned(other.to_string()),
        }
    }

    /// The kind of one entity property.
    fn of_field(field: &FieldInfo) -> Self {
        let base = match field.kind {
            FieldKind::Bool | FieldKind::Integer | FieldKind::Float => Self::Copy,
            FieldKind::String => Self::Str,
            FieldKind::Date => Self::Owned("date".to_string()),
            FieldKind::Codelist => Self::Owned("codelist".to_string()),
            FieldKind::Unsupported => Self::Collection,
        };
        if !field.optional {
            return base;
        }
        match base {
            Self::Copy => Self::OptCopy,
            Self::Str => Self::OptStr,
            _ => Self::OptOwned,
        }
    }

    fn is_optional(&self) -> bool {
        matches!(self, Self::OptCopy | Self::OptStr | Self::OptOwned)
    }
}

/// A lowered value expression.
#[derive(Debug, Clone)]
struct Val {
    text: String,
    kind: ValKind,
}

impl Val {
    fn is_record(&self) -> bool {
        self.kind == ValKind::Record
    }
}

/// Lower one checked statement body.
///
/// # Errors
/// [`Refusal`] when the body uses a construct outside the documented subset
/// (naming the op and the construct), when a raise names an undeclared
/// error, or when an assignment can never be persisted.
pub fn lower_body(scope: &BodyScope<'_>, body: &StmtBody) -> Result<BodyPlan, Refusal> {
    Lowerer::new(scope).run(body)
}

/// Parse the canonical `StmtBody` JSON the driver checked.
///
/// # Errors
/// [`Refusal`] when the payload is not canonical `StmtBody` JSON (a hand-built
/// or stale graph node).
pub fn parse_body(op_name: &str, json: &str) -> Result<StmtBody, Refusal> {
    serde_json::from_str(json).map_err(|error| {
        Refusal(format!(
            "service op `{op_name}`: the `stmt` body payload is not canonical \
             `StmtBody` JSON ({error}); regenerate the graph"
        ))
    })
}

struct Lowerer<'a> {
    scope: &'a BodyScope<'a>,
    bindings: HashMap<String, Binding>,
    /// Every identifier token the body's expression slots mention (the
    /// unused-binding scan: a `let` nobody reads emits `let _ = …` so the
    /// generated handler carries no dead-name warnings).
    used_tokens: std::collections::HashSet<String>,
    raises: Vec<BodyRaiseDoc>,
    assigned_fields: Vec<String>,
    uses_patch: bool,
    saves: bool,
    may_refuse: bool,
    manages_state: bool,
    /// Local names the body assigns (they need `mut` at binding time).
    mutated: Vec<String>,
}

impl<'a> Lowerer<'a> {
    fn new(scope: &'a BodyScope<'a>) -> Self {
        Self {
            scope,
            bindings: HashMap::new(),
            used_tokens: std::collections::HashSet::new(),
            raises: Vec::new(),
            assigned_fields: Vec::new(),
            uses_patch: false,
            saves: false,
            may_refuse: false,
            manages_state: false,
            mutated: Vec::new(),
        }
    }

    fn refuse(&self, construct: &str) -> Refusal {
        Refusal(format!(
            "service op `{}` of service `{}`: the statement body uses {construct}, \
             which cannot be lowered to the generated action handler; the \
             supported subset is `let`, `let … else error`, `guard … else \
             error`, feature-path assignment on the loaded aggregate, \
             `self.save(..)`, `if`/`else`, and `return`",
            self.scope.op_name, self.scope.service
        ))
    }

    fn run(mut self, body: &StmtBody) -> Result<BodyPlan, Refusal> {
        if body.stmts.is_empty() {
            return Err(Refusal(format!(
                "service op `{}` of service `{}`: the `stmt` body is empty — an \
                 empty body cannot be lowered (drop the body to keep the \
                 delegation lowering, or author statements)",
                self.scope.op_name, self.scope.service
            )));
        }
        if !self.scope.record_load {
            return Err(Refusal(format!(
                "service op `{}` of service `{}`: the body answers with the \
                 addressed aggregate, but the bound entity `{}` exposes no read \
                 operation (no record load)",
                self.scope.op_name, self.scope.service, self.scope.entity_name
            )));
        }
        self.bind_params()?;
        collect_assigned_roots(&body.stmts, &mut self.mutated);
        collect_used_tokens(&body.stmts, &mut self.used_tokens);

        let statements = self.lower_block(&body.stmts)?;

        if !self.assigned_fields.is_empty() && !self.saves && !self.manages_state {
            return Err(Refusal(format!(
                "service op `{}` of service `{}`: the body assigns `{}` but \
                 never calls `self.save(..)` — the assignment cannot be \
                 persisted, and emitting an implicit save would diverge from \
                 the model",
                self.scope.op_name,
                self.scope.service,
                self.assigned_fields.join("`, `"),
            )));
        }

        let applied_state = if self.manages_state {
            None
        } else {
            self.scope.transition.cloned()
        };
        if applied_state.is_some() {
            self.uses_patch = true;
        }
        let persists = self.saves || applied_state.is_some();

        let mut block = String::new();
        block.push_str(&format!(
            "    // `.ddd` statement body for `{}` (issue #662) — the closed lowering\n\
             \x20   // subset: let / let-else / guard / assign / if / return.\n",
            self.scope.op_name
        ));
        if let Some(receiver) = self.scope.receiver {
            block.push_str(&format!(
                "    // Primary repository (`body_receiver`): `{receiver}` — already\n    \
                 // address-loaded above; `self.findById(..)` is that record.\n"
            ));
        }
        if let Some(state) = self.scope.state_field
            && self.manages_state
        {
            block.push_str(&format!(
                "    // The body manages `{state}` itself: the inferred machine transition\n    \
                 // is NOT applied (TODO(#662): unifying body assignment with the designed\n    \
                 // machine is a recorded follow-up).\n"
            ));
        }
        for param in self.scope.params {
            block.push_str(&self.param_extraction(param));
        }
        if self.uses_patch {
            block.push_str("    let mut patch = serde_json::Map::new();\n");
        }
        if persists {
            block.push_str("    let mut latest = None;\n");
        }
        if let Some(state) = &applied_state {
            block.push_str(&self.transition_arm(state));
        }
        block.push_str(&statements);
        if applied_state.is_some() && !self.saves {
            // The transition is the plane's, not the model's: flush it even
            // when the body never saves (the #590 P0 update semantics).
            block.push_str("    if !patch.is_empty() {\n");
            block.push_str(&self.flush("        "));
            block.push_str("    }\n");
            block.push_str(&format!("    latest = Some({});\n", self.reload_expr()));
        }
        if persists {
            block.push_str("    let data = latest.unwrap_or(response);\n");
            block.push_str(
                "    Ok(axum::Json(serde_json::json!({\n        \"data\": data,\n        \
                 \"meta\": { \"correlation_id\": correlation_id.to_string() }\n    })).into_response())\n",
            );
        } else {
            block.push_str(
                "    Ok(axum::Json(serde_json::json!({\n        \"data\": response,\n        \
                 \"meta\": { \"correlation_id\": correlation_id.to_string() }\n    })).into_response())\n",
            );
        }

        let mut raises = std::mem::take(&mut self.raises);
        raises.sort_by(|a, b| a.status.cmp(&b.status).then(a.name.cmp(&b.name)));
        raises.dedup();
        let responses = BodyPlan::response_docs(&raises);
        Ok(BodyPlan {
            block,
            params: self.scope.params.to_vec(),
            raises,
            state: applied_state,
            body_manages_state: self
                .manages_state
                .then(|| self.scope.state_field.unwrap_or_default().to_string()),
            saves: self.saves,
            may_refuse: self.may_refuse,
            responses,
        })
    }

    // ── Parameters ──────────────────────────────────────────────────────

    fn bind_params(&mut self) -> Result<(), Refusal> {
        for param in self.scope.params {
            self.check_local_name(&param.name)?;
            self.bindings
                .insert(param.name.clone(), Binding::Scalar(param.kind()));
        }
        Ok(())
    }

    /// Render one parameter extraction (the unused-name rule applies: an
    /// unread parameter still VALIDATES its presence, then binds `_`).
    fn param_extraction(&self, param: &BodyParam) -> String {
        param_extraction_with(param, &self.binding_pattern(&param.name))
    }

    // ── Statements ──────────────────────────────────────────────────────

    fn lower_block(&mut self, stmts: &[Stmt]) -> Result<String, Refusal> {
        let mut out = String::new();
        for stmt in stmts {
            out.push_str(&self.lower_stmt(stmt)?);
        }
        Ok(out)
    }

    fn lower_stmt(&mut self, stmt: &Stmt) -> Result<String, Refusal> {
        match stmt {
            Stmt::Let { name, expr } => self.lower_let(name, expr),
            Stmt::LetElse { name, expr, error } => self.lower_let_else(name, expr, error),
            Stmt::Assign { path, expr } => self.lower_assign(path, expr),
            Stmt::Guard { expr, error } => self.lower_guard(expr, error),
            Stmt::If { cond, then, else_ } => self.lower_if(cond, then, else_),
            Stmt::Return { expr } => self.lower_return(expr.as_deref()),
        }
    }

    fn lower_let(&mut self, name: &str, expr: &str) -> Result<String, Refusal> {
        self.check_local_name(name)?;
        let parsed = self.parse_expr(expr)?;
        if let Some(call) = call_parts(&parsed) {
            if call.op.eq_ignore_ascii_case("save") {
                return self.lower_save(name, call);
            }
            if call.op.eq_ignore_ascii_case("findById") {
                return Err(self.refuse(
                    "`self.findById(..)` outside a `let … else error` (a load is \
                     `Option`-typed and declares the raise for the absent row)",
                ));
            }
            return Err(self.refuse(&format!(
                "the call `self.{}(..)` — the generated action plane lowers only \
                 `self.findById(..)` and `self.save(..)`; a domain operation call \
                 rides the mox op plane (recorded follow-up, TODO(#662))",
                call.op
            )));
        }
        let value = self.lower_value(&parsed)?;
        let mutable = self.mutated.iter().any(|m| m == name);
        if value.is_record() {
            let binding = self.binding_pattern(name);
            self.bindings.insert(name.to_string(), Binding::Record);
            let text = if value.text == "response" {
                value.text
            } else {
                format!("{}.clone()", value.text)
            };
            return Ok(format!(
                "    let {mut}{binding} = {text};\n",
                mut = if mutable { "mut " } else { "" }
            ));
        }
        self.bindings
            .insert(name.to_string(), Binding::Scalar(value.kind));
        let binding = self.binding_pattern(name);
        Ok(format!("    let {binding} = {};\n", value.text))
    }

    fn lower_let_else(
        &mut self,
        name: &str,
        expr: &str,
        error: &BodyRaise,
    ) -> Result<String, Refusal> {
        self.check_local_name(name)?;
        let parsed = self.parse_expr(expr)?;
        if let Some(call) = call_parts(&parsed) {
            if call.op.eq_ignore_ascii_case("findById") {
                self.require_record_load()?;
                self.check_find_by_id_argument(call.args)?;
                // The raise still belongs to the declared vocabulary (the
                // absent arm is the route's 404, but the mapping must be
                // total and the docs/hurl planners need the status).
                let target = self.reserve_raise(error)?;
                if target.status() != 404 {
                    return Err(self.refuse(&format!(
                        "`self.findById(..) else error {}` — the absent row is the \
                         route's 404 contract, so the declared raise must be a 404 \
                         (`NotFound`)",
                        error.variant
                    )));
                }
                let mutable = self.mutated.iter().any(|m| m == name);
                let binding = self.binding_pattern(name);
                self.bindings.insert(name.to_string(), Binding::Record);
                return Ok(format!(
                    "    // `self.findById(..)`: the route's address-scoped load — a missing\n    \
                     // row answered 404 before this body runs (the declared raise).\n    \
                     let {mut}{binding} = response.clone();\n",
                    mut = if mutable { "mut " } else { "" }
                ));
            }
            return Err(self.refuse(&format!(
                "the call `self.{}(..)` as a `let … else` initializer — only \
                 `self.findById(..)` is Option-typed in the body subset",
                call.op
            )));
        }
        let value = self.lower_value(&parsed)?;
        let Some(unwrapped) = optional_unwrap(&value.kind) else {
            return Err(self.refuse(&format!(
                "`let {name} = {expr} else error …` — the initializer is not \
                 Option-typed, so the `else` arm can never fire"
            )));
        };
        let raise = self.lower_raise(error)?;
        self.may_refuse = true;
        let binding = self.binding_pattern(name);
        self.bindings
            .insert(name.to_string(), Binding::Scalar(unwrapped));
        Ok(format!(
            "    let Some({binding}) = {} else {{\n        return Err({raise});\n    }};\n",
            value.text
        ))
    }

    fn lower_guard(&mut self, expr: &str, error: &BodyRaise) -> Result<String, Refusal> {
        let parsed = self.parse_expr(expr)?;
        let cond = self.lower_cond(&parsed)?;
        let raise = self.lower_raise(error)?;
        self.may_refuse = true;
        Ok(format!(
            "    if !({cond}) {{\n        return Err({raise});\n    }}\n"
        ))
    }

    fn lower_if(&mut self, cond: &str, then: &[Stmt], else_: &[Stmt]) -> Result<String, Refusal> {
        let parsed = self.parse_expr(cond)?;
        let cond = self.lower_cond(&parsed)?;
        // Branch locals never escape.
        let saved = self.bindings.clone();
        let then_block = self.lower_block(then)?;
        self.bindings = saved.clone();
        let else_block = self.lower_block(else_)?;
        self.bindings = saved;
        if else_.is_empty() {
            Ok(format!("    if {cond} {{\n{then_block}    }}\n"))
        } else {
            Ok(format!(
                "    if {cond} {{\n{then_block}    }} else {{\n{else_block}    }}\n"
            ))
        }
    }

    fn lower_return(&mut self, expr: Option<&str>) -> Result<String, Refusal> {
        let Some(expr) = expr else {
            // A void op's bare `return;` answers with the addressed record.
            return Ok(
                "    return Ok(axum::Json(serde_json::json!({\n        \"data\": response,\n        \
                 \"meta\": { \"correlation_id\": correlation_id.to_string() }\n    }})).into_response());\n"
                    .to_string(),
            );
        };
        let parsed = self.parse_expr(expr)?;
        let value = self.lower_value(&parsed)?;
        if matches!(value.kind, ValKind::Collection) {
            return Err(self.refuse(&format!(
                "`return {expr}` — a collection-valued return is outside the body \
                 subset (collections cannot be navigated or returned)"
            )));
        }
        Ok(format!(
            "    return Ok(axum::Json(serde_json::json!({{\n        \"data\": &({}),\n        \
             \"meta\": {{ \"correlation_id\": correlation_id.to_string() }}\n    }})).into_response());\n",
            value.text
        ))
    }

    fn lower_assign(&mut self, path: &[String], expr: &str) -> Result<String, Refusal> {
        let Some((root, rest)) = path.split_first() else {
            return Err(self.refuse("an assignment with no target path"));
        };
        if rest.is_empty() {
            return Err(self.refuse(&format!(
                "the whole-local reassignment `{root} = …` (a body assigns \
                 aggregate feature paths)"
            )));
        }
        if rest.len() > 1 {
            return Err(self.refuse(&format!(
                "the nested assignment path `{}` (one feature hop is supported)",
                path.join(".")
            )));
        }
        let Some(binding) = self.bindings.get(root).cloned() else {
            return Err(self.refuse(&format!(
                "an assignment to `{root}`, which is not a body local"
            )));
        };
        if !matches!(binding, Binding::Record) {
            return Err(self.refuse(&format!(
                "an assignment to `{root}`, which is not the loaded aggregate"
            )));
        }
        let field_name = &rest[0];
        let Some(field) = self.scope.fields.get(field_name).cloned() else {
            return Err(self.refuse(&format!(
                "an assignment to `{root}.{field_name}`, which is not a field of `{}`",
                self.scope.entity_name
            )));
        };
        let parsed = self.parse_expr(expr)?;
        let value = self.lower_value(&parsed)?;
        if value.is_record() || matches!(value.kind, ValKind::Collection) {
            return Err(self.refuse(&format!(
                "an assignment of a record/collection to `{root}.{}`",
                field.rust_name
            )));
        }
        if !self.scope.has_update {
            return Err(Refusal(format!(
                "service op `{}` of service `{}`: the body assigns \
                 `{root}.{field_name}`, but the bound entity `{}` exposes no \
                 update operation (no command plane to persist it)",
                self.scope.op_name, self.scope.service, self.scope.entity_name
            )));
        }
        if self.scope.state_field == Some(field.rust_name.as_str()) {
            self.manages_state = true;
        }
        self.assigned_fields
            .push(format!("{root}.{}", field.rust_name));
        self.uses_patch = true;
        let wire_key = dto_wire_key(&field.rust_name, self.scope.dto_casing);
        let mut out = format!("    {root}.{} = {};\n", field.rust_name, value.text);
        out.push_str(&format!(
            "    patch.insert({key}.to_string(), serde_json::json!(&{root}.{field}));\n",
            key = rust_string_literal(wire_key.as_str()),
            field = field.rust_name,
        ));
        Ok(out)
    }

    fn lower_save(&mut self, name: &str, call: Call) -> Result<String, Refusal> {
        let Some(argument) = call.args.first() else {
            return Err(self.refuse("`self.save(..)` with no aggregate argument"));
        };
        let ExprKind::Name(target) = &argument.kind else {
            return Err(self.refuse(
                "`self.save(..)` over a non-local (save the loaded aggregate \
                 local the body assigns)",
            ));
        };
        if !matches!(self.bindings.get(target), Some(Binding::Record)) {
            return Err(self.refuse(&format!(
                "`self.save({target})`, whose argument is not a local bound to \
                 the loaded aggregate"
            )));
        }
        let used = self.binding_pattern(name);
        let mut out = String::new();
        if self.assigned_fields.is_empty() && self.scope.transition.is_none() {
            // Nothing to persist: the save is an idempotent read of the
            // addressed record (the #590 update-leg convention).
            out.push_str(&format!(
                "    // `self.save({target})`: the body assigned nothing, so the save\n    \
                 // is an idempotent read of the addressed record.\n"
            ));
            if used == "_" {
                out.push_str("    latest = Some(response.clone());\n");
            } else {
                out.push_str(&format!("    let {used} = response.clone();\n"));
                out.push_str(&format!("    latest = Some({used}.clone());\n"));
            }
            self.saves = true;
            return Ok(out);
        }
        self.saves = true;
        let expression = {
            let mut text = String::new();
            text.push_str("if patch.is_empty() {\n        response.clone()\n    } else {\n");
            text.push_str(&self.flush("        "));
            text.push_str(&format!("        {}\n    }}", self.reload_expr()));
            text
        };
        if used == "_" {
            out.push_str(&format!("    latest = Some({expression});\n"));
        } else {
            out.push_str(&format!("    let {used} = {expression};\n"));
            out.push_str(&format!("    latest = Some({used}.clone());\n"));
        }
        Ok(out)
    }

    fn lower_raise(&mut self, raise: &BodyRaise) -> Result<String, Refusal> {
        let target = self.reserve_raise(raise)?;
        let args = self.lower_raise_args(&target, &raise.args)?;
        let render = match &target {
            RaiseTarget::NotFound => {
                let [resource, key] = args.as_slice() else {
                    return Err(self.refuse(&format!(
                        "`error NotFound(..)` with {} argument(s) — the prelude \
                         declares `NotFound(resource: string, key: string)`",
                        args.len()
                    )));
                };
                format!("format!(\"{{}} {{}} not found\", {resource}, {key})")
            }
            RaiseTarget::Conflict => {
                let [detail] = args.as_slice() else {
                    return Err(self.refuse(&format!(
                        "`error Conflict(..)` with {} argument(s) — the prelude \
                         declares `Conflict(detail: string)`",
                        args.len()
                    )));
                };
                detail.clone()
            }
            RaiseTarget::Declared(error) => {
                let joined = args.join(", ");
                if joined.is_empty() {
                    rust_string_literal(&format!(
                        "{}: operation `{}` refused",
                        error.variant, self.scope.op_name
                    ))
                } else {
                    format!("format!(\"{}: {{}}\", {joined})", error.variant)
                }
            }
        };
        Ok(match &target {
            RaiseTarget::NotFound => {
                format!("AppError::not_found({render}).with_correlation_id(correlation_id)")
            }
            RaiseTarget::Conflict => {
                format!("AppError::conflict({render}).with_correlation_id(correlation_id)")
            }
            RaiseTarget::Declared(error) => {
                let domain_error = format!(
                    "crate::domain::{}::errors::{}Error",
                    self.scope.domain,
                    codegraph_naming::to_pascal_case(self.scope.domain)
                );
                // DD7 (issue #665): the payload fields surface in the
                // error envelope's `details` entries in declaration order —
                // the same order the raise arguments lower in (the arity
                // is enforced above), so field N names argument N.
                let details = if error.fields.is_empty() {
                    "None".to_string()
                } else {
                    let entries: Vec<String> = error
                        .fields
                        .iter()
                        .zip(args.iter())
                        .map(|(field, value)| {
                            format!(
                                "crate::error::FieldError {{ field: {field}, message: {value}.to_string() }}",
                                field = rust_string_literal(&field.name),
                            )
                        })
                        .collect();
                    format!("Some(vec![{}])", entries.join(", "))
                };
                format!(
                    "AppError {{\n            status: {domain_error}::{variant}.http_status(),\n            \
                     code: {domain_error}::{variant}.code(),\n            \
                     message: {render},\n            details: {details},\n            \
                     correlation_id: Some(correlation_id),\n        }}",
                    variant = error.variant,
                )
            }
        })
    }

    /// Resolve and record one raise (deduped docs), rejecting an undeclared
    /// name and one the operation's `throws` clause does not carry.
    fn reserve_raise(&mut self, raise: &BodyRaise) -> Result<RaiseTarget, Refusal> {
        if !self.scope.throws.iter().any(|name| name == &raise.variant) {
            let declared = if self.scope.throws.is_empty() {
                "the operation declares no `throws` clause, so it raises no error".to_string()
            } else {
                format!("declared: {}", self.scope.throws.join(", "))
            };
            return Err(Refusal(format!(
                "service op `{}` of service `{}`: the body raises `{}`, which is \
                 not in the operation's `throws` ({declared}) — the raised \
                 vocabulary is explicit and complete",
                self.scope.op_name, self.scope.service, raise.variant
            )));
        }
        let Some(target) = self.scope.raises.resolve(&raise.variant) else {
            let declared: Vec<&str> = self
                .scope
                .raises
                .declared
                .keys()
                .map(String::as_str)
                .collect();
            let declared = if declared.is_empty() {
                "no declared errors are in scope".to_string()
            } else {
                format!("declared: {}", declared.join(", "))
            };
            return Err(Refusal(format!(
                "service op `{}` of service `{}`: the body raises `{}`, which maps \
                 to no generated `{}Error` variant (the prelude is \
                 `NotFound`/`Conflict`; {declared}) — declare it with \
                 `error {}` and add it to the operation's `throws`",
                self.scope.op_name,
                self.scope.service,
                raise.variant,
                codegraph_naming::to_pascal_case(self.scope.domain),
                raise.variant
            )));
        };
        self.raises.push(BodyRaiseDoc {
            status: target.status(),
            name: target.name().to_string(),
        });
        Ok(target)
    }

    fn lower_raise_args(
        &mut self,
        target: &RaiseTarget,
        args: &[String],
    ) -> Result<Vec<String>, Refusal> {
        let expected = match target {
            RaiseTarget::NotFound => 2,
            RaiseTarget::Conflict => 1,
            // A declared error's payload arity is the model's own. When
            // the payload fields are known (the DD7 graph vocabulary),
            // they ENFORCE it: fewer arguments than fields would silently
            // truncate the `details` entries (issue #665 review N3), so a
            // mismatch is a refusal. An unknown payload (a hand-built
            // table) keeps the argument count.
            RaiseTarget::Declared(error) => {
                if error.fields.is_empty() {
                    args.len()
                } else {
                    error.fields.len()
                }
            }
        };
        if args.len() != expected {
            return Err(self.refuse(&format!(
                "`error {}(..)` with {} argument(s); the declared payload has \
                 {expected}",
                target.name(),
                args.len()
            )));
        }
        let mut out = Vec::with_capacity(args.len());
        for arg in args {
            let parsed = self.parse_expr(arg)?;
            let value = self.lower_value(&parsed)?;
            if value.is_record() || matches!(value.kind, ValKind::Collection) {
                return Err(self.refuse(&format!("a record/collection raise argument `{arg}`")));
            }
            out.push(value.text);
        }
        Ok(out)
    }

    // ── Expressions ─────────────────────────────────────────────────────

    fn parse_expr(&self, source: &str) -> Result<Expr, Refusal> {
        let parsed = phero_expr::parse(source);
        match parsed.ast {
            Some(ast) if parsed.errors.is_empty() => Ok(ast),
            _ => Err(self.refuse(&format!(
                "`{source}`, which is not valid pherolang expression syntax"
            ))),
        }
    }

    fn lower_value(&mut self, expr: &Expr) -> Result<Val, Refusal> {
        match &expr.kind {
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) => Ok(Val {
                text: literal_text(expr),
                kind: ValKind::Copy,
            }),
            ExprKind::String(text) => Ok(Val {
                text: rust_string_literal(text),
                kind: ValKind::Str,
            }),
            ExprKind::Date { .. } => Err(self
                .refuse("a date literal (date comparison/assignment is outside the body subset)")),
            ExprKind::Null => {
                Err(self.refuse("a bare `null` (compare an optional field to null instead)"))
            }
            ExprKind::Name(name) => match self.bindings.get(name).cloned() {
                Some(Binding::Record) => Ok(Val {
                    text: name.clone(),
                    kind: ValKind::Record,
                }),
                Some(Binding::Scalar(kind)) => Ok(Val {
                    text: name.clone(),
                    kind,
                }),
                None => Err(self.refuse(&format!(
                    "the name `{name}`, which is not a body local or parameter"
                ))),
            },
            ExprKind::FeatureAccess {
                receiver,
                name,
                optional_safe,
            } => {
                if *optional_safe {
                    return Err(self.refuse("`?.` (safe navigation)"));
                }
                let ExprKind::Name(root) = &receiver.kind else {
                    return Err(self.refuse(&format!(
                        "the nested feature access `{}` (one hop off a loaded aggregate)",
                        write_path(expr)
                    )));
                };
                if !matches!(self.bindings.get(root), Some(Binding::Record)) {
                    return Err(self.refuse(&format!(
                        "the feature access `{}`, whose receiver `{root}` is not a \
                         loaded aggregate",
                        write_path(expr)
                    )));
                }
                let Some(field) = self.scope.fields.get(&name.value).cloned() else {
                    return Err(self.refuse(&format!(
                        "the feature `{}`, which is not a field of `{}`",
                        name.value, self.scope.entity_name
                    )));
                };
                let kind = ValKind::of_field(&field);
                if matches!(kind, ValKind::Collection) {
                    return Err(self.refuse(&format!(
                        "the collection feature `{}` (collections cannot be \
                         navigated by the body subset)",
                        name.value
                    )));
                }
                let clone = match kind {
                    ValKind::Str | ValKind::OptStr | ValKind::Owned(_) | ValKind::OptOwned => {
                        ".clone()"
                    }
                    _ => "",
                };
                Ok(Val {
                    text: format!("{root}.{}{clone}", field.rust_name),
                    kind,
                })
            }
            ExprKind::Unary {
                op: UnOp::Neg,
                expr,
            } => match &expr.kind {
                ExprKind::Int(_) | ExprKind::Float(_) => Ok(Val {
                    text: format!("-{}", literal_text(expr)),
                    kind: ValKind::Copy,
                }),
                _ => Err(self.refuse("negation of a non-literal (arithmetic)")),
            },
            ExprKind::Unary { .. } => Err(self.refuse("unary `!` outside a condition")),
            ExprKind::Binary { .. } => Err(self.refuse(
                "a binary expression in a value slot (arithmetic is outside the \
                 body subset)",
            )),
            ExprKind::Coalesce { .. } => Err(self.refuse("`?:`")),
            ExprKind::Call { .. } | ExprKind::FunctionCall { .. } => {
                Err(self.refuse("a call in a value slot"))
            }
            ExprKind::Algebra { kind, .. } => {
                Err(self.refuse(&format!("collection algebra `{kind}`")))
            }
            ExprKind::If { .. } => Err(self.refuse("`if` as an expression")),
            ExprKind::Let { .. } => Err(self.refuse("an expression-level `let`")),
            ExprKind::Lambda { .. } => Err(self.refuse("a lambda")),
            ExprKind::ListLiteral(_) => Err(self.refuse("a list literal")),
        }
    }

    fn lower_cond(&mut self, expr: &Expr) -> Result<String, Refusal> {
        match &expr.kind {
            ExprKind::Bool(true) => Ok("true".to_string()),
            ExprKind::Bool(false) => Ok("false".to_string()),
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::And | BinOp::Or => {
                    let left = self.lower_cond(lhs)?;
                    let right = self.lower_cond(rhs)?;
                    let joiner = if matches!(op, BinOp::And) { "&&" } else { "||" };
                    Ok(format!("(({left}) {joiner} ({right}))"))
                }
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                    self.lower_comparison(op, lhs, rhs)
                }
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    Err(self.refuse(&format!("arithmetic (`{op}`) in a condition")))
                }
            },
            ExprKind::Unary {
                op: UnOp::Not,
                expr,
            } => {
                let inner = self.lower_cond(expr)?;
                Ok(format!("(!({inner}))"))
            }
            ExprKind::Name(name) => match self.bindings.get(name).cloned() {
                Some(Binding::Scalar(ValKind::Copy)) => Ok(name.clone()),
                Some(_) => Err(self.refuse(&format!(
                    "`{name}` as a condition — it is not a boolean scalar"
                ))),
                None => Err(self.refuse(&format!(
                    "the condition name `{name}`, which is not a body local"
                ))),
            },
            _ => Err(self.refuse(&format!(
                "`{}` as a condition (only comparisons, `&&`, `||`, `!`, and \
                 boolean locals are supported)",
                write_path(expr)
            ))),
        }
    }

    fn lower_comparison(&mut self, op: &BinOp, lhs: &Expr, rhs: &Expr) -> Result<String, Refusal> {
        // `field == null` / `field != null` flip to an Option check.
        if matches!(op, BinOp::Eq | BinOp::Ne)
            && (matches!(lhs.kind, ExprKind::Null) || matches!(rhs.kind, ExprKind::Null))
        {
            let value = if matches!(lhs.kind, ExprKind::Null) {
                self.lower_value(rhs)?
            } else {
                self.lower_value(lhs)?
            };
            return match (op, value.kind.is_optional()) {
                (BinOp::Eq, true) => Ok(format!("{}.is_none()", value.text)),
                (BinOp::Ne, true) => Ok(format!("{}.is_some()", value.text)),
                (BinOp::Eq, false) => Ok("false".to_string()),
                _ => Ok("true".to_string()),
            };
        }
        let left = self.lower_value(lhs)?;
        let right = self.lower_value(rhs)?;
        let rust_op = match op {
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And | BinOp::Or | BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                unreachable!("only comparison operators reach here")
            }
        };
        if left.is_record() || right.is_record() {
            return Err(self.refuse("a comparison against the whole aggregate record"));
        }
        let is_ordering = matches!(op, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge);
        match (&left.kind, &right.kind) {
            (ValKind::Copy | ValKind::Str, ValKind::Copy | ValKind::Str) => {
                Ok(format!("({} {rust_op} {})", left.text, right.text))
            }
            (ValKind::OptCopy, ValKind::Copy) => Ok(format!(
                "{}.is_some_and(|value| value {rust_op} {})",
                left.text, right.text
            )),
            (ValKind::Copy, ValKind::OptCopy) => Ok(format!(
                "{}.is_some_and(|value| value {rust_op} {})",
                right.text, left.text
            )),
            (ValKind::OptStr, ValKind::Str) if !is_ordering => Ok(format!(
                "({}.as_deref() {rust_op} Some({}))",
                left.text, right.text
            )),
            (ValKind::Str, ValKind::OptStr) if !is_ordering => Ok(format!(
                "({}.as_deref() {rust_op} Some({}))",
                right.text, left.text
            )),
            (ValKind::OptStr, ValKind::Str) => Ok(format!(
                "{}.is_some_and(|value| value {rust_op} {})",
                left.text, right.text
            )),
            (ValKind::Owned(what), _) | (_, ValKind::Owned(what)) => Err(self.refuse(&format!(
                "a `{what}` comparison (dates, decimals, and codelists compare \
                 outside the body subset)"
            ))),
            (ValKind::OptOwned, _) | (_, ValKind::OptOwned) => Err(self
                .refuse("an optional date/decimal/codelist comparison (outside the body subset)")),
            (ValKind::Collection, _) | (_, ValKind::Collection) => {
                Err(self.refuse("a collection comparison"))
            }
            _ => Err(self.refuse(&format!(
                "a comparison whose left side is {} — the operand shapes do not \
                 unify in the body subset",
                kind_name(&left.kind)
            ))),
        }
    }

    // ── Emitted machinery ───────────────────────────────────────────────

    fn require_record_load(&self) -> Result<(), Refusal> {
        if self.scope.record_load {
            return Ok(());
        }
        Err(Refusal(format!(
            "service op `{}` of service `{}`: the body loads the aggregate, but \
             the bound entity `{}` exposes no read operation (no record load)",
            self.scope.op_name, self.scope.service, self.scope.entity_name
        )))
    }

    fn check_find_by_id_argument(&self, args: &[Expr]) -> Result<(), Refusal> {
        let Some(argument) = args.first() else {
            return Err(self.refuse("`self.findById(..)` with no argument"));
        };
        let accepted = match &argument.kind {
            ExprKind::String(text) => text == "current",
            ExprKind::Name(name) => name == self.scope.id_param,
            _ => false,
        };
        if accepted {
            Ok(())
        } else {
            Err(self.refuse(&format!(
                "`self.findById({})` — the action route is addressed by id, so the \
                 body loads the addressed aggregate: pass `\"current\"` (or `{}`)",
                write_path(argument),
                self.scope.id_param
            )))
        }
    }

    /// The Rust pattern one binding renders as: its own name when the body
    /// reads it, `_` otherwise (no dead-name warnings in the handler).
    fn binding_pattern(&self, name: &str) -> String {
        if self.used_tokens.contains(name) {
            name.to_string()
        } else {
            "_".to_string()
        }
    }

    fn check_local_name(&self, name: &str) -> Result<(), Refusal> {
        const RESERVED: [&str; 11] = [
            "response",
            "body",
            "state",
            "api_key_info",
            "correlation_id",
            "patch",
            "parent_id",
            "latest",
            "data",
            "cmd",
            "current_state",
        ];
        if RESERVED.contains(&name) {
            return Err(Refusal(format!(
                "service op `{}` of service `{}`: the body local `{name}` shadows \
                 the generated handler's `{name}`; rename it in the model",
                self.scope.op_name, self.scope.service
            )));
        }
        Ok(())
    }

    fn transition_arm(&self, state: &ServiceOpState) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "    // The inferred machine transition (the body leaves `{}` alone) —\n",
            self.scope.state_field.unwrap_or("the state field")
        ));
        out.push_str(&format!(
            "    let current_state = {};\n",
            state.current_expr
        ));
        if !state.op.is_empty() {
            // The kernel decides (issue #818, Option A): the candidate
            // targets are probed through the runtime; exactly one legal
            // edge fires, none is the 409, an ambiguous fork names its
            // targets. The hand-rendered match arms are retired.
            out.push_str(&crate::ddd::kernel_probe::runtime_construction(
                self.scope.domain_types_crate,
                &state.machine_id,
                "    ",
            ));
            let candidates: Vec<String> = {
                let mut targets: Vec<String> = Vec::new();
                for transition in &state.op {
                    let target = transition.to.as_str().to_string();
                    if !targets.contains(&target) {
                        targets.push(target);
                    }
                }
                targets
            };
            out.push_str(&crate::ddd::kernel_probe::probe_legal_block(
                &candidates,
                "    ",
            ));
            out.push_str(&crate::ddd::kernel_probe::named_verdict_block(
                self.scope.op_name,
                "    ",
            ));
            out.push_str(&format!(
                "    patch.insert({}.to_string(), serde_json::Value::String(target_state.to_string()));\n",
                rust_string_literal(state.wire_key.as_str())
            ));
        }
        out
    }

    /// The update-command call that flushes `patch` through the command plane.
    fn flush(&self, pad: &str) -> String {
        let domain_error = format!(
            "{}Error",
            codegraph_naming::to_pascal_case(self.scope.domain)
        );
        let mut out = String::new();
        out.push_str(&format!(
            "{pad}let cmd: crate::domain::{domain}::{module}::dto_update::Update{entity}Request =\n\
             {pad}    serde_json::from_value(serde_json::Value::Object(std::mem::take(&mut patch)))\n\
             {pad}    .map_err(|error| {{\n\
             {pad}        AppError::bad_request(format!(\"invalid {entity} update body: {{error}}\"))\n\
             {pad}            .with_correlation_id(correlation_id)\n\
             {pad}    }})?;\n",
            domain = self.scope.domain,
            module = self.scope.module_name,
            entity = self.scope.entity_name,
        ));
        out.push_str(&format!(
            "{pad}state.{domain}_{module}_commands.update(\n\
             {pad}    {id},\n\
             {pad}    cmd,\n\
             {pad}    {domain_types}::SourceContext::api(),\n\
             {pad}    correlation_id,\n\
             {pad}    api_key_info.api_key_id,\n\
             {pad}    api_key_info.organization_id,\n\
             {pad}    api_key_info.user_id,\n\
             {pad}    api_key_info.role.clone(),\n\
             {pad}).await.map_err(|error: {domain_error}| {{\n\
             {pad}    AppError::from_domain_error(error.http_status(), error.to_string(), \"Failed to update {entity}\")\n\
             {pad}        .with_correlation_id(correlation_id)\n\
             {pad}}})?;\n",
            domain = self.scope.domain,
            module = self.scope.module_name,
            id = self.scope.id_param,
            domain_types = self.scope.domain_types_crate,
            entity = self.scope.entity_name,
        ));
        out
    }

    /// The stale-row-proof reload the route answers with after a save.
    fn reload_expr(&self) -> String {
        let scoped = match self.scope.scoped {
            Some(parent) => format!(", {parent}"),
            None => String::new(),
        };
        let auditable = if self.scope.auditable { ", false" } else { "" };
        let domain_error = format!(
            "{}Error",
            codegraph_naming::to_pascal_case(self.scope.domain)
        );
        format!(
            "state.{domain}_{module}_queries.{method}({id}{scoped}{auditable}, api_key_info.api_key_id, \
             api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n\
             \x20       .await\n\
             \x20       .map_err(|error: {domain_error}| {{\n\
             \x20           AppError::from_domain_error(error.http_status(), error.to_string(), \"Failed to reload {entity}\")\n\
             \x20               .with_correlation_id(correlation_id)\n\
             \x20       }})?\n\
             \x20       .ok_or_else(|| AppError::not_found(\"{entity} not found\").with_correlation_id(correlation_id))?",
            domain = self.scope.domain,
            module = self.scope.module_name,
            method = if self.scope.scoped.is_some() {
                "find_by_id_scoped"
            } else {
                "find_by_id"
            },
            id = self.scope.id_param,
            entity = self.scope.entity_name,
        )
    }
}

// ── Free helpers ───────────────────────────────────────────────────────

struct Call<'a> {
    op: &'a str,
    args: &'a [Expr],
}

/// The `self.<op>(args)` shape, when `expr` is a call through the `self`
/// receiver (the body's primary repository).
fn call_parts(expr: &Expr) -> Option<Call<'_>> {
    let ExprKind::Call {
        receiver,
        name,
        args,
        optional_safe,
    } = &expr.kind
    else {
        return None;
    };
    if *optional_safe {
        return None;
    }
    if !matches!(&receiver.kind, ExprKind::Name(root) if root == "self") {
        return None;
    }
    Some(Call {
        op: &name.value,
        args,
    })
}

/// The unwrapped kind of an `Option`-shaped value.
fn optional_unwrap(kind: &ValKind) -> Option<ValKind> {
    match kind {
        ValKind::OptCopy => Some(ValKind::Copy),
        ValKind::OptStr => Some(ValKind::Str),
        ValKind::OptOwned => Some(ValKind::Owned("value".to_string())),
        _ => None,
    }
}

fn kind_name(kind: &ValKind) -> &'static str {
    match kind {
        ValKind::Record => "the aggregate record",
        ValKind::Copy => "a scalar",
        ValKind::Str => "a string",
        ValKind::Owned(_) => "an owned scalar",
        ValKind::OptCopy => "an optional scalar",
        ValKind::OptStr => "an optional string",
        ValKind::OptOwned => "an optional owned scalar",
        ValKind::Collection => "a collection",
    }
}

/// Render a literal expression (`5`, `1.5`, `true`).
fn literal_text(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Int(value) => value.to_string(),
        ExprKind::Float(value) => {
            if value.is_finite() && value.fract() == 0.0 && value.abs() < 1e15 {
                format!("{value}.0")
            } else {
                format!("{value}")
            }
        }
        ExprKind::Bool(value) => value.to_string(),
        _ => String::new(),
    }
}

fn write_path(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Name(name) => name.clone(),
        ExprKind::String(text) => format!("{text:?}"),
        ExprKind::Int(value) => value.to_string(),
        ExprKind::Float(value) => value.to_string(),
        ExprKind::Bool(value) => value.to_string(),
        ExprKind::Null => "null".to_string(),
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
        ExprKind::Call {
            receiver,
            name,
            args,
            optional_safe,
        } => format!(
            "{}{}{}({})",
            write_path(receiver),
            if *optional_safe { "?." } else { "." },
            name.value,
            args.iter().map(write_path).collect::<Vec<_>>().join(", ")
        ),
        _ => "<expression>".to_string(),
    }
}

/// Every identifier token the body's expression slots mention (plus every
/// assignment path root) — the unused-binding scan behind
/// [`Lowerer::binding_pattern`].
fn collect_used_tokens(stmts: &[Stmt], out: &mut std::collections::HashSet<String>) {
    fn add(text: &str, out: &mut std::collections::HashSet<String>) {
        for token in text
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|token| !token.is_empty())
        {
            out.insert(token.to_string());
        }
    }
    for stmt in stmts {
        match stmt {
            Stmt::Let { expr, .. } => add(expr, out),
            Stmt::LetElse { expr, error, .. } => {
                add(expr, out);
                for arg in &error.args {
                    add(arg, out);
                }
            }
            Stmt::Assign { path, expr } => {
                if let Some(root) = path.first() {
                    add(root, out);
                }
                add(expr, out);
            }
            Stmt::Guard { expr, error } => {
                add(expr, out);
                for arg in &error.args {
                    add(arg, out);
                }
            }
            Stmt::If { cond, then, else_ } => {
                add(cond, out);
                collect_used_tokens(then, out);
                collect_used_tokens(else_, out);
            }
            Stmt::Return { expr } => {
                if let Some(expr) = expr {
                    add(expr, out);
                }
            }
        }
    }
}

/// The root names every assignment statement in the body targets (branch
/// bodies included) — those locals need `mut`.
fn collect_assigned_roots(stmts: &[Stmt], out: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign { path, .. } => {
                if let Some(root) = path.first()
                    && !out.contains(root)
                {
                    out.push(root.clone());
                }
            }
            Stmt::If { then, else_, .. } => {
                collect_assigned_roots(then, out);
                collect_assigned_roots(else_, out);
            }
            Stmt::Let { .. } | Stmt::LetElse { .. } | Stmt::Guard { .. } | Stmt::Return { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal `{Entity}Response` feature set: a required int, a required
    /// string, an optional string, and a codelist (the shapes the subset
    /// accepts and refuses).
    fn fields() -> HashMap<String, FieldInfo> {
        let mut map = HashMap::new();
        let mut field = |name: &str, kind: FieldKind, optional: bool| {
            map.insert(
                name.to_string(),
                FieldInfo {
                    rust_name: codegraph_naming::to_snake_case(name),
                    kind,
                    optional,
                },
            );
        };
        field("total", FieldKind::Integer, false);
        field("note", FieldKind::String, true);
        field("payRunStatus", FieldKind::Codelist, false);
        map
    }

    /// The fixture operation's declared `throws` vocabulary.
    fn throws_vocabulary() -> &'static [String] {
        static ONCE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
        ONCE.get_or_init(|| {
            vec![
                "NotFound".to_string(),
                "Conflict".to_string(),
                "PaymentDeclined".to_string(),
            ]
        })
        .as_slice()
    }

    fn scopes<'a>(
        fields: &'a HashMap<String, FieldInfo>,
        raises: &'a RaiseTable,
        params: &'a [BodyParam],
        transition: Option<&'a ServiceOpState>,
    ) -> BodyScope<'a> {
        BodyScope {
            op_name: "applyPayRun",
            service: "PayRunService",
            domain: "payroll",
            entity_name: "PayRun",
            module_name: "pay_run",
            id_param: "pay_run_id",
            scoped: None,
            auditable: true,
            domain_types_crate: "domain_types",
            dto_casing: DtoKeyCasing::Snake,
            fields,
            state_field: Some("pay_run_status"),
            transition,
            raises,
            params,
            throws: throws_vocabulary(),
            receiver: Some("PayRunRepository"),
            has_update: true,
            record_load: true,
        }
    }

    fn body(json: &str) -> StmtBody {
        serde_json::from_str(json).expect("canonical StmtBody JSON")
    }

    fn raise_table() -> RaiseTable {
        let mut table = RaiseTable::default();
        table.declared.insert(
            "PaymentDeclined".to_string(),
            DeclaredError {
                variant: "PaymentDeclined".to_string(),
                code: "PAYMENT_DECLINED".to_string(),
                status: 422,
                fields: Vec::new(),
            },
        );
        table
    }

    const FIND: &str = r#"{"kind":"letElse","name":"run","expr":"self.findById(\"current\")","error":{"variant":"NotFound","args":["\"PayRun\"","\"current\""]}}"#;

    fn int_param() -> BodyParam {
        BodyParam {
            name: "total".to_string(),
            rust_type: "i32".to_string(),
            optional: false,
            direct_body: false,
            sample: serde_json::json!(1),
            request_body: None,
        }
    }

    /// The full consumer-critical shape: load, guard, assign, save.
    #[test]
    fn the_save_payload_body_lowers_end_to_end() {
        let fields = fields();
        let raises = raise_table();
        let params = vec![int_param()];
        let scope = scopes(&fields, &raises, &params, None);
        let json = format!(
            r#"{{"stmts":[{FIND},
             {{"kind":"guard","expr":"run.total < 100","error":{{"variant":"Conflict","args":["\"limit\""]}}}},
             {{"kind":"assign","path":["run","total"],"expr":"total"}},
             {{"kind":"let","name":"saved","expr":"self.save(run)"}}]}}"#
        );
        let plan = lower_body(&scope, &body(&json)).expect("lowering");
        // Parameters are extracted from the action body.
        assert!(
            plan.block.contains("let total: i32 = body.as_ref()"),
            "{}",
            plan.block
        );
        // The aggregate local binds the route's loaded record.
        assert!(
            plan.block.contains("let mut run = response.clone();"),
            "{}",
            plan.block
        );
        // The guard is a REAL 4xx naming the declared error.
        assert!(
            plan.block.contains("if !((run.total < 100))"),
            "{}",
            plan.block
        );
        assert!(
            plan.block.contains(r#"AppError::conflict("limit")"#),
            "{}",
            plan.block
        );
        // The assignment writes the record and records the DTO key.
        assert!(plan.block.contains("run.total = total;"), "{}", plan.block);
        assert!(
            plan.block
                .contains(r#"patch.insert("total".to_string(), serde_json::json!(&run.total));"#),
            "{}",
            plan.block
        );
        // The save flushes through the command plane and reloads.
        assert!(plan.block.contains("_commands.update("), "{}", plan.block);
        assert!(
            plan.block.contains("dto_update::UpdatePayRunRequest"),
            "{}",
            plan.block
        );
        assert!(
            plan.block.contains("find_by_id(pay_run_id, false"),
            "{}",
            plan.block
        );
        assert!(
            plan.block
                .contains("let data = latest.unwrap_or(response);"),
            "{}",
            plan.block
        );
        assert!(plan.saves, "the plan records the save");
        assert!(plan.may_refuse, "the guard may answer 4xx");
        assert_eq!(
            plan.raises,
            vec![
                BodyRaiseDoc {
                    status: 404,
                    name: "NotFound".to_string()
                },
                BodyRaiseDoc {
                    status: 409,
                    name: "Conflict".to_string()
                },
            ]
        );
        assert_eq!(plan.responses[0].status, 404);
        assert_eq!(plan.responses[0].errors, "NotFound");
        assert_eq!(plan.responses[1].errors, "Conflict");
    }

    /// `if`/`else` + a value `return`: the branch returns the route envelope.
    #[test]
    fn if_else_and_return_lower_to_a_typed_envelope() {
        let fields = fields();
        let raises = raise_table();
        let params: Vec<BodyParam> = Vec::new();
        let scope = scopes(&fields, &raises, &params, None);
        let json = format!(
            r#"{{"stmts":[{FIND},
             {{"kind":"if","cond":"run.total > 0","then":[{{"kind":"return","expr":"run.total"}}],"else":[{{"kind":"return","expr":"0"}}]}}]}}"#
        );
        let plan = lower_body(&scope, &body(&json)).expect("lowering");
        assert!(
            plan.block.contains("if (run.total > 0) {"),
            "{}",
            plan.block
        );
        assert!(
            plan.block.contains(r#""data": &(run.total),"#),
            "{}",
            plan.block
        );
        assert!(plan.block.contains(r#""data": &(0),"#), "{}", plan.block);
    }

    /// `let … else` over an OPTIONAL feature: the absent arm raises.
    #[test]
    fn let_else_over_an_optional_feature_raises() {
        let fields = fields();
        let raises = raise_table();
        let params: Vec<BodyParam> = Vec::new();
        let scope = scopes(&fields, &raises, &params, None);
        let json = format!(
            r#"{{"stmts":[{FIND},
             {{"kind":"letElse","name":"note","expr":"run.note","error":{{"variant":"Conflict","args":["\"missing note\""]}}}}]}}"#
        );
        let plan = lower_body(&scope, &body(&json)).expect("lowering");
        assert!(
            plan.block
                .contains("let Some(note) = run.note.clone() else {"),
            "{}",
            plan.block
        );
        assert!(plan.may_refuse, "{}", plan.block);
    }

    /// A declared (graph-seeded) error maps onto its OWN variant's status and
    /// code — the emitted mapping is total over the declared vocabulary.
    #[test]
    fn declared_errors_map_onto_the_generated_error_plane() {
        let fields = fields();
        let raises = raise_table();
        let params: Vec<BodyParam> = Vec::new();
        let scope = scopes(&fields, &raises, &params, None);
        let json = format!(
            r#"{{"stmts":[{FIND},
             {{"kind":"guard","expr":"run.total < 100","error":{{"variant":"PaymentDeclined","args":["\"insufficient\""]}}}}]}}"#
        );
        let plan = lower_body(&scope, &body(&json)).expect("lowering");
        assert!(
            plan.block.contains(
                "crate::domain::payroll::errors::PayrollError::PaymentDeclined.http_status()"
            ),
            "{}",
            plan.block
        );
        assert!(
            plan.block
                .contains("crate::domain::payroll::errors::PayrollError::PaymentDeclined.code()"),
            "{}",
            plan.block
        );
        assert_eq!(plan.raises[1].status, 422);
        assert_eq!(plan.raises[1].name, "PaymentDeclined");
    }

    /// The machine transition applies when the body leaves the state field
    /// alone, and is DROPPED when the body assigns it (the TODO(#662)
    /// boundary).
    #[test]
    fn machine_transition_survives_only_when_the_body_leaves_the_state_alone() {
        let fields = fields();
        let raises = raise_table();
        let params: Vec<BodyParam> = Vec::new();
        let state = ServiceOpState {
            current_expr: "response.pay_run_status.to_string()".to_string(),
            wire_key: crate::ddd::service_ops::WireKey::new("pay_run_status"),
            initial: crate::ddd::service_ops::StateName::new("Draft"),
            machine_id: "payroll_pay_run_status".to_string(),
            all: Vec::new(),
            op: vec![crate::ddd::service_ops::ServiceOpTransition {
                from: crate::ddd::service_ops::StateName::new("Draft"),
                to: crate::ddd::service_ops::StateName::new("Calculating"),
            }],
        };
        let scope = scopes(&fields, &raises, &params, Some(&state));
        let json = format!(
            r#"{{"stmts":[{FIND},
             {{"kind":"assign","path":["run","total"],"expr":"0"}},
             {{"kind":"let","name":"saved","expr":"self.save(run)"}}]}}"#
        );
        let plan = lower_body(&scope, &body(&json)).expect("lowering");
        assert!(plan.state.is_some(), "{}", plan.block);
        assert!(plan.body_manages_state.is_none());
        // The kernel decides (issue #818): the probe + verdict, never a
        // match arm.
        assert!(
            plan.block
                .contains(r#"let legal: Vec<&str> = ["Calculating"]"#),
            "{}",
            plan.block
        );
        assert!(
            plan.block
                .contains("runtime.transition_for(&current_state, candidate).is_some()"),
            "{}",
            plan.block
        );
        assert!(
            !plan.block.contains(r#" => Some("#),
            "the retired match-arm spelling must not render:\n{}",
            plan.block
        );

        // The body assigns the state field: it owns the machine.
        let json = format!(
            r#"{{"stmts":[{FIND},
             {{"kind":"assign","path":["run","payRunStatus"],"expr":"run.payRunStatus"}},
             {{"kind":"let","name":"saved","expr":"self.save(run)"}}]}}"#
        );
        let plan = lower_body(&scope, &body(&json)).expect("lowering");
        assert!(plan.state.is_none(), "{}", plan.block);
        assert_eq!(plan.body_manages_state.as_deref(), Some("pay_run_status"));
        assert!(
            plan.block
                .contains("The body manages `pay_run_status` itself"),
            "{}",
            plan.block
        );
    }

    /// Every out-of-subset construct is a refusal naming the op and the
    /// construct — never a silent seam.
    #[test]
    fn out_of_subset_constructs_are_named_refusals() {
        let fields = fields();
        let raises = raise_table();
        let params = vec![int_param()];
        let scope = scopes(&fields, &raises, &params, None);
        let cases: Vec<(String, &str)> = vec![
            (
                r#"{"stmts":[{"kind":"let","name":"x","expr":"1 + 2"}]}"#.to_string(),
                "binary expression in a value slot",
            ),
            (
                format!(
                    r#"{{"stmts":[{FIND},{{"kind":"let","name":"x","expr":"[1, 2]"}}]}}"#
                ),
                "a list literal",
            ),
            (
                format!(
                    r#"{{"stmts":[{FIND},{{"kind":"let","name":"count","expr":"run.note.size()"}}]}}"#
                ),
                "collection algebra `size`",
            ),
            (
                format!(
                    r#"{{"stmts":[{FIND},{{"kind":"let","name":"s","expr":"self.totalFor(\"x\")"}},{{"kind":"let","name":"q","expr":"s"}}]}}"#
                ),
                "self.totalFor",
            ),
            (
                format!(
                    r#"{{"stmts":[{FIND},{{"kind":"guard","expr":"run.total > 0","error":{{"variant":"Nope","args":[]}}}}]}}"#
                ),
                "raises `Nope`",
            ),
            (
                format!(
                    r#"{{"stmts":[{FIND},{{"kind":"assign","path":["run","total"],"expr":"1"}}]}}"#
                ),
                "never calls `self.save(..)`",
            ),
            (
                format!(
                    r#"{{"stmts":[{FIND},{{"kind":"assign","path":["run","note","x"],"expr":"1"}}]}}"#
                ),
                "nested assignment path",
            ),
            (
                r#"{"stmts":[{"kind":"let","name":"patch","expr":"1"}]}"#.to_string(),
                "shadows the generated handler's",
            ),
            (
                r#"{"stmts":[{"kind":"letElse","name":"run","expr":"self.findById(\"other\")","error":{"variant":"NotFound","args":["\"A\"","\"b\""]}}]}"#.to_string(),
                "addressed by id",
            ),
            (
                r#"{"stmts":[{"kind":"letElse","name":"x","expr":"1","error":{"variant":"NotFound","args":["\"A\"","\"b\""]}}]}"#.to_string(),
                "not Option-typed",
            ),
            (
                r#"{"stmts":[{"kind":"return","expr":"nope"}]}"#.to_string(),
                "not a body local",
            ),
        ];
        for (json, needle) in cases {
            let refusals = match lower_body(&scope, &body(&json)) {
                Ok(plan) => panic!("expected a refusal for {json}, got {}", plan.block),
                Err(refusal) => refusal.0,
            };
            assert!(
                refusals.contains(needle),
                "the refusal must name `{needle}`: {refusals}"
            );
            assert!(
                refusals.contains("applyPayRun"),
                "the refusal must name the op: {refusals}"
            );
        }
    }

    /// A CHILD route (nested under the parent) reloads through the SCOPED
    /// read — the same `find_by_id_scoped(<id>, parent_id)` the rest of the
    /// child plane uses.
    #[test]
    fn a_child_route_reloads_through_the_scoped_read() {
        let fields = fields();
        let raises = raise_table();
        let params = vec![int_param()];
        let mut scope = scopes(&fields, &raises, &params, None);
        scope.scoped = Some("parent_id");
        let json = format!(
            r#"{{"stmts":[{FIND},{{"kind":"assign","path":["run","total"],"expr":"total"}},{{"kind":"let","name":"saved","expr":"self.save(run)"}}]}}"#
        );
        let plan = lower_body(&scope, &body(&json)).expect("lowering");
        assert!(
            plan.block
                .contains("find_by_id_scoped(pay_run_id, parent_id, false"),
            "{}",
            plan.block
        );
    }

    /// The body plane refuses to load a second aggregate, and refuses a
    /// body on an entity with no record load / no command plane.
    #[test]
    fn structural_preconditions_are_refusals() {
        let fields = fields();
        let raises = raise_table();
        let params = vec![int_param()];
        let json = format!(
            r#"{{"stmts":[{FIND},{{"kind":"assign","path":["run","total"],"expr":"total"}},{{"kind":"let","name":"saved","expr":"self.save(run)"}}]}}"#
        );

        let mut scope = scopes(&fields, &raises, &params, None);
        scope.has_update = false;
        let refusal = lower_body(&scope, &body(&json)).expect_err("no command plane");
        assert!(refusal.0.contains("no update operation"), "{}", refusal.0);

        let mut scope = scopes(&fields, &raises, &params, None);
        scope.record_load = false;
        let refusal = lower_body(&scope, &body(&json)).expect_err("no record load");
        assert!(refusal.0.contains("no record load"), "{}", refusal.0);

        // A raise outside the declared vocabulary cannot be named.
        let scope = scopes(&fields, &raises, &params, None);
        let json = format!(
            r#"{{"stmts":[{FIND},{{"kind":"guard","expr":"run.total < 100","error":{{"variant":"Gone","args":[]}}}}]}}"#
        );
        let refusal = lower_body(&scope, &body(&json)).expect_err("undeclared raise");
        assert!(
            refusal.0.contains("not in the operation's `throws`"),
            "{}",
            refusal.0
        );

        // A raise the operation's `throws` clause omits is refused even when
        // the error plane carries the variant (a stale/hand-built artifact).
        let mut scope = scopes(&fields, &raises, &params, None);
        scope.throws = &[];
        let json = format!(
            r#"{{"stmts":[{FIND},{{"kind":"guard","expr":"run.total < 100","error":{{"variant":"Conflict","args":["\"x\""]}}}}]}}"#
        );
        let refusal = lower_body(&scope, &body(&json)).expect_err("raise outside throws");
        assert!(
            refusal.0.contains("declares no `throws` clause"),
            "{}",
            refusal.0
        );
    }

    /// Issue #667 review: an OPTIONAL direct-body parameter must deserialize
    /// the INNER type — `from_value::<Option<T>>` yields `Option<Option<T>>`
    /// and does not compile against the wrapper let binding.
    #[test]
    fn optional_direct_body_extraction_binds_the_inner_type() {
        let param = BodyParam {
            name: "input".to_string(),
            rust_type: "Option<domain_types::dto::catalog::NewBookInput>".to_string(),
            optional: true,
            direct_body: true,
            sample: serde_json::Value::Null,
            request_body: None,
        };
        let block = param_extraction_with(&param, "_");
        assert!(
            block.contains(
                "serde_json::from_value::<domain_types::dto::catalog::NewBookInput>(value.0.clone())"
            ),
            "{block}"
        );
        assert!(
            !block.contains("from_value::<Option<"),
            "the inner type must be deserialized, not the Option wrapper: {block}"
        );
    }

    /// DD3 (issue #667): a DTO-typed action parameter resolves to the
    /// generated boundary type, a LONE one becomes the request body itself,
    /// and output-only / unknown boundaries are named refusals.
    #[test]
    fn body_params_resolve_modeled_dto_boundaries() {
        use codegraph_core::types::{DddDtoMember, DddDtoShapeNode};
        let shape = |name: &str, direction: &str, members: Vec<DddDtoMember>| DddDtoShapeNode {
            application: "Library".to_string(),
            module: "catalog".to_string(),
            name: name.to_string(),
            source_json: None,
            direction: direction.to_string(),
            members,
            ordinal: 0,
        };
        let member =
            |name: &str, kind: &str, type_json: serde_json::Value, optional: bool| DddDtoMember {
                name: name.to_string(),
                kind: kind.to_string(),
                type_json,
                multiplicity_json: optional
                    .then(|| serde_json::json!({"lower": 0, "upper": {"finite": 1}})),
                path: Vec::new(),
                expression: (kind == "derive").then(|| "1 + 1".to_string()),
            };
        let index = crate::ddd::dto_boundaries::DtoBoundaryIndex::for_tests(vec![
            shape(
                "NewBookInput",
                "input",
                vec![
                    member(
                        "title",
                        "declared",
                        serde_json::json!({"type": "primitive", "value": "string"}),
                        false,
                    ),
                    member(
                        "isbn",
                        "declared",
                        serde_json::json!({"type": "primitive", "value": "string"}),
                        true,
                    ),
                ],
            ),
            shape(
                "BookSummary",
                "output",
                vec![member(
                    "minutes",
                    "derive",
                    serde_json::json!({"type": "primitive", "value": "int"}),
                    false,
                )],
            ),
        ]);
        let dto_param = |name: &str, boundary: &str| codegraph_core::types::DddParam {
            name: name.to_string(),
            type_json: serde_json::json!({"dto": boundary}),
            multiplicity: None,
        };

        let params = body_params(
            "createBook",
            &[dto_param("input", "NewBookInput")],
            &index,
            "domain_types",
            crate::project_config::DtoKeyCasing::Snake,
        )
        .expect("input boundary binds");
        assert_eq!(params.len(), 1);
        assert_eq!(
            params[0].rust_type,
            "domain_types::dto::catalog::NewBookInput"
        );
        assert!(params[0].direct_body, "a lone DTO param is the body");
        assert_eq!(
            params[0].sample,
            serde_json::json!({"title": "contract-sample"}),
            "the minimal body omits optional members"
        );

        let output = body_params(
            "summarise",
            &[dto_param("summary", "BookSummary")],
            &index,
            "domain_types",
            crate::project_config::DtoKeyCasing::Snake,
        )
        .expect_err("output-only boundary refuses");
        assert!(output.0.contains("output-only"), "{}", output.0);
        assert!(output.0.contains("BookSummary"), "{}", output.0);

        let unknown = body_params(
            "createBook",
            &[dto_param("input", "NotDeclared")],
            &index,
            "domain_types",
            crate::project_config::DtoKeyCasing::Snake,
        )
        .expect_err("unknown boundary refuses");
        assert!(unknown.0.contains("does not declare"), "{}", unknown.0);
    }

    /// Action-body parameters: primitives only, collections refused, and an
    /// optional parameter binds `Option<T>`.
    #[test]
    fn body_params_cover_the_primitive_surface() {
        let json = |name: &str, type_json: serde_json::Value, multiplicity| {
            codegraph_core::types::DddParam {
                name: name.to_string(),
                type_json,
                multiplicity,
            }
        };
        let params = vec![
            json(
                "total",
                serde_json::json!({"type": "primitive", "value": "int"}),
                None,
            ),
            json(
                "note",
                serde_json::json!({"type": "primitive", "value": "string"}),
                Some(serde_json::json!({"lower": 0, "upper": {"finite": 1}})),
            ),
        ];
        let params = body_params(
            "applyPayRun",
            &params,
            &Default::default(),
            "domain_types",
            crate::project_config::DtoKeyCasing::Snake,
        )
        .expect("params");
        assert_eq!(params[0].rust_type, "i32");
        assert!(!params[0].optional);
        assert_eq!(params[1].rust_type, "Option<String>");
        assert!(params[1].optional);

        let class = vec![json(
            "audit",
            serde_json::json!({"type": "class", "value": {"package": "payroll", "name": "Audit"}}),
            None,
        )];
        let refusal = body_params(
            "applyPayRun",
            &class,
            &Default::default(),
            "domain_types",
            crate::project_config::DtoKeyCasing::Snake,
        )
        .expect_err("class param");
        assert!(refusal.0.contains("primitives only"), "{}", refusal.0);

        let many = vec![json(
            "ids",
            serde_json::json!({"type": "primitive", "value": "string"}),
            Some(serde_json::json!({"lower": 0, "upper": "unbounded"})),
        )];
        let refusal = body_params(
            "applyPayRun",
            &many,
            &Default::default(),
            "domain_types",
            crate::project_config::DtoKeyCasing::Snake,
        )
        .expect_err("collection param");
        assert!(refusal.0.contains("collection"), "{}", refusal.0);
    }

    /// The payload must be canonical `StmtBody` JSON — a stale/hand-built
    /// graph node is named, not panicked on.
    #[test]
    fn a_non_canonical_payload_is_a_named_refusal() {
        let refusal = parse_body("applyPayRun", "{not json").expect_err("refusal");
        assert!(refusal.0.contains("applyPayRun"), "{}", refusal.0);
        assert!(refusal.0.contains("canonical"), "{}", refusal.0);
    }

    #[test]
    fn empty_bodies_are_refused() {
        let fields = fields();
        let raises = raise_table();
        let params: Vec<BodyParam> = Vec::new();
        let scope = scopes(&fields, &raises, &params, None);
        let refusal = lower_body(&scope, &StmtBody::default()).expect_err("empty body");
        assert!(refusal.0.contains("empty"), "{}", refusal.0);
    }
}
