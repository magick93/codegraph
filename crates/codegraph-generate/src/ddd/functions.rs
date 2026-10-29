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

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{FunctionNode, FunctionOperation};
use codegraph_naming::{escape_rust_keyword, to_snake_case};

use crate::ProjectConfig;
use crate::code_writer::{CodeWriter, wln};
use crate::ddd::functions_api::{request_struct, resolve_api_type, response_struct};
use crate::error::Result;
use crate::rosetta_expr::{ExprContext, transpile_scoped};
use crate::traits::{DomainGenerator, DomainGeneratorKind, GeneratedFile};
use codegraph_config::{DomainConfig, FunctionExposure, FunctionImplementation};

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
    fn kind(&self) -> DomainGeneratorKind {
        DomainGeneratorKind::Functions
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        domain: &str,
        _entity_titles: &[String],
        config: &DomainConfig,
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
        let mut surfaces: Vec<FunctionSurface> = functions
            .iter()
            .map(|node| build_surface(node, &names))
            .collect();

        // Issue #450 B0: `[functions."Name"]` bindings place each function.
        // The split layout (functions/{mod,generated,manual}.rs) activates
        // for a domain only when a NON-DEFAULT binding is present — the
        // absent table (and all-default entries) keep today's single-file
        // emission byte-identical.
        let is_manual = |s: &FunctionSurface| {
            config
                .functions
                .get(&s.node.name)
                .is_some_and(|b| b.implementation == FunctionImplementation::Manual)
        };
        let any_non_default = surfaces.iter().any(|s| {
            config.functions.get(&s.node.name).is_some_and(|b| {
                b.implementation != FunctionImplementation::Dsl
                    || b.exposure != FunctionExposure::Internal
            })
        });

        let module_dir = self.output_dir.join("src").join("domain").join(domain);

        if !any_non_default {
            // Transition sweep: a previously bound run may have left the
            // split directory behind (the stale-cleanup pass preserves the
            // `functions` dir for the write-once manual file).
            let stale_dir = module_dir.join("functions");
            if stale_dir.is_dir() {
                let _ = std::fs::remove_dir_all(&stale_dir);
            }
            let content =
                emit_domain_functions(domain, &surfaces, project, false, &BTreeSet::new());
            return Ok(vec![GeneratedFile {
                path: module_dir.join("functions.rs"),
                content,
            }]);
        }

        // Transition sweep: the stale single file would collide with the
        // module directory of the same name.
        let stale_file = module_dir.join("functions.rs");
        if stale_file.is_file() {
            let _ = std::fs::remove_file(&stale_file);
        }

        // Issue #337 B1: resolve the API-plane types for every HTTP-exposed
        // function so the emitted DTOs, handler calls, and signatures agree
        // and compile. Issue #450 slice C: `implementation = "external"`
        // resolves the same way — its typed req/resp structs ride the same
        // serde surface. Everything else keeps today's verbatim
        // signatures. Unresolvable types are a hard generation error
        // naming the function and the input — never a bare name (the #517
        // contract).
        let suffix = &config.defaults.type_suffix;
        let mut imports_by_fn: HashMap<String, BTreeSet<String>> = HashMap::new();
        for surface in &mut surfaces {
            let Some(binding) = config.functions.get(&surface.node.name) else {
                continue;
            };
            if binding.exposure != FunctionExposure::Http
                && binding.implementation != FunctionImplementation::External
            {
                continue;
            }
            let mut imports = BTreeSet::new();
            let fail = |function: &str, reason: String| {
                codegraph_core::error::Error::Config(format!(
                    "function '{function}': {reason} — exposed and manual functions need \
                     primitive, codelist, or entity-typed signatures",
                ))
            };
            for (slot, input) in surface.args.iter_mut().zip(&surface.node.inputs) {
                let resolved = resolve_api_type(db, domain, &input.type_ref, suffix).await;
                match resolved {
                    Ok(arg) => {
                        if let Some(path) = arg.import {
                            imports.insert(path);
                        }
                        slot.1 = arg.rust_type;
                    }
                    Err(reason) => return Err(fail(&surface.node.name, reason)),
                }
            }
            if let (Some(out_slot), Some(out_node)) = (&surface.output, &surface.node.output) {
                let resolved = resolve_api_type(db, domain, &out_node.type_ref, suffix).await;
                match resolved {
                    Ok(arg) => {
                        if let Some(path) = arg.import {
                            imports.insert(path);
                        }
                        surface.output = Some((out_slot.0.clone(), arg.rust_type));
                    }
                    Err(reason) => return Err(fail(&surface.node.name, reason)),
                }
            }
            imports_by_fn.insert(surface.node.name.clone(), imports);
        }

        let is_http = |s: &FunctionSurface| {
            config
                .functions
                .get(&s.node.name)
                .is_some_and(|b| b.exposure == FunctionExposure::Http)
        };

        // Issue #450 slice B: `implementation = "sql"` resolves the
        // signature on the sql plane (primitives, dates-as-text, codelists
        // — hard error naming the function otherwise) and plans the
        // Postgres body. The migration itself is the global
        // `functions_sql` generator; this plane emits the call side
        // (`SqlFnImpls` trait + `DefaultSqlImpls`).
        let is_sql = |s: &FunctionSurface| {
            config
                .functions
                .get(&s.node.name)
                .is_some_and(|b| b.implementation == FunctionImplementation::Sql)
        };
        let mut sql_plans: Vec<crate::ddd::functions_sql::SqlFnPlan> = Vec::new();
        if surfaces.iter().any(is_sql) {
            let codelists: HashSet<String> = db
                .list_codelists()
                .await?
                .into_iter()
                .map(|c| c.name)
                .collect();
            let entry = config.domains.get(domain).ok_or_else(|| {
                codegraph_core::error::Error::Config(format!(
                    "domain '{domain}' is not configured in domains.toml"
                ))
            })?;
            for surface in &mut surfaces {
                if !is_sql(surface) {
                    continue;
                }
                for (slot, input) in surface.args.iter_mut().zip(&surface.node.inputs) {
                    let (rust, _pg) = crate::ddd::functions_sql::resolve_sql_type(
                        &surface.node.name,
                        &format!("input '{}'", input.name),
                        &input.type_ref,
                        &codelists,
                    )
                    .map_err(codegraph_core::error::Error::Config)?;
                    slot.1 = rust;
                }
                if let (Some(out_slot), Some(out_node)) = (&surface.output, &surface.node.output) {
                    let (rust, _pg) = crate::ddd::functions_sql::resolve_sql_type(
                        &surface.node.name,
                        &format!("output '{}'", out_node.name),
                        &out_node.type_ref,
                        &codelists,
                    )
                    .map_err(codegraph_core::error::Error::Config)?;
                    surface.output = Some((out_slot.0.clone(), rust));
                }
                sql_plans.push(
                    crate::ddd::functions_sql::plan_sql_function(
                        &surface.node,
                        &entry.postgres_schema,
                        &codelists,
                    )
                    .map_err(codegraph_core::error::Error::Config)?,
                );
            }
        }
        let sql_gapped: HashSet<String> = sql_plans
            .iter()
            .filter(|p| p.body.is_err())
            .map(|p| p.name.clone())
            .collect();

        let partial: HashSet<String> = surfaces
            .iter()
            .filter(|s| {
                !is_manual(s)
                    && !is_sql(s)
                    && is_http(s)
                    && surface_is_partial(s, &surfaces, project)
            })
            .map(|s| s.node.name.clone())
            .collect();
        let manual_names: HashSet<String> = surfaces
            .iter()
            .filter(|s| is_manual(s))
            .map(|s| s.node.name.clone())
            .collect();

        let is_external = |s: &FunctionSurface| {
            config
                .functions
                .get(&s.node.name)
                .is_some_and(|b| b.implementation == FunctionImplementation::External)
        };
        let (manual, rest): (Vec<FunctionSurface>, Vec<FunctionSurface>) =
            surfaces.into_iter().partition(is_manual);
        let (sql_bound, rest2): (Vec<FunctionSurface>, Vec<FunctionSurface>) =
            rest.into_iter().partition(is_sql);
        let (external, generated): (Vec<FunctionSurface>, Vec<FunctionSurface>) =
            rest2.into_iter().partition(is_external);
        let sql_names: HashSet<String> = sql_bound.iter().map(|s| s.node.name.clone()).collect();
        let external_names: HashSet<String> =
            external.iter().map(|s| s.node.name.clone()).collect();

        let fn_imports = |names: &[&FunctionSurface]| {
            names
                .iter()
                .filter_map(|s| imports_by_fn.get(&s.node.name))
                .cloned()
                .reduce(|a, b| a.union(&b).cloned().collect())
                .unwrap_or_default()
        };

        // Write-once: the hand-owned stub file is emitted only when absent.
        let manual_rs = module_dir.join("functions").join("manual.rs");
        let manual_present = manual_rs.exists();

        let mut files = Vec::new();
        if !generated.is_empty() {
            let generated_imports = fn_imports(&generated.iter().collect::<Vec<_>>());
            files.push(GeneratedFile {
                path: module_dir.join("functions").join("generated.rs"),
                content: emit_domain_functions(
                    domain,
                    &generated,
                    project,
                    true,
                    &generated_imports,
                ),
            });
        }
        if (!manual.is_empty() || !external.is_empty()) && !manual_present {
            let manual_imports =
                fn_imports(&manual.iter().chain(external.iter()).collect::<Vec<_>>());
            files.push(GeneratedFile {
                path: manual_rs,
                content: emit_manual_stub(domain, &manual, &external, &manual_imports),
            });
        }
        // Issue #450 slice B: the sql call plane — regenerated every run
        // (it is derived output; the migration is its record).
        if !sql_bound.is_empty() {
            let plan_refs: Vec<&crate::ddd::functions_sql::SqlFnPlan> = sql_plans.iter().collect();
            files.push(GeneratedFile {
                path: module_dir.join("functions").join("sql_impl.rs"),
                content: crate::ddd::functions_sql::emit_sql_impl_rs(
                    domain,
                    &plan_refs,
                    &project.identity.generator_name,
                ),
            });
        }
        files.push(GeneratedFile {
            path: module_dir.join("functions").join("mod.rs"),
            content: emit_functions_module(
                domain,
                &manual,
                &sql_bound,
                &sql_plans,
                &external,
                project,
                manual_present,
            ),
        });

        // Issue #337 B1: the HTTP surface for this domain's exposed
        // functions. Absent bindings ⇒ no file (byte-identity).
        let http: Vec<&FunctionSurface> = generated
            .iter()
            .chain(manual.iter())
            .chain(sql_bound.iter())
            .chain(external.iter())
            .filter(|s| is_http(s))
            .collect();
        if !http.is_empty() {
            let api_imports = fn_imports(&http);
            files.push(GeneratedFile {
                path: self
                    .output_dir
                    .join("src")
                    .join("api")
                    .join(domain)
                    .join("functions_api.rs"),
                content: emit_functions_api_rs(
                    domain,
                    &http,
                    &partial,
                    &manual_names,
                    &sql_names,
                    &sql_gapped,
                    &external_names,
                    &api_imports,
                    &project.identity.api_version,
                    project,
                ),
            });
        }
        Ok(files)
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

/// Emit the complete transpiled module body for one domain (deterministic
/// by construction — surfaces arrive name-ordered). `cross_module_imports`
/// is the split-layout flag: `generated.rs` re-imports `super::*` so a
/// dispatch arm may delegate to a manual-bound child function (single-file
/// emission never imports — byte-identity for the unbound default).
/// `imports` carries the resolved API-plane `use` lines (split mode only).
fn emit_domain_functions(
    domain: &str,
    functions: &[FunctionSurface],
    project: &ProjectConfig,
    cross_module_imports: bool,
    imports: &BTreeSet<String>,
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
        project.identity.generator_name
    );
    wln!(
        code,
        "//! from the graphed `Expr::to_json` payloads; unsupported constructs"
    );
    wln!(code, "//! carry TODO(#263) markers instead of guesses.");
    wln!(code, "#![allow(dead_code)]");
    if cross_module_imports {
        wln!(code);
        wln!(code, "use super::*;");
        for path in imports {
            wln!(code, "use {path};");
        }
    }

    for function in functions {
        emit_function(&mut code, function, project, functions);
    }

    code.into_string()
}

