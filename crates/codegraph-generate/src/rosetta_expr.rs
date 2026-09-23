//! Rosetta `expr_json` → Rust transpiler (issue #262, slices 1–3).
//!
//! The rosetta bridge stores conditions as `ConditionNode.expr_json` — the
//! canonical [`sigil_model::expr::Expr::to_json`] serialization (serde_json
//! `Value`, documented stable shape). This module turns those payloads into
//! Rust expression fragments for the `condition_validations` generator.
//!
//! # Strategy: untyped emission with minimal local inference
//!
//! Sigil does NOT type-check expressions (resolve checks head symbols only),
//! so THIS transpiler owns semantics. It emits Rust **untyped** — the only
//! inference is caller-provided field knowledge via [`ExprContext`]:
//!
//! - `optional_fields` — `Option<T>` DTO fields (from `PropertyNode.is_nullable`).
//!   Required fields → direct access (`dto.total`); optional fields accessed
//!   bare → [`TranspileError`] (the generator keeps a `TODO(#262)` marker
//!   naming the reason). `exists` / `absent` ARE the sanctioned way to touch
//!   optional fields: `X exists` → `dto.x.is_some()`, `X absent` →
//!   `dto.x.is_none()` (only a BARE symbol argument is sanctioned — deep
//!   chains through an optional receiver are refused).
//! - `collection_fields` — snake names of array properties (`is_array`).
//!   Gates the word-binaries (`contains` / `disjoint` / `join`), `Count`,
//!   and the exists cardinality modifiers: their natural Rust forms only
//!   exist on collections, and a string-vs-Vec receiver cannot be told apart
//!   without type knowledge, so a non-collection root is REFUSED rather
//!   than emitted wrong.
//! - `numeric_fields` / `integer_fields` — snake names of number- and
//!   integer-typed properties (the integer set is a subset of the numeric
//!   one). Gates `Sum` / `Min` / `Max` and picks their element type
//!   (`i64` vs `f64`). Three sets (not two) because the i64/f64 choice
//!   needs the integer subset; the generator derives all of them from
//!   `PropertyNode.prop_type` / `rust_field_type`.
//!
//! Every bare symbol is treated as a field on the receiver (unknown-symbol
//! handling is the caller's concern).
//!
//! # The fragment-style contract
//!
//! Collection operations emit iterator-based Rust **EXPRESSIONS** — never
//! statements — so any fragment composes as an operand of the next
//! operation (`prices distinct reverse first`, `lines filter [...] count`).
//! The collection/aggregate suffixes (`.iter()`, `.len()`, `.sum::<T>()`)
//! therefore only compose cleanly with collection-typed fragments (fields,
//! `collect`-terminated chains); chaining them onto a lazy iterator adapter
//! directly is a documented compile-time rejection downstream.
//!
//! ## Per-family emission decisions (slices 2–3)
//!
//! - **`contains`** → `L.contains(&R)`; `R` is parenthesized when its kind
//!   is `Binary`/`Conditional`/`Switch`/`Then` (Rust `&` binds tighter than
//!   operators). REFUSED for a non-collection left root: a string haystack
//!   needs `.contains(&pattern)` semantics we cannot pick untyped —
//!   documented refusal, kind stays `Binary`.
//! - **`disjoint`** → `L.iter().all(|x| !R.contains(x))` (same collection
//!   gate on the left root).
//! - **`default`** → `L.unwrap_or(R)`; the left argument is emitted with
//!   the exists-sanction (bare optional access is the point of `default`).
//! - **`Join`** → `L.join(sep)`; `sep` is the transpiled right operand when
//!   `explicitSeparator` is true, else the literal `", "`. DELIBERATE
//!   DIVERGENCE: sigil stores a generated `""` separator literal for the
//!   separator-less form; we emit `", "` (Rust's slice-join default and the
//!   display-intent reading) per the #262 slice-2 mandate.
//! - **`Flatten`** → `A.iter().flatten().collect::<Vec<_>>()`; **`Distinct`**
//!   → `A.iter().collect::<std::collections::BTreeSet<_>>()` (deterministic
//!   order); **`Reverse`** → `A.iter().rev().collect::<Vec<_>>()`; **`First`**
//!   / **`Last`** → `A.first()` / `A.last()`. Ungated: a non-collection
//!   argument fails at rustc, the same documented caveat class as
//!   exists-on-required.
//! - **`Sum`** → `A.iter().sum::<i64|f64>()` (integer root → `i64`, else
//!   `f64`); non-numeric root → `Unsupported("sum requires numeric element
//!   type")`. **`Min`** / **`Max`** (bare form) →
//!   `A.iter().copied().fold(<identity>, i64::min|f64::min|…max)` — always
//!   yields a value so the fragment stays composable; the documented caveat
//!   is that an EMPTY collection yields the identity sentinel.
//! - **`Count`** → `A.len()` but ONLY when the root is a collection field
//!   (`.len()` on a non-collection would be a wrong-typed guess).
//! - **Exists modifiers** — `single` → `A.len() == 1`, `multiple` →
//!   `A.len() >= 2`, gated on a collection root (the argument keeps the
//!   exists-sanction); `none` is unchanged (`.is_some()`).
//! - **`Filter` / `Map`** (extract) →
//!   `A.iter().filter(|p| body).collect::<Vec<_>>()` /
//!   `A.iter().map(|p| body).collect::<Vec<_>>()`. The `collect` suffix is
//!   deliberate: without it the fragment is a lazy iterator and composes
//!   with nothing downstream (`len`/`iter`/`first` all need a collection).
//! - **Lambda scope** — a filter/map body is emitted under a binding frame:
//!   explicit parameters (`filter p [...]`) bind their name; the
//!   implicit-parameter form (`filter [...]`) binds `item`. A bare
//!   `SymbolReference` matching the innermost frame's parameter emits the
//!   parameter bare; any other bare symbol stays a receiver-field access
//!   (sigil keeps no scoping in the JSON — parameter-name matching is the
//!   only honest approximation, innermost frame first). The implicit
//!   variable `item` under EXPLICIT parameters is refused (it is shadowed).
//! - **`Reduce`** → `Unsupported` (documented gap): sigil serializes no
//!   init field, so the `a, b [body]` form (first-element-as-init) would
//!   need `Iterator::reduce`, whose `Option` result breaks expression
//!   composition, and the `reduce [init]` bracket form is stored as an
//!   empty-parameter inline function — indistinguishable from a greedy
//!   implicit body.
//! - **`Sort`** → `Unsupported` (documented gap): std has no expression-form
//!   sort; `sort_by` requires statement form.
//! - **`Min`/`Max` with a lambda** (`min p [p > 0.0]`) → `Unsupported`
//!   (documented gap): min-by/mapped-min semantics are ambiguous in the
//!   serialization and have no clean expression form.
//! - **`Switch`** → an if-else-chain EXPRESSION over the argument
//!   (`if A == g1 { e1 } else if … else { ed }`; the argument fragment is
//!   re-emitted per guard — deterministic, side-effect-free). Exactly one
//!   default case is required; `Reference` guards (enum/choice options)
//!   are `Unsupported` (type knowledge not carried in the payload).
//! - **`Conditional`** → `if cond { then } else { else }`. The `full`
//!   flag is informational only: a `full == false` conditional still
//!   carries its generated empty-list `else` in the payload, which is
//!   emitted faithfully (`vec![]`).
//! - **Casts** — `ToString` → `.to_string()`; `ToNumber` →
//!   `.parse::<f64>().ok()`; `ToInt` → `.parse::<i64>().ok()`
//!   (Option-propagating: a failed parse surfaces as `None`, pairing with
//!   exists/default downstream). `ToEnum` → `Unsupported` (codelist
//!   knowledge is not carried by `ExprContext`). `ToDate` /
//!   `ToDateTime` / `ToZonedDateTime` / `ToTime` → `Unsupported` (a
//!   date-time crate — chrono — is deliberately absent from
//!   codegraph-generate; adding deps is out of scope for #262).
//! - **`Then`** — `function: null` → argument passthrough; a named-ref
//!   function (body is an explicit argument-less `SymbolReference`) → call
//!   form `f(A)`; otherwise the body is an implicit inline lambda whose
//!   implicit variable binds the ARGUMENT FRAGMENT (parenthesized when the
//!   argument is itself a complex operand). Parameterized then-functions →
//!   `Unsupported`.
//! - **`WithMeta`** → `{A} /* meta dropped */` — the argument passes
//!   through, the metadata entries are dropped with an inline marker
//!   comment (valid Rust anywhere inside an expression).
//! - **`OnlyExists`** → the boolean conjunction of per-argument
//!   `X.is_some()` (each argument keeps the exists-sanction). DOCUMENTED
//!   GAP: Rosetta's exclusivity ("only" — all other optional attributes
//!   must be absent) needs full type knowledge and is NOT enforced.
//! - **`As`** (type-system cast), **`AsKey`** (map-key semantics),
//!   **`OneOf`** / **`Choice`** (choice-variant representation deferred),
//!   **`Constructor`** (DTO construction knowledge) → `Unsupported`
//!   stubs, each carrying its kind string.
//!
//! # Known emission caveats (documented, deliberate)
//!
//! - **Number/Int literals are emitted VERBATIM** (Rosetta keeps source
//!   text). A trailing-dot BigDecimal literal (`5.`) is valid Rosetta but
//!   not valid Rust — such conditions are rejected downstream by rustc, not
//!   silently rewritten here.
//! - **`X exists` / `X absent` on REQUIRED fields** still emit
//!   `.is_some()` / `.is_none()`, which does not compile against a
//!   non-`Option` field. Untyped policy: exists/absent are only meaningful
//!   on optionals (models write them on optionals).
//! - **`OnlyElement` emits `.first()`** — SEMANTIC GAP: Rust `first()`
//!   does not enforce Rosetta's uniqueness contract.
//! - **Nested binary operands are parenthesized** (deterministic,
//!   precedence-safe): `(a && b) || c`. Top level is not wrapped;
//!   `Conditional`/`Switch`/`Then` operands are wrapped the same way.
//! - **Scalar-element comparisons inside lambda bodies** (`filter p
//!   [p > 1.0]` over a number array) emit `p > 1.0` where the closure
//!   binds a reference — std's `PartialOrd` impls do not cover
//!   `&f64 > f64`, so such bodies are downstream compile rejections,
//!   not silent rewrites.
//!
//! codegraph-generate must NOT depend on sigil (issue #255 rule) — this
//! module operates purely on the stored `serde_json::Value`.

