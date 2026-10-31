//! `rules` — rosetta rule codegen (issue #264).
//!
//! Per domain, emits `src/domain/<domain>/rules.rs`: the sigil `Rule`
//! family as Rust free functions, with the sigil source shape mapped as
//! follows (grounded in the sigil `Rule` model — rules carry a name, an
//! optional definition, an `eligibility` flag, an optional `from TypeCall`
//! input, doc references, and ONE expression; they do NOT extend):
//!
//! | sigil source shape                                  | Rust emission |
//! |-----------------------------------------------------|---------------|
//! | `reporting rule N from T: expr` (transpilable, primitive return) | `pub fn n(input: &T) -> R { <expr> }` — the computed-field function attached to entity `T` |
//! | `eligibility rule N from T: expr` (bool-shaped)     | `pub fn n(input: &T) -> bool { <expr> }` — the endpoint guard, with its doc comment naming the `ApiOperation`s it gates |
//! | body the transpiler rejects                          | skipped fn + `// TODO(#264): transpile rule 'N' (unsupported: {kind}: {detail})` marker — the module stays the record, never a guess |
//! | body transpiles but the return type is not primitive | skipped fn + `// TODO(#264): rule 'N' return type unknown (kind {kind})` marker |
//! | no `from` input clause                               | skipped fn + marker (bare symbols have no receiver to resolve against) |
//!
//! Return types are inferred from the expression root (bool for the
//! boolean/comparison/quantifier family, f64 for arithmetic and number
//! literals, i64 for int literals and counts, String for strings/joins);
//! anything else is `None` — codegraph-generate is untyped by design, so
//! non-primitive rules emit markers instead of guessed signatures.
//!
//! Field knowledge for the transpiler (optionality, collections, numeric
//! classes) comes from the input entity's PropertyNodes — the same
//! construction `validations.rs` uses for conditions. Bodies transpile
//! with the `input` receiver (`total` → `input.total`).
//!
//! Eligibility guards resolve their guarded operations through the
//! ApiOperation precedent (`resolve_entity_operations`, the same
//! priority chain the api generator uses) and name the operation nodes in
//! the guard's doc comment. Handler-site insertion (issue #338) IS done
//! here: `eligibility_guards_for` is the shared resolution the
//! `HandlerGenerator` consumes to emit load-or-404 → evaluate → 403
//! blocks before the gated operations, and this generator types every
//! emitted rule function over the entity's real record type
//! (`{Entity}Response`) so the call sites compile. list/search
//! row-filtering remains a documented follow-up.
//!
//! A separate generator from `functions` (issue #263) on purpose: profile
//! independence — consumers can adopt rules without the func machinery —
//! and a different emission shape (single-expression functions vs
//! multi-operation dispatch bodies).
//!
//! Gated behind the `rosetta_backend` profile feature via the capability
//! registry — OFF ⇒ the generator never runs and output is byte-identical.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{PropertyNode, RuleKind, RuleNode};
use codegraph_naming::{escape_rust_keyword, to_snake_case};

use crate::ProjectConfig;
use crate::api::api_model::{normalized_resource_name, resolve_entity_operations};
use crate::code_writer::{CodeWriter, wln};
use crate::error::Result;
use crate::rosetta_expr::{ExprContext, transpile};
use crate::traits::{DomainGenerator, DomainGeneratorKind, GeneratedFile};
use codegraph_config::DomainConfig;

/// Transpiler field knowledge for one input entity (the `validations.rs`
/// construction): snake_case DTO field names keyed by class.
#[derive(Default)]
struct FieldKnowledge {
    optional_fields: HashSet<String>,
    collection_fields: HashSet<String>,
    numeric_fields: HashSet<String>,
    integer_fields: HashSet<String>,
}

impl FieldKnowledge {
    fn build(props: &[PropertyNode]) -> Self {
        let mut knowledge = FieldKnowledge::default();
        for p in props {
            let (numeric, integer) = numeric_class(&p.prop_type, &p.rust_field_type);
            if p.is_nullable {
                knowledge.optional_fields.insert(p.rust_field_name.clone());
            }
            if p.is_array {
                knowledge
                    .collection_fields
                    .insert(p.rust_field_name.clone());
            }
            if numeric {
                knowledge.numeric_fields.insert(p.rust_field_name.clone());
            }
            if integer {
                knowledge.integer_fields.insert(p.rust_field_name.clone());
            }
        }
        knowledge
    }
}

