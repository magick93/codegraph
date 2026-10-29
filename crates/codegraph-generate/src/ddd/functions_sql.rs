//! `implementation = "sql"` function bindings (issue #450): a modeled
//! function whose body should live in Postgres. Two coordinated planes:
//!
//! - **The migration** (`migrations/022000_functions.sql`, this module's
//!   global generator): one `CREATE OR REPLACE FUNCTION {schema}.{fn}`
//!   per sql-bound function, lowered from the same `Expr::to_json`
//!   payloads the Rust transpiler consumes (`db/expr_sql.rs` is the
//!   policy-RLS precedent for the closed-subset philosophy — this walker
//!   operates on the JSON payloads, not source text).
//! - **The call plane** (`functions/{mod,sql_impl}.rs`, emitted by the
//!   functions domain generator): an async `SqlFnImpls` trait (explicit
//!   `&DatabaseConnection` handle — the async trait class of the #450
//!   design) plus a generated `DefaultSqlImpls` that runs
//!   `SELECT * FROM {schema}.{fn}($1, …)` through the pool.
//!
//! Closed subset (documented; everything else refuses to a **stub +
//! completion brief** — the module is the record, never a guess):
//!
//! | rexlang                    | SQL                                    |
//! |----------------------------|----------------------------------------|
//! | number / int literal       | verbatim                               |
//! | string literal             | `'…'` (quotes doubled)                 |
//! | boolean literal            | TRUE / FALSE                           |
//! | input ref                  | `p_{snake}`                            |
//! | output ref                 | `v_{snake}`                            |
//! | `+ - * /`                  | verbatim                               |
//! | `= <> < <= > >=`           | `= <> < <= > >=`                       |
//! | `and` / `or` / `not`       | AND / OR / NOT                         |
//! | `if c then a else b`       | `CASE WHEN c THEN a ELSE b END`        |
//! | alias read                 | inlined (JSON-level substitution)      |
//!
//! Signature slots resolve to primitives, date builtins (ISO text), and
//! codelists (`text` on the wire — serde round trips the generated enum);
//! anything else (entities, value objects, unknown titles, arrays,
//! optionals) is a HARD generation error naming the function — a
//! signature that does not resolve cannot compile on either side of the
//! wire (the #517 contract). Postgres only; `implementation = "sql"` on a
//! sqlite target is refused at validation time
//! (`FunctionBindings::validate_for_target`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::FunctionNode;
use codegraph_naming::{escape_rust_keyword, to_snake_case};

use crate::code_writer::{CodeWriter, wln};
use crate::error::{Error, Result};
use crate::traits::{GeneratedFile, GlobalGenerator, GlobalGeneratorKind};
use codegraph_config::{DomainConfig, FunctionImplementation};

/// Migration sequence for the sql-function band — after the per-entity
/// RLS band (010000+) and the policy bands (020000/021000), grouping the
/// model-derived migration tail together. Functions are standalone (no
/// table references), so ordering against the entity band is free.
pub const FUNCTIONS_SQL_MIGRATION_SEQ: usize = 22000;

/// One sql-bound parameter (Rust name/type + the `p_`-prefixed PG form).
#[derive(Debug, Clone)]
pub struct SqlParam {
    pub name: String,
    pub rust_type: String,
    pub pg_type: String,
}

/// One planned sql-bound function: the resolved signature plus the
/// lowered body (or the named gap that stubbed it).
#[derive(Debug, Clone)]
pub struct SqlFnPlan {
    /// The modeled function name (diagnostics + comments).
    pub name: String,
    /// The sigil definition text, when authored (trait doc comments).
    pub definition: Option<String>,
    /// snake_case function name on both sides.
    pub fn_name: String,
    /// Postgres schema (the domain's `postgres_schema`).
    pub schema: String,
    pub params: Vec<SqlParam>,
    /// `(rust_name, rust_type, pg_type)` of the declared output.
    pub returns: Option<(String, String, String)>,
    /// Lowered plpgsql statements (without the final RETURN), or the gap
    /// that refused the body.
    pub body: std::result::Result<Vec<String>, SqlGap>,
}

/// A named refusal: the construct family + why it is outside the subset.
#[derive(Debug, Clone)]
pub struct SqlGap {
    pub kind: String,
    pub detail: String,
}