use std::collections::HashSet;
use std::fmt;

use codegraph_naming::{escape_rust_keyword, to_snake_case};

/// Why an expression could not be transpiled.
///
/// `kind` carries the `Expr::to_json` kind tag that was rejected
/// (`"Binary"`, `"Switch"`, ...) so generators can surface it in
/// `TODO(#262)` markers; `detail` explains the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranspileError {
    pub kind: String,
    pub detail: String,
}

impl TranspileError {
    fn new(kind: &str, detail: impl Into<String>) -> Self {
        Self {
            kind: kind.to_string(),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for TranspileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unsupported expression ({}): {}", self.kind, self.detail)
    }
}

impl std::error::Error for TranspileError {}

/// Transpilation context: the receiver variable name (e.g. `"dto"`) plus
/// the caller-derived field knowledge (all snake_case DTO field names):
///
/// - `optional_fields`: `Option<T>` fields (`PropertyNode.is_nullable`);
/// - `collection_fields`: array properties (`PropertyNode.is_array`);
/// - `numeric_fields` / `integer_fields`: number- and integer-typed
///   properties (integer is a subset of numeric) — gates the aggregate
///   family and picks its element type.
pub struct ExprContext<'a> {
    pub receiver: &'a str,
    pub optional_fields: &'a HashSet<String>,
    pub collection_fields: &'a HashSet<String>,
    pub numeric_fields: &'a HashSet<String>,
    pub integer_fields: &'a HashSet<String>,
}

/// Transpile one `Expr::to_json` payload into a Rust expression fragment.
pub fn transpile(
    payload: &serde_json::Value,
    ctx: &ExprContext<'_>,
) -> Result<String, TranspileError> {
    Emitter {
        ctx,
        frames: Vec::new(),
    }
    .emit(payload, false)
}

/// One lambda binding frame (innermost frame last in the stack).
struct Frame {
    /// Parameter names (snake_case, keyword-escaped) matchable by bare
    /// symbol references.
    params: Vec<String>,
    /// What the implicit variable `item` binds to. `Some` for the
    /// implicit-parameter form (`filter [...]` binds the element) and for
    /// `then` bodies (binds the argument fragment); `None` under explicit
    /// parameters, where the implicit variable is shadowed.
    implicit: Option<String>,
}

struct Emitter<'a> {
    ctx: &'a ExprContext<'a>,
    frames: Vec<Frame>,
}