/// Emit the split layout's `mod.rs` (issue #450): module declarations,
/// re-exports, and — when the domain carries manual-bound functions — the
/// `ManualImpls` provider seam. `manual_present` records that the
/// write-once stub file already exists on disk (its declaration is
/// emitted regardless so hand edits keep compiling).
fn emit_functions_module(
    domain: &str,
    manual: &[FunctionSurface],
    sql_bound: &[FunctionSurface],
    sql_plans: &[crate::ddd::functions_sql::SqlFnPlan],
    external: &[FunctionSurface],
    project: &ProjectConfig,
    manual_present: bool,
) -> String {
    let mut code = CodeWriter::new();
    wln!(
        code,
        "//! Rosetta functions for the {domain} domain (issue #263; bindings issue #450).",
        domain = domain,
    );
    wln!(code, "//!");
    wln!(
        code,
        "//! Generated by {} — DO NOT EDIT. `generated.rs` carries the",
        project.identity.generator_name
    );
    wln!(
        code,
        "//! transpiled bodies; `manual.rs` is the write-once hand-owned file"
    );
    wln!(code, "//! (regeneration never overwrites it).");
    wln!(code, "#![allow(dead_code)]");
    wln!(code);
    wln!(code, "pub mod generated;");
    wln!(code, "pub use generated::*;");

    if !sql_bound.is_empty() {
        wln!(code);
        wln!(code, "pub mod sql_impl;");
        wln!(code, "pub use sql_impl::*;");
        let plan_refs: Vec<&crate::ddd::functions_sql::SqlFnPlan> = sql_plans.iter().collect();
        crate::ddd::functions_sql::emit_sql_trait_block(&mut code, &plan_refs);
    }

    if !external.is_empty() {
        emit_external_surface(&mut code, external, project);
    }

    if manual.is_empty() {
        return code.into_string();
    }

    wln!(code);
    wln!(
        code,
        "pub mod manual;{}",
        if manual_present {
            " // write-once: not re-emitted"
        } else {
            ""
        }
    );
    wln!(code, "pub use manual::*;");
    wln!(code);
    wln!(
        code,
        "/// Provider seam for hand-owned function implementations (issue #450)."
    );
    wln!(
        code,
        "/// Fill in the write-once `manual.rs` stubs, or implement this trait"
    );
    wln!(
        code,
        "/// yourself and wire the provider where the generated handlers (or"
    );
    wln!(code, "/// your own call sites) can reach it.");
    wln!(code, "pub trait ManualImpls: Send + Sync {{");
    for function in manual {
        wln!(code);
        if let Some(definition) = &function.node.definition {
            wln!(code, "    /// {definition}");
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
            "    fn {fn}(&self{args}){return_ty};",
            fn = function.fn_name,
            args = if args.is_empty() {
                String::new()
            } else {
                format!(", {args}")
            },
        );
    }
    wln!(code, "}}");

    code.into_string()
}

