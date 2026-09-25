//! `functions` — computation-plane codegen (issue #263).
//!
//! Per domain, emits `src/domain/<domain>/functions.rs`: one Rust free
//! function per rosetta FunctionNode, with the sigil operation shape
//! mapped to Rust as follows (grounded in the sigil `Function` model):
//!
//! | sigil source shape                              | Rust emission |
//! |-------------------------------------------------|---------------|
//! | `inputs: n T (c..c)`                            | `n: RustType` args (builtins mapped: number→f64, int→i64, string→String, boolean→bool, date/dateTime/time→String; other titles resolve in the consumer crate) |
//! | `alias a: expr`                                 | `let a = <transpiled expr>;` (declaration order; `let mut` when an operation assigns to it) |
//! | `set root: expr`                                | `root = <expr>;` |
//! | `set root -> f1 -> f2: expr`                    | `root.f1.f2 = <expr>;` |
//! | `add root: expr`                                | `root += <expr>;` |
//! | `add root -> f1: expr`                          | `root.f1.push(<expr>);` (Rosetta `add` appends to a multi) |
//! | dispatch head `(attr: Enum->VALUE)`             | `match attr { Enum::VALUE => <ops>, Enum::CHILD_VALUE => child_fn(args), _ => todo!(...) }` — one arm per extending child carrying its own dispatch head, sorted by name |
//! | `post-condition P: expr`                        | `debug_assert!(<expr>, "P");` when the `function_postconditions` profile feature is set; absent otherwise (default OFF) |
//!
//! Expressions transpile through [`crate::rosetta_expr::transpile_scoped`]
//! with the function's inputs + aliases + output as locals (they are
//! locals of the free function, not receiver fields). Unsupported
//! constructs keep the `// TODO(#263)` marker convention of
//! `validations.rs`; functions referencing UNRESOLVED aliases or extends
//! get markers too (never stderr — the module is the record). Transform
//! annotations (`[ingest X]` / `[enrich]` / `[projection Y]`) ride as doc
//! comment metadata.
//!
//! Gated behind the `rosetta_backend` profile feature via the capability
//! registry — OFF ⇒ the generator never runs and output is byte-identical.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{FunctionNode, FunctionOperation};
use codegraph_naming::{escape_rust_keyword, to_snake_case};

use crate::code_writer::{wln, CodeWriter};
use crate::error::Result;
use crate::rosetta_expr::{transpile_scoped, ExprContext};
use crate::traits::{DomainGenerator, GeneratedFile};
use crate::ProjectConfig;
use codegraph_config::DomainConfig;

/// Rust primitive for a rosetta builtin type title. `None` for
/// non-builtins (their titles resolve in the consumer crate — codelist
/// enums, entity types; documented caveat).
fn builtin_rust_type(type_ref: &str) -> Option<&'static str> {
    match type_ref {
        "number" => Some("f64"),
        "int" => Some("i64"),
        "string" => Some("String"),
        "boolean" => Some("bool"),
        // Date/time builtins carry ISO strings (the bridge's TEXT-mapping
        // precedent — codegraph-generate has no chrono by design).
        "date" | "dateTime" | "zonedDateTime" | "time" => Some("String"),
        _ => None,
    }
}

/// The Rust type for a function input/output: builtin primitive or the
/// written title verbatim.
fn rust_type(type_ref: &str) -> String {
    builtin_rust_type(type_ref)
        .map(str::to_string)
        .unwrap_or_else(|| type_ref.to_string())
}

/// One transpiled function surface element.
enum Transpiled {
    Ok(String),
    Unsupported { kind: String, detail: String },
}

/// One resolved function ready for emission.
struct FunctionSurface {
    /// The function node (name, definition, transforms, properties ride).
    node: FunctionNode,
    /// snake_case Rust fn name.
    fn_name: String,
    /// `(name, rust_type)` args in declaration order.
    args: Vec<(String, String)>,
    /// Output `(name, rust_type)`, when declared.
    output: Option<(String, String)>,
    /// Locals the transpiler may emit bare: args + aliases + output.
    locals: Vec<String>,
    /// Transpiled alias bodies, in declaration order.
    aliases: Vec<(String, Transpiled)>,
    /// Transpiled post-condition bodies, in declaration order.
    post_conditions: Vec<(String, Transpiled)>,
    /// Names of aliases whose body did NOT transpile (ops referencing them
    /// get TODO markers instead of code).
    unresolved_aliases: HashSet<String>,
    /// Parent function name, when `extends` resolves within this domain.
    extends: Option<String>,
    /// `extends` was written but names no function of this domain.
    extends_unresolved: bool,
}

pub struct FunctionsGenerator {
    output_dir: PathBuf,
}