impl Emitter<'_> {
    /// Core emitter. `allow_optional_root` sanctions a bare optional-field
    /// reference — set ONLY for exists/absent/default/only-exists arguments
    /// (the sanctioned ways to touch optionals).
    fn emit(
        &mut self,
        payload: &serde_json::Value,
        allow_optional_root: bool,
    ) -> Result<String, TranspileError> {
        let kind = payload.get("kind").and_then(|k| k.as_str());
        match kind {
            Some("Boolean") => {
                let value = payload
                    .get("value")
                    .and_then(|v| v.as_bool())
                    .ok_or_else(|| {
                        TranspileError::new("Boolean", "malformed payload: missing 'value'")
                    })?;
                Ok(if value { "true" } else { "false" }.to_string())
            }
            Some("String") => {
                let value = payload
                    .get("value")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        TranspileError::new("String", "malformed payload: missing 'value'")
                    })?;
                Ok(string_literal(value))
            }
            // Rosetta keeps the source text of numeric literals — emit
            // verbatim. Trailing-dot BigDecimal text (`5.`) is valid Rosetta
            // but not valid Rust; see the module docs.
            Some("Number") | Some("Int") => {
                let text = payload
                    .get("text")
                    .and_then(|t| t.as_str())
                    .ok_or_else(|| {
                        TranspileError::new(
                            kind.unwrap_or_default(),
                            "malformed payload: missing 'text'",
                        )
                    })?;
                Ok(text.to_string())
            }
            Some("List") => {
                let elements = payload
                    .get("elements")
                    .and_then(|e| e.as_array())
                    .ok_or_else(|| {
                        TranspileError::new("List", "malformed payload: missing 'elements'")
                    })?;
                let mut parts = Vec::with_capacity(elements.len());
                for element in elements {
                    parts.push(self.emit(element, false)?);
                }
                Ok(format!("vec![{}]", parts.join(", ")))
            }
            Some("SymbolReference") => self.emit_symbol_reference(payload, allow_optional_root),
            Some("ImplicitVariable") => self.implicit_binding(),
            Some(call @ ("FeatureCall" | "DeepFeatureCall")) => {
                let Some(feature) = payload.get("feature").and_then(|f| f.as_str()) else {
                    return Err(TranspileError::new(
                        call,
                        "bare '->' projection (no feature) is unsupported",
                    ));
                };
                let receiver = payload.get("receiver").ok_or_else(|| {
                    TranspileError::new(call, "malformed payload: missing 'receiver'")
                })?;
                let recv = self
                    .emit(receiver, false)
                    .map_err(|e| optional_receiver_error(call, e))?;
                Ok(format!(
                    "{}.{}",
                    recv,
                    escape_rust_keyword(&to_snake_case(feature))
                ))
            }
            Some("Binary") => self.emit_binary(payload),
            Some("Exists") => self.emit_exists(payload),
            Some("Absent") => {
                let argument = payload.get("argument").ok_or_else(|| {
                    TranspileError::new("Absent", "malformed payload: missing 'argument'")
                })?;
                Ok(format!("{}.is_none()", self.emit(argument, true)?))
            }
            // SEMANTIC GAP: `.first()` does not enforce Rosetta's uniqueness
            // contract (module docs).
            Some("OnlyElement") => {
                let argument = payload.get("argument").ok_or_else(|| {
                    TranspileError::new("OnlyElement", "malformed payload: missing 'argument'")
                })?;
                Ok(format!("{}.first()", self.emit(argument, false)?))
            }
            Some("Count") => {
                let argument = payload.get("argument").ok_or_else(|| {
                    TranspileError::new("Count", "malformed payload: missing 'argument'")
                })?;
                if !self.is_collection(argument) {
                    return Err(TranspileError::new(
                        "Count",
                        "count requires a collection field",
                    ));
                }
                Ok(format!("{}.len()", self.emit(argument, false)?))
            }
            Some("Flatten") => {
                let argument = payload.get("argument").ok_or_else(|| {
                    TranspileError::new("Flatten", "malformed payload: missing 'argument'")
                })?;
                Ok(format!(
                    "{}.iter().flatten().collect::<Vec<_>>()",
                    self.emit(argument, false)?
                ))
            }
            Some("Distinct") => {
                let argument = payload.get("argument").ok_or_else(|| {
                    TranspileError::new("Distinct", "malformed payload: missing 'argument'")
                })?;
                Ok(format!(
                    "{}.iter().collect::<std::collections::BTreeSet<_>>()",
                    self.emit(argument, false)?
                ))
            }
            Some("Reverse") => {
                let argument = payload.get("argument").ok_or_else(|| {
                    TranspileError::new("Reverse", "malformed payload: missing 'argument'")
                })?;
                Ok(format!(
                    "{}.iter().rev().collect::<Vec<_>>()",
                    self.emit(argument, false)?
                ))
            }
            Some("First") => {
                let argument = payload.get("argument").ok_or_else(|| {
                    TranspileError::new("First", "malformed payload: missing 'argument'")
                })?;
                Ok(format!("{}.first()", self.emit(argument, false)?))
            }
            Some("Last") => {
                let argument = payload.get("argument").ok_or_else(|| {
                    TranspileError::new("Last", "malformed payload: missing 'argument'")
                })?;
                Ok(format!("{}.last()", self.emit(argument, false)?))
            }
            Some("Sum") => self.emit_aggregate(payload, "sum"),
            Some("Min") => self.emit_aggregate(payload, "min"),
            Some("Max") => self.emit_aggregate(payload, "max"),
            Some("Join") => self.emit_join(payload),
            Some("Switch") => self.emit_switch(payload),
            Some("Conditional") => self.emit_conditional(payload),
            Some("ToString") => self.emit_cast(payload, ".to_string()"),
            Some("ToNumber") => self.emit_cast(payload, ".parse::<f64>().ok()"),
            Some("ToInt") => self.emit_cast(payload, ".parse::<i64>().ok()"),
            Some("ToDate" | "ToDateTime" | "ToZonedDateTime" | "ToTime") => {
                Err(TranspileError::new(
                    kind.unwrap_or_default(),
                    date_time_gap(kind.unwrap_or_default()),
                ))
            }
            Some("ToEnum") => {
                if payload.get("argument").is_none() {
                    return Err(TranspileError::new(
                        "ToEnum",
                        "malformed payload: missing 'argument'",
                    ));
                }
                Err(TranspileError::new(
                    "ToEnum",
                    "to-enum needs codelist knowledge not carried by ExprContext (documented gap)",
                ))
            }
            Some("Filter") => self.emit_functional(payload, "filter"),
            Some("Map") => self.emit_functional(payload, "map"),
            Some("Reduce") => {
                if payload.get("argument").is_none() {
                    return Err(TranspileError::new(
                        "Reduce",
                        "malformed payload: missing 'argument'",
                    ));
                }
                Err(TranspileError::new("Reduce", reduce_gap()))
            }
            Some("Sort") => {
                if payload.get("argument").is_none() {
                    return Err(TranspileError::new(
                        "Sort",
                        "malformed payload: missing 'argument'",
                    ));
                }
                Err(TranspileError::new(
                    "Sort",
                    "sort has no std expression form (sort_by needs statement form; documented gap)",
                ))
            }
            Some("Then") => self.emit_then(payload),
            Some("WithMeta") => {
                let argument = payload.get("argument").ok_or_else(|| {
                    TranspileError::new("WithMeta", "malformed payload: missing 'argument'")
                })?;
                Ok(format!(
                    "{} /* meta dropped */",
                    self.emit(argument, false)?
                ))
            }
            Some("OnlyExists") => self.emit_only_exists(payload),
            Some("As") => Err(TranspileError::new(
                "As",
                "type-system cast 'as' needs type knowledge (documented stub)",
            )),
            Some("AsKey") => Err(TranspileError::new(
                "AsKey",
                "map-key semantics are out of scope (documented stub)",
            )),
            Some("OneOf") => Err(TranspileError::new(
                "OneOf",
                "choice-variant membership needs enum/choice knowledge (variant representation deferred)",
            )),
            Some("Choice") => Err(TranspileError::new(
                "Choice",
                "choice-variant representation is deferred (documented stub)",
            )),
            Some("Constructor") => Err(TranspileError::new(
                "Constructor",
                "constructor lowering needs DTO struct knowledge (documented stub)",
            )),
            Some(other) => Err(TranspileError::new(
                other,
                "expression family is not part of the transpiler surface",
            )),
            None => Err(TranspileError::new("Unknown", "payload has no 'kind' tag")),
        }
    }

    /// `SymbolReference` → field access on the receiver, resolved through
    /// the lambda frames first: the innermost frame whose parameters
    /// contain the symbol binds it (the parameter is emitted bare).
    /// Function-like refs (`explicit == true` or non-empty `args`) are
    /// unsupported; bare access to a known-optional field is refused unless
    /// sanctioned by exists/absent/default/only-exists.
    fn emit_symbol_reference(
        &mut self,
        payload: &serde_json::Value,
        allow_optional_root: bool,
    ) -> Result<String, TranspileError> {
        let symbol = payload
            .get("symbol")
            .and_then(|s| s.as_str())
            .ok_or_else(|| {
                TranspileError::new("SymbolReference", "malformed payload: missing 'symbol'")
            })?;
        let explicit = payload
            .get("explicit")
            .and_then(|e| e.as_bool())
            .unwrap_or(false);
        let has_args = payload
            .get("args")
            .and_then(|a| a.as_array())
            .is_some_and(|a| !a.is_empty());
        if explicit || has_args {
            return Err(TranspileError::new(
                "SymbolReference",
                format!("function-like reference '{symbol}(...)' is not a field access"),
            ));
        }
        // Lambda frames bind innermost-first.
        let field = escape_rust_keyword(&to_snake_case(symbol));
        for frame in self.frames.iter().rev() {
            if frame.params.iter().any(|p| p == &field) {
                return Ok(field);
            }
        }
        if !allow_optional_root && self.ctx.optional_fields.contains(&field) {
            return Err(optional_field_error(&field));
        }
        Ok(format!("{}.{}", self.ctx.receiver, field))
    }

    /// The fragment the implicit variable `item` binds to: the innermost
    /// frame's implicit binding, else the receiver (top level). Under
    /// explicit lambda parameters the implicit variable is shadowed —
    /// refused.
    fn implicit_binding(&self) -> Result<String, TranspileError> {
        match self.frames.last() {
            None => Ok(self.ctx.receiver.to_string()),
            Some(frame) => match &frame.implicit {
                Some(binding) => Ok(binding.clone()),
                None => Err(TranspileError::new(
                    "ImplicitVariable",
                    "implicit variable 'item' is shadowed by explicit lambda parameters",
                )),
            },
        }
    }

    /// `exists`: modifier `none` → `.is_some()`; `single`/`multiple` →
    /// collection-length checks gated on a collection root.
    fn emit_exists(&mut self, payload: &serde_json::Value) -> Result<String, TranspileError> {
        let modifier = payload
            .get("modifier")
            .and_then(|m| m.as_str())
            .unwrap_or("none");
        let argument = payload.get("argument").ok_or_else(|| {
            TranspileError::new("Exists", "malformed payload: missing 'argument'")
        })?;
        match modifier {
            "none" => Ok(format!("{}.is_some()", self.emit(argument, true)?)),
            "single" | "multiple" => {
                if !self.is_collection(argument) {
                    return Err(TranspileError::new(
                        "Exists",
                        format!("exists modifier '{modifier}' requires a collection field"),
                    ));
                }
                let arg = self.emit(argument, true)?;
                Ok(if modifier == "single" {
                    format!("{arg}.len() == 1")
                } else {
                    format!("{arg}.len() >= 2")
                })
            }
            other => Err(TranspileError::new(
                "Exists",
                format!("unknown exists modifier '{other}'"),
            )),
        }
    }

    /// `Sum` / `Min` / `Max` (bare form): gated on a numeric root; the
    /// integer subset picks the element type. Lambda-carrying Min/Max is
    /// refused (documented gap).
    fn emit_aggregate(
        &mut self,
        payload: &serde_json::Value,
        op: &str,
    ) -> Result<String, TranspileError> {
        let capitalized = kind_for(op);
        let argument = payload.get("argument").ok_or_else(|| {
            TranspileError::new(capitalized, "malformed payload: missing 'argument'")
        })?;
        if payload.get("function").is_some_and(|f| !f.is_null()) {
            return Err(TranspileError::new(
                capitalized,
                format!(
                    "{op} with an inline function is a documented gap (ambiguous min-by semantics)"
                ),
            ));
        }
        let Some(elem) = self.numeric_elem_type(argument) else {
            return Err(TranspileError::new(
                capitalized,
                format!("{op} requires numeric element type"),
            ));
        };
        let arg = self.emit(argument, false)?;
        Ok(match op {
            "sum" => format!("{arg}.iter().sum::<{elem}>()"),
            "min" => {
                if elem == "i64" {
                    format!("{arg}.iter().copied().fold(i64::MAX, i64::min)")
                } else {
                    format!("{arg}.iter().copied().fold(f64::INFINITY, f64::min)")
                }
            }
            _ => {
                if elem == "i64" {
                    format!("{arg}.iter().copied().fold(i64::MIN, i64::max)")
                } else {
                    format!("{arg}.iter().copied().fold(f64::NEG_INFINITY, f64::max)")
                }
            }
        })
    }

    /// `join`: collection-gated; the separator is the transpiled right
    /// operand when explicit, else the literal `", "` (module docs).
    fn emit_join(&mut self, payload: &serde_json::Value) -> Result<String, TranspileError> {
        let left = payload
            .get("left")
            .ok_or_else(|| TranspileError::new("Join", "malformed payload: missing 'left'"))?;
        let right = payload
            .get("right")
            .ok_or_else(|| TranspileError::new("Join", "malformed payload: missing 'right'"))?;
        let explicit = payload
            .get("explicitSeparator")
            .and_then(|e| e.as_bool())
            .unwrap_or(false);
        if !self.is_collection(left) {
            return Err(TranspileError::new(
                "Join",
                "join requires a collection field",
            ));
        }
        let l = self.emit(left, false)?;
        let sep = if explicit {
            self.emit(right, false)?
        } else {
            "\", \"".to_string()
        };
        Ok(format!("{l}.join({sep})"))
    }

    /// `switch` → if-else-chain expression. Requires exactly one default
    /// case; reference guards are refused (module docs).
    fn emit_switch(&mut self, payload: &serde_json::Value) -> Result<String, TranspileError> {
        let argument = payload.get("argument").ok_or_else(|| {
            TranspileError::new("Switch", "malformed payload: missing 'argument'")
        })?;
        let cases = payload
            .get("cases")
            .and_then(|c| c.as_array())
            .ok_or_else(|| TranspileError::new("Switch", "malformed payload: missing 'cases'"))?;
        let defaults: Vec<&serde_json::Value> = cases
            .iter()
            .filter(|c| c.get("default").and_then(|d| d.as_bool()).unwrap_or(false))
            .collect();
        if defaults.len() != 1 {
            return Err(TranspileError::new(
                "Switch",
                "switch requires exactly one default case",
            ));
        }
        let arg = self.emit(argument, false)?;
        let arg = wrap_operand(argument, arg);
        let default_expr = self.emit(&defaults[0]["expression"], false)?;
        let mut out = String::new();
        for case in cases {
            if case.get("default").is_some() {
                continue;
            }
            let expr = self.emit(&case["expression"], false)?;
            let cond = match case.get("guard") {
                Some(guard) if guard.get("kind").and_then(|k| k.as_str()) == Some("Literal") => {
                    let value = guard.get("value").ok_or_else(|| {
                        TranspileError::new(
                            "Switch",
                            "malformed payload: literal guard missing 'value'",
                        )
                    })?;
                    format!("{arg} == {}", self.emit(value, false)?)
                }
                Some(guard) if guard.get("kind").and_then(|k| k.as_str()) == Some("Reference") => {
                    let target = guard
                        .get("target")
                        .and_then(|t| t.as_str())
                        .unwrap_or_default();
                    return Err(TranspileError::new(
                        "Switch",
                        format!(
                            "reference guard '{target}' needs enum/choice type knowledge (documented gap)"
                        ),
                    ));
                }
                _ => {
                    return Err(TranspileError::new(
                        "Switch",
                        "malformed payload: case missing 'guard'",
                    ));
                }
            };
            if out.is_empty() {
                out = format!("if {cond} {{ {expr} }}");
            } else {
                out = format!("{out} else if {cond} {{ {expr} }}");
            }
        }
        out = format!("{out} else {{ {default_expr} }}");
        Ok(out)
    }

    /// `if c then a else b` → if-else expression. `full == false` still
    /// carries the generated empty-list else in the payload; it is emitted
    /// faithfully (module docs).
    fn emit_conditional(&mut self, payload: &serde_json::Value) -> Result<String, TranspileError> {
        let cond = payload
            .get("if")
            .ok_or_else(|| TranspileError::new("Conditional", "malformed payload: missing 'if'"))?;
        let then = payload.get("then").ok_or_else(|| {
            TranspileError::new("Conditional", "malformed payload: missing 'then'")
        })?;
        let els = payload.get("else").ok_or_else(|| {
            TranspileError::new("Conditional", "malformed payload: missing 'else'")
        })?;
        Ok(format!(
            "if {} {{ {} }} else {{ {} }}",
            wrap_operand(cond, self.emit(cond, false)?),
            self.emit(then, false)?,
            self.emit(els, false)?
        ))
    }

    /// Cast family: append the cast suffix to the argument fragment.
    fn emit_cast(
        &mut self,
        payload: &serde_json::Value,
        suffix: &str,
    ) -> Result<String, TranspileError> {
        let argument = payload.get("argument").ok_or_else(|| {
            TranspileError::new(
                payload
                    .get("kind")
                    .and_then(|k| k.as_str())
                    .unwrap_or_default(),
                "malformed payload: missing 'argument'",
            )
        })?;
        Ok(format!("{}{}", self.emit(argument, false)?, suffix))
    }

    /// `filter` / `extract`: emit the argument, bind the lambda frame, emit
    /// the body, and terminate the chain with `collect::<Vec<_>>()` so the
    /// fragment stays a composable collection (module docs).
    fn emit_functional(
        &mut self,
        payload: &serde_json::Value,
        op: &str,
    ) -> Result<String, TranspileError> {
        let kind = kind_for(op);
        let argument = payload
            .get("argument")
            .ok_or_else(|| TranspileError::new(kind, "malformed payload: missing 'argument'"))?;
        let function = payload
            .get("function")
            .ok_or_else(|| TranspileError::new(kind, "malformed payload: missing 'function'"))?;
        if function.is_null() {
            return Err(TranspileError::new(
                kind,
                format!("function-less '{op}' payloads carry no body (documented)"),
            ));
        }
        let params = function
            .get("parameters")
            .and_then(|p| p.as_array())
            .ok_or_else(|| {
                TranspileError::new(kind, "malformed payload: function missing 'parameters'")
            })?;
        let body = function.get("body").ok_or_else(|| {
            TranspileError::new(kind, "malformed payload: function missing 'body'")
        })?;
        if params.len() > 1 {
            return Err(TranspileError::new(
                kind,
                format!("multi-parameter '{op}' functions are unsupported (map-like bindings)"),
            ));
        }
        let param = match params.first() {
            None => "item".to_string(),
            Some(p) => p
                .as_str()
                .map(|p| escape_rust_keyword(&to_snake_case(p)))
                .ok_or_else(|| {
                    TranspileError::new(kind, "malformed payload: parameter is not a string")
                })?,
        };
        let arg = self.emit(argument, false)?;
        self.frames.push(Frame {
            params: vec![param.clone()],
            // Implicit-parameter form: `item` IS the element binding.
            // Explicit parameters shadow the implicit variable.
            implicit: if params.is_empty() {
                Some("item".to_string())
            } else {
                None
            },
        });
        let body_frag = match self.emit(body, false) {
            Ok(frag) => frag,
            Err(e) => {
                self.frames.pop();
                return Err(e);
            }
        };
        self.frames.pop();
        Ok(format!(
            "{arg}.iter().{op}(|{param}| {body_frag}).collect::<Vec<_>>()"
        ))
    }

    /// `a then b`: passthrough for `function: null`; call form for a
    /// named-ref function; otherwise the body is an implicit inline lambda
    /// whose implicit variable binds the argument fragment (module docs).
    fn emit_then(&mut self, payload: &serde_json::Value) -> Result<String, TranspileError> {
        let argument = payload
            .get("argument")
            .ok_or_else(|| TranspileError::new("Then", "malformed payload: missing 'argument'"))?;
        let function = payload
            .get("function")
            .ok_or_else(|| TranspileError::new("Then", "malformed payload: missing 'function'"))?;
        if function.is_null() {
            return self.emit(argument, false);
        }
        let params = function
            .get("parameters")
            .and_then(|p| p.as_array())
            .ok_or_else(|| {
                TranspileError::new("Then", "malformed payload: function missing 'parameters'")
            })?;
        let body = function.get("body").ok_or_else(|| {
            TranspileError::new("Then", "malformed payload: function missing 'body'")
        })?;
        if !params.is_empty() {
            return Err(TranspileError::new(
                "Then",
                "parameterized then-functions are unsupported (documented)",
            ));
        }
        // Named-ref form: the body is an explicit argument-less symbol
        // reference → plain call on the transpiled argument.
        if body.get("kind").and_then(|k| k.as_str()) == Some("SymbolReference")
            && body
                .get("explicit")
                .and_then(|e| e.as_bool())
                .unwrap_or(false)
            && body
                .get("args")
                .and_then(|a| a.as_array())
                .is_some_and(|a| a.is_empty())
        {
            let f = body.get("symbol").and_then(|s| s.as_str()).ok_or_else(|| {
                TranspileError::new("Then", "malformed payload: function missing 'symbol'")
            })?;
            let arg = self.emit(argument, false)?;
            return Ok(format!("{f}({arg})"));
        }
        let arg = self.emit(argument, false)?;
        let binding = if wraps_as_operand(argument) {
            format!("({arg})")
        } else {
            arg
        };
        // No matchable parameters: the only binding is the implicit
        // variable, which carries the argument FRAGMENT.
        self.frames.push(Frame {
            params: Vec::new(),
            implicit: Some(binding),
        });
        let out = self.emit(body, false);
        self.frames.pop();
        out
    }

    /// `only exists` → conjunction of per-argument `.is_some()`, each with
    /// the exists-sanction. Exclusivity is a documented gap (module docs).
    fn emit_only_exists(&mut self, payload: &serde_json::Value) -> Result<String, TranspileError> {
        let args = payload
            .get("args")
            .and_then(|a| a.as_array())
            .ok_or_else(|| {
                TranspileError::new("OnlyExists", "malformed payload: missing 'args'")
            })?;
        if args.is_empty() {
            return Err(TranspileError::new(
                "OnlyExists",
                "only-exists without arguments",
            ));
        }
        let mut parts = Vec::with_capacity(args.len());
        for arg in args {
            parts.push(format!("{}.is_some()", self.emit(arg, true)?));
        }
        Ok(parts.join(" && "))
    }

    /// Binary operations: arithmetic, logical, equality, comparison, plus
    /// the word-binaries `contains` / `disjoint` / `default`.
    fn emit_binary(&mut self, payload: &serde_json::Value) -> Result<String, TranspileError> {
        if payload
            .get("cardMod")
            .and_then(|c| c.as_str())
            .is_some_and(|m| m != "none")
        {
            return Err(TranspileError::new(
                "Binary",
                "cardinality comparison (any/all =) requires collection knowledge (documented gap)",
            ));
        }
        let op = payload
            .get("op")
            .and_then(|o| o.as_str())
            .ok_or_else(|| TranspileError::new("Binary", "malformed payload: missing 'op'"))?;
        let left = payload
            .get("left")
            .ok_or_else(|| TranspileError::new("Binary", "malformed payload: missing 'left'"))?;
        let right = payload
            .get("right")
            .ok_or_else(|| TranspileError::new("Binary", "malformed payload: missing 'right'"))?;
        let rust_op = match op {
            "+" => "+",
            "-" => "-",
            "*" => "*",
            "/" => "/",
            "and" => "&&",
            "or" => "||",
            "=" => "==",
            "<>" => "!=",
            ">" => ">",
            "<" => "<",
            ">=" => ">=",
            "<=" => "<=",
            "contains" => {
                if !self.is_collection(left) {
                    return Err(TranspileError::new(
                        "Binary",
                        "contains requires a collection receiver (string contains needs type knowledge)",
                    ));
                }
                let l = self.emit(left, false)?;
                let r = self.emit(right, false)?;
                return Ok(format!("{l}.contains(&{})", wrap_operand(right, r)));
            }
            "disjoint" => {
                if !self.is_collection(left) {
                    return Err(TranspileError::new(
                        "Binary",
                        "disjoint requires a collection receiver",
                    ));
                }
                let l = self.emit(left, false)?;
                let r = self.emit(right, false)?;
                return Ok(format!(
                    "{l}.iter().all(|x| !{}.contains(x))",
                    wrap_operand(right, r)
                ));
            }
            "default" => {
                // Sanctioned optional access: `default` IS the sanctioned
                // way to read a bare optional field.
                let l = self.emit(left, true)?;
                let r = self.emit(right, false)?;
                return Ok(format!("{l}.unwrap_or({})", wrap_operand(right, r)));
            }
            other => {
                return Err(TranspileError::new(
                    "Binary",
                    format!("unknown operator '{other}'"),
                ));
            }
        };
        Ok(format!(
            "{} {rust_op} {}",
            self.wrap_nested_binary(left)?,
            self.wrap_nested_binary(right)?
        ))
    }

    /// Emit a binary operand, parenthesizing nested Binary/Conditional/
    /// Switch/Then subtrees so Rust precedence can never reinterpret the
    /// composition.
    fn wrap_nested_binary(
        &mut self,
        payload: &serde_json::Value,
    ) -> Result<String, TranspileError> {
        let code = self.emit(payload, false)?;
        Ok(wrap_operand(payload, code))
    }

    /// Whether the root of a receiver/argument chain is a known collection
    /// field.
    fn is_collection(&self, payload: &serde_json::Value) -> bool {
        root_symbol(payload).is_some_and(|root| self.ctx.collection_fields.contains(&root))
    }

    /// The element type of a numeric-typed root (`i64` / `f64`), or `None`
    /// when the root is not a known numeric field.
    fn numeric_elem_type(&self, payload: &serde_json::Value) -> Option<&'static str> {
        let root = root_symbol(payload)?;
        if !self.ctx.numeric_fields.contains(&root) {
            return None;
        }
        Some(if self.ctx.integer_fields.contains(&root) {
            "i64"
        } else {
            "f64"
        })
    }
}