impl SqlGap {
    fn new(kind: &str, detail: impl Into<String>) -> Self {
        Self {
            kind: kind.to_string(),
            detail: detail.into(),
        }
    }
}

/// Rust primitive for a rosetta builtin — the shared functions-plane
/// mapping (dates carry ISO text; see `ddd/functions.rs`).
fn builtin_rust_type(type_ref: &str) -> Option<&'static str> {
    match type_ref {
        "number" => Some("f64"),
        "int" => Some("i64"),
        "string" => Some("String"),
        "boolean" => Some("bool"),
        "date" | "dateTime" | "zonedDateTime" | "time" => Some("String"),
        _ => None,
    }
}

/// PG type for a rosetta builtin.
fn builtin_pg_type(type_ref: &str) -> Option<&'static str> {
    match type_ref {
        "number" => Some("double precision"),
        "int" => Some("bigint"),
        "string" | "date" | "dateTime" | "zonedDateTime" | "time" => Some("text"),
        "boolean" => Some("boolean"),
        _ => None,
    }
}

/// Resolve one signature slot for the sql plane: builtin primitive (or
/// date-as-text), or a codelist enum (`text` on the wire — serde round
/// trips the generated enum). Everything else is a hard error naming the
/// function: the slot must resolve on BOTH sides (the #517 contract).
pub fn resolve_sql_type(
    function: &str,
    slot: &str,
    type_ref: &str,
    codelists: &HashSet<String>,
) -> std::result::Result<(String, String), String> {
    if let Some(rust) = builtin_rust_type(type_ref) {
        return Ok((rust.to_string(), builtin_pg_type(type_ref).unwrap().into()));
    }
    if codelists.contains(type_ref) {
        return Ok((type_ref.to_string(), "text".to_string()));
    }
    Err(format!(
        "function '{function}': {slot} type '{type_ref}' does not resolve to a sql \
         signature slot — sql functions need primitive or codelist-typed \
         signatures (issue #450)"
    ))
}

/// SQL spelling of a string literal (quotes doubled).
fn string_literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// Expand alias READS at the JSON level: every `SymbolReference` whose
/// symbol names an alias is replaced by the alias's parsed payload. Pure
/// substitution — expressions are side-effect free, so multiple reads
/// simply recompute. Aliases may reference earlier aliases (iterated to
/// fixpoint with a depth cap; a cycle is a gap, mirroring the Rust
/// transpiler's declaration-order semantics).
fn expand_aliases(
    payload: &serde_json::Value,
    aliases: &BTreeMap<String, serde_json::Value>,
    depth: usize,
    fn_name: &str,
) -> std::result::Result<serde_json::Value, SqlGap> {
    if depth > 32 {
        return Err(SqlGap::new(
            "alias",
            format!("alias cycle or expansion overflow in '{fn_name}'"),
        ));
    }
    match payload {
        serde_json::Value::Object(map) => {
            if map.get("kind").and_then(|k| k.as_str()) == Some("SymbolReference")
                && let Some(symbol) = map.get("symbol").and_then(|s| s.as_str())
                && let Some(alias) = aliases.get(symbol)
            {
                // Substitute the alias payload, expanding ITS references
                // too (the alias may read an earlier alias).
                return expand_aliases(alias, aliases, depth + 1, fn_name);
            }
            let mut out = serde_json::Map::with_capacity(map.len());
            for (key, value) in map {
                let expanded = match key.as_str() {
                    "receiver" | "left" | "right" | "if" | "then" | "else" | "argument" => {
                        expand_aliases(value, aliases, depth, fn_name)?
                    }
                    _ => value.clone(),
                };
                out.insert(key.clone(), expanded);
            }
            Ok(serde_json::Value::Object(out))
        }
        other => Ok(other.clone()),
    }
}