impl FunctionsGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl DomainGenerator for FunctionsGenerator {
    fn name(&self) -> &str {
        "functions"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        domain: &str,
        _entity_titles: &[String],
        _config: &DomainConfig,
        _tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        // list_functions orders by (domain, name); the per-domain filter
        // preserves that — the deterministic emission order.
        let functions: Vec<FunctionNode> = db
            .list_functions()
            .await?
            .into_iter()
            .filter(|f| f.domain.as_deref() == Some(domain))
            .collect();
        if functions.is_empty() {
            return Ok(Vec::new());
        }

        let names: HashSet<&str> = functions.iter().map(|f| f.name.as_str()).collect();
        let surfaces: Vec<FunctionSurface> = functions
            .iter()
            .map(|node| build_surface(node, &names))
            .collect();

        let content = emit_domain_functions(domain, &surfaces, project);
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("domain")
                .join(domain)
                .join("functions.rs"),
            content,
        }])
    }
}

/// Resolve one node against its domain's function-name universe.
fn build_surface(node: &FunctionNode, domain_functions: &HashSet<&str>) -> FunctionSurface {
    let fn_name = escape_rust_keyword(&to_snake_case(&node.name));
    let args: Vec<(String, String)> = node
        .inputs
        .iter()
        .map(|input| {
            (
                escape_rust_keyword(&to_snake_case(&input.name)),
                rust_type(&input.type_ref),
            )
        })
        .collect();
    let output = node.output.as_ref().map(|out| {
        (
            escape_rust_keyword(&to_snake_case(&out.name)),
            rust_type(&out.type_ref),
        )
    });

    // Locals the transpiler emits bare: args, then the output, then
    // aliases (innermost-scope names win — sigil forbids shadowing anyway).
    let mut locals: Vec<String> = args.iter().map(|(name, _)| name.clone()).collect();
    if let Some((out_name, _)) = &output {
        locals.push(out_name.clone());
    }
    locals.extend(
        node.aliases
            .iter()
            .map(|a| escape_rust_keyword(&to_snake_case(&a.name))),
    );

    // Empty ExprContext: with every bare symbol bound as a local, the
    // receiver prefix is only reached by unknown symbols (downstream
    // compile noise, the documented untyped policy); no field
    // optionality/collection/enum knowledge exists for function bodies.
    let empty = HashSet::new();
    let ctx = ExprContext {
        receiver: output
            .as_ref()
            .map(|(name, _)| name.as_str())
            .unwrap_or("value"),
        optional_fields: &empty,
        collection_fields: &empty,
        numeric_fields: &empty,
        integer_fields: &empty,
        enum_types: &empty,
    };

    let mut unresolved_aliases = HashSet::new();
    let mut aliases: Vec<(String, Transpiled)> = Vec::with_capacity(node.aliases.len());
    for alias in &node.aliases {
        let name = escape_rust_keyword(&to_snake_case(&alias.name));
        let t = match transpile_scoped(&parse_json(&alias.expr_json), &ctx, &locals) {
            Ok(code) => Transpiled::Ok(code),
            Err(e) => {
                unresolved_aliases.insert(name.clone());
                Transpiled::Unsupported {
                    kind: e.kind,
                    detail: e.detail,
                }
            }
        };
        aliases.push((name, t));
    }

    let post_conditions: Vec<(String, Transpiled)> = node
        .post_conditions
        .iter()
        .map(|post| {
            let name = escape_rust_keyword(&to_snake_case(&post.name));
            let t = match transpile_scoped(&parse_json(&post.expr_json), &ctx, &locals) {
                Ok(code) => Transpiled::Ok(code),
                Err(e) => Transpiled::Unsupported {
                    kind: e.kind,
                    detail: e.detail,
                },
            };
            (name, t)
        })
        .collect();

    let extends = node
        .extends
        .clone()
        .filter(|parent| domain_functions.contains(parent.as_str()));
    let extends_unresolved = node.extends.is_some() && extends.is_none();

    FunctionSurface {
        node: node.clone(),
        fn_name,
        args,
        output,
        locals,
        aliases,
        post_conditions,
        unresolved_aliases,
        extends,
        extends_unresolved,
    }
}

fn parse_json(expr_json: &str) -> serde_json::Value {
    serde_json::from_str(expr_json)
        .unwrap_or_else(|_| serde_json::Value::String(expr_json.to_string()))
}

/// Append every line of `nested` into `code`, indented by `spaces` extra
/// columns (match-arm bodies are emitted into their own buffer so the
/// statement-level padding composes).
fn push_indented(code: &mut CodeWriter, nested: &CodeWriter, spaces: usize) {
    let pad = " ".repeat(spaces);
    for line in nested.as_str().lines() {
        if line.is_empty() {
            code.push_line("");
        } else {
            code.push_line(&format!("{pad}{line}"));
        }
    }
}