/// The snake_case symbol at the root of a receiver/argument chain, if the
/// chain bottoms out in a plain field reference. Used for collection /
/// numeric knowledge gating: `prices filter [...] sum` roots at `prices`.
fn root_symbol(payload: &serde_json::Value) -> Option<String> {
    let kind = payload.get("kind").and_then(|k| k.as_str())?;
    match kind {
        "SymbolReference" => payload
            .get("symbol")
            .and_then(|s| s.as_str())
            .map(to_snake_case),
        "FeatureCall" | "DeepFeatureCall" => root_symbol(payload.get("receiver")?),
        // Postfix / functional / cast operations chain onto their argument.
        "Flatten" | "Distinct" | "Reverse" | "First" | "Last" | "Sum" | "Min" | "Max"
        | "OnlyElement" | "Count" | "Filter" | "Map" | "Reduce" | "Sort" | "Then" | "WithMeta"
        | "ToString" | "ToNumber" | "ToInt" | "ToDate" | "ToDateTime" | "ToZonedDateTime"
        | "ToTime" => root_symbol(payload.get("argument")?),
        _ => None,
    }
}

/// Kinds whose fragment must be parenthesized when composed as an operand —
/// their surface form can bind differently in Rust.
fn wraps_as_operand(payload: &serde_json::Value) -> bool {
    matches!(
        payload.get("kind").and_then(|k| k.as_str()),
        Some("Binary" | "Conditional" | "Switch" | "Then")
    )
}