/// Emit the write-once `manual.rs` stub (issue #450): one
/// `DefaultManualImpls` with a `todo!` body per manual-bound function.
/// Hand implementations live HERE — the module is the record.
fn emit_manual_stub(
    domain: &str,
    manual: &[FunctionSurface],
    external: &[FunctionSurface],
    imports: &BTreeSet<String>,
) -> String {
    let mut code = CodeWriter::new();
    let manual_only = external.is_empty();
    wln!(
        code,
        "//! Hand-owned implementations for the {domain} domain (issue #450).",
        domain = domain,
    );
    wln!(code, "//!");
    wln!(
        code,
        "//! WRITE-ONCE: this file is emitted only when absent — regeneration"
    );
    wln!(
        code,
        "//! never overwrites hand edits. The traits it implements are"
    );
    wln!(
        code,
        "//! re-emitted into `mod.rs` on every run, so a model change that"
    );
    wln!(
        code,
        "//! alters a signature fails to compile here instead of drifting."
    );
    wln!(code, "#![allow(dead_code)]");
    wln!(code);
    if !manual_only {
        wln!(code, "use async_trait::async_trait;");
    }
    if !manual.is_empty() {
        wln!(code, "use super::ManualImpls;");
    }
    if !external.is_empty() {
        wln!(code, "use super::ExternalFnImpls;");
        wln!(
            code,
            "use super::{{{}Request, {}Response}};",
            request_struct(&external[0].node.name),
            response_struct(&external[0].node.name)
        );
        for function in external.iter().skip(1) {
            wln!(
                code,
                "use super::{{{}Request, {}Response}};",
                request_struct(&function.node.name),
                response_struct(&function.node.name)
            );
        }
    }
    for path in imports {
        wln!(code, "use {path};");
    }
    if !manual.is_empty() {
        wln!(code);
        wln!(
            code,
            "/// Default hand-owned provider: replace the `todo!` bodies in place."
        );
        wln!(code, "pub struct DefaultManualImpls;");
        wln!(code);
        wln!(code, "impl ManualImpls for DefaultManualImpls {{");
        for function in manual {
            wln!(code);
            if let Some(definition) = &function.node.definition {
                wln!(code, "    /// {definition}");
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
                "    fn {fn}(&self{args}){return_ty} {{",
                fn = function.fn_name,
                args = if args.is_empty() {
                    String::new()
                } else {
                    format!(", {args}")
                },
            );
            // Silence unused-parameter warnings in the stub body; the args
            // stay named so the hand impl keeps the model's vocabulary.
            let owned: Vec<String> = function.args.iter().map(|(name, _)| name.clone()).collect();
            wln!(code, "        let _ = (self{});", paren_tail(&owned));
            wln!(
                code,
                "        todo!(\"hand-owned implementation of '{fn}' (modeled function '{name}')\")",
                fn = function.fn_name,
                name = function.node.name,
            );
            wln!(code, "    }}");
        }
        wln!(code, "}}");
    }
    if !external.is_empty() {
        emit_external_hook(&mut code, external);
    }

    code.into_string()
}