/// The closed-subset expression lowering. `scope` maps snake_case local
/// names to their SQL spelling (`p_amount`, `v_result`).
fn lower_expr(
    payload: &serde_json::Value,
    scope: &HashMap<String, String>,
) -> std::result::Result<String, SqlGap> {
    let kind = payload.get("kind").and_then(|k| k.as_str()).unwrap_or("");
    let reject = |kind: &str, what: &str| {
        SqlGap::new(
            kind,
            format!("{what} — outside the sql-function subset (issue #450)"),
        )
    };
    match kind {
        "Boolean" => {
            let value = payload
                .get("value")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| SqlGap::new("Boolean", "malformed payload: missing 'value'"))?;
            Ok(if value { "TRUE" } else { "FALSE" }.to_string())
        }
        "String" => {
            let value = payload
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| SqlGap::new("String", "malformed payload: missing 'value'"))?;
            Ok(string_literal(value))
        }
        // Numeric literals keep their source text (the emitter precedent).
        "Number" | "Int" => {
            let text = payload
                .get("text")
                .and_then(|t| t.as_str())
                .ok_or_else(|| SqlGap::new(kind, "malformed payload: missing 'text'"))?;
            Ok(text.to_string())
        }
        "SymbolReference" => {
            let symbol = payload
                .get("symbol")
                .and_then(|s| s.as_str())
                .ok_or_else(|| {
                    SqlGap::new("SymbolReference", "malformed payload: missing 'symbol'")
                })?;
            let snake = escape_rust_keyword(&to_snake_case(symbol));
            scope.get(&snake).cloned().ok_or_else(|| {
                reject(
                    "SymbolReference",
                    &format!("reference to '{symbol}' is not a function input or the output"),
                )
            })
        }
        "Binary" => {
            if payload
                .get("cardMod")
                .and_then(|c| c.as_str())
                .is_some_and(|m| m != "none")
            {
                return Err(reject("Binary", "cardinality comparison (any/all =)"));
            }
            let op = payload
                .get("op")
                .and_then(|o| o.as_str())
                .ok_or_else(|| SqlGap::new("Binary", "malformed payload: missing 'op'"))?;
            let sql_op = match op {
                "+" | "-" | "*" | "/" => op,
                "=" => "=",
                "<>" => "<>",
                "<" => "<",
                "<=" => "<=",
                ">" => ">",
                ">=" => ">=",
                "and" => "AND",
                "or" => "OR",
                other => return Err(reject("Binary", &format!("operator '{other}'"))),
            };
            let left = payload
                .get("left")
                .ok_or_else(|| SqlGap::new("Binary", "malformed payload: missing 'left'"))?;
            let right = payload
                .get("right")
                .ok_or_else(|| SqlGap::new("Binary", "malformed payload: missing 'right'"))?;
            let l = lower_expr(left, scope)?;
            let r = lower_expr(right, scope)?;
            Ok(format!(
                "{} {} {}",
                parenthesize(left, l),
                sql_op,
                parenthesize(right, r)
            ))
        }
        "Conditional" => {
            let cond = payload
                .get("if")
                .ok_or_else(|| SqlGap::new("Conditional", "malformed payload: missing 'if'"))?;
            let then = payload
                .get("then")
                .ok_or_else(|| SqlGap::new("Conditional", "malformed payload: missing 'then'"))?;
            let els = payload
                .get("else")
                .ok_or_else(|| SqlGap::new("Conditional", "malformed payload: missing 'else'"))?;
            let c = lower_expr(cond, scope)?;
            let t = lower_expr(then, scope)?;
            let e = lower_expr(els, scope)?;
            Ok(format!(
                "CASE WHEN {} THEN {} ELSE {} END",
                parenthesize(cond, c),
                t,
                e
            ))
        }
        "Unary" => {
            let op = payload
                .get("op")
                .and_then(|o| o.as_str())
                .ok_or_else(|| SqlGap::new("Unary", "malformed payload: missing 'op'"))?;
            let argument = payload
                .get("argument")
                .ok_or_else(|| SqlGap::new("Unary", "malformed payload: missing 'argument'"))?;
            let inner = parenthesize(argument, lower_expr(argument, scope)?);
            match op {
                "not" => Ok(format!("NOT {inner}")),
                "-" => Ok(format!("-{inner}")),
                other => Err(reject("Unary", &format!("unary operator '{other}'"))),
            }
        }
        other => Err(reject(other, "this expression family has no sql lowering")),
    }
}

/// Parenthesize nested composite operands so SQL precedence can never
/// reinterpret the composition (the `wrap_nested_binary` precedent).
fn parenthesize(payload: &serde_json::Value, sql: String) -> String {
    let kind = payload.get("kind").and_then(|k| k.as_str()).unwrap_or("");
    if matches!(kind, "Binary" | "Conditional") {
        format!("({sql})")
    } else {
        sql
    }
}