fn wrap_operand(payload: &serde_json::Value, code: String) -> String {
    if wraps_as_operand(payload) {
        format!("({code})")
    } else {
        code
    }
}

fn kind_for(op: &str) -> &'static str {
    match op {
        "filter" => "Filter",
        "map" => "Map",
        "min" => "Min",
        "max" => "Max",
        "sum" => "Sum",
        _ => "Reduce",
    }
}

fn date_time_gap(kind: &str) -> String {
    format!(
        "{kind} needs a date-time library (chrono is absent from codegraph-generate by design; documented gap)"
    )
}

fn reduce_gap() -> String {
    "reduce has no init field in the sigil serialization: the a,b[body] form needs \
     first-element-as-init (Iterator::reduce yields a non-composable Option) and the \
     [init] bracket form is stored as an empty-parameter inline function — documented gap"
        .to_string()
}

fn optional_field_error(field: &str) -> TranspileError {
    TranspileError::new(
        "SymbolReference",
        format!("optional field '{field}' accessed bare (touch optionals via exists/absent)"),
    )
}

/// Re-wrap a bare-optional-field error raised for a feature-call receiver:
/// the optional rule is the same, but the call is the rejected node.
fn optional_receiver_error(call: &str, err: TranspileError) -> TranspileError {
    if err.kind == "SymbolReference" {
        if let Some(field) = err
            .detail
            .strip_prefix("optional field '")
            .and_then(|rest| rest.split('\'').next())
        {
            return TranspileError::new(
                call,
                format!("optional receiver requires exists-guard: '{field}'"),
            );
        }
    }
    err
}