/// Numeric class of a property from its JSON type and DTO field type:
/// `(numeric, integer)` — the `validations.rs` classification. Arrays are
/// unwrapped one `Vec<>` level so `Vec<f64>` reads as a numeric collection
/// element type.
fn numeric_class(prop_type: &str, rust_field_type: &str) -> (bool, bool) {
    let base = rust_field_type
        .strip_prefix("Vec<")
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(rust_field_type);
    let integer = prop_type == "integer"
        || matches!(
            base,
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
    let numeric = integer || prop_type == "number" || matches!(base, "f32" | "f64");
    (numeric, integer)
}

/// Infer the Rust primitive return type from the expression root.
/// `None` = unknown (the rule emits a marker instead of a signature).
fn infer_return_type(payload: &serde_json::Value) -> Option<&'static str> {
    let kind = payload.get("kind").and_then(|k| k.as_str())?;
    match kind {
        "Boolean" | "Exists" | "Absent" | "OnlyExists" => Some("bool"),
        "Binary" => {
            let op = payload.get("op").and_then(|o| o.as_str())?;
            match op {
                "and" | "or" | "<>" | "=" | ">=" | "<=" | ">" | "<" | "contains" | "disjoint" => {
                    Some("bool")
                }
                "+" | "-" | "*" | "/" => Some("f64"),
                _ => None,
            }
        }
        "Number" => Some("f64"),
        "Int" => Some("i64"),
        "Count" => Some("i64"),
        "Sum" | "Min" | "Max" => Some("f64"),
        "String" | "Join" | "ToString" => Some("String"),
        // A conditional's type is its branches' type; a `default`
        // operation's type is the default literal's when the receiver's is
        // unknown.
        "Conditional" => payload.get("then").and_then(infer_return_type),
        "Default" => payload
            .get("right")
            .and_then(infer_return_type)
            .or_else(|| payload.get("left").and_then(infer_return_type)),
        _ => None,
    }
}

/// One rule's emission surface.
struct RuleSurface {
    node: RuleNode,
    /// snake_case Rust fn name.
    fn_name: String,
    /// The Rust input type text (bare title), when the rule has one.
    input_type: Option<String>,
    /// The resolved record type the function takes (issue #338) —
    /// `{Entity}Response` — with its import line. `None` when the input
    /// title does not resolve in the domain (the bare sigil title stays).
    record: Option<(String, String)>,
    /// The transpiled body, or the rejection reason.
    body: std::result::Result<String, TranspileFailure>,
    /// The inferred primitive return type, when known.
    return_type: Option<&'static str>,
    /// For eligibility rules: the guarded ApiOperation node names, in the
    /// api generator's resolution order.
    guarded_operations: Vec<String>,
}

enum TranspileFailure {
    Unsupported { kind: String, detail: String },
    NoReceiver,
    UnknownReturnType { kind: String },
}

/// One handler-site eligibility guard (issue #338): the compiled boolean
/// the `HandlerGenerator` evaluates before the gated operations. Serialized
/// into the Tera context (`#[serde(skip_serializing_if = "Vec::is_empty")]`
/// on the context key — empty ⇒ byte-identical handler).
#[derive(Debug, Clone, serde::Serialize)]
pub struct EligibilityGuard {
    /// The sigil rule name (as authored).
    pub rule_name: String,
    /// Rust fn name in `src/domain/{domain}/rules.rs`.
    pub fn_name: String,
    /// Fully-qualified call path the handler emits
    /// (`crate::domain::{domain}::rules::{fn_name}`).
    pub call_path: String,
    /// The gated operations — the entity's resolved ApiOperations
    /// restricted to `handler_guarded_operations`.
    pub operations: Vec<String>,
}

/// The operations the handler plane guards: load-or-404 → evaluate → 403
/// for create/read/update/delete. `list`/`search` need row filtering — a
/// documented follow-up, never wired here.
fn handler_guarded_operations(resolved: &[String]) -> Vec<String> {
    resolved
        .iter()
        .filter(|op| matches!(op.as_str(), "create" | "read" | "update" | "delete"))
        .cloned()
        .collect()
}

/// Field knowledge for one entity title (the deduped `generate()`
/// construction), shared with the guard resolver.
async fn field_knowledge_for(db: &dyn GraphQuerier, title: &str) -> Result<FieldKnowledge> {
    let mut deduped: HashSet<String> = HashSet::new();
    let props: Vec<PropertyNode> = db
        .get_properties(title)
        .await?
        .into_iter()
        .filter(|p| deduped.insert(p.rust_field_name.clone()))
        .collect();
    Ok(FieldKnowledge::build(&props))
}

