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
//! the guard's doc comment. Handler-site insertion is a documented
//! follow-up — this generator does not rewrite route generation.
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

use crate::api::api_model::{normalized_resource_name, resolve_entity_operations};
use crate::code_writer::{wln, CodeWriter};
use crate::error::Result;
use crate::rosetta_expr::{transpile, ExprContext};
use crate::traits::{DomainGenerator, GeneratedFile};
use crate::ProjectConfig;
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
    fn name(&self) -> &str {
        "rules"
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
        // share an input type).
        let mut knowledge: HashMap<String, FieldKnowledge> = HashMap::new();
        for title in rules.iter().filter_map(|r| r.input_type.as_deref()) {
            if knowledge.contains_key(title) {
                continue;
            }
            let mut deduped: HashSet<String> = HashSet::new();
            let props: Vec<PropertyNode> = db
                .get_properties(title)
                .await?
                .into_iter()
                .filter(|p| deduped.insert(p.rust_field_name.clone()))
                .collect();
            knowledge.insert(title.to_string(), FieldKnowledge::build(&props));
        }

        let mut surfaces: Vec<RuleSurface> = Vec::with_capacity(rules.len());
        for node in &rules {
            surfaces.push(build_surface(db, node, &knowledge, domain, config).await);
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
    domain: &str,
    config: &DomainConfig,
) -> RuleSurface {
    let fn_name = escape_rust_keyword(&to_snake_case(&node.name));
    let input_type = node.input_type.clone();

    let body = match &node.input_type {
        None => Err(TranspileFailure::NoReceiver),
        Some(title) => {
            let empty = FieldKnowledge::default();
            let fields = knowledge.get(title).unwrap_or(&empty);
            let ctx = ExprContext {
                receiver: "input",
                optional_fields: &fields.optional_fields,
                collection_fields: &fields.collection_fields,
                numeric_fields: &fields.numeric_fields,
                integer_fields: &fields.integer_fields,
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
        project.generator_name
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
    // nodes this guard gates.
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
                "/// Guard wiring: gates the `{ops}` ApiOperations — call this guard at",
                ops = rule.guarded_operations.join("`, `"),
            );
            wln!(
                code,
                "/// the handler site (deny with 403 when it returns false). Route-"
            );
            wln!(
                code,
                "/// generator insertion is a documented follow-up, not done here."
            );
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
        input = input,
        ret = return_type,
    );
    wln!(code, "    {body}");
    wln!(code, "}}");
}