/// A Rust string literal with `"` / `\` (and control-char) escaping.
fn string_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Caller-derived field knowledge for tests, mirroring what the
    /// generator builds from PropertyNodes.
    struct Sets {
        optional: HashSet<String>,
        collections: HashSet<String>,
        numeric: HashSet<String>,
        integer: HashSet<String>,
    }

    impl Sets {
        fn empty() -> Self {
            Self {
                optional: HashSet::new(),
                collections: HashSet::new(),
                numeric: HashSet::new(),
                integer: HashSet::new(),
            }
        }
        fn optional_fields(fields: &[&str]) -> Self {
            Self {
                optional: fields.iter().map(|f| f.to_string()).collect(),
                ..Self::empty()
            }
        }
        fn collections(fields: &[&str]) -> Self {
            Self {
                collections: fields.iter().map(|f| f.to_string()).collect(),
                ..Self::empty()
            }
        }
        fn numeric(numeric: &[&str], integer: &[&str]) -> Self {
            Self {
                numeric: numeric.iter().map(|f| f.to_string()).collect(),
                integer: integer.iter().map(|f| f.to_string()).collect(),
                ..Self::empty()
            }
        }
        fn ctx(&self) -> ExprContext<'_> {
            ExprContext {
                receiver: "dto",
                optional_fields: &self.optional,
                collection_fields: &self.collections,
                numeric_fields: &self.numeric,
                integer_fields: &self.integer,
            }
        }
    }

    fn t(payload: serde_json::Value) -> String {
        t_in(payload, &Sets::empty())
    }

    fn t_in(payload: serde_json::Value, sets: &Sets) -> String {
        transpile(&payload, &sets.ctx()).expect("transpiles")
    }

    fn e(payload: serde_json::Value) -> TranspileError {
        e_in(payload, &Sets::empty())
    }

    fn e_in(payload: serde_json::Value, sets: &Sets) -> TranspileError {
        transpile(&payload, &sets.ctx()).expect_err("rejected")
    }

    fn sym(s: &str) -> serde_json::Value {
        json!({"kind":"SymbolReference","symbol":s,"explicit":false,"args":[]})
    }

    fn int(n: &str) -> serde_json::Value {
        json!({"kind":"Int","text":n})
    }

    // ── Literals ─────────────────────────────────────────────────────────

    #[test]
    fn boolean_literals() {
        assert_eq!(t(json!({"kind":"Boolean","value":true})), "true");
        assert_eq!(t(json!({"kind":"Boolean","value":false})), "false");
    }

    #[test]
    fn string_literal_escapes_quote_and_backslash() {
        assert_eq!(t(json!({"kind":"String","value":"plain"})), "\"plain\"");
        assert_eq!(
            t(json!({"kind":"String","value":"he said \"hi\" \\ ok"})),
            "\"he said \\\"hi\\\" \\\\ ok\""
        );
    }

    #[test]
    fn number_and_int_literals_are_verbatim() {
        assert_eq!(t(json!({"kind":"Number","text":"12.5"})), "12.5");
        assert_eq!(t(json!({"kind":"Number","text":"0.0"})), "0.0");
        assert_eq!(t(json!({"kind":"Int","text":"42"})), "42");
        assert_eq!(t(json!({"kind":"Int","text":"-3"})), "-3");
    }

    #[test]
    fn list_literal_becomes_vec() {
        assert_eq!(
            t(json!({"kind":"List","elements":[
                {"kind":"String","value":"a"},
                {"kind":"Int","text":"1"}
            ]})),
            "vec![\"a\", 1]"
        );
        assert_eq!(t(json!({"kind":"List","elements":[]})), "vec![]");
    }

    // ── Symbol references / implicit variable ────────────────────────────

    #[test]
    fn symbol_reference_becomes_field_access() {
        assert_eq!(t(sym("orderId")), "dto.order_id");
    }

    #[test]
    fn symbol_reference_escapes_rust_keywords() {
        assert_eq!(t(sym("type")), "dto.r#type");
    }

    #[test]
    fn optional_symbol_reference_is_refused() {
        let sets = Sets::optional_fields(&["memo"]);
        let err = e_in(sym("memo"), &sets);
        assert_eq!(err.kind, "SymbolReference");
        assert!(err.detail.contains("optional field 'memo'"), "{err}");
    }

    #[test]
    fn function_like_symbol_reference_is_unsupported() {
        let err = e(json!({"kind":"SymbolReference","symbol":"f","explicit":true,"args":[]}));
        assert_eq!(err.kind, "SymbolReference");
        assert!(err.detail.contains("function-like"), "{err}");

        let err =
            e(json!({"kind":"SymbolReference","symbol":"f","explicit":false,"args":[int("1")]}));
        assert_eq!(err.kind, "SymbolReference");
    }

    #[test]
    fn implicit_variable_is_the_receiver() {
        assert_eq!(t(json!({"kind":"ImplicitVariable"})), "dto");
    }

    // ── Feature calls ────────────────────────────────────────────────────

    #[test]
    fn feature_call_chain_on_required_fields() {
        assert_eq!(
            t(json!({"kind":"FeatureCall","receiver":
                json!({"kind":"FeatureCall","receiver": sym("counterparty"),
            "feature":"name"}),
            "feature":"upper"})),
            "dto.counterparty.name.upper"
        );
    }

    #[test]
    fn deep_feature_call_nests_the_same_way() {
        assert_eq!(
            t(json!({"kind":"DeepFeatureCall","receiver": sym("counterparty"),"feature":"name"})),
            "dto.counterparty.name"
        );
    }

    #[test]
    fn bare_projection_feature_is_unsupported() {
        let err = e(json!({"kind":"FeatureCall","receiver": sym("a"),"feature":null}));
        assert_eq!(err.kind, "FeatureCall");
        assert!(err.detail.contains("bare '->'"), "{err}");
    }

    #[test]
    fn optional_feature_call_receiver_is_refused() {
        let sets = Sets::optional_fields(&["counterparty"]);
        let err = transpile(
            &json!({"kind":"FeatureCall","receiver": sym("counterparty"),"feature":"name"}),
            &sets.ctx(),
        )
        .expect_err("optional receiver refused");
        assert_eq!(err.kind, "FeatureCall");
        assert!(
            err.detail
                .contains("optional receiver requires exists-guard"),
            "{err}"
        );
    }

    // ── Binary operations ────────────────────────────────────────────────

    #[test]
    fn arithmetic_comparison_logical_ops_map() {
        let ops = [
            ("+", "+"),
            ("-", "-"),
            ("*", "*"),
            ("/", "/"),
            ("and", "&&"),
            ("or", "||"),
            ("=", "=="),
            ("<>", "!="),
            (">", ">"),
            ("<", "<"),
            (">=", ">="),
            ("<=", "<="),
        ];
        for (rosetta, rust) in ops {
            let payload = json!({"kind":"Binary","op":rosetta,
                "left":sym("a"),"right":int("1")});
            assert_eq!(t(payload), format!("dto.a {rust} 1"), "op {rosetta}");
        }
    }

    #[test]
    fn nested_binaries_are_parenthesized_deterministically() {
        assert_eq!(
            t(json!({"kind":"Binary","op":"and",
                "left":{"kind":"Binary","op":">","left":sym("a"),"right":int("1")},
                "right":{"kind":"Binary","op":"<","left":sym("b"),"right":int("2")}})),
            "(dto.a > 1) && (dto.b < 2)"
        );
    }

    #[test]
    fn cardinality_modifier_comparison_is_unsupported() {
        let err = e(json!({"kind":"Binary","op":"=","cardMod":"any",
            "left":sym("a"),"right":int("1")}));
        assert_eq!(err.kind, "Binary");
        assert!(err.detail.contains("cardinality comparison"), "{err}");
    }

    #[test]
    fn card_mod_none_is_fine() {
        assert_eq!(
            t(json!({"kind":"Binary","op":"=","cardMod":"none",
                "left":sym("a"),"right":int("1")})),
            "dto.a == 1"
        );
    }

    // ── contains / disjoint / default (slice 2) ──────────────────────────

    #[test]
    fn contains_on_collection_field() {
        let sets = Sets::collections(&["aliases"]);
        assert_eq!(
            t_in(
                json!({"kind":"Binary","op":"contains","left":sym("aliases"),
                    "right":{"kind":"String","value":"sale"}}),
                &sets
            ),
            "dto.aliases.contains(&\"sale\")"
        );
    }

    #[test]
    fn contains_wraps_complex_right_operands() {
        let sets = Sets::collections(&["aliases"]);
        let payload = json!({"kind":"Binary","op":"contains","left":sym("aliases"),
            "right":{"kind":"Binary","op":"+","left":sym("a"),"right":sym("b")}});
        assert_eq!(
            t_in(payload, &sets),
            "dto.aliases.contains(&(dto.a + dto.b))"
        );
    }

    #[test]
    fn contains_on_non_collection_is_refused() {
        let err = e(json!({"kind":"Binary","op":"contains","left":sym("name"),
            "right":{"kind":"String","value":"x"}}));
        assert_eq!(err.kind, "Binary");
        assert!(err.detail.contains("collection receiver"), "{err}");
    }

    #[test]
    fn disjoint_on_collection_field() {
        let sets = Sets::collections(&["tags"]);
        assert_eq!(
            t_in(
                json!({"kind":"Binary","op":"disjoint","left":sym("tags"),
                    "right":{"kind":"List","elements":[{"kind":"String","value":"a"}]}}),
                &sets
            ),
            "dto.tags.iter().all(|x| !vec![\"a\"].contains(x))"
        );
    }

    #[test]
    fn default_sanctions_bare_optional_left() {
        let sets = Sets::optional_fields(&["memo"]);
        assert_eq!(
            t_in(
                json!({"kind":"Binary","op":"default","left":sym("memo"),
                    "right":{"kind":"String","value":"x"}}),
                &sets
            ),
            "dto.memo.unwrap_or(\"x\")"
        );
    }

    // ── exists / absent ──────────────────────────────────────────────────

    #[test]
    fn exists_sanctions_optional_fields() {
        let sets = Sets::optional_fields(&["settled_on"]);
        let out = t_in(
            json!({"kind":"Exists","modifier":"none","argument":sym("settledOn")}),
            &sets,
        );
        assert_eq!(out, "dto.settled_on.is_some()");
    }

    #[test]
    fn absent_sanctions_optional_fields() {
        let sets = Sets::optional_fields(&["memo"]);
        assert_eq!(
            t_in(json!({"kind":"Absent","argument":sym("memo")}), &sets),
            "dto.memo.is_none()"
        );
    }

    #[test]
    fn exists_single_and_multiple_check_collection_length() {
        let sets = Sets::collections(&["lines"]);
        assert_eq!(
            t_in(
                json!({"kind":"Exists","modifier":"single","argument":sym("lines")}),
                &sets
            ),
            "dto.lines.len() == 1"
        );
        assert_eq!(
            t_in(
                json!({"kind":"Exists","modifier":"multiple","argument":sym("lines")}),
                &sets
            ),
            "dto.lines.len() >= 2"
        );
    }

    #[test]
    fn exists_modifiers_on_non_collections_are_refused() {
        for modifier in ["single", "multiple"] {
            let err = e(json!({"kind":"Exists","modifier":modifier,"argument":sym("price")}));
            assert_eq!(err.kind, "Exists");
            assert!(err.detail.contains("collection field"), "{err}");
        }
    }

    // ── join ─────────────────────────────────────────────────────────────

    #[test]
    fn join_with_explicit_separator() {
        let sets = Sets::collections(&["tags"]);
        assert_eq!(
            t_in(
                json!({"kind":"Join","left":sym("tags"),
                    "right":{"kind":"String","value":", "},"explicitSeparator":true}),
                &sets
            ),
            "dto.tags.join(\", \")"
        );
    }

    #[test]
    fn join_without_separator_defaults_to_comma_space() {
        let sets = Sets::collections(&["tags"]);
        assert_eq!(
            t_in(
                json!({"kind":"Join","left":sym("tags"),
                    "right":{"kind":"String","value":""},"explicitSeparator":false}),
                &sets
            ),
            "dto.tags.join(\", \")"
        );
    }

    #[test]
    fn join_on_non_collection_is_refused() {
        let err = e(json!({"kind":"Join","left":sym("name"),
            "right":{"kind":"String","value":", "},"explicitSeparator":true}));
        assert_eq!(err.kind, "Join");
        assert!(err.detail.contains("collection field"), "{err}");
    }

    // ── collection ops (fragment-style contract) ─────────────────────────

    #[test]
    fn flatten_distinct_reverse_first_last_chain_fragments() {
        assert_eq!(
            t(json!({"kind":"Flatten","argument":sym("rows")})),
            "dto.rows.iter().flatten().collect::<Vec<_>>()"
        );
        assert_eq!(
            t(json!({"kind":"Distinct","argument":sym("prices")})),
            "dto.prices.iter().collect::<std::collections::BTreeSet<_>>()"
        );
        assert_eq!(
            t(json!({"kind":"Reverse","argument":sym("prices")})),
            "dto.prices.iter().rev().collect::<Vec<_>>()"
        );
        assert_eq!(
            t(json!({"kind":"First","argument":sym("prices")})),
            "dto.prices.first()"
        );
        assert_eq!(
            t(json!({"kind":"Last","argument":sym("prices")})),
            "dto.prices.last()"
        );
    }

    #[test]
    fn collection_ops_compose_as_postfix_chain() {
        // prices distinct flatten reverse first last sum
        let mut payload = sym("prices");
        for kind in ["Distinct", "Flatten", "Reverse", "First", "Last"] {
            payload = json!({"kind":kind,"argument":payload});
        }
        let sets = Sets::numeric(&["prices"], &[]);
        assert_eq!(
            t_in(json!({"kind":"Sum","argument":payload}), &sets),
            "dto.prices.iter().collect::<std::collections::BTreeSet<_>>()\
             .iter().flatten().collect::<Vec<_>>()\
             .iter().rev().collect::<Vec<_>>().first().last().iter().sum::<f64>()"
        );
    }

    // ── aggregates ───────────────────────────────────────────────────────

    #[test]
    fn sum_picks_element_type_from_field_knowledge() {
        let float = Sets::numeric(&["prices"], &[]);
        assert_eq!(
            t_in(json!({"kind":"Sum","argument":sym("prices")}), &float),
            "dto.prices.iter().sum::<f64>()"
        );
        let integer = Sets::numeric(&["counts"], &["counts"]);
        assert_eq!(
            t_in(json!({"kind":"Sum","argument":sym("counts")}), &integer),
            "dto.counts.iter().sum::<i64>()"
        );
    }

    #[test]
    fn sum_on_non_numeric_is_refused() {
        let err = e(json!({"kind":"Sum","argument":sym("lines")}));
        assert_eq!(err.kind, "Sum");
        assert!(err.detail.contains("numeric element type"), "{err}");
    }

    #[test]
    fn min_max_fold_with_identity() {
        let float = Sets::numeric(&["prices"], &[]);
        assert_eq!(
            t_in(json!({"kind":"Min","argument":sym("prices")}), &float),
            "dto.prices.iter().copied().fold(f64::INFINITY, f64::min)"
        );
        assert_eq!(
            t_in(json!({"kind":"Max","argument":sym("prices")}), &float),
            "dto.prices.iter().copied().fold(f64::NEG_INFINITY, f64::max)"
        );
        let integer = Sets::numeric(&["counts"], &["counts"]);
        assert_eq!(
            t_in(json!({"kind":"Min","argument":sym("counts")}), &integer),
            "dto.counts.iter().copied().fold(i64::MAX, i64::min)"
        );
    }

    #[test]
    fn min_with_lambda_is_a_documented_gap() {
        let sets = Sets::numeric(&["prices"], &[]);
        let err = e_in(
            json!({"kind":"Min","argument":sym("prices"),"function":{
                "parameters":["p"],
                "body":{"kind":"Binary","op":">","left":sym("p"),"right":{"kind":"Number","text":"0.0"}}}}),
            &sets,
        );
        assert_eq!(err.kind, "Min");
        assert!(err.detail.contains("documented gap"), "{err}");
    }

    #[test]
    fn count_requires_a_collection_field() {
        let sets = Sets::collections(&["lines"]);
        assert_eq!(
            t_in(json!({"kind":"Count","argument":sym("lines")}), &sets),
            "dto.lines.len()"
        );
        let err = e(json!({"kind":"Count","argument":sym("price")}));
        assert_eq!(err.kind, "Count");
        assert!(err.detail.contains("collection field"), "{err}");
    }

    // ── filter / extract (lambda frames) ─────────────────────────────────

    #[test]
    fn filter_implicit_body_binds_item() {
        let sets = Sets::collections(&["lines"]);
        assert_eq!(
            t_in(
                json!({"kind":"Filter","argument":sym("lines"),"function":{
                    "parameters":[],
                    "body":{"kind":"Binary","op":"<","left":sym("quantity"),"right":int("10")}}}),
                &sets
            ),
            "dto.lines.iter().filter(|item| dto.quantity < 10).collect::<Vec<_>>()"
        );
    }

    #[test]
    fn filter_explicit_param_binds_by_name() {
        let sets = Sets::collections(&["lines"]);
        assert_eq!(
            t_in(
                json!({"kind":"Filter","argument":sym("lines"),"function":{
                    "parameters":["l"],
                    "body":{"kind":"Binary","op":"=","left":
                        json!({"kind":"FeatureCall","receiver":sym("l"),"feature":"itemId"}),
                    "right":{"kind":"String","value":"x"}}}}),
                &sets
            ),
            "dto.lines.iter().filter(|l| l.item_id == \"x\").collect::<Vec<_>>()"
        );
    }

    #[test]
    fn filter_param_shadows_receiver_field() {
        // A parameter named like a field binds the parameter (innermost
        // frame first), even when the field exists on the receiver.
        let sets = Sets::collections(&["lines"]);
        assert_eq!(
            t_in(
                json!({"kind":"Filter","argument":sym("lines"),"function":{
                    "parameters":["price"],
                    "body":{"kind":"Binary","op":">","left":sym("price"),"right":{"kind":"Number","text":"1.0"}}}}),
                &sets
            ),
            "dto.lines.iter().filter(|price| price > 1.0).collect::<Vec<_>>()"
        );
    }

    #[test]
    fn extract_maps_with_the_lambda_body() {
        let sets = Sets::collections(&["prices"]);
        assert_eq!(
            t_in(
                json!({"kind":"Map","argument":sym("prices"),"function":{
                    "parameters":["p"],"body":sym("p")}}),
                &sets
            ),
            "dto.prices.iter().map(|p| p).collect::<Vec<_>>()"
        );
    }

    #[test]
    fn filter_without_a_function_is_refused() {
        let sets = Sets::collections(&["lines"]);
        let err = e_in(
            json!({"kind":"Filter","argument":sym("lines"),"function":null}),
            &sets,
        );
        assert_eq!(err.kind, "Filter");
        assert!(err.detail.contains("no body"), "{err}");
    }

    #[test]
    fn multi_parameter_filter_is_refused() {
        let sets = Sets::collections(&["lines"]);
        let err = e_in(
            json!({"kind":"Filter","argument":sym("lines"),"function":{
                "parameters":["a","b"],"body":sym("a")}}),
            &sets,
        );
        assert_eq!(err.kind, "Filter");
        assert!(err.detail.contains("multi-parameter"), "{err}");
    }

    #[test]
    fn implicit_variable_is_refused_under_explicit_params() {
        let sets = Sets::collections(&["lines"]);
        let err = e_in(
            json!({"kind":"Filter","argument":sym("lines"),"function":{
                "parameters":["l"],"body":json!({"kind":"ImplicitVariable"})}}),
            &sets,
        );
        assert_eq!(err.kind, "ImplicitVariable");
    }

    // ── reduce / sort (documented gaps) ──────────────────────────────────

    #[test]
    fn reduce_is_a_documented_gap_in_both_forms() {
        let explicit = e(json!({"kind":"Reduce","argument":sym("prices"),"function":{
            "parameters":["a","b"],
            "body":{"kind":"Binary","op":"+","left":sym("a"),"right":sym("b")}}}));
        assert_eq!(explicit.kind, "Reduce");
        assert!(explicit.detail.contains("documented gap"), "{explicit}");

        let init = e(json!({"kind":"Reduce","argument":sym("prices"),"function":{
            "parameters":[],"body":{"kind":"List","elements":[{"kind":"Number","text":"0.0"}]}}}));
        assert_eq!(init.kind, "Reduce");
    }

    #[test]
    fn sort_is_a_documented_gap() {
        let err = e(json!({"kind":"Sort","argument":sym("prices"),"function":{
            "parameters":["a","b"],
            "body":{"kind":"Binary","op":"-","left":sym("a"),"right":sym("b")}}}));
        assert_eq!(err.kind, "Sort");
        assert!(err.detail.contains("no std expression form"), "{err}");
    }

    // ── switch / conditional ─────────────────────────────────────────────

    fn switch_case(guard: serde_json::Value, expr: serde_json::Value) -> serde_json::Value {
        json!({"guard":guard,"expression":expr})
    }

    #[test]
    fn switch_literal_guards_become_if_else_chain() {
        let payload = json!({"kind":"Switch","argument":sym("price"),"cases":[
            switch_case(json!({"kind":"Literal","value":int("1")}), json!({"kind":"String","value":"a"})),
            switch_case(json!({"kind":"Literal","value":{"kind":"Number","text":"2.5"}}),
                json!({"kind":"String","value":"b"})),
            json!({"default":true,"expression":{"kind":"String","value":"c"}}),
        ]});
        assert_eq!(
            t(payload),
            "if dto.price == 1 { \"a\" } else if dto.price == 2.5 { \"b\" } else { \"c\" }"
        );
    }

    #[test]
    fn switch_without_default_is_refused() {
        let payload = json!({"kind":"Switch","argument":sym("price"),"cases":[
            switch_case(json!({"kind":"Literal","value":int("1")}), json!({"kind":"Boolean","value":true})),
        ]});
        let err = e(payload);
        assert_eq!(err.kind, "Switch");
        assert!(err.detail.contains("exactly one default"), "{err}");
    }

    #[test]
    fn switch_reference_guard_is_refused() {
        let payload = json!({"kind":"Switch","argument":sym("status"),"cases":[
            switch_case(json!({"kind":"Reference","target":"TradeStatus"}),
                json!({"kind":"String","value":"x"})),
            json!({"default":true,"expression":{"kind":"String","value":"y"}}),
        ]});
        let err = e(payload);
        assert_eq!(err.kind, "Switch");
        assert!(err.detail.contains("reference guard"), "{err}");
    }

    #[test]
    fn switch_used_as_operand_is_parenthesized() {
        let switch = json!({"kind":"Switch","argument":sym("quantity"),"cases":[
            switch_case(json!({"kind":"Literal","value":int("1")}), json!({"kind":"Boolean","value":true})),
            json!({"default":true,"expression":{"kind":"Boolean","value":false}}),
        ]});
        let payload = json!({"kind":"Binary","op":"=","left":switch,
            "right":{"kind":"Boolean","value":true}});
        assert_eq!(
            t(payload),
            "(if dto.quantity == 1 { true } else { false }) == true"
        );
    }

    #[test]
    fn conditional_becomes_if_else_expression() {
        let payload = json!({"kind":"Conditional",
            "if":{"kind":"Binary","op":">","left":sym("price"),"right":{"kind":"Number","text":"1.0"}},
            "then":sym("prices"),
            "else":{"kind":"List","elements":[{"kind":"Number","text":"0.0"}]},
            "full":true});
        assert_eq!(
            t(payload),
            "if (dto.price > 1.0) { dto.prices } else { vec![0.0] }"
        );
    }

    #[test]
    fn conditional_without_else_emits_the_generated_empty_list() {
        let payload = json!({"kind":"Conditional",
            "if":{"kind":"Binary","op":">","left":sym("price"),"right":{"kind":"Number","text":"1.0"}},
            "then":sym("prices"),
            "else":{"kind":"List","elements":[]},
            "full":false});
        assert_eq!(
            t(payload),
            "if (dto.price > 1.0) { dto.prices } else { vec![] }"
        );
    }

    #[test]
    fn conditional_as_operand_is_parenthesized() {
        let conditional = json!({"kind":"Conditional",
            "if":{"kind":"Boolean","value":true},"then":int("1"),"else":int("2"),
            "full":true});
        let payload = json!({"kind":"Binary","op":"+","left":conditional,"right":int("3")});
        assert_eq!(t(payload), "(if true { 1 } else { 2 }) + 3");
    }

    // ── casts ────────────────────────────────────────────────────────────

    #[test]
    fn value_casts_append_their_suffix() {
        assert_eq!(
            t(json!({"kind":"ToString","argument":sym("tradeId")})),
            "dto.trade_id.to_string()"
        );
        assert_eq!(
            t(json!({"kind":"ToNumber","argument":sym("tradeId")})),
            "dto.trade_id.parse::<f64>().ok()"
        );
        assert_eq!(
            t(json!({"kind":"ToInt","argument":sym("tradeId")})),
            "dto.trade_id.parse::<i64>().ok()"
        );
    }

    #[test]
    fn date_time_casts_are_documented_gaps() {
        for kind in ["ToDate", "ToDateTime", "ToZonedDateTime", "ToTime"] {
            let err = e(json!({"kind":kind,"argument":sym("tradeId")}));
            assert_eq!(err.kind, kind);
            assert!(err.detail.contains("date-time library"), "{err}");
        }
    }

    #[test]
    fn to_enum_is_a_documented_gap() {
        let err = e(json!({"kind":"ToEnum","enumeration":"TradeStatus","argument":sym("tradeId")}));
        assert_eq!(err.kind, "ToEnum");
        assert!(err.detail.contains("codelist knowledge"), "{err}");
    }

    // ── then ─────────────────────────────────────────────────────────────

    #[test]
    fn then_with_null_function_passes_through() {
        assert_eq!(
            t(json!({"kind":"Then","argument":sym("prices"),"function":null})),
            "dto.prices"
        );
    }

    #[test]
    fn then_named_ref_function_becomes_a_call() {
        assert_eq!(
            t(json!({"kind":"Then","argument":sym("price"),"function":{
                "parameters":[],
                "body":{"kind":"SymbolReference","symbol":"round","explicit":true,"args":[]}}})),
            "round(dto.price)"
        );
    }

    #[test]
    fn then_implicit_body_binds_the_argument_fragment() {
        // prices extract p [p] then item — the then-body's implicit variable
        // binds the extract fragment.
        let extract = json!({"kind":"Map","argument":sym("prices"),"function":{
            "parameters":["p"],"body":sym("p")}});
        let sets = Sets::collections(&["prices"]);
        assert_eq!(
            t_in(
                json!({"kind":"Then","argument":extract,"function":{
                    "parameters":[],"body":json!({"kind":"ImplicitVariable"})}}),
                &sets
            ),
            "dto.prices.iter().map(|p| p).collect::<Vec<_>>()"
        );
    }

    #[test]
    fn then_with_parameters_is_refused() {
        let err = e(json!({"kind":"Then","argument":sym("price"),"function":{
            "parameters":["p"],"body":sym("p")}}));
        assert_eq!(err.kind, "Then");
        assert!(err.detail.contains("parameterized"), "{err}");
    }

    // ── slice 3 stubs ────────────────────────────────────────────────────

    #[test]
    fn with_meta_passes_through_with_a_dropped_marker() {
        assert_eq!(
            t(json!({"kind":"WithMeta","argument":sym("price"),"entries":[
                {"key":"scheme","value":{"kind":"String","value":"x"}}]})),
            "dto.price /* meta dropped */"
        );
    }

    #[test]
    fn only_exists_is_a_conjunction_of_exists() {
        let sets = Sets::optional_fields(&["price", "quantity"]);
        assert_eq!(
            t_in(
                json!({"kind":"OnlyExists","args":[sym("price"), sym("quantity")],
                    "parentheses":true}),
                &sets
            ),
            "dto.price.is_some() && dto.quantity.is_some()"
        );
    }

    #[test]
    fn only_exists_without_arguments_is_refused() {
        let err = e(json!({"kind":"OnlyExists","args":[],"parentheses":false}));
        assert_eq!(err.kind, "OnlyExists");
    }

    #[test]
    fn type_system_stubs_carry_their_kind() {
        for (family, payload) in [
            ("As", json!({"kind":"As","type":"Foo","argument":sym("a")})),
            ("AsKey", json!({"kind":"AsKey","argument":sym("a")})),
            ("OneOf", json!({"kind":"OneOf","argument":sym("a")})),
            (
                "Choice",
                json!({"kind":"Choice","necessity":"optional","attributes":["x"],"argument":sym("a")}),
            ),
            (
                "Constructor",
                json!({"kind":"Constructor","type":{"name":"Foo","arguments":[]},
                "values":[],"implicitEmpty":false}),
            ),
        ] {
            let err = e(payload);
            assert_eq!(err.kind, family, "{family}");
        }
    }

    // ── malformed payloads carry their family kind ────────────────────────

    #[test]
    fn unsupported_families_carry_the_kind_string() {
        let families = [
            "Join",
            "Conditional",
            "Switch",
            "WithMeta",
            "As",
            "Then",
            "Filter",
            "Map",
            "Reduce",
            "Sort",
            "Min",
            "Max",
            "Flatten",
            "Distinct",
            "Reverse",
            "First",
            "Last",
            "Sum",
            "AsKey",
            "OneOf",
            "Choice",
            "Constructor",
            "OnlyExists",
            "ToString",
            "ToDate",
        ];
        for family in families {
            let err = e(json!({"kind":family}));
            assert_eq!(err.kind, family, "{family}");
        }
    }

    #[test]
    fn missing_kind_tag_is_reported() {
        let err = e(json!({"nope":true}));
        assert_eq!(err.kind, "Unknown");
    }

    #[test]
    fn unknown_kind_is_reported() {
        let err = e(json!({"kind":"Mystery","argument":sym("a")}));
        assert_eq!(err.kind, "Mystery");
    }

    #[test]
    fn display_is_marker_friendly() {
        let err = TranspileError::new("Switch", "someday");
        assert_eq!(err.to_string(), "unsupported expression (Switch): someday");
    }
}