/// The external-function client surface in `mod.rs` (issue #339): typed
/// req/resp structs derived from the modeled signature plus the async
/// `ExternalFnImpls` trait. The write-once hook in `manual.rs` implements
/// it with a pre-wired HTTP client.
fn emit_external_surface(
    code: &mut CodeWriter,
    external: &[FunctionSurface],
    project: &ProjectConfig,
) {
    let camel = project.codegen.dto_key_casing == crate::project_config::DtoKeyCasing::Camel;
    let serde_attr = if camel {
        "\n#[serde(rename_all = \"camelCase\")]"
    } else {
        ""
    };
    wln!(code);
    wln!(
        code,
        "/// Typed wire structs + provider seam for external-bound functions"
    );
    wln!(
        code,
        "/// (issue #339). The write-once hook in `manual.rs` ships a"
    );
    wln!(code, "/// pre-wired HTTP client implementing this trait.");
    wln!(code, "#[async_trait::async_trait]");
    wln!(code, "pub trait ExternalFnImpls: Send + Sync {{");
    for function in external {
        wln!(code);
        if let Some(definition) = &function.node.definition {
            wln!(code, "    /// {definition}");
        }
        wln!(
            code,
            "    async fn {fn}(",
            fn = function.fn_name,
        );
        wln!(code, "        &self,");
        wln!(code, "        base_url: &str,");
        wln!(
            code,
            "        request: {Request},",
            Request = request_struct(&function.node.name),
        );
        wln!(
            code,
            "    ) -> Result<{Response}, String>;",
            Response = response_struct(&function.node.name)
        );
    }
    wln!(code, "}}");
    for function in external {
        wln!(code);
        if let Some(definition) = &function.node.definition {
            wln!(code, "/// {definition}");
        }
        wln!(
            code,
            "#[derive(Debug, Clone, Serialize, Deserialize)]{serde_attr}",
            serde_attr = serde_attr,
        );
        wln!(
            code,
            "pub struct {Request} {{",
            Request = request_struct(&function.node.name),
        );
        for (name, ty) in &function.args {
            wln!(code, "    pub {name}: {ty},");
        }
        wln!(code, "}}");
        wln!(code);
        if let Some(definition) = &function.node.definition {
            wln!(code, "/// {definition}");
        }
        wln!(
            code,
            "#[derive(Debug, Clone, Serialize, Deserialize)]{serde_attr}",
            serde_attr = serde_attr,
        );
        wln!(
            code,
            "pub struct {Response} {{",
            Response = response_struct(&function.node.name),
        );
        if let Some((out_name, out_ty)) = &function.output {
            wln!(code, "    pub {out_name}: {out_ty},");
        }
        wln!(code, "}}");
    }
}

