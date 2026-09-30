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
//! - `enum_types` — enum TYPE names (Pascal, e.g. `PositionStatusEnum`;
//!   from codelist-reference properties via
//!   `codelist_enum_name_from_ref`, the same helper dto.rs uses to pick
//!   the `GeneratedEnum` field type). Gates the guarded-optional family
//!   (issue #283 slice b): enum-literal lowering and enum equality.
//!
//! # Name resolution order (issue #283)
//!
//! A bare `SymbolReference` (and a `FeatureCall` receiver) resolves
//! through four tiers, first match wins:
//!
//! 1. **locals** — the lambda binding frames (and `transpile_scoped`'s
//!    function-body locals): the symbol is emitted bare;
//! 2. **receiver fields** — the snake_cased symbol is one of the known
//!    field sets (`optional` / `collection` / `numeric` / `integer`):
//!    emitted as a receiver-field access (with the bare-optional
//!    sanction rule);
//! 3. **enum type names** — the raw symbol is in `enum_types` and is NOT
//!    a known field (fields win ties): an enum-namespace reference. As a
//!    `FeatureCall` receiver it lowers to a qualified variant literal
//!    (`PositionStatusEnum -> Closed` → `PositionStatusEnum::Closed`);
//!    bare, outside a qualified literal, it is REFUSED (a type name is
//!    not a value — and real CDM overwhelmingly writes the qualified
//!    form; bare variants are sigil-rejected E0101 per the spike).
//! 4. **unknown** — emitted as a receiver-field access (unknown-symbol
//!    handling is the caller's concern, the documented untyped policy).
//!
//! # Guarded-optional lowering (issue #283 slice b)
//!
//! - **Deep-chain exists/absent** — `Exists` / `Absent` / `OnlyExists`
//!   arguments may be `FeatureCall`/`DeepFeatureCall` chains, not just
//!   bare symbols. A chain through an OPTIONAL root lowers to the
//!   canonical map shape (ONE shape, deterministic):
//!   `exists(primitiveInstruction -> execution)` →
//!   `dto.primitive_instruction.as_ref().map(|v| v.execution.is_some()).unwrap_or(false)`,
//!   `… absent` → `dto.primitive_instruction.as_ref().map(|v| v.execution.is_none()).unwrap_or(true)`
//!   (a `None` root means the whole path is absent). A REQUIRED root
//!   stays plain dot access (`dto.a.b.is_some()`); a local root (lambda
//!   frame / scoped local) emits the plain path over the local. Only the
//!   chain ROOT's optionality is known (`ExprContext` carries the owner
//!   entity's fields) — deeper hops emit plain access, so a genuinely
//!   optional deeper hop is a downstream compile rejection, the module's
//!   documented caveat class. `OnlyExists` lowers each argument through
//!   the same chain rule and &&-conjoins.
//! - **Optional enum equality** — `Binary` `=` / `<>` with one side an
//!   enum literal and the other a bare receiver field:
//!   optional field → `dto.position_state.as_ref() == Some(&PositionStatusEnum::Closed)`
//!   (`<>` → `!=`); required field → plain `dto.intent == Lit`. A
//!   field-vs-field comparison (neither side a literal) is NOT handled
//!   here: a bare optional field keeps its refusal, so optional
//!   field-vs-field stays unsupported (documented).
//! - **Value-position chains** (NOT under exists/absent/only-exists)
//!   through an optional receiver remain REFUSED — the slice-1 rule is
//!   unchanged: exists/absent (and now only-exists + literal equality)
//!   are the sanctioned ways to touch optionals.
//! - **Still refused**: `switch` with a `Reference` guard (enum/choice
//!   knowledge not carried by the payload — the IsOptionPayout shape),
//!   `Choice`/`OneOf` (choice-variant representation deferred).
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
//!   are `Unsupported` (type knowledge not carried in the payload). In
//!   bool position the default arm follows the [`Conditional` else
//!   rule](#boolean-position-issue-283): a `List` default lowers to
//!   `false` (issue #283). Switches REQUIRE an authored default, so the
//!   `full == false` generated-else shape cannot arise here — an
//!   empty-list default in bool position is an authored one, and it is
//!   indistinguishable from a non-bool default anyway.
//! - **`Conditional`** → `if cond { then } else { else }`. The `full`
//!   flag is informational only: a `full == false` conditional still
//!   carries its generated empty-list `else` in the payload. In VALUE
//!   position that else is emitted faithfully (`vec![]` — the documented
//!   slice-2 decision). In BOOL position the issue #283 rule applies:
//!   **a `List` else-arm lowers to `false`**; every other else kind is
//!   emitted normally (see "Boolean position" below).
//!
//! ## Boolean position (issue #283)
//!
//! [`transpile`] starts in BOOL position: its callers (condition
//! validations, eligibility rules) transpile boolean expressions, and a
//! `full == false` conditional's generated `vec![]` else is not bool —
//! the emitted crate could not compile (`if !(… else { vec![] }) {`).
//! The flag threads through the emitter exactly like operand
//! parenthesization and only changes behavior at `Conditional`/`Switch`:
//!
//! - **bool position** — the payload root; operands of `and`/`or` (a
//!   logical operation's operands are bool by definition); the `if`
//!   (condition) arm of a `Conditional`; the switch guard comparisons
//!   (bool-typed by construction — they emit `arg == literal` directly,
//!   so no threading happens there); the arguments of
//!   `exists`/`absent`/`only exists`.
//! - **value position** — arithmetic and comparison operands (a
//!   comparison is itself the bool thing; its operands are values),
//!   switch arguments and guard values (compared against guard
//!   literals), function-call arguments, lambda bodies, list elements,
//!   and the [`transpile_scoped`] root (function aliases/operations are
//!   values).
//! - **The rule** (`Conditional`): in bool position, if the else-arm's
//!   kind is `List`, emit `false`. A real authored `else: []` is
//!   indistinguishable from the generated one in the payload, and in
//!   bool position ANY list else is non-bool anyway — so the rule covers
//!   both. Any other else kind is emitted normally, and the then/else
//!   arms thread the conditional's own position (both branches of a
//!   bool-position conditional must be bool-shaped). In value position
//!   the else is emitted faithfully (`vec![]`), unchanged.
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
//!   `X.is_some()` (each argument keeps the exists-sanction; chain
//!   arguments lower through the guarded-optional chain rule — module
//!   docs, issue #283). DOCUMENTED
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