fn parse_json(expr_json: &str) -> serde_json::Value {
    serde_json::from_str(expr_json)
        .unwrap_or_else(|_| serde_json::Value::String(expr_json.to_string()))
}

/// Plan one sql-bound function: resolve the signature, expand aliases,
/// lower the operations. Signature failures are HARD errors (the caller
/// turns them into `Error::Config`); body failures become the named gap
/// that stubs the emitted body.
pub fn plan_sql_function(
    node: &FunctionNode,
    schema: &str,
    codelists: &HashSet<String>,
) -> std::result::Result<SqlFnPlan, String> {
    let fn_name = escape_rust_keyword(&to_snake_case(&node.name));
    let mut params = Vec::new();
    for input in &node.inputs {
        if input.is_array || input.is_optional {
            return Err(format!(
                "function '{}': input '{}' — sql signatures are scalar and \
                 required (arrays/optionals have no PG parameter form in the \
                 #450 subset)",
                node.name, input.name,
            ));
        }
        let (rust, pg) = resolve_sql_type(
            &node.name,
            &format!("input '{}'", input.name),
            &input.type_ref,
            codelists,
        )?;
        params.push(SqlParam {
            name: escape_rust_keyword(&to_snake_case(&input.name)),
            rust_type: rust,
            pg_type: pg,
        });
    }
    let returns = match &node.output {
        Some(out) => {
            let (rust, pg) = resolve_sql_type(
                &node.name,
                &format!("output '{}'", out.name),
                &out.type_ref,
                codelists,
            )?;
            Some((escape_rust_keyword(&to_snake_case(&out.name)), rust, pg))
        }
        None => None,
    };

    // Structural refusals first: dispatch heads and extends need the
    // match/child machinery that has no sql analogue.
    let structural = if node.dispatch.is_some() {
        Some(SqlGap::new(
            "dispatch",
            "a dispatch head needs the generated match/extends machinery",
        ))
    } else if node.extends.is_some() {
        Some(SqlGap::new(
            "extends",
            "extends chains lower on the Rust plane only",
        ))
    } else {
        None
    };

    // Alias payloads for read-substitution; keys are the alias's RAW name
    // (symbol spellings in payloads match declarations).
    let alias_map: BTreeMap<String, serde_json::Value> = node
        .aliases
        .iter()
        .map(|alias| (alias.name.clone(), parse_json(&alias.expr_json)))
        .collect();
    let expand = |payload: &serde_json::Value| -> std::result::Result<serde_json::Value, SqlGap> {
        expand_aliases(payload, &alias_map, 0, &node.name)
    };

    // Scope: params + the output variable.
    let mut scope: HashMap<String, String> = HashMap::new();
    for param in &params {
        scope.insert(param.name.clone(), format!("p_{}", param.name));
    }
    if let Some((rust_name, _, _)) = &returns {
        scope.insert(rust_name.clone(), format!("v_{rust_name}"));
    }

    let body = if let Some(gap) = structural {
        Err(gap)
    } else {
        lower_body(node, &expand, &returns, &scope)
    };

    Ok(SqlFnPlan {
        name: node.name.clone(),
        definition: node.definition.clone(),
        fn_name,
        schema: schema.to_string(),
        params,
        returns,
        body,
    })
}

