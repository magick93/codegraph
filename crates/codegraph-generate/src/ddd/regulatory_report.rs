//! `regulatory_reports` — regulatory-plane codegen SCAFFOLDING (issue #265).
//!
//! Per domain, emits `src/domain/<domain>/regulatory_reports.rs`: one
//! module per corpus with an API endpoint stub and a rule-dispatch table
//! seeded from the corpus' reports (report → corpus/segment
//! `RegulatoryReference` edges are captured on the node's `regulatory`
//! properties by the bridge; the dispatch arms come from the report's
//! segment references), plus rule-source and `[transform]` hook stubs.
//!
//! Function nodes LANDED (#263): the transform-hook stubs reference their
//! bridged functions by name instead of awaiting them. Rule nodes LANDED
//! (#264): the dispatch arms resolve the reporting rules bound through the
//! report's rule source (`RuleReference` edges from the querier) and
//! `bindings_registered()` counts the resolvable rule-source bindings —
//! the rule implementations themselves emit via the `rules` generator.
//! The genuinely-deferred seams (axum handler wiring for the corpus
//! endpoint, wire-format binding for transform hooks) keep `TODO(#264)`
//! markers. No runtime behavior is claimed — the emitted module is
//! deliberately inert (`match` arms return `None`, hooks have empty
//! bodies) so it compiles standalone.
//!
//! Gated behind the `rosetta_backend` profile feature via the capability
//! registry — OFF ⇒ the generator never runs and output is byte-identical.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{RegulatoryKind, RegulatoryNode, RuleRefRecord};
use codegraph_naming::{escape_rust_keyword, to_snake_case};

use crate::code_writer::{wln, CodeWriter};
use crate::error::Result;
use crate::traits::{DomainGenerator, GeneratedFile};
use crate::ProjectConfig;
use codegraph_config::DomainConfig;

/// One report's dispatch surface, flattened from its `regulatory`
/// properties payload.
struct ReportDispatch {
    /// Synthesized report name (the RegulatoryNode name).
    name: String,
    /// `(segment name, reference)` pairs in declaration order.
    segments: Vec<(String, String)>,
    /// Rule source name, when the report carries `with source S`.
    rule_source: Option<String>,
    /// Reporting rules bound through the rule source (querier-resolved,
    /// source-ordered then deduped).
    rules: Vec<String>,
}

/// One corpus' report surface.
struct CorpusReport {
    name: String,
    label: Option<String>,
    reports: Vec<ReportDispatch>,
}

/// One rule-source class attribute (`(+|-) attr [ruleReference R]*`).
struct ExternalAttribute {
    add: bool,
    attribute: String,
    rule_references: Vec<String>,
}

/// One `[transform]` hook captured on a rule schema.
struct TransformHook {
    function: String,
    kind: String,
}

/// One rule schema's surface.
struct RuleSchemaSurface {
    name: String,
    format: String,
    hooks: Vec<TransformHook>,
}

/// One rule source's surface: its classes with typed attributes.
struct RuleSourceSurface {
    name: String,
    classes: Vec<(String, Vec<ExternalAttribute>)>,
}

pub struct RegulatoryReportGenerator {
    output_dir: PathBuf,
}

