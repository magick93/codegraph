use codegraph_naming::{escape_rust_keyword, to_pascal_case, to_snake_case};

use super::ExprContext;
use super::error::TranspileError;
use super::helpers::{
    chain_parts, date_time_gap, kind_for, optional_field_error, optional_receiver_error,
    reduce_gap, root_symbol, string_literal, wrap_operand, wraps_as_operand,
};

/// One lambda binding frame (innermost frame last in the stack).
pub(super) struct Frame {
    /// Parameter names (snake_case, keyword-escaped) matchable by bare
    /// symbol references.
    pub(super) params: Vec<String>,
    /// What the implicit variable `item` binds to. `Some` for the
    /// implicit-parameter form (`filter [...]` binds the element) and for
    /// `then` bodies (binds the argument fragment); `None` under explicit
    /// parameters, where the implicit variable is shadowed.
    pub(super) implicit: Option<String>,
}

pub(super) struct Emitter<'a> {
    pub(super) ctx: &'a ExprContext<'a>,
    pub(super) frames: Vec<Frame>,
}

impl Emitter<'_> {
    /// Core emitter. `allow_optional_root` sanctions a bare optional-field
    /// reference — set ONLY for exists/absent/default/only-exists arguments
    /// (the sanctioned ways to touch optionals). `bool_position` threads
    /// the boolean-position context (issue #283, module docs): it changes
    /// behavior only at `Conditional`/`Switch`.
    pub(super) fn emit(
        &mut self,
        payload: &serde_json::Value,
        allow_optional_root: bool,
        bool_position: bool,
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
                    parts.push(self.emit(element, false, false)?);
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
                // Enum-namespace receiver: `Enum -> Variant` →
                // `Enum::Variant` (issue #283; resolution order in the
                // module docs — fields win ties, so this only fires when
                // the receiver is not a known field).
                if let Some(literal) = self.enum_variant_literal(payload) {
                    return Ok(literal);
                }
                let receiver = payload.get("receiver").ok_or_else(|| {
                    TranspileError::new(call, "malformed payload: missing 'receiver'")
                })?;
                let recv = self
                    .emit(receiver, false, false)
                    .map_err(|e| optional_receiver_error(call, e))?;
                Ok(format!(
                    "{}.{}",
                    recv,
                    escape_rust_keyword(&to_snake_case(feature))
                ))
            }
            // A Binary node is position-transparent: comparisons and
            // arithmetic are already bool/value-shaped by construction.
            // Only the OPERAND position depends on the op (and/or).
            Some("Binary") => self.emit_binary(payload),
            Some("Exists") => self.emit_exists(payload),
            Some("Absent") => {
                let argument = payload.get("argument").ok_or_else(|| {
                    TranspileError::new("Absent", "malformed payload: missing 'argument'")
                })?;
                if let Some(frag) = self.emit_chain_exists(argument, true)? {
                    return Ok(frag);
                }
                Ok(format!("{}.is_none()", self.emit(argument, true, true)?))
            }
            // SEMANTIC GAP: `.first()` does not enforce Rosetta's uniqueness
            // contract (module docs).
            Some("OnlyElement") => {
                let argument = payload.get("argument").ok_or_else(|| {
                    TranspileError::new("OnlyElement", "malformed payload: missing 'argument'")
                })?;
                Ok(format!("{}.first()", self.emit(argument, false, false)?))
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
                Ok(format!("{}.len()", self.emit(argument, false, false)?))
            }
            Some("Flatten") => {
                let argument = payload.get("argument").ok_or_else(|| {
                    TranspileError::new("Flatten", "malformed payload: missing 'argument'")
                })?;
                Ok(format!(
                    "{}.iter().flatten().collect::<Vec<_>>()",
                    self.emit(argument, false, false)?
                ))
            }
            Some("Distinct") => {
                let argument = payload.get("argument").ok_or_else(|| {
                    TranspileError::new("Distinct", "malformed payload: missing 'argument'")
                })?;
                Ok(format!(
                    "{}.iter().collect::<std::collections::BTreeSet<_>>()",
                    self.emit(argument, false, false)?
                ))
            }
            Some("Reverse") => {
                let argument = payload.get("argument").ok_or_else(|| {
                    TranspileError::new("Reverse", "malformed payload: missing 'argument'")
                })?;
                Ok(format!(
                    "{}.iter().rev().collect::<Vec<_>>()",
                    self.emit(argument, false, false)?
                ))
            }
            Some("First") => {
                let argument = payload.get("argument").ok_or_else(|| {
                    TranspileError::new("First", "malformed payload: missing 'argument'")
                })?;
                Ok(format!("{}.first()", self.emit(argument, false, false)?))
            }
            Some("Last") => {
                let argument = payload.get("argument").ok_or_else(|| {
                    TranspileError::new("Last", "malformed payload: missing 'argument'")
                })?;
                Ok(format!("{}.last()", self.emit(argument, false, false)?))
            }
            Some("Sum") => self.emit_aggregate(payload, "sum"),
            Some("Min") => self.emit_aggregate(payload, "min"),
            Some("Max") => self.emit_aggregate(payload, "max"),
            Some("Join") => self.emit_join(payload),
            Some("Switch") => self.emit_switch(payload, bool_position),
            Some("Conditional") => self.emit_conditional(payload, bool_position),
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
                    self.emit(argument, false, false)?
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
        // Enum-namespace reference outside a qualified `Enum -> Variant`
        // literal: a type name is not a value (issue #283; resolution
        // order in the module docs).
        if self.is_enum_namespace(symbol) {
            return Err(TranspileError::new(
                "SymbolReference",
                format!("enum type name '{symbol}' outside a qualified 'Enum -> Variant' literal"),
            ));
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
            "none" => {
                if let Some(frag) = self.emit_chain_exists(argument, false)? {
                    return Ok(frag);
                }
                Ok(format!("{}.is_some()", self.emit(argument, true, true)?))
            }
            "single" | "multiple" => {
                if !self.is_collection(argument) {
                    return Err(TranspileError::new(
                        "Exists",
                        format!("exists modifier '{modifier}' requires a collection field"),
                    ));
                }
                let arg = self.emit(argument, true, true)?;
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
        let arg = self.emit(argument, false, false)?;
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
        let l = self.emit(left, false, false)?;
        let sep = if explicit {
            self.emit(right, false, false)?
        } else {
            "\", \"".to_string()
        };
        Ok(format!("{l}.join({sep})"))
    }

    /// `switch` → if-else-chain expression. Requires exactly one default
    /// case; reference guards are refused (module docs). In bool position
    /// the default arm follows the issue #283 List rule (module docs) —
    /// a `List` default lowers to `false`; case expressions thread the
    /// switch's own position.
    fn emit_switch(
        &mut self,
        payload: &serde_json::Value,
        bool_position: bool,
    ) -> Result<String, TranspileError> {
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
        let arg = self.emit(argument, false, false)?;
        let arg = wrap_operand(argument, arg);
        let default_expr = self.emit_switch_arm(&defaults[0]["expression"], bool_position)?;
        let mut out = String::new();
        for case in cases {
            if case.get("default").is_some() {
                continue;
            }
            let expr = self.emit_switch_arm(&case["expression"], bool_position)?;
            let cond = match case.get("guard") {
                Some(guard) if guard.get("kind").and_then(|k| k.as_str()) == Some("Literal") => {
                    let value = guard.get("value").ok_or_else(|| {
                        TranspileError::new(
                            "Switch",
                            "malformed payload: literal guard missing 'value'",
                        )
                    })?;
                    // The guard-derived comparison is bool-typed by
                    // construction; its operands stay value-position.
                    format!("{arg} == {}", self.emit(value, false, false)?)
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
        if out.is_empty() {
            // Default-only switch: no guarded cases — emit the default
            // expression directly (`else { .. }` alone is not valid Rust).
            return Ok(default_expr);
        }
        out = format!("{out} else {{ {default_expr} }}");
        Ok(out)
    }

    /// One switch arm (case expression or default), emitted in the
    /// switch's own position — with the issue #283 List rule applied to
    /// the default-shaped situation in bool position: a `List` arm in
    /// bool position is non-bool, so it lowers to `false`. (The rule is
    /// load-bearing for the default; case expressions only thread the
    /// position.)
    fn emit_switch_arm(
        &mut self,
        expr: &serde_json::Value,
        bool_position: bool,
    ) -> Result<String, TranspileError> {
        if bool_position && expr.get("kind").and_then(|k| k.as_str()) == Some("List") {
            return Ok("false".to_string());
        }
        self.emit(expr, false, bool_position)
    }

    /// `if c then a else b` → if-else expression.
    ///
    /// The condition arm is always emitted in bool position. The issue
    /// #283 rule applies to the else-arm in bool position: a `List` else
    /// (the generated `full == false` empty list — and any authored list,
    /// which is indistinguishable in the payload) lowers to `false`;
    /// every other else kind is emitted normally. The then/else arms
    /// thread the conditional's own position (both branches of a
    /// bool-position conditional must be bool-shaped). In value position
    /// the `full == false` else is emitted faithfully as `vec![]`
    /// (slice 2; module docs).
    fn emit_conditional(
        &mut self,
        payload: &serde_json::Value,
        bool_position: bool,
    ) -> Result<String, TranspileError> {
        let cond = payload
            .get("if")
            .ok_or_else(|| TranspileError::new("Conditional", "malformed payload: missing 'if'"))?;
        let then = payload.get("then").ok_or_else(|| {
            TranspileError::new("Conditional", "malformed payload: missing 'then'")
        })?;
        let els = payload.get("else").ok_or_else(|| {
            TranspileError::new("Conditional", "malformed payload: missing 'else'")
        })?;
        let else_frag = if bool_position && els.get("kind").and_then(|k| k.as_str()) == Some("List")
        {
            // Issue #283: a List else in bool position is not bool.
            "false".to_string()
        } else {
            self.emit(els, false, bool_position)?
        };
        Ok(format!(
            "if {} {{ {} }} else {{ {} }}",
            wrap_operand(cond, self.emit(cond, false, true)?),
            self.emit(then, false, bool_position)?,
            else_frag
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
        Ok(format!("{}{}", self.emit(argument, false, false)?, suffix))
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
        let arg = self.emit(argument, false, false)?;
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
        let body_frag = match self.emit(body, false, false) {
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
            return self.emit(argument, false, false);
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
            let arg = self.emit(argument, false, false)?;
            return Ok(format!("{f}({arg})"));
        }
        let arg = self.emit(argument, false, false)?;
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
        let out = self.emit(body, false, false);
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
            if let Some(frag) = self.emit_chain_exists(arg, false)? {
                parts.push(frag);
            } else {
                parts.push(format!("{}.is_some()", self.emit(arg, true, true)?));
            }
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
        // Enum-literal equality (issue #283): exactly one side an enum
        // literal, the other a bare receiver field. Anything else falls
        // through to the generic operand path (a bare OPTIONAL field on
        // either side keeps its refusal there — field-vs-field optional
        // comparison stays unsupported).
        if matches!(op, "=" | "<>")
            && let Some(frag) = self.emit_enum_equality(op, left, right)?
        {
            return Ok(frag);
        }
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
                let l = self.emit(left, false, false)?;
                let r = self.emit(right, false, false)?;
                return Ok(format!("{l}.contains(&{})", wrap_operand(right, r)));
            }
            "disjoint" => {
                if !self.is_collection(left) {
                    return Err(TranspileError::new(
                        "Binary",
                        "disjoint requires a collection receiver",
                    ));
                }
                let l = self.emit(left, false, false)?;
                let r = self.emit(right, false, false)?;
                return Ok(format!(
                    "{l}.iter().all(|x| !{}.contains(x))",
                    wrap_operand(right, r)
                ));
            }
            "default" => {
                // Sanctioned optional access: `default` IS the sanctioned
                // way to read a bare optional field.
                let l = self.emit(left, true, false)?;
                let r = self.emit(right, false, false)?;
                return Ok(format!("{l}.unwrap_or({})", wrap_operand(right, r)));
            }
            other => {
                return Err(TranspileError::new(
                    "Binary",
                    format!("unknown operator '{other}'"),
                ));
            }
        };
        // Logical operands are bool-position regardless of the ambient
        // position (a logical operation's operands are bool by
        // definition, issue #283); arithmetic and comparison operands
        // are values — the comparison itself is the bool thing.
        let operand_bool = matches!(op, "and" | "or");
        Ok(format!(
            "{} {rust_op} {}",
            self.wrap_nested_binary(left, operand_bool)?,
            self.wrap_nested_binary(right, operand_bool)?
        ))
    }

    /// Emit a binary operand, parenthesizing nested Binary/Conditional/
    /// Switch/Then subtrees so Rust precedence can never reinterpret the
    /// composition.
    fn wrap_nested_binary(
        &mut self,
        payload: &serde_json::Value,
        bool_position: bool,
    ) -> Result<String, TranspileError> {
        let code = self.emit(payload, false, bool_position)?;
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

    /// Whether the snake_cased name is a known receiver field in ANY field
    /// set (fields win ties over enum type names).
    fn is_known_field(&self, field: &str) -> bool {
        self.ctx.optional_fields.contains(field)
            || self.ctx.collection_fields.contains(field)
            || self.ctx.numeric_fields.contains(field)
            || self.ctx.integer_fields.contains(field)
    }

    /// Whether the raw (Pascal) symbol is an enum-namespace reference: an
    /// `enum_types` member that is NOT also a known field (tier 3 of the
    /// resolution order — fields win ties).
    fn is_enum_namespace(&self, symbol: &str) -> bool {
        self.ctx.enum_types.contains(symbol) && !self.is_known_field(&to_snake_case(symbol))
    }

    /// `FeatureCall`/`DeepFeatureCall` with an enum-namespace receiver →
    /// the qualified variant literal (`PositionStatusEnum -> Closed` →
    /// `PositionStatusEnum::Closed`, variant Pascal-cased). `None` when
    /// the receiver is not an enum-namespace reference (the generic field
    /// path applies).
    fn enum_variant_literal(&self, payload: &serde_json::Value) -> Option<String> {
        let receiver = payload.get("receiver")?;
        if receiver.get("kind").and_then(|k| k.as_str()) != Some("SymbolReference") {
            return None;
        }
        let symbol = receiver.get("symbol").and_then(|s| s.as_str())?;
        if !self.is_enum_namespace(symbol) {
            return None;
        }
        let feature = payload.get("feature").and_then(|f| f.as_str())?;
        Some(format!("{}::{}", symbol, to_pascal_case(feature)))
    }

    /// The bare receiver-field name a payload refers to, if it is a plain
    /// `SymbolReference` resolving to a field (tier 2 — not a lambda
    /// local, not an enum-namespace reference).
    fn bare_receiver_field(&self, payload: &serde_json::Value) -> Option<String> {
        if payload.get("kind").and_then(|k| k.as_str()) != Some("SymbolReference") {
            return None;
        }
        let symbol = payload.get("symbol").and_then(|s| s.as_str())?;
        let field = escape_rust_keyword(&to_snake_case(symbol));
        if self
            .frames
            .iter()
            .rev()
            .any(|f| f.params.iter().any(|p| p == &field))
        {
            return None;
        }
        if self.is_enum_namespace(symbol) {
            return None;
        }
        Some(field)
    }

    /// Enum-literal equality (issue #283): exactly one side an enum
    /// literal (`Enum -> Variant`), the other a bare receiver field.
    /// Optional field → `dto.f.as_ref() == Some(&Lit)` (`<>` → `!=`);
    /// required field → plain `dto.f == Lit`. Returns `None` when neither
    /// pairing matches — the generic operand path applies, where a bare
    /// optional field keeps its refusal (field-vs-field optional
    /// comparison stays unsupported).
    fn emit_enum_equality(
        &mut self,
        op: &str,
        left: &serde_json::Value,
        right: &serde_json::Value,
    ) -> Result<Option<String>, TranspileError> {
        let rust_op = if op == "=" { "==" } else { "!=" };
        for (literal_payload, field_payload) in [(left, right), (right, left)] {
            let Some(literal) = self.enum_variant_literal(literal_payload) else {
                continue;
            };
            let Some(field) = self.bare_receiver_field(field_payload) else {
                continue;
            };
            if self.ctx.optional_fields.contains(&field) {
                return Ok(Some(format!(
                    "{}.{field}.as_ref() {rust_op} Some(&{literal})",
                    self.ctx.receiver
                )));
            }
            return Ok(Some(format!(
                "{}.{field} {rust_op} {literal}",
                self.ctx.receiver
            )));
        }
        Ok(None)
    }

    /// Exists/absent/only-exists argument lowering for feature CHAINS
    /// (issue #283 slice b). Returns `Ok(None)` when the argument is not
    /// a `FeatureCall`/`DeepFeatureCall` chain bottoming in a plain
    /// symbol — callers fall back to the legacy bare-fragment emission.
    ///
    /// Canonical shapes (module docs): an OPTIONAL root lowers to
    /// `dto.a.as_ref().map(|v| v.b.is_some()).unwrap_or(false)` (absent:
    /// `map(|v| v.b.is_none()).unwrap_or(true)` — a `None` root means the
    /// whole path is absent); a required root stays plain dot access; a
    /// local root (lambda frame) emits the plain path over the local.
    /// Only the root's optionality is known — deeper hops emit plain
    /// access (documented caveat class).
    fn emit_chain_exists(
        &mut self,
        argument: &serde_json::Value,
        absent: bool,
    ) -> Result<Option<String>, TranspileError> {
        let Some((root, features)) = chain_parts(argument) else {
            return Ok(None);
        };
        let symbol = root.get("symbol").and_then(|s| s.as_str()).ok_or_else(|| {
            TranspileError::new("Exists", "malformed payload: chain root missing 'symbol'")
        })?;
        // A function-like root is not a field chain — legacy path refuses
        // it with the precise error.
        let explicit = root
            .get("explicit")
            .and_then(|e| e.as_bool())
            .unwrap_or(false);
        let has_args = root
            .get("args")
            .and_then(|a| a.as_array())
            .is_some_and(|a| !a.is_empty());
        if explicit || has_args {
            return Ok(None);
        }
        let field = escape_rust_keyword(&to_snake_case(symbol));
        let path = features
            .iter()
            .map(|f| escape_rust_keyword(&to_snake_case(f)))
            .collect::<Vec<_>>()
            .join(".");
        let terminal = if absent { "is_none()" } else { "is_some()" };
        // Bare-symbol argument (empty feature path): the slice-1 direct
        // shape — `recv.field.is_some()` — not the chain/map shape (which
        // would emit a stray empty segment, `v..is_some()`).
        if path.is_empty() {
            for frame in self.frames.iter().rev() {
                if frame.params.iter().any(|p| p == &field) {
                    return Ok(Some(format!("{field}.{terminal}")));
                }
            }
            if self.is_enum_namespace(symbol) {
                return Err(TranspileError::new(
                    "Exists",
                    format!("enum type name '{symbol}' cannot be exists-checked"),
                ));
            }
            return Ok(Some(format!("{}.{field}.{terminal}", self.ctx.receiver)));
        }
        // Lambda locals first (resolution order tier 1): a local-rooted
        // chain is a plain value path.
        for frame in self.frames.iter().rev() {
            if frame.params.iter().any(|p| p == &field) {
                return Ok(Some(format!("{field}.{path}.{terminal}")));
            }
        }
        if self.is_enum_namespace(symbol) {
            return Err(TranspileError::new(
                "Exists",
                format!("enum type name '{symbol}' cannot be exists-checked"),
            ));
        }
        if self.ctx.optional_fields.contains(&field) {
            let fallback = if absent { "true" } else { "false" };
            Ok(Some(format!(
                "{}.{field}.as_ref().map(|v| v.{path}.{terminal}).unwrap_or({fallback})",
                self.ctx.receiver
            )))
        } else {
            Ok(Some(format!(
                "{}.{field}.{path}.{terminal}",
                self.ctx.receiver
            )))
        }
    }
}