fn lower_body(
    node: &FunctionNode,
    expand: &dyn Fn(&serde_json::Value) -> std::result::Result<serde_json::Value, SqlGap>,
    returns: &Option<(String, String, String)>,
    scope: &HashMap<String, String>,
) -> std::result::Result<Vec<String>, SqlGap> {
    let out_name = returns.as_ref().map(|(rust_name, _, _)| rust_name.clone());
    let out_var = returns
        .as_ref()
        .map(|(rust_name, _, _)| format!("v_{rust_name}"));

    let lower = |expr_json: &str| -> std::result::Result<String, SqlGap> {
        let payload = expand(&parse_json(expr_json))?;
        lower_expr(&payload, scope)
    };

    let mut statements = Vec::new();
    for op in &node.operations {
        let expr = lower(&op.expr_json)?;
        let root_snake = escape_rust_keyword(&to_snake_case(&op.assign_root));
        let targets_output = out_name.as_deref() == Some(root_snake.as_str());
        if !op.path.is_empty() {
            return Err(SqlGap::new(
                "set/add path",
                "pathed assignment (`set root -> f: …`) has no sql lowering — sql \
                 functions are scalar",
            ));
        }
        if !targets_output {
            return Err(SqlGap::new(
                "assignment target",
                format!(
                    "operation assigns '{}' — only the declared output can be \
                     assigned on the sql plane (aliases are inlined reads)",
                    op.assign_root
                ),
            ));
        }
        let var = out_var.clone().ok_or_else(|| {
            SqlGap::new(
                "assignment target",
                "the function declares no output to assign",
            )
        })?;
        if op.is_add {
            statements.push(format!("{var} := {var} + ({expr});"));
        } else {
            statements.push(format!("{var} := {expr};"));
        }
    }
    // Collapse: no aliases + a single plain `set` on the output lowers to
    // one pure `RETURN <expr>;` (no plpgsql variable needed).
    if node.aliases.is_empty()
        && statements.len() == 1
        && let Some((_, expr)) = statements[0]
            .strip_prefix(&format!("{} := ", out_var.as_deref().unwrap_or_default()))
            .map(|expr| (expr.to_string(), expr))
        && let Some(expr) = expr.strip_suffix(';')
    {
        return Ok(vec![format!("RETURN {expr};")]);
    }
    Ok(statements)
}

/// The plpgsql body for the migration: either the lowered statements (+ a
/// final RETURN unless the body already ends in one) or the stub +
/// completion brief naming the gap.
pub fn sql_function_body(plan: &SqlFnPlan) -> Vec<String> {
    match &plan.body {
        Ok(statements) => {
            let mut out = statements.clone();
            if out.last().is_some_and(|s| s.starts_with("RETURN")) {
                return out;
            }
            match &plan.returns {
                Some((rust_name, _, _)) => out.push(format!("RETURN v_{rust_name};")),
                None => out.push("RETURN;".to_string()),
            }
            out
        }
        Err(gap) => vec![
            format!(
                "-- TODO(#450): the modeled body of '{}' did not lower to the sql-function subset:",
                plan.name
            ),
            format!("--   {} — {}", gap.kind, gap.detail),
            "--   Completion brief: extend `ddd/functions_sql.rs::lower_expr` for this".to_string(),
            "--   construct family, regenerate, and the body lands here verbatim.".to_string(),
            format!(
                "RAISE EXCEPTION 'function {}: body not lowered (sql subset: {})'",
                quote_single(&plan.name),
                quote_single(&gap.kind)
            ),
        ],
    }
}

fn quote_single(text: &str) -> String {
    text.replace('\'', "''")
}

/// The `CREATE OR REPLACE FUNCTION` + `GRANT EXECUTE` block for one plan.
pub fn emit_create_function(plan: &SqlFnPlan) -> String {
    let mut code = CodeWriter::new();
    let params = plan
        .params
        .iter()
        .map(|p| format!("p_{} {}", p.name, p.pg_type))
        .collect::<Vec<_>>()
        .join(", ");
    let returns = plan
        .returns
        .as_ref()
        .map(|(_, _, pg)| pg.clone())
        .unwrap_or_else(|| "void".to_string());
    wln!(
        code,
        "CREATE OR REPLACE FUNCTION {schema}.{fn}({params})",
        schema = plan.schema,
        fn = plan.fn_name,
        params = params,
    );
    wln!(code, "RETURNS {returns}", returns = returns);
    wln!(code, "LANGUAGE plpgsql");
    wln!(code, "AS $fn$");
    wln!(code, "BEGIN");
    for statement in sql_function_body(plan) {
        wln!(code, "    {statement}", statement = statement);
    }
    wln!(code, "END");
    wln!(code, "$fn$;");
    wln!(code);
    let sig = plan
        .params
        .iter()
        .map(|p| p.pg_type.clone())
        .collect::<Vec<_>>()
        .join(", ");
    wln!(
        code,
        "GRANT EXECUTE ON FUNCTION {schema}.{fn}({sig}) TO app_user;",
        schema = plan.schema,
        fn = plan.fn_name,
        sig = sig,
    );
    code.into_string()
}