impl RegulatoryReportGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl DomainGenerator for RegulatoryReportGenerator {
    fn name(&self) -> &str {
        "regulatory_reports"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        domain: &str,
        _entity_titles: &[String],
        _config: &DomainConfig,
        _tera: &tera::Tera,
        _project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let nodes: Vec<RegulatoryNode> = db
            .list_regulatory()
            .await?
            .into_iter()
            .filter(|node| node.domain.as_deref() == Some(domain))
            .collect();
        // Function nodes landed (#263): transform hooks reference their
        // bridged functions by name.
        let function_names: HashSet<String> = db
            .list_functions()
            .await?
            .into_iter()
            .map(|function| function.name)
            .collect();
        // Rule nodes landed (#264): rule-source bindings resolve through
        // the querier (RuleReference edges), so dispatch arms and
        // bindings_registered() name real rules.
        let rule_refs: Vec<RuleRefRecord> = db.list_rule_references().await?;

        let mut corpora: Vec<CorpusReport> = Vec::new();
        let mut reports: Vec<ReportDispatch> = Vec::new();
        let mut rule_sources: Vec<RuleSourceSurface> = Vec::new();
        let mut schemas: Vec<RuleSchemaSurface> = Vec::new();

        for node in &nodes {
            match node.kind {
                RegulatoryKind::Corpus => {
                    corpora.push(CorpusReport {
                        name: node.name.clone(),
                        label: node.label.clone(),
                        reports: Vec::new(),
                    });
                }
                RegulatoryKind::Report => {
                    if let Some(mut dispatch) = report_dispatch(node) {
                        if let Some(source) = &dispatch.rule_source {
                            let mut seen: HashSet<String> = HashSet::new();
                            dispatch.rules = rule_refs
                                .iter()
                                .filter(|r| &r.rule_source == source && seen.insert(r.rule.clone()))
                                .map(|r| r.rule.clone())
                                .collect();
                        }
                        reports.push(dispatch);
                    }
                }
                RegulatoryKind::RuleSource => {
                    rule_sources.push(rule_source_surface(node));
                }
                RegulatoryKind::RuleSchema => {
                    schemas.push(rule_schema_surface(node));
                }
                _ => {}
            }
        }

        // Attach reports to their corpora: a report's synthesized name
        // embeds its body + corpora (`Report <body> <corpus...> (<timing>)`,
        // see the bridge), so a name lookup homes each report.
        for dispatch in reports {
            let home = corpora
                .iter_mut()
                .find(|corpus| dispatch.name.contains(&format!(" {}", corpus.name)));
            match home {
                Some(corpus) => corpus.reports.push(dispatch),
                // A report naming no bridged corpus is impossible via the
                // resolver (regulatory refs are checked); the fallback only
                // guards corpus-less degenerate models.
                None => {
                    if let Some(corpus) = corpora.first_mut() {
                        corpus.reports.push(dispatch);
                    }
                }
            }
        }

        if corpora.is_empty() && rule_sources.is_empty() && schemas.is_empty() {
            return Ok(Vec::new());
        }

        let content = emit_regulatory_reports(
            domain,
            &corpora,
            &rule_sources,
            &schemas,
            &function_names,
            &rule_refs,
        );
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("domain")
                .join(domain)
                .join("regulatory_reports.rs"),
            content,
        }])
    }
}

/// Flatten a report node's `regulatory` properties payload.
fn report_dispatch(node: &RegulatoryNode) -> Option<ReportDispatch> {
    let regulatory = node.properties.get("regulatory")?;
    let segments = regulatory
        .get("segments")?
        .as_array()?
        .iter()
        .filter_map(|segment| {
            Some((
                segment.get("segment")?.as_str()?.to_string(),
                segment.get("reference")?.as_str()?.to_string(),
            ))
        })
        .collect();
    Some(ReportDispatch {
        name: node.name.clone(),
        segments,
        rule_source: node
            .properties
            .get("rule_source")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        // Querier-resolved per report in `generate` (needs the
        // RuleReference edge list).
        rules: Vec::new(),
    })
}