/// Transpile one rule body over its input entity's field knowledge.
fn compile_rule_body(
    node: &RuleNode,
    fields: &FieldKnowledge,
) -> std::result::Result<String, TranspileFailure> {
    if node.input_type.is_none() {
        return Err(TranspileFailure::NoReceiver);
    }
    // No enum-type knowledge on the rules plane yet: enum literals keep
    // the untyped receiver-field policy here.
    let no_enums = HashSet::new();
    let ctx = ExprContext {
        receiver: "input",
        optional_fields: &fields.optional_fields,
        collection_fields: &fields.collection_fields,
        numeric_fields: &fields.numeric_fields,
        integer_fields: &fields.integer_fields,
        enum_types: &no_enums,
    };
    let payload: serde_json::Value = serde_json::from_str(&node.expr_json)
        .unwrap_or_else(|_| serde_json::Value::String(node.expr_json.clone()));
    match transpile(&payload, &ctx) {
        Ok(code) => Ok(code),
        Err(e) => Err(TranspileFailure::Unsupported {
            kind: e.kind,
            detail: e.detail,
        }),
    }
}

/// Whether the rule body transpiles AND infers `bool` — the callable-guard
/// decision shared by the generator emission and the handler wiring.
fn rule_compiles_to_bool(node: &RuleNode, fields: &FieldKnowledge) -> bool {
    let payload: serde_json::Value = serde_json::from_str(&node.expr_json)
        .unwrap_or_else(|_| serde_json::Value::String(node.expr_json.clone()));
    if infer_return_type(&payload) != Some("bool") {
        return false;
    }
    compile_rule_body(node, fields).is_ok()
}

/// Resolve the handler-site eligibility guards for one entity (issue
/// #338). Shared by the `HandlerGenerator` (context building) and this
/// module's doc wiring so both planes always agree. Rules whose body does
/// not transpile to `bool` keep the TODO-marker shape — no guard exists
/// to call.
pub async fn eligibility_guards_for(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    schema_title: &str,
) -> Result<Vec<EligibilityGuard>> {
    let mut guards = Vec::new();
    for node in db.list_rules().await? {
        // list_rules orders by (domain, name) — deterministic guard order.
        if node.kind != RuleKind::Eligibility
            || node.domain.as_deref() != Some(domain)
            || node.input_type.as_deref() != Some(schema_title)
        {
            continue;
        }
        let knowledge = field_knowledge_for(db, schema_title).await?;
        if !rule_compiles_to_bool(&node, &knowledge) {
            continue;
        }
        let operations = handler_guarded_operations(
            &resolve_entity_operations(db, config, domain, schema_title).await,
        );
        if operations.is_empty() {
            continue;
        }
        let fn_name = escape_rust_keyword(&to_snake_case(&node.name));
        guards.push(EligibilityGuard {
            rule_name: node.name.clone(),
            call_path: format!("crate::domain::{domain}::rules::{fn_name}"),
            fn_name,
            operations,
        });
    }
    Ok(guards)
}

/// The record type + import path for a rule input entity: the schema's
/// `{rust_type_name}Response` from `crate::domain/{domain}/{table}/
/// dto_response`. `None` when the title does not resolve in the domain —
/// emission keeps the bare sigil title (the pre-#338 shape).
async fn record_type_for(
    db: &dyn GraphQuerier,
    domain: &str,
    title: &str,
) -> Result<Option<(String, String)>> {
    let Some(schema) = db.get_schema_in_domain(title, domain).await? else {
        return Ok(None);
    };
    if schema.pg_table_name.is_empty() {
        return Ok(None);
    }
    let record_type = format!("{}Response", schema.rust_type_name);
    let import = format!(
        "use crate::domain::{domain}::{}::dto_response::{record_type};",
        schema.pg_table_name
    );
    Ok(Some((record_type, import)))
}

pub struct RulesGenerator {
    output_dir: PathBuf,
}