/// The migration file content: one block per sql-bound function, grouped
/// per domain in `(domain, name)` order.
pub fn emit_functions_migration(plans: &[&SqlFnPlan], generator_name: &str) -> String {
    let mut code = CodeWriter::new();
    wln!(
        code,
        "-- Function bindings migration (issue #450): Postgres functions for"
    );
    wln!(code, "-- `implementation = \"sql\"` modeled functions.");
    wln!(code, "-- Generated by {generator_name} — DO NOT EDIT.");
    wln!(code);
    for (index, plan) in plans.iter().enumerate() {
        if index > 0 {
            wln!(code);
        }
        code.push_str(emit_create_function(plan).trim_end_matches('\n'));
        wln!(code);
    }
    code.into_string()
}

/// The generated `functions/sql_impl.rs`: `DefaultSqlImpls` calling each
/// Postgres function through the pool. Values bind per Rust type; codelist
/// enums ride serde (both directions).
pub fn emit_sql_impl_rs(domain: &str, plans: &[&SqlFnPlan], generator_name: &str) -> String {
    let mut code = CodeWriter::new();
    wln!(
        code,
        "//! SQL-bound function impls for the {domain} domain (issue #450).",
        domain = domain,
    );
    wln!(code, "//!");
    wln!(
        code,
        "//! Generated by {} — DO NOT EDIT. Each impl calls the Postgres",
        generator_name
    );
    wln!(
        code,
        "//! function created by migration {:06}_functions.sql; the migration",
        FUNCTIONS_SQL_MIGRATION_SEQ
    );
    wln!(code, "//! is the record of each body's lowering state.");
    wln!(code);
    wln!(code, "use async_trait::async_trait;");
    wln!(
        code,
        "use sea_orm::{{DatabaseBackend, DatabaseConnection, DbErr, Statement}};"
    );
    wln!(code);
    wln!(code, "use super::SqlFnImpls;");
    wln!(code);
    wln!(code, "/// Bind a codelist enum as its serde text form.");
    wln!(
        code,
        "fn enum_text<T: serde::Serialize>(value: &T) -> sea_orm::Value {{"
    );
    wln!(code, "    match serde_json::to_value(value) {{");
    wln!(
        code,
        "        Ok(serde_json::Value::String(text)) => text.into(),"
    );
    wln!(code, "        _ => String::new().into(),");
    wln!(code, "    }}");
    wln!(code, "}}");
    wln!(code);
    wln!(
        code,
        "/// Decode a returned codelist text back into the generated enum."
    );
    wln!(
        code,
        "fn enum_from_text<T: serde::de::DeserializeOwned>(text: String) -> Result<T, DbErr> {{"
    );
    wln!(
        code,
        "    serde_json::from_value(serde_json::Value::String(text))"
    );
    wln!(
        code,
        "        .map_err(|e| DbErr::Custom(format!(\"codelist decode failed: {{e}}\")))"
    );
    wln!(code, "}}");
    wln!(code);
    wln!(code, "/// Generated default SQL-bound implementations.");
    wln!(code, "pub struct DefaultSqlImpls;");
    wln!(code);
    wln!(code, "#[async_trait]");
    wln!(code, "impl SqlFnImpls for DefaultSqlImpls {{");
    for plan in plans {
        emit_sql_impl_method(&mut code, plan);
    }
    wln!(code, "}}");
    code.into_string()
}