mod emitter;
mod error;
mod helpers;

use emitter::{Emitter, Frame};

pub use error::TranspileError;

#[cfg(test)]
mod tests;

/// Transpilation context: the receiver variable name (e.g. `"dto"`) plus
/// the caller-derived field knowledge (all snake_case DTO field names):
///
/// - `optional_fields`: `Option<T>` fields (`PropertyNode.is_nullable`);
/// - `collection_fields`: array properties (`PropertyNode.is_array`);
/// - `numeric_fields` / `integer_fields`: number- and integer-typed
///   properties (integer is a subset of numeric) — gates the aggregate
///   family and picks its element type;
/// - `enum_types`: enum TYPE names (Pascal, e.g. `PositionStatusEnum`) —
///   gates the enum-literal and enum-equality lowerings (issue #283).
pub struct ExprContext<'a> {
    pub receiver: &'a str,
    pub optional_fields: &'a HashSet<String>,
    pub collection_fields: &'a HashSet<String>,
    pub numeric_fields: &'a HashSet<String>,
    pub integer_fields: &'a HashSet<String>,
    pub enum_types: &'a HashSet<String>,
}

/// Transpile one `Expr::to_json` payload into a Rust expression fragment.
///
/// The payload ROOT is a BOOL position (issue #283): callers transpile
/// boolean expressions (condition validations, eligibility rules), so the
/// bool-position rules (module docs) apply from the root down.
pub fn transpile(
    payload: &serde_json::Value,
    ctx: &ExprContext<'_>,
) -> Result<String, TranspileError> {
    Emitter {
        ctx,
        frames: Vec::new(),
    }
    .emit(payload, false, true)
}

/// [`transpile`] with a scope of local variable names (issue #263, slice-4
/// addition for function bodies): a bare `SymbolReference` matching a
/// local emits the bare name instead of a receiver-field access — a
/// function's inputs, aliases, and output are locals of the generated
/// free function, not fields of a receiver struct. Purely additive:
/// existing `transpile` calls are unchanged. The ROOT is a VALUE position
/// (aliases and operations compute values; the bool-position rules do
/// not apply from here).
pub fn transpile_scoped(
    payload: &serde_json::Value,
    ctx: &ExprContext<'_>,
    locals: &[String],
) -> Result<String, TranspileError> {
    let mut emitter = Emitter {
        ctx,
        frames: Vec::new(),
    };
    if !locals.is_empty() {
        emitter.frames.push(Frame {
            params: locals.to_vec(),
            // The implicit variable `item` is a collection-lambda concept;
            // at function-body top level there is no implicit binding.
            implicit: None,
        });
    }
    emitter.emit(payload, false, false)
}