impl RulesGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl DomainGenerator for RulesGenerator {
    fn kind(&self) -> DomainGeneratorKind {
        DomainGeneratorKind::Rules
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
        // list_rules orders by (domain, name); the per-domain filter
        // preserves that — the deterministic emission order.
        let rules: Vec<RuleNode> = db
            .list_rules()
            .await?
            .into_iter()
            .filter(|r| r.domain.as_deref() == Some(domain))
            .collect();
        if rules.is_empty() {
            return Ok(Vec::new());
        }

        // Field knowledge per input entity (deduped — several rules may
        // share an input type), plus the record type each input resolves
        // to (issue #338: emitted functions take the entity's real
        // `{Entity}Response` so handler guard call sites compile).
        let mut knowledge: HashMap<String, FieldKnowledge> = HashMap::new();
        let mut record_types: HashMap<String, (String, String)> = HashMap::new();
        for title in rules.iter().filter_map(|r| r.input_type.as_deref()) {
            if knowledge.contains_key(title) {
                continue;
            }
            knowledge.insert(title.to_string(), field_knowledge_for(db, title).await?);
            if let Some(resolved) = record_type_for(db, domain, title).await? {
                record_types.insert(title.to_string(), resolved);
            }
        }

        let mut surfaces: Vec<RuleSurface> = Vec::with_capacity(rules.len());
        for node in &rules {
            surfaces.push(build_surface(db, node, &knowledge, &record_types, domain, config).await);
        }

        let content = emit_domain_rules(domain, &surfaces, project);
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("domain")
                .join(domain)
                .join("rules.rs"),
            content,
        }])
    }
}

/// Resolve one rule node into its emission surface.
async fn build_surface(
    db: &dyn GraphQuerier,
    node: &RuleNode,
    knowledge: &HashMap<String, FieldKnowledge>,
    record_types: &HashMap<String, (String, String)>,
    domain: &str,
    config: &DomainConfig,
) -> RuleSurface {
    let fn_name = escape_rust_keyword(&to_snake_case(&node.name));
    let input_type = node.input_type.clone();
    let record = input_type
        .as_deref()
        .and_then(|title| record_types.get(title).cloned());

    let body = match &node.input_type {
        None => Err(TranspileFailure::NoReceiver),
        Some(title) => {
            let empty = FieldKnowledge::default();
            let fields = knowledge.get(title).unwrap_or(&empty);
            compile_rule_body(node, fields)
        }
    };

    let payload: serde_json::Value = serde_json::from_str(&node.expr_json)
        .unwrap_or_else(|_| serde_json::Value::String(node.expr_json.clone()));
    let inferred = infer_return_type(&payload);
    // Guards are boolean or nothing; reporting rules take any primitive.
    let eligible = match inferred {
        Some("bool") => true,
        _ => node.kind == RuleKind::Reporting && inferred.is_some(),
    };
    let body = match body {
        Ok(code) if eligible => Ok(code),
        Ok(_) => Err(TranspileFailure::UnknownReturnType {
            kind: payload
                .get("kind")
                .and_then(|k| k.as_str())
                .unwrap_or("unknown")
                .to_string(),
        }),
        // An unsupported body subsumes the unknown-return case: the
        // marker names the transpiler rejection.
        Err(failure) => Err(failure),
    };
    let return_type = if body.is_ok() { inferred } else { None };

    // Endpoint-guard wiring follows the ApiOperation precedent:
    // resolve_entity_operations (explicit config > graph ops > defaults),
    // operation nodes named `{kind}_{Resource}`.
    let guarded_operations = match (&node.kind, &node.input_type) {
        (RuleKind::Eligibility, Some(input)) => {
            let resource = normalized_resource_name(input);
            resolve_entity_operations(db, config, domain, input)
                .await
                .into_iter()
                .map(|kind| format!("{kind}_{resource}"))
                .collect()
        }
        _ => Vec::new(),
    };

    RuleSurface {
        node: node.clone(),
        fn_name,
        input_type,
        record,
        body,
        return_type,
        guarded_operations,
    }
}