fn emit_sql_impl_method(code: &mut CodeWriter, plan: &SqlFnPlan) {
    wln!(code);
    if let Some(gap) = plan.body.as_ref().err() {
        wln!(
            code,
            "    // TODO(#450): '{}' did not lower to the sql-function subset:",
            plan.name
        );
        wln!(code, "    //   {} — {}", gap.kind, gap.detail);
        wln!(
            code,
            "    //   The migration stub raises until the body lands."
        );
    }
    let args = plan
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.rust_type))
        .collect::<Vec<_>>()
        .join(", ");
    let return_ty = plan
        .returns
        .as_ref()
        .map(|(_, rust, _)| rust.clone())
        .unwrap_or_else(|| "()".to_string());
    wln!(
        code,
        "    async fn {fn}(&self, db: &DatabaseConnection{args}) -> Result<{ret}, DbErr> {{",
        fn = plan.fn_name,
        args = if args.is_empty() {
            String::new()
        } else {
            format!(", {args}")
        },
        ret = return_ty,
    );
    let binds = plan
        .params
        .iter()
        .map(|param| match param.rust_type.as_str() {
            "f64" | "i64" | "String" | "bool" => format!("{}.into()", param.name),
            _ => format!("enum_text(&{})", param.name),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = (1..=plan.params.len())
        .map(|i| format!("${i}"))
        .collect::<Vec<_>>()
        .join(", ");
    wln!(code, "        let row = db");
    wln!(
        code,
        "            .query_one(Statement::from_sql_and_values("
    );
    wln!(code, "                DatabaseBackend::Postgres,");
    wln!(
        code,
        "                r#\"SELECT * FROM {schema}.{fn}({placeholders})\"#,",
        schema = plan.schema,
        fn = plan.fn_name,
        placeholders = placeholders,
    );
    wln!(code, "                [{binds}],");
    wln!(code, "            ))");
    wln!(code, "            .await?;");
    if plan.returns.is_some() {
        wln!(
            code,
            "        let row = row.ok_or_else(|| DbErr::RecordNotFound(\"{fn} returned no row\".to_owned()))?;",
            fn = plan.fn_name,
        );
        match &plan.returns {
            Some((_, rust, _)) if matches!(rust.as_str(), "f64" | "i64" | "String" | "bool") => {
                wln!(
                    code,
                    "        Ok(row.try_get::<{rust}, _>(\"\", 0)?)",
                    rust = rust,
                );
            }
            Some((_, rust, _)) => {
                wln!(
                    code,
                    "        let text = row.try_get::<String, _>(\"\", 0)?;"
                );
                wln!(code, "        enum_from_text::<{rust}>(text)", rust = rust);
            }
            None => unreachable!("returns.is_some() held"),
        }
    } else {
        wln!(code, "        let _ = row;");
        wln!(code, "        Ok(())");
    }
    wln!(code, "    }}");
}

/// The `SqlFnImpls` trait lines for `functions/mod.rs` (the async trait
/// class of the #450 design: explicit handle, one method per function).
pub fn emit_sql_trait_block(code: &mut CodeWriter, plans: &[&SqlFnPlan]) {
    wln!(code);
    wln!(
        code,
        "/// Provider seam for sql-bound functions (issue #450)."
    );
    wln!(
        code,
        "/// The generated `DefaultSqlImpls` calls the Postgres functions"
    );
    wln!(
        code,
        "/// created by the functions migration; implement this trait to"
    );
    wln!(code, "/// override a call path.");
    wln!(code, "#[async_trait::async_trait]");
    wln!(code, "pub trait SqlFnImpls: Send + Sync {{");
    for plan in plans {
        wln!(code);
        if let Some(definition) = &plan.definition {
            wln!(code, "    /// {definition}");
        }
        let args = plan
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, p.rust_type))
            .collect::<Vec<_>>()
            .join(", ");
        let return_ty = plan
            .returns
            .as_ref()
            .map(|(_, rust, _)| rust.clone())
            .unwrap_or_else(|| "()".to_string());
        wln!(
            code,
            "    async fn {fn}(&self, db: &sea_orm::DatabaseConnection{args}) -> Result<{ret}, sea_orm::DbErr>;",
            fn = plan.fn_name,
            args = if args.is_empty() {
                String::new()
            } else {
                format!(", {args}")
            },
            ret = return_ty,
        );
    }
    wln!(code, "}}");
}