/// The write-once pre-wired hook (issue #339): `DefaultExternalImpls`
/// carries an injected `reqwest::Client` and one handler per external
/// function — POST `{base_url}/{fn_snake}` with the typed request struct,
/// decoding the typed response. Path/auth are the hand-owned part
/// (`TODO(#339)`).
fn emit_external_hook(code: &mut CodeWriter, external: &[FunctionSurface]) {
    wln!(code);
    wln!(code);
    wln!(
        code,
        "/// Prewired external-service client for the external-bound functions"
    );
    wln!(
        code,
        "/// (issue #339). Register it as `AppState.{{domain}}_external_impls`"
    );
    wln!(
        code,
        "/// and set the `{{DOMAIN}}_SERVICE_BASE_URL` config slot; fill in the"
    );
    wln!(code, "/// endpoint path + auth per function.");
    wln!(code, "pub struct DefaultExternalImpls {{");
    wln!(code, "    pub client: reqwest::Client,");
    wln!(code, "}}");
    wln!(code);
    wln!(code, "impl DefaultExternalImpls {{");
    wln!(code, "    pub fn new() -> Self {{");
    wln!(code, "        Self {{");
    wln!(code, "            client: reqwest::Client::new(),");
    wln!(code, "        }}");
    wln!(code, "    }}");
    wln!(code, "}}");
    wln!(code);
    wln!(code, "impl Default for DefaultExternalImpls {{");
    wln!(code, "    fn default() -> Self {{");
    wln!(code, "        Self::new()");
    wln!(code, "    }}");
    wln!(code, "}}");
    wln!(code);
    wln!(code, "#[async_trait]");
    wln!(code, "impl ExternalFnImpls for DefaultExternalImpls {{");
    for function in external {
        wln!(code);
        if let Some(definition) = &function.node.definition {
            wln!(code, "    /// {definition}");
        }
        wln!(
            code,
            "    async fn {fn}(",
            fn = function.fn_name,
        );
        wln!(code, "        &self,");
        wln!(code, "        base_url: &str,");
        wln!(
            code,
            "        request: {Request},",
            Request = request_struct(&function.node.name),
        );
        wln!(
            code,
            "    ) -> Result<{Response}, String> {{",
            Response = response_struct(&function.node.name),
        );
        wln!(
            code,
            "        // TODO(#339): point this at the real endpoint path + auth scheme."
        );
        wln!(code, "        let url = format!(\"{{base_url}}/{fn}\");", fn = function.fn_name);
        wln!(code, "        let response = self");
        wln!(code, "            .client");
        wln!(code, "            .post(url)");
        wln!(code, "            .json(&request)");
        wln!(code, "            .send()");
        wln!(code, "            .await");
        wln!(
            code,
            "            .map_err(|e| format!(\"{fn} request failed: {{e}}\"))?;",
            fn = function.fn_name,
        );
        if function.output.is_some() {
            wln!(code, "        response");
            wln!(code, "            .error_for_status()");
            wln!(
                code,
                "            .map_err(|e| format!(\"{fn} failed: {{e}}\"))?",
                fn = function.fn_name,
            );
            wln!(
                code,
                "            .json::<{Response}>()",
                Response = response_struct(&function.node.name),
            );
            wln!(code, "            .await");
            wln!(
                code,
                "            .map_err(|e| format!(\"{fn} decode failed: {{e}}\"))",
                fn = function.fn_name,
            );
        } else {
            wln!(
                code,
                "        response.error_for_status().map_err(|e| format!(\"{fn} failed: {{e}}\"))?;",
                fn = function.fn_name,
            );
            wln!(code, "        Ok(())");
        }
        wln!(code, "    }}");
    }
    wln!(code, "}}");
}