/// Emit the complete `rules.rs` for one domain (deterministic by
/// construction — surfaces arrive name-ordered).
fn emit_domain_rules(domain: &str, rules: &[RuleSurface], project: &ProjectConfig) -> String {
    let mut code = CodeWriter::new();
    wln!(
        code,
        "//! Rosetta rules for the {domain} domain (issue #264).",
        domain = domain,
    );
    wln!(code, "//!");
    wln!(
        code,
        "//! Generated by {} — DO NOT EDIT. Reporting rules are computed-",
        project.identity.generator_name
    );
    wln!(
        code,
        "//! field functions attached to their input entity; eligibility"
    );
    wln!(
        code,
        "//! rules are endpoint guards. Bodies are transpiled from the"
    );
    wln!(
        code,
        "//! graphed `Expr::to_json` payloads; unsupported constructs carry"
    );
    wln!(code, "//! TODO(#264) markers instead of guesses.");
    wln!(code, "#![allow(dead_code)]");

    // Record-type imports (issue #338): every emitted function takes the
    // entity's real `{Entity}Response` from its dto_response module.
    let mut imports: Vec<&String> = rules
        .iter()
        .filter_map(|r| r.record.as_ref().map(|(_, import)| import))
        .collect();
    imports.sort();
    imports.dedup();
    for import in imports {
        wln!(code, "{import}");
    }

    let reporting: Vec<&RuleSurface> = rules
        .iter()
        .filter(|r| r.node.kind == RuleKind::Reporting)
        .collect();
    let eligibility: Vec<&RuleSurface> = rules
        .iter()
        .filter(|r| r.node.kind == RuleKind::Eligibility)
        .collect();

    if !reporting.is_empty() {
        wln!(code);
        wln!(
            code,
            "// ── Reporting rules (computed fields) ──────────────"
        );
        for rule in &reporting {
            emit_rule(&mut code, rule);
        }
    }
    if !eligibility.is_empty() {
        wln!(code);
        wln!(
            code,
            "// ── Eligibility rules (endpoint guards) ────────────"
        );
        for rule in &eligibility {
            emit_rule(&mut code, rule);
        }
    }

    code.into_string()
}

/// Emit one rule function (or its TODO marker block).
fn emit_rule(code: &mut CodeWriter, rule: &RuleSurface) {
    wln!(code);
    if let Some(definition) = &rule.node.definition {
        wln!(code, "/// {definition}");
    }
    if let Some(input) = &rule.input_type {
        wln!(code, "///");
        wln!(
            code,
            "/// Computed from {} — attached to the `{input}` entity (RuleAppliesTo).",
            rule.node.kind.as_str(),
            input = input,
        );
    }
    // Guard wiring follows the ApiOperation precedent: name the operation
    // nodes this guard gates, and state the #338 handler-site contract.
    if rule.node.kind == RuleKind::Eligibility {
        if rule.guarded_operations.is_empty() {
            wln!(code, "///");
            wln!(
                code,
                "/// Guard wiring: no ApiOperations resolved for this entity — the guard"
            );
            wln!(
                code,
                "/// stays unwired until the entity joins the API model."
            );
        } else {
            wln!(code, "///");
            wln!(
                code,
                "/// Guard wiring: gates the `{ops}` ApiOperations — handler-site guards",
                ops = rule.guarded_operations.join("`, `"),
            );
            wln!(
                code,
                "/// (issue #338) load the record (404 when absent), evaluate this guard,"
            );
            wln!(
                code,
                "/// and deny with 403 when it returns false. list/search row-filtering"
            );
            wln!(code, "/// is a documented follow-up.");
        }
    }

    let Some(input) = &rule.input_type else {
        wln!(
            code,
            "// TODO(#264): rule '{name}' declares no `from` input — bare symbols have",
            name = rule.node.name,
        );
        wln!(
            code,
            "// no receiver to resolve against; no function emitted."
        );
        return;
    };
    let body = match &rule.body {
        Ok(body) => body,
        Err(TranspileFailure::Unsupported { kind, detail }) => {
            wln!(
                code,
                "// TODO(#264): transpile rule '{name}' (unsupported: {kind}: {detail})",
                name = rule.node.name,
            );
            return;
        }
        Err(TranspileFailure::UnknownReturnType { kind }) => {
            wln!(
                code,
                "// TODO(#264): rule '{name}' return type unknown (kind {kind}) —",
                name = rule.node.name,
            );
            wln!(code, "// non-primitive rules emit no function until return");
            wln!(code, "// types land in the graph.");
            return;
        }
        Err(TranspileFailure::NoReceiver) => unreachable!("handled above"),
    };
    let Some(return_type) = rule.return_type else {
        return;
    };

    wln!(
        code,
        "pub fn {fn}(input: &{input}) -> {ret} {{",
        fn = rule.fn_name,
        // Issue #338: the function takes the entity's real record type so
        // handler guard call sites compile; an unresolvable input keeps
        // the bare sigil title (pre-#338 shape).
        input = rule
            .record
            .as_ref()
            .map(|(record, _)| record.clone())
            .unwrap_or_else(|| input.clone()),
        ret = return_type,
    );
    wln!(code, "    {body}");
    wln!(code, "}}");
}