/// Gather every sql-bound function across the graph, in `(domain, name)`
/// order, resolved against the domain's postgres schema.
pub async fn collect_sql_plans(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<(String, SqlFnPlan)>> {
    let codelists: HashSet<String> = db
        .list_codelists()
        .await
        .map_err(Error::Graph)?
        .into_iter()
        .map(|c| c.name)
        .collect();
    let mut out = Vec::new();
    for node in db.list_functions().await.map_err(Error::Graph)? {
        let Some(binding) = config.functions.get(&node.name) else {
            continue;
        };
        if binding.implementation != FunctionImplementation::Sql {
            continue;
        }
        let domain = node.domain.clone().ok_or_else(|| {
            Error::Config(format!(
                "function '{}': implementation \"sql\" requires a domain",
                node.name
            ))
        })?;
        let entry = config.domains.get(&domain).ok_or_else(|| {
            Error::Config(format!(
                "function '{}': domain '{domain}' is not configured in domains.toml",
                node.name
            ))
        })?;
        let plan =
            plan_sql_function(&node, &entry.postgres_schema, &codelists).map_err(Error::Config)?;
        out.push((domain, plan));
    }
    Ok(out)
}

/// The global generator: ONE migration for every sql-bound function.
/// Presence-gated (no sql bindings ⇒ no file), Postgres-only.
pub struct FunctionsSqlGenerator {
    output_dir: PathBuf,
}

impl FunctionsSqlGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for FunctionsSqlGenerator {
    fn kind(&self) -> GlobalGeneratorKind {
        GlobalGeneratorKind::FunctionsSql
    }

    fn supported_targets(&self) -> Option<Vec<crate::db::dialect::DatabaseTarget>> {
        Some(vec![crate::db::dialect::DatabaseTarget::Postgres])
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        _generation_order: &[crate::GenerationEntry],
        _tera: &tera::Tera,
        project: &crate::ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        if config.functions.is_empty() {
            return Ok(vec![]);
        }
        let plans = collect_sql_plans(db, config).await?;
        if plans.is_empty() {
            return Ok(vec![]);
        }
        let refs: Vec<&SqlFnPlan> = plans.iter().map(|(_, p)| p).collect();
        Ok(vec![GeneratedFile {
            path: crate::db::migrations_root(&self.output_dir)
                .join(format!("{:06}_functions.sql", FUNCTIONS_SQL_MIGRATION_SEQ)),
            content: emit_functions_migration(&refs, &project.identity.generator_name),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> HashMap<String, String> {
        HashMap::from([
            ("amount".to_string(), "p_amount".to_string()),
            ("result".to_string(), "v_result".to_string()),
        ])
    }

    #[test]
    fn lowers_arithmetic_and_params() {
        let payload: serde_json::Value = serde_json::from_str(
            r#"{"kind":"Binary","op":"+","left":{"kind":"SymbolReference","symbol":"amount"},"right":{"kind":"Number","text":"1.0"}}"#,
        )
        .unwrap();
        let sql = lower_expr(&payload, &scope()).unwrap();
        assert_eq!(sql, "p_amount + 1.0");
    }

    #[test]
    fn lowers_conditional_to_case_when() {
        let payload: serde_json::Value = serde_json::from_str(
            r#"{"kind":"Conditional","if":{"kind":"Binary","op":">","left":{"kind":"SymbolReference","symbol":"amount"},"right":{"kind":"Number","text":"10.0"}},"then":{"kind":"Number","text":"2.0"},"else":{"kind":"Number","text":"1.0"}}"#,
        )
        .unwrap();
        let sql = lower_expr(&payload, &scope()).unwrap();
        assert_eq!(sql, "CASE WHEN (p_amount > 10.0) THEN 2.0 ELSE 1.0 END");
    }

    #[test]
    fn refuses_count_and_names_the_construct() {
        let payload: serde_json::Value = serde_json::from_str(
            r#"{"kind":"Count","argument":{"kind":"SymbolReference","symbol":"amount"}}"#,
        )
        .unwrap();
        let gap = lower_expr(&payload, &scope()).unwrap_err();
        assert_eq!(gap.kind, "Count");
    }

    #[test]
    fn refuses_unknown_symbols() {
        let payload: serde_json::Value =
            serde_json::from_str(r#"{"kind":"SymbolReference","symbol":"mystery"}"#).unwrap();
        assert!(lower_expr(&payload, &scope()).is_err());
    }

    #[test]
    fn alias_reads_expand_to_their_payloads() {
        let alias: serde_json::Value =
            serde_json::from_str(r#"{"kind":"SymbolReference","symbol":"amount"}"#).unwrap();
        let aliases = BTreeMap::from([("base".to_string(), alias)]);
        let payload: serde_json::Value = serde_json::from_str(
            r#"{"kind":"Binary","op":"+","left":{"kind":"SymbolReference","symbol":"base"},"right":{"kind":"Number","text":"2.0"}}"#,
        )
        .unwrap();
        let expanded = expand_aliases(&payload, &aliases, 0, "F").unwrap();
        let sql = lower_expr(&expanded, &scope()).unwrap();
        assert_eq!(sql, "p_amount + 2.0");
    }
}