fn operation_label(op: &FunctionOperation) -> String {
    let verb = if op.is_add { "add" } else { "set" };
    if op.path.is_empty() {
        format!("{verb} {}", op.assign_root)
    } else {
        format!("{verb} {} -> {}", op.assign_root, op.path.join(" -> "))
    }
}

/// Emit the complete `functions.rs` for one domain (deterministic by
/// construction — surfaces arrive name-ordered).
fn emit_domain_functions(
    domain: &str,
    functions: &[FunctionSurface],
    project: &ProjectConfig,
) -> String {
    let mut code = CodeWriter::new();
    wln!(
        code,
        "//! Rosetta functions for the {domain} domain (issue #263).",
        domain = domain,
    );
    wln!(code, "//!");
    wln!(
        code,
        "//! Generated by {} — DO NOT EDIT. Function bodies are transpiled",
        project.generator_name
    );
    wln!(
        code,
        "//! from the graphed `Expr::to_json` payloads; unsupported constructs"
    );
    wln!(code, "//! carry TODO(#263) markers instead of guesses.");
    wln!(code, "#![allow(dead_code)]");

    for function in functions {
        emit_function(&mut code, function, project, functions);
    }

    code.into_string()
}

/// Emit one free function (signature, match head or direct body).
fn emit_function(
    code: &mut CodeWriter,
    function: &FunctionSurface,
    project: &ProjectConfig,
    all: &[FunctionSurface],
) {
    wln!(code);
    if let Some(definition) = &function.node.definition {
        wln!(code, "/// {definition}");
    }
    for transform in &function.node.transform_annotations {
        match &transform.reference {
            Some(reference) => wln!(
                code,
                "/// Transform: [{} {reference}]",
                transform.kind.as_str(),
            ),
            None => wln!(code, "/// Transform: [{}]", transform.kind.as_str()),
        }
    }
    if function.extends_unresolved {
        wln!(
            code,
            "// TODO(#263): extends '{}' unresolved (no such function in this domain)",
            function.node.extends.as_deref().unwrap_or_default(),
        );
    }

    let args = function
        .args
        .iter()
        .map(|(name, ty)| format!("{name}: {ty}"))
        .collect::<Vec<_>>()
        .join(", ");
    let return_ty = match &function.output {
        Some((_, ty)) => format!(" -> {ty}"),
        None => String::new(),
    };
    wln!(
        code,
        "pub fn {fn}({args}){return_ty} {{",
        fn = function.fn_name,
        return_ty = return_ty,
    );

    match &function.node.dispatch {
        Some(dispatch) => emit_dispatch_body(code, function, dispatch, project, all),
        None => emit_direct_body(code, function, project),
    }

    wln!(code, "}}");
}

/// Emit `let` bindings for the aliases that transpiled (`let mut` when an
/// operation assigns to the alias).
fn emit_alias_bindings(code: &mut CodeWriter, function: &FunctionSurface) {
    let mutated: HashSet<&str> = function
        .node
        .operations
        .iter()
        .map(|op| op.assign_root.as_str())
        .collect();
    for (name, transpiled) in &function.aliases {
        match transpiled {
            Transpiled::Ok(expr) => {
                let mut_kw = if mutated.contains(name.as_str()) {
                    "mut "
                } else {
                    ""
                };
                wln!(code, "    let {mut_kw}{name} = {expr};");
            }
            Transpiled::Unsupported { kind, detail } => {
                wln!(
                    code,
                    "    // TODO(#263): transpile alias '{name}' (unsupported: {kind}: {detail})"
                );
            }
        }
    }
}

/// Emit one operation statement (or its TODO marker).
fn emit_operation(code: &mut CodeWriter, function: &FunctionSurface, op: &FunctionOperation) {
    let root = escape_rust_keyword(&to_snake_case(&op.assign_root));
    let root_is_local = function.locals.contains(&root);
    let references_unresolved = function.unresolved_aliases.contains(root.as_str());
    if !root_is_local || references_unresolved {
        wln!(
            code,
            "    // TODO(#263): transpile operation '{label}' (unresolved assign root '{root}')",
            label = operation_label(op),
        );
        return;
    }
    let mut target = root.clone();
    for segment in &op.path {
        target.push('.');
        target.push_str(&escape_rust_keyword(&to_snake_case(segment)));
    }
    let payload = parse_json(&op.expr_json);
    let empty = HashSet::new();
    let ctx = ExprContext {
        receiver: &root,
        optional_fields: &empty,
        collection_fields: &empty,
        numeric_fields: &empty,
        integer_fields: &empty,
        enum_types: &empty,
    };
    let expr = match transpile_scoped(&payload, &ctx, &function.locals) {
        Ok(expr) => expr,
        Err(e) => {
            wln!(
                code,
                "    // TODO(#263): transpile operation '{label}' (unsupported: {kind}: {detail})",
                label = operation_label(op),
                kind = e.kind,
                detail = e.detail,
            );
            return;
        }
    };
    let statement = match (op.is_add, op.path.is_empty()) {
        (false, true) => format!("{target} = {expr};"),
        (true, true) => format!("{target} += {expr};"),
        (false, false) => format!("{target} = {expr};"),
        (true, false) => format!("{target}.push({expr});"),
    };
    wln!(code, "    {statement}");
}