/// `(, a, b)` tail for the stub's `let _ = (self…);` discard — empty when
/// the function takes no inputs.
fn paren_tail(args: &[String]) -> String {
    args.iter().map(|a| format!(", {a}")).collect::<String>()
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
            // Copy-from-before seeding (issue #284 slice B): CDM primitive
            // functions start with a wholesale `set {out}: {before}` — the
            // snapshot value semantics. Seed the output local from that
            // input instead of `Default::default()` (which would silently
            // drop every unassigned field of the "before" state) and skip
            // the seeding op.
            match function
                .node
                .operations
                .first()
                .and_then(|op| wholesale_copy_source(op, name, &function.args))
            {
                Some(before) => {
                    wln!(code, "    let mut {name} = {before};");
                    for op in function.node.operations.iter().skip(1) {
                        emit_operation(code, function, op);
                    }
                }
                None => {
                    wln!(code, "    let mut {name} = {ty}::default();");
                    for op in &function.node.operations {
                        emit_operation(code, function, op);
                    }
                }
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

/// The snake_cased input a leading wholesale `set {output}: {input}` copies
/// from, if the first operation is exactly that (empty path, `set`, bare
/// symbol resolving to an argument). This is the CDM copy-from-before
/// authoring pattern (`Create_Reset`, `Create_TradeState`).
fn wholesale_copy_source(
    op: &FunctionOperation,
    output_name: &str,
    args: &[(String, String)],
) -> Option<String> {
    if op.is_add || !op.path.is_empty() {
        return None;
    }
    if !op.assign_root.eq_ignore_ascii_case(output_name)
        && codegraph_naming::to_snake_case(&op.assign_root) != output_name
    {
        return None;
    }
    let payload: serde_json::Value = serde_json::from_str(&op.expr_json).ok()?;
    if payload.get("kind").and_then(|k| k.as_str()) != Some("SymbolReference") {
        return None;
    }
    let symbol = payload.get("symbol").and_then(|s| s.as_str())?;
    let snake = codegraph_naming::to_snake_case(symbol);
    args.iter()
        .find(|(name, _)| *name == snake)
        .map(|(name, _)| name.clone())
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
                if project.integration.has_function_postconditions {
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

// ── HTTP surface (issue #337 B1) ────────────────────────────────────────

/// A function's emitted body is "partial" when it carries a `todo!` or a
/// `TODO(#263)` marker — the record-in-the-module rule. Exposed partial
/// functions answer 501 instead of risking a runtime panic.
fn surface_is_partial(
    surface: &FunctionSurface,
    all: &[FunctionSurface],
    project: &ProjectConfig,
) -> bool {
    let mut code = CodeWriter::new();
    emit_function(&mut code, surface, project, all);
    let text = code.into_string();
    text.contains("todo!") || text.contains("TODO(#263)")
}

/// Emit `src/api/{domain}/functions_api.rs` (issue #337): authenticated
/// POST endpoints for the domain's HTTP-exposed functions, dispatching
/// into the functions module. Complete `dsl` functions call the generated
/// body; partial ones answer 501 naming the gap; `manual` ones call the
/// `ManualImpls` provider from AppState (501 until registered); `sql`
/// ones dispatch through the generated `DefaultSqlImpls` (issue #450).
#[allow(clippy::too_many_arguments)]
fn emit_functions_api_rs(
    domain: &str,
    http: &[&FunctionSurface],
    partial: &HashSet<String>,
    manual: &HashSet<String>,
    sql_names: &HashSet<String>,
    sql_gapped: &HashSet<String>,
    external: &HashSet<String>,
    imports: &BTreeSet<String>,
    api_version: &str,
    project: &ProjectConfig,
) -> String {
    let mut code = CodeWriter::new();
    wln!(
        code,
        "//! HTTP surface for exposed {domain} functions (issues #337 + #450).",
        domain = domain,
    );
    wln!(code, "//!");
    wln!(
        code,
        "//! Generated by {} — DO NOT EDIT. Authenticated POST endpoints",
        project.identity.generator_name
    );
    wln!(
        code,
        "//! dispatching into `crate::domain::{domain}::functions`. Functions"
    );
    wln!(
        code,
        "//! whose bodies are not fully transpilable — and hand-owned functions"
    );
    wln!(
        code,
        "//! without a registered `ManualImpls` provider — answer 501 naming"
    );
    wln!(code, "//! the gap.");
    wln!(code);
    wln!(code, "use axum::extract::State;");
    wln!(code, "use axum::http::StatusCode;");
    wln!(code, "use axum::Json;");
    wln!(code, "use serde::{{Deserialize, Serialize}};");
    wln!(code, "use utoipa::ToSchema;");
    wln!(code);
    for path in imports {
        wln!(code, "use {path};");
    }
    if !imports.is_empty() {
        wln!(code);
    }
    wln!(code, "use crate::app_state::AppState;");
    wln!(code);
    wln!(code, "/// 501 payload naming the function and the gap.");
    wln!(
        code,
        "fn not_implemented(function: &str, hint: &str) -> (StatusCode, Json<serde_json::Value>) {{"
    );
    wln!(code, "    (");
    wln!(code, "        StatusCode::NOT_IMPLEMENTED,");
    wln!(code, "        Json(serde_json::json!({{");
    wln!(code, "            \"error\": \"not_implemented\",");
    wln!(code, "            \"function\": function,");
    wln!(code, "            \"hint\": hint,");
    wln!(code, "        }})),");
    wln!(code, "    )");
    wln!(code, "}}");
    wln!(code);
    wln!(
        code,
        "/// 500 payload for a failed sql-bound call (issue #450)."
    );
    wln!(
        code,
        "fn internal_error(function: &str, error: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {{"
    );
    wln!(code, "    (");
    wln!(code, "        StatusCode::INTERNAL_SERVER_ERROR,");
    wln!(code, "        Json(serde_json::json!({{");
    wln!(code, "            \"error\": \"internal_error\",");
    wln!(code, "            \"function\": function,");
    wln!(code, "            \"detail\": error.to_string(),");
    wln!(code, "        }})),");
    wln!(code, "    )");
    wln!(code, "}}");

    let camel = project.codegen.dto_key_casing == crate::project_config::DtoKeyCasing::Camel;

    for function in http {
        let is_partial = partial.contains(&function.node.name);
        let is_manual = manual.contains(&function.node.name);
        let is_sql = sql_names.contains(&function.node.name);
        let is_sql_gapped = sql_gapped.contains(&function.node.name);
        let is_external = external.contains(&function.node.name);
        emit_function_endpoint(
            &mut code,
            domain,
            api_version,
            function,
            is_partial,
            is_manual,
            is_sql,
            is_sql_gapped,
            is_external,
            camel,
        );
    }

    wln!(code);
    wln!(
        code,
        "/// The domain's exposed-function routes, merged into the domain router"
    );
    wln!(
        code,
        "/// by `router.rs` (auth is inherited from the shared `/api` layer)."
    );
    wln!(
        code,
        "pub fn functions_routes() -> axum::Router<AppState> {{"
    );
    wln!(code, "    use axum::routing::post;");
    wln!(code);
    wln!(code, "    axum::Router::new()");
    for function in http {
        wln!(
            code,
            "        .route(\"/{fn_name}\", post({fn_name}))",
            fn_name = function.fn_name,
        );
    }
    wln!(code, "}}");

    code.into_string()
}

/// Emit one endpoint: request/response DTOs, the `#[utoipa::path]`-attributed
/// handler. (Routes register in `functions_routes`.) `sql_gap` marks a
/// sql-bound function whose modeled body did not lower — its migration
/// stub raises, so the endpoint answers 501 naming the migration.
#[allow(clippy::too_many_arguments)]
fn emit_function_endpoint(
    code: &mut CodeWriter,
    domain: &str,
    api_version: &str,
    function: &FunctionSurface,
    partial: bool,
    manual: bool,
    sql: bool,
    sql_gap: bool,
    external: bool,
    camel: bool,
) {
    let endpoint = &function.fn_name;
    let request = format!(
        "{}Request",
        codegraph_naming::to_pascal_case(&function.node.name)
    );
    let response = format!(
        "{}Response",
        codegraph_naming::to_pascal_case(&function.node.name)
    );
    let serde_attr = if camel {
        "\n#[serde(rename_all = \"camelCase\")]"
    } else {
        ""
    };

    wln!(code);
    if let Some(definition) = &function.node.definition {
        wln!(code, "/// {definition}");
    }
    wln!(
        code,
        "#[derive(Debug, Deserialize, ToSchema)]{serde_attr}",
        serde_attr = serde_attr,
    );
    wln!(code, "pub struct {request} {{");
    for (name, ty) in &function.args {
        wln!(code, "    pub {name}: {ty},");
    }
    wln!(code, "}}");
    wln!(code);
    if let Some(definition) = &function.node.definition {
        wln!(code, "/// {definition}");
    }
    wln!(
        code,
        "#[derive(Debug, Serialize, ToSchema)]{serde_attr}",
        serde_attr = serde_attr,
    );
    wln!(code, "pub struct {response} {{");
    if let Some((out_name, out_ty)) = &function.output {
        wln!(code, "    pub {out_name}: {out_ty},");
    }
    wln!(code, "}}");
    wln!(code);
    if let Some(definition) = &function.node.definition {
        wln!(code, "/// {definition}");
    }
    wln!(code, "#[utoipa::path(");
    wln!(code, "    post,");
    wln!(
        code,
        "    path = \"/api/{api_version}/{domain}/{endpoint}\",",
        api_version = api_version,
        domain = domain,
        endpoint = endpoint,
    );
    wln!(code, "    tag = \"{domain} functions\",", domain = domain,);
    wln!(
        code,
        "    operation_id = \"{domain}_functions_{endpoint}\",",
        domain = domain,
        endpoint = endpoint,
    );
    wln!(code, "    request_body(content = {request}),");
    wln!(code, "    responses(");
    wln!(
        code,
        "        (status = 200, description = \"Function result\", body = {response}),"
    );
    wln!(
        code,
        "        (status = 501, description = \"Function not implemented (partial transpilation or awaiting hand-owned provider)\"),"
    );
    wln!(code, "    ),");
    wln!(code, ")]");
    wln!(
        code,
        "#[tracing::instrument(skip(state, body), fields(domain = \"{domain}\", function = \"{name}\"))]",
        domain = domain,
        name = function.node.name,
    );
    wln!(code, "pub async fn {endpoint}(");
    wln!(code, "    State(state): State<AppState>,");
    wln!(code, "    Json(body): Json<{request}>,");
    wln!(
        code,
        ") -> Result<Json<{response}>, (StatusCode, Json<serde_json::Value>)> {{"
    );

    let arg_list = function
        .args
        .iter()
        .map(|(name, _)| format!("body.{name}"))
        .collect::<Vec<_>>()
        .join(", ");

    if partial {
        wln!(code, "    let _ = (state, body);");
        wln!(code, "    Err(not_implemented(");
        wln!(code, "        \"{name}\",", name = function.node.name,);
        wln!(
            code,
            "        \"function body is not fully transpilable — see \
             crate::domain::{domain}::functions for the TODO(#263) markers naming \
             the missing constructs\",",
            domain = domain,
        );
        wln!(code, "    ))")
    } else if sql && sql_gap {
        // sql-bound whose modeled body did not lower: the migration stub
        // raises, so the endpoint answers 501 naming the migration.
        wln!(code, "    let _ = (state, body);");
        wln!(code, "    Err(not_implemented(");
        wln!(code, "        \"{name}\",", name = function.node.name,);
        wln!(
            code,
            "        \"sql-bound function whose modeled body did not lower to \
             the sql-function subset — the migration stub raises until it \
             lands; see migrations/{:06}_functions.sql and \
             crate::domain::{domain}::functions::sql_impl\",",
            crate::ddd::functions_sql::FUNCTIONS_SQL_MIGRATION_SEQ,
            domain = domain,
        );
        wln!(code, "    ))")
    } else if manual {
        wln!(
            code,
            "    match state.{domain}_manual_impls.as_ref() {{",
            domain = domain,
        );
        wln!(code, "        Some(provider) => {{");
        if let Some((out_name, _)) = &function.output {
            wln!(
                code,
                "            let {out} = provider.{endpoint}({args});",
                out = out_name,
                args = arg_list,
            );
            wln!(
                code,
                "            Ok(Json({response} {{ {out} }}))",
                out = out_name,
            );
        } else {
            wln!(
                code,
                "            provider.{endpoint}({args});",
                args = arg_list,
            );
            wln!(code, "            Ok(Json({response} {{}}))");
        }
        wln!(code, "        }}");
        wln!(code, "        None => Err(not_implemented(");
        wln!(code, "            \"{name}\",", name = function.node.name,);
        wln!(
            code,
            "            \"hand-owned function (implementation 'manual') awaiting its \
             provider — implement crate::domain::{domain}::functions::ManualImpls \
             and set AppState::{domain}_manual_impls\",",
            domain = domain,
        );
        wln!(code, "        )),");
        wln!(code, "    }}");
    } else if external {
        let env_var = format!("{}_SERVICE_BASE_URL", domain.to_uppercase());
        let field_inits = function
            .args
            .iter()
            .map(|(name, _)| format!("{name}: body.{name}"))
            .collect::<Vec<_>>()
            .join(", ");
        wln!(
            code,
            "    let request = crate::domain::{domain}::functions::{Request} {{ {field_inits} }};",
            domain = domain,
            Request = request_struct(&function.node.name),
            field_inits = field_inits,
        );
        wln!(
            code,
            "    match (state.{domain}_external_impls.as_ref(), state.{domain}_service_base_url.as_deref()) {{",
            domain = domain,
        );
        wln!(code, "        (Some(provider), Some(base_url)) => {{");
        wln!(
            code,
            "            let response = provider.{endpoint}(base_url, request).await.map_err(|e| internal_error(\"{name}\", e))?;",
            endpoint = endpoint,
            name = function.node.name,
        );
        if let Some((out_name, _)) = &function.output {
            wln!(
                code,
                "            Ok(Json({response} {{ {out}: response.{out} }}))",
                response = response,
                out = out_name,
            );
        } else {
            wln!(code, "            Ok(Json({response} {{}}))");
        }
        wln!(code, "        }}");
        wln!(code, "        _ => Err(not_implemented(");
        wln!(code, "            \"{name}\",", name = function.node.name,);
        wln!(
            code,
            "            \"external function awaiting its provider — implement \
             crate::domain::{domain}::functions::ExternalFnImpls, set \
             AppState.{domain}_external_impls, and configure the \
             {env_var} env var\",",
            domain = domain,
            env_var = env_var,
        );
        wln!(code, "        )),");
        wln!(code, "    }}");
    } else if sql {
        wln!(
            code,
            "    let provider = crate::domain::{domain}::functions::DefaultSqlImpls;",
            domain = domain,
        );
        if let Some((out_name, _)) = &function.output {
            wln!(
                code,
                "    let {out} = provider.{endpoint}(&state.db, {args}).await.map_err(|e| internal_error(\"{name}\", e))?;",
                out = out_name,
                endpoint = endpoint,
                args = arg_list,
                name = function.node.name,
            );
            wln!(code, "    Ok(Json({response} {{ {out} }}))", out = out_name,);
        } else {
            wln!(
                code,
                "    provider.{endpoint}(&state.db, {args}).await.map_err(|e| internal_error(\"{name}\", e))?;",
                endpoint = endpoint,
                args = arg_list,
                name = function.node.name,
            );
            wln!(code, "    Ok(Json({response} {{}}))");
        }
    } else {
        wln!(code, "    let _ = state;");
        if let Some((out_name, _)) = &function.output {
            wln!(
                code,
                "    let {out} = crate::domain::{domain}::functions::{endpoint}({args});",
                out = out_name,
                domain = domain,
                args = arg_list,
            );
            wln!(code, "    Ok(Json({response} {{ {out} }}))", out = out_name,);
        } else {
            wln!(
                code,
                "    crate::domain::{domain}::functions::{endpoint}({args});",
                domain = domain,
                args = arg_list,
            );
            wln!(code, "    Ok(Json({response} {{}}))");
        }
    }
    wln!(code, "}}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_op(root: &str, path: &[&str], expr_json: &str) -> FunctionOperation {
        FunctionOperation {
            is_add: false,
            assign_root: root.to_string(),
            path: path.iter().map(|p| p.to_string()).collect(),
            expr_json: expr_json.to_string(),
        }
    }

    #[test]
    fn wholesale_copy_detects_set_output_from_bare_input() {
        let op = set_op(
            "reset",
            &[],
            r#"{"kind":"SymbolReference","symbol":"tradeState","args":[],"explicit":false}"#,
        );
        let args = vec![
            ("instruction".to_string(), "ResetInstruction".to_string()),
            ("trade_state".to_string(), "TradeState".to_string()),
        ];
        assert_eq!(
            wholesale_copy_source(&op, "reset", &args).as_deref(),
            Some("trade_state")
        );
    }

    #[test]
    fn wholesale_copy_refuses_pathed_or_add_or_non_input_sources() {
        let args = vec![("trade_state".to_string(), "TradeState".to_string())];
        // `add` never seeds.
        let mut add = set_op(
            "reset",
            &[],
            r#"{"kind":"SymbolReference","symbol":"tradeState","args":[],"explicit":false}"#,
        );
        add.is_add = true;
        assert_eq!(wholesale_copy_source(&add, "reset", &args), None);
        // Pathed assignment (`set out -> field: in`) is not wholesale.
        let pathed = set_op(
            "reset",
            &["reset_history"],
            r#"{"kind":"SymbolReference","symbol":"tradeState","args":[],"explicit":false}"#,
        );
        assert_eq!(wholesale_copy_source(&pathed, "reset", &args), None);
        // Copying from something that is not an argument is not seeding.
        let alias_src = set_op(
            "reset",
            &[],
            r#"{"kind":"SymbolReference","symbol":"helper","args":[],"explicit":false}"#,
        );
        assert_eq!(wholesale_copy_source(&alias_src, "reset", &args), None);
        // Different assign root (`set other: in`) does not seed `reset`.
        let other_root = set_op(
            "other",
            &[],
            r#"{"kind":"SymbolReference","symbol":"tradeState","args":[],"explicit":false}"#,
        );
        assert_eq!(wholesale_copy_source(&other_root, "reset", &args), None);
        // Non-bare expressions (feature calls, literals) do not seed.
        let call = set_op(
            "reset",
            &[],
            r#"{"kind":"FeatureCall","receiver":{"kind":"SymbolReference","symbol":"tradeState"},"feature":"clone"}"#,
        );
        assert_eq!(wholesale_copy_source(&call, "reset", &args), None);
    }
}