/// Flatten a rule-source node into its surface (classes with typed
/// attributes).
fn rule_source_surface(node: &RegulatoryNode) -> RuleSourceSurface {
    let classes = node
        .properties
        .get("classes")
        .and_then(serde_json::Value::as_array)
        .map(|classes| {
            classes
                .iter()
                .filter_map(|class| {
                    let data = class.get("data")?.as_str()?.to_string();
                    let attributes = class
                        .get("attributes")
                        .and_then(serde_json::Value::as_array)
                        .map(|attributes| {
                            attributes
                                .iter()
                                .filter_map(|attribute| {
                                    Some(ExternalAttribute {
                                        add: attribute.get("add")?.as_bool()?,
                                        attribute: attribute
                                            .get("attribute")?
                                            .as_str()?
                                            .to_string(),
                                        rule_references: attribute
                                            .get("rule_references")
                                            .and_then(serde_json::Value::as_array)
                                            .map(|refs| {
                                                refs.iter()
                                                    .filter_map(|r| {
                                                        r.get("rule")?.as_str().map(str::to_string)
                                                    })
                                                    .collect::<Vec<String>>()
                                            })
                                            .unwrap_or_default(),
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    Some((data, attributes))
                })
                .collect()
        })
        .unwrap_or_default();
    RuleSourceSurface {
        name: node.name.clone(),
        classes,
    }
}

/// Flatten a rule-schema node into its `[transform]` hooks.
fn rule_schema_surface(node: &RegulatoryNode) -> RuleSchemaSurface {
    let hooks = node
        .properties
        .get("transform_annotations")
        .and_then(serde_json::Value::as_array)
        .map(|hooks| {
            hooks
                .iter()
                .filter_map(|hook| {
                    Some(TransformHook {
                        function: hook.get("function")?.as_str()?.to_string(),
                        kind: hook.get("kind")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    RuleSchemaSurface {
        name: node.name.clone(),
        format: node
            .properties
            .get("format")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        hooks,
    }
}

/// Emit the complete `regulatory_reports.rs` for one domain (deterministic
/// by construction — nodes arrive kind+name ordered from the querier).
fn emit_regulatory_reports(
    domain: &str,
    corpora: &[CorpusReport],
    rule_sources: &[RuleSourceSurface],
    schemas: &[RuleSchemaSurface],
    function_names: &HashSet<String>,
    rule_refs: &[RuleRefRecord],
) -> String {
    let mut code = CodeWriter::new();
    wln!(
        code,
        "//! Regulatory report scaffolding for the {domain} domain (issue #265).",
        domain = domain,
    );
    wln!(code, "//!");
    wln!(
        code,
        "//! Function nodes landed (#263) and rule nodes landed (#264): the"
    );
    wln!(
        code,
        "//! transform hooks reference their bridged functions and the rule"
    );
    wln!(
        code,
        "//! dispatch resolves the rules bound through each report's rule"
    );
    wln!(
        code,
        "//! source (the rule implementations themselves emit via the"
    );
    wln!(
        code,
        "//! `rules` generator). The deferred seams — axum handler wiring for"
    );
    wln!(
        code,
        "//! the corpus endpoint and transform-hook wire-format binding —"
    );
    wln!(
        code,
        "//! carry TODO(#264) markers. This module is deliberately inert —"
    );
    wln!(
        code,
        "//! dispatch arms return `None` and hooks have empty bodies — so it"
    );
    wln!(code, "//! compiles standalone.");
    wln!(code, "#![allow(dead_code)]");
    wln!(code);

    for corpus in corpora {
        emit_corpus(&mut code, corpus);
    }
    for source in rule_sources {
        emit_rule_source(&mut code, source, rule_refs);
    }
    for schema in schemas {
        emit_rule_schema_hooks(&mut code, schema, function_names);
    }
    code.into_string()
}

/// Emit one corpus module: endpoint stub + rule-dispatch table.
fn emit_corpus(code: &mut CodeWriter, corpus: &CorpusReport) {
    let module = escape_rust_keyword(&to_snake_case(&corpus.name));
    wln!(
        code,
        "/// Corpus {name} ({display}) — per-corpus report skeleton (issue #265).",
        name = corpus.name,
        display = corpus.label.as_deref().unwrap_or("no display name"),
    );
    wln!(code, "pub mod {module} {{", module = module,);
    wln!(
        code,
        "    /// Rule-dispatch table for corpus {corpus}, seeded from the reports'",
        corpus = corpus.name,
    );
    wln!(
        code,
        "    /// segment references. Rule nodes landed (#264): each arm names"
    );
    wln!(
        code,
        "    /// the reporting rules bound through the report's rule source"
    );
    wln!(
        code,
        "    /// (their computed-field functions emit via the `rules`"
    );
    wln!(
        code,
        "    /// generator); the dispatch still returns `None` — this module"
    );
    wln!(code, "    /// is deliberately inert scaffolding.");
    wln!(
        code,
        "    pub fn {module}_report_dispatch(segment: &str) -> Option<&'static str> {{"
    );
    wln!(code, "        match segment {{");
    let mut seen_segments: Vec<(String, String)> = Vec::new();
    for report in &corpus.reports {
        for (segment, reference) in &report.segments {
            let key = (segment.clone(), reference.clone());
            if seen_segments.contains(&key) {
                continue;
            }
            seen_segments.push(key);
            wln!(code, "            // {report}", report = report.name,);
            match (&report.rule_source, report.rules.as_slice()) {
                (Some(source), []) => {
                    wln!(
                        code,
                        "            // with source {source} — no rules bound to its classes.",
                        source = source,
                    );
                }
                (Some(source), rules) => {
                    wln!(
                        code,
                        "            // with source {source} — rules: {rules}.",
                        source = source,
                        rules = rules.join(", "),
                    );
                    wln!(
                        code,
                        "            // Rule fns: super::rules::{{{names}}} (via the `rules` \
                         generator).",
                        names = rules
                            .iter()
                            .map(|r| to_snake_case(r))
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                }
                (None, _) => {
                    wln!(
                        code,
                        "            // no rule source — segment {segment} has no bound rules.",
                        segment = segment,
                    );
                }
            }
            wln!(code, "            {reference:?} => {{");
            wln!(code, "                None");
            wln!(code, "            }}");
        }
    }
    if corpus.reports.is_empty() {
        wln!(code, "            // No reports reference this corpus yet.");
    }
    wln!(code, "            _ => None,");
    wln!(code, "        }}");
    wln!(code, "    }}");
    wln!(code);
    wln!(
        code,
        "    /// API endpoint stub for the corpus report (GET shape)."
    );
    wln!(code, "    ///");
    wln!(
        code,
        "    /// TODO(#264): wire the axum handler + query params to"
    );
    wln!(
        code,
        "    /// {module}_report_dispatch (route-generation wiring is a follow-up).",
        module = module,
    );
    wln!(
        code,
        "    pub async fn {module}_report_endpoint() -> Option<&'static str> {{"
    );
    wln!(
        code,
        "        // Dispatches through {module}_report_dispatch once the route wiring",
        module = module,
    );
    wln!(code, "        // lands.");
    wln!(code, "        None");
    wln!(code, "    }}");
    wln!(code, "}}");
    wln!(code);
}

/// Emit one rule-source section: class attributes + rule references as
/// documented stubs.
fn emit_rule_source(
    code: &mut CodeWriter,
    source: &RuleSourceSurface,
    rule_refs: &[RuleRefRecord],
) {
    let module = escape_rust_keyword(&to_snake_case(&source.name));
    // Rule nodes landed (#264): the bindings this source actually expresses
    // (querier-resolved RuleReference edges), used for the count below.
    let registered: Vec<&RuleRefRecord> = rule_refs
        .iter()
        .filter(|r| r.rule_source == source.name)
        .collect();
    wln!(
        code,
        "/// Rule source {name} — external reporting-rule bindings (issue #265).",
        name = source.name,
    );
    wln!(code, "pub mod {module} {{", module = module,);
    for (data, attributes) in &source.classes {
        wln!(code, "    /// Class {data}:", data = data,);
        for attribute in attributes {
            let operator = if attribute.add { "+" } else { "-" };
            let references = if attribute.rule_references.is_empty() {
                String::new()
            } else {
                format!(" -> rules [{}]", attribute.rule_references.join(", "))
            };
            wln!(
                code,
                "    ///   {operator} {attr}{references}",
                operator = operator,
                attr = attribute.attribute,
                references = references,
            );
        }
    }
    wln!(code, "    ///");
    if registered.is_empty() {
        wln!(
            code,
            "    /// No rules of this run are bound to this source's classes."
        );
    } else {
        wln!(
            code,
            "    /// Bound rules: {rules} — their computed-field functions emit via the",
            rules = registered
                .iter()
                .map(|r| r.rule.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        );
        wln!(code, "    /// `rules` generator (`super::rules`).");
    }
    wln!(code, "    pub fn bindings_registered() -> usize {{");
    wln!(code, "        {}", registered.len());
    wln!(code, "    }}");
    wln!(code, "}}");
    wln!(code);
}

/// Emit `[transform]` hook stubs captured on one rule schema. Function
/// nodes landed (#263): each hook references its bridged function's
/// generated fn; the wire-format binding stays a #264 seam.
fn emit_rule_schema_hooks(
    code: &mut CodeWriter,
    schema: &RuleSchemaSurface,
    function_names: &HashSet<String>,
) {
    if schema.hooks.is_empty() {
        return;
    }
    let module = escape_rust_keyword(&to_snake_case(&schema.name));
    wln!(
        code,
        "/// Rule schema {name} (format {format}) — transform hooks (issue #265).",
        name = schema.name,
        format = schema.format,
    );
    wln!(code, "pub mod {module} {{", module = module,);
    for hook in &schema.hooks {
        let hook_name = escape_rust_keyword(&to_snake_case(&hook.function));
        wln!(code);
        wln!(
            code,
            "    /// `[{kind}]` targeting this schema, captured from function `{name}`.",
            kind = hook.kind,
            name = hook.function,
        );
        if function_names.contains(&hook.function) {
            wln!(
                code,
                "    /// Function `{name}` is bridged; its body emits via the `functions` \
                 generator (`{fn_name}`).",
                name = hook.function,
                fn_name = hook_name,
            );
        } else {
            wln!(
                code,
                "    /// TODO(#263): function `{name}` is not bridged in this graph.",
                name = hook.function,
            );
        }
        wln!(
            code,
            "    /// TODO(#264): the transform-hook wire-format binding is unwired."
        );
        wln!(
            code,
            "    pub fn {hook_name}_hook() {{",
            hook_name = hook_name,
        );
        wln!(code, "    }}");
    }
    wln!(code, "}}");
    wln!(code);
}