/// Emit the operations + post-conditions + return for one arm body.
fn emit_ops_and_return(code: &mut CodeWriter, function: &FunctionSurface, project: &ProjectConfig) {
    // The output local initializes to its default so `set result -> field`
    // and `add` paths have a target; `set result` overwrites wholesale.
    match &function.output {
        Some((name, ty)) => {
            wln!(code, "    let mut {name} = {ty}::default();");
            for op in &function.node.operations {
                emit_operation(code, function, op);
            }
            emit_post_conditions(code, function, project);
            wln!(code, "    {name}");
        }
        // Output-less functions compute through their aliases; sigil
        // forbids assigning the output that does not exist.
        None => {
            for op in &function.node.operations {
                emit_operation(code, function, op);
            }
            emit_post_conditions(code, function, project);
        }
    }
}

/// Emit gated post-conditions (`debug_assert!` under the
/// `function_postconditions` feature; markers regardless — the record of
/// what did not emit).
fn emit_post_conditions(
    code: &mut CodeWriter,
    function: &FunctionSurface,
    project: &ProjectConfig,
) {
    for ((name, transpiled), post) in function
        .post_conditions
        .iter()
        .zip(&function.node.post_conditions)
    {
        match transpiled {
            Transpiled::Ok(expr) => {
                if project.has_function_postconditions {
                    wln!(
                        code,
                        "    debug_assert!({expr}, \"{}\");",
                        post.name.replace('"', "\\\"")
                    );
                }
            }
            Transpiled::Unsupported { kind, detail } => {
                wln!(
                    code,
                    "    // TODO(#263): transpile post-condition '{name}' (unsupported: {kind}: {detail})"
                );
            }
        }
    }
}

/// Direct body (no dispatch head): aliases, ops, post-conditions, return.
fn emit_direct_body(code: &mut CodeWriter, function: &FunctionSurface, project: &ProjectConfig) {
    emit_alias_bindings(code, function);
    emit_ops_and_return(code, function, project);
}

/// Dispatch body: a `match` on the head attribute with the function's own
/// case, one arm per extending child carrying its own dispatch head
/// (sorted by name — children arrive name-ordered), and a `todo!` default
/// arm.
fn emit_dispatch_body(
    code: &mut CodeWriter,
    function: &FunctionSurface,
    dispatch: &codegraph_core::types::FunctionDispatch,
    project: &ProjectConfig,
    all: &[FunctionSurface],
) {
    let attr = escape_rust_keyword(&to_snake_case(&dispatch.attribute));
    let enumeration = dispatch.enumeration.clone();
    let args = function
        .args
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>()
        .join(", ");

    wln!(code, "    match {attr} {{");
    wln!(
        code,
        "        {enumeration}::{value} => {{",
        value = dispatch.value,
    );
    let mut arm = CodeWriter::new();
    emit_alias_bindings(&mut arm, function);
    emit_ops_and_return(&mut arm, function, project);
    push_indented(code, &arm, 8);
    wln!(code, "        }}");

    // Extending children become sibling arms. Family members without a
    // dispatch head cannot pick a match guard — documented TODO instead of
    // a guess.
    for child in all {
        if child.extends.as_deref() != Some(function.node.name.as_str()) {
            continue;
        }
        let Some(child_dispatch) = &child.node.dispatch else {
            wln!(
                code,
                "        // TODO(#263): dispatch arm for '{}' has no dispatch head (extends '{}')",
                child.node.name,
                function.node.name,
            );
            continue;
        };
        wln!(
            code,
            "        {enum_}::{value} => {child}({args}),",
            enum_ = child_dispatch.enumeration,
            value = child_dispatch.value,
            child = child.fn_name,
        );
    }

    wln!(
        code,
        "        _ => todo!(\"function '{}': unhandled {} dispatch value\"),",
        function.node.name,
        enumeration,
    );
    wln!(code, "    }}");
}
