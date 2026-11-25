//! The upstream semantic graph + graph-query analytics as a doctor
//! validation plane (issue #673 — P6 of the pherolang 0.3/0.4 roadmap).
//!
//! Upstream ships a unified, versioned semantic graph
//! (`phero_ir::graph::GraphModel`, built by `phero_driver::graph::build`
//! from the compiled artifacts of every surface) plus four artifact-only
//! analytics: `separationOfDuty`, `monotonicity`, `deadCapabilities`, and
//! `reachability` (`phero_driver::graph::analytics`). They encode exactly
//! the cross-policy questions codegraph had no check for:
//!
//! - **separation-of-duty** — an actor holding both sides of a declared
//!   `never_both` pair. The pherolang actor compiler rejects a violation
//!   only when the grants and the constraint share one file; policies
//!   MERGED across multiple imports (each file valid, union violates)
//!   were invisible to every codegraph check (the service-op plane emits
//!   *runtime* gates for them, nothing reported them statically).
//! - **monotonicity** — an `extends` edge where the child cannot do
//!   everything the parent may.
//! - **dead capabilities** — a capability every permitting actor also
//!   forbids (`latent` — declared, never granted — is deliberately NOT a
//!   finding, and neither is `unknown`).
//!
//! `reachability` needs an explicit target, so it has no sweep semantics;
//! it rides doctor as the `--graph-reachability` passthrough query.
//!
//! # The artifact-alone contract
//!
//! Analysis never reads sources or recompiles: [`UnionGather::build_model`]
//! serializes the built model and deserializes it again before the
//! analytics run, so the plane structurally consumes the versioned wire
//! artifact exactly as upstream promises — and `--graph-artifact` lets a
//! doctor run analyze a committed artifact with the verdict driven by the
//! artifact alone (pinned by
//! `graph_artifact_mode_needs_no_recompile`).
//!
//! # Shared gather (deliverable 3)
//!
//! The gather below and `check_union_integrity`'s cross-surface gather
//! are ONE function ([`gather_cross_surface`]): the doctor integrity pass
//! and the graph plane consume the same compiled artifacts, so the two
//! cannot drift. The Grafeo actor graph (the run path's generation store)
//! and the `GraphModel` artifact stay separate projections of the same
//! compiled `ActorModel`s — feeding one projection from the other would
//! invert a dependency for no generation benefit (documented in
//! `docs/pherolang-upgrade.md`).
//!
//! # Ask 1 protected-op join (issue #619)
//!
//! [`report_protected_op_dead_triggers`] is the one cross-surface check this
//! module owns besides the analytics: doctor's half of the run path's
//! "a `protected` op warns only when an `.ifml` action binds it" WARN,
//! joined over the compiled designs + flows the gather already holds.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use phero_driver::graph::analytics::{
    AnalyticsError, Finding, MonotonicityOptions, ReachabilityOptions, SeparationOfDutyOptions,
    dead_capabilities, monotonicity, reachability, separation_of_duty,
};
use phero_driver::graph::{GraphFile, GraphIndex, GraphInputs, build};
use phero_ir::ddd::DddModel;
use phero_ir::deploy::DeployModel;
use phero_ir::events::EventModel;
use phero_ir::graph::GraphModel;
use phero_ir::ifml::IfmlModel;
use phero_ir::{ActorModel, Model};

use crate::ifml_actor_import::resolve_actor_imports;
use crate::ingest::ddd_ingest::read_and_compile_ddd;
use crate::ingest::deploy_ingest::read_and_resolve_deploy;
use crate::ingest::evt_ingest::read_and_compile_evt;
use crate::ingest::phero_imports::scan_imports;
use crate::init::commands::{DoctorArgs, MoxSources};

/// One definite analytic finding, in upstream vocabulary.
///
/// `analytic` is the envelope's camelCase query name
/// (`separationOfDuty`/`monotonicity`/`deadCapabilities`/`reachability`);
/// `subject` and `detail` carry the upstream node ids so a finding reads
/// the same here as in `pherolang graph query` output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphFinding {
    pub analytic: &'static str,
    pub subject: String,
    pub detail: String,
}

/// The reachability passthrough's parsed spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReachQuery {
    pub from_actor: Option<String>,
    pub from_capability: Option<String>,
    pub to: String,
}

/// One reachability answer: whether the target was reached, the upstream
/// envelope JSON (verbatim, so doctor output carries the witness path),
/// and the finding when unreachable.
#[derive(Debug, Clone)]
pub struct ReachOutcome {
    pub reachable: bool,
    pub envelope_json: String,
    pub findings: Vec<GraphFinding>,
}

/// The gathered compiled artifacts of one doctor invocation, keyed the
/// way [`phero_driver::graph::GraphInputs`] consumes them.
///
/// Every field is best-effort over the compiling subset (the doctor
/// per-surface checks report the failures); an unresolved reference
/// produces no graph edge, never a guess — upstream's derivation rule.
#[derive(Debug, Clone, Default)]
pub struct UnionGather {
    /// `.mox` domain models, one [`GraphFile`] per source file (the mox
    /// union model split by package — each file declares exactly one).
    pub domains: Vec<GraphFile<Model>>,
    /// `.actor` policy artifacts, one per resolved policy import.
    pub actors: Vec<GraphFile<ActorModel>>,
    /// `.ddd` design artifacts.
    pub designs: Vec<GraphFile<DddModel>>,
    /// `.evt` event-contract artifacts.
    pub events: Vec<GraphFile<EventModel>>,
    /// `.deploy` deployment artifacts.
    pub deploys: Vec<GraphFile<DeployModel>>,
    /// `.ifml` interaction-flow artifacts.
    pub flows: Vec<GraphFile<IfmlModel>>,
}

impl UnionGather {
    /// Whether the gather carries ANY compiled artifact.
    pub fn is_empty(&self) -> bool {
        self.domains.is_empty()
            && self.actors.is_empty()
            && self.designs.is_empty()
            && self.events.is_empty()
            && self.deploys.is_empty()
            && self.flows.is_empty()
    }

    /// Whether policy-bearing artifacts are present (the sweeps' input).
    pub fn has_policy_inputs(&self) -> bool {
        !self.actors.is_empty()
    }

    /// Build the semantic graph and return the SERIALIZED artifact —
    /// `build` → `to_json_pretty` → `from_json`. The analytics consume
    /// the wire artifact alone (the module docs' contract).
    pub fn build_model(&self) -> std::result::Result<GraphModel, String> {
        build_model_from(self)
    }
}

/// The mox union model split per source file: one [`GraphFile`] per
/// `.mox` input, carrying only the packages that file declares (each
/// compiling file declares exactly one — the doctor's own gate), so node
/// attribution matches upstream's per-file shape.
fn split_domains(mox: &MoxSources, model: &Model) -> Vec<GraphFile<Model>> {
    let mut domains = Vec::new();
    for (path, text) in &mox.sources {
        let Some(package) = crate::init::commands::declared_package(text) else {
            continue;
        };
        let packages: Vec<_> = model
            .packages
            .iter()
            .filter(|p| p.name == package)
            .cloned()
            .collect();
        if packages.is_empty() {
            continue;
        }
        let sub = Model {
            format_version: model.format_version,
            phero_version: model.phero_version.clone(),
            packages,
        };
        let display = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path)
            .to_string();
        domains.push(GraphFile::new(
            display,
            normalize_path(Path::new(path)),
            Vec::new(),
            sub,
        ));
    }
    domains
}

/// Gathers the compiled artifacts of one doctor invocation into the
/// graph plane's input. Best-effort: files that failed their per-surface
/// compile are skipped (doctor already reported them as hard failures).
pub fn gather_artifacts(
    mox_files: &[PathBuf],
    ddd_files: &[PathBuf],
    evt_files: &[PathBuf],
    deploy_files: &[PathBuf],
    ifml_files: &[PathBuf],
) -> UnionGather {
    let mox = crate::init::commands::read_mox_sources(mox_files);
    let (cross, _sources) = gather_cross_surface(ddd_files, evt_files, deploy_files, ifml_files);
    full_gather(&mox, &cross)
}

/// The shared cross-surface gather — ONE function feeds the doctor
/// integrity pass (`check_union_integrity`) and the graph plane, so the
/// two cannot drift. Returns the [`UnionGather`] plus the
/// path → source map the integrity renderer tags diagnostics with
/// (unused by the graph plane, which consumes artifacts only).
pub(crate) fn gather_cross_surface(
    ddd_files: &[PathBuf],
    evt_files: &[PathBuf],
    deploy_files: &[PathBuf],
    ifml_files: &[PathBuf],
) -> (UnionGather, std::collections::BTreeMap<String, String>) {
    let mut gather = UnionGather::default();
    let mut sources: std::collections::BTreeMap<String, String> = Default::default();
    let mut seen_actors: HashSet<String> = HashSet::new();

    for path in ddd_files {
        if let Ok(compiled) = read_and_compile_ddd(path) {
            let Some(model) = compiled.compilation.model else {
                continue;
            };
            sources.insert(
                path.display().to_string(),
                std::fs::read_to_string(path).unwrap_or_default(),
            );
            let imports = compiled
                .mox_sources
                .iter()
                .map(|(p, _)| normalize_path(Path::new(p)))
                .collect();
            gather.designs.push(GraphFile::new(
                path.display().to_string(),
                normalize_path(path),
                imports,
                model,
            ));
        }
    }

    for path in evt_files {
        if let Ok(compiled) = read_and_compile_evt(path) {
            let Some(model) = compiled.compilation.model else {
                continue;
            };
            sources.insert(
                path.display().to_string(),
                std::fs::read_to_string(path).unwrap_or_default(),
            );
            let imports = compiled
                .mox_sources
                .iter()
                .map(|(p, _)| normalize_path(Path::new(p)))
                .collect();
            gather.events.push(GraphFile::new(
                path.display().to_string(),
                normalize_path(path),
                imports,
                model,
            ));
        }
    }

    for path in ifml_files {
        if let Ok(compiled) = crate::ifml_compile::compile_and_check_ifml(path) {
            sources.insert(
                path.display().to_string(),
                std::fs::read_to_string(path).unwrap_or_default(),
            );
            let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
            let imports: Vec<String> = compiled
                .policy_imports
                .iter()
                .map(|raw| normalize_path(&base_dir.join(raw)))
                .collect();
            gather.flows.push(GraphFile::new(
                path.display().to_string(),
                normalize_path(path),
                imports,
                compiled.compilation.model,
            ));
            // One `.actor` policy import → one actor GraphFile, carrying
            // the `.actor`'s own import lines as its graph import edges
            // (that is what clusters the application). The models are the
            // SAME `resolve_actor_imports` output the integrity pass and
            // the run path check — one source, cannot drift.
            for actor in resolve_actor_imports(&compiled.policy_imports, base_dir) {
                let actor_dir = actor.resolved.parent().unwrap_or_else(|| Path::new("."));
                let key = normalize_path(&actor.resolved);
                if !seen_actors.insert(key.clone()) {
                    continue;
                }
                let imports = scan_imports(&actor.source)
                    .iter()
                    .map(|raw| normalize_path(&actor_dir.join(raw)))
                    .collect();
                gather
                    .actors
                    .push(GraphFile::new(actor.raw.clone(), key, imports, actor.model));
            }
        }
    }

    for path in deploy_files {
        let compiled = match read_and_resolve_deploy(path) {
            Ok(compiled) => compiled,
            Err(_) => continue,
        };
        let Some(model) = compiled.model else {
            continue;
        };
        sources.insert(compiled.path.clone(), compiled.source.clone());
        for (import_path, import_source) in compiled.design_sources.iter() {
            sources.insert(import_path.clone(), import_source.clone());
        }
        for (import_path, import_source) in compiled.flow_sources.iter() {
            sources.insert(import_path.clone(), import_source.clone());
        }
        let mut imports: Vec<String> = compiled
            .design_sources
            .iter()
            .map(|(p, _)| normalize_path(Path::new(p)))
            .collect();
        imports.extend(
            compiled
                .flow_sources
                .iter()
                .map(|(p, _)| normalize_path(Path::new(p))),
        );
        gather.deploys.push(GraphFile::new(
            compiled.path.clone(),
            normalize_path(path),
            imports,
            model,
        ));
    }

    (gather, sources)
}

/// Host path normalization: canonicalized where the file exists,
/// absolutized where it does not — the matching key the application
/// clustering compares (display paths are the as-given spellings).
fn normalize_path(path: &Path) -> String {
    match std::fs::canonicalize(path) {
        Ok(canonical) => canonical.display().to_string(),
        Err(_) => std::path::absolute(path)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| path.display().to_string()),
    }
}

/// Build the semantic graph and round-trip it through the serialized
/// artifact (the module docs' contract).
pub fn build_model_from(gather: &UnionGather) -> std::result::Result<GraphModel, String> {
    let inputs = GraphInputs {
        domains: gather.domains.clone(),
        actors: gather.actors.clone(),
        designs: gather.designs.clone(),
        events: gather.events.clone(),
        deploys: gather.deploys.clone(),
        flows: gather.flows.clone(),
        ..Default::default()
    };
    let model = build(&inputs);
    let json = model
        .to_json_pretty()
        .map_err(|e| format!("graph artifact serialization failed: {e}"))?;
    GraphModel::from_json(&json).map_err(|e| format!("graph artifact round-trip failed: {e}"))
}

/// Load a committed semantic-graph artifact from disk — the
/// `--graph-artifact` mode's input. Format-gated by the wire contract
/// (a stale artifact is a named error, never a guess).
pub fn load_graph_artifact(path: &Path) -> std::result::Result<GraphModel, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    GraphModel::from_json(&text).map_err(|e| {
        format!(
            "{} is not a valid pherolang graph artifact: {e}",
            path.display()
        )
    })
}

/// The three sweep-able analytics over the artifact alone: declared
/// `never_both` pairs, `extends` monotonicity, and dead capabilities.
/// Findings are DEFINITE negatives only — `unknown` and `latent` records
/// never surface (upstream's `Finding` contract).
pub fn analyze_sweeps(model: &GraphModel) -> Vec<GraphFinding> {
    let index = GraphIndex::new(model);
    let mut findings = Vec::new();

    if let Ok(envelope) = separation_of_duty(&index, &SeparationOfDutyOptions::default()) {
        for result in &envelope.results {
            for violation in &result.violations {
                findings.push(GraphFinding {
                    analytic: "separationOfDuty",
                    subject: violation.actor.clone(),
                    detail: format!(
                        "holds both {} and {} (declared never_both pair)",
                        result.pair.first().cloned().unwrap_or_default(),
                        result.pair.get(1).cloned().unwrap_or_default(),
                    ),
                });
            }
        }
    }

    if let Ok(envelope) = monotonicity(&index, &MonotonicityOptions::default()) {
        for result in &envelope.results {
            if result.is_finding() {
                findings.push(GraphFinding {
                    analytic: "monotonicity",
                    subject: result.child.clone(),
                    detail: format!(
                        "cannot do everything {} may — missing: {}",
                        result.parent,
                        result.missing.join(", "),
                    ),
                });
            }
        }
    }

    let envelope = dead_capabilities(&index, None);
    for result in &envelope.results {
        if result.is_finding() {
            findings.push(GraphFinding {
                analytic: "deadCapabilities",
                subject: result.capability.clone(),
                detail: format!(
                    "is dead — every permitting actor also forbids it (blocked by: {})",
                    result.blocked_by.join(", "),
                ),
            });
        }
    }

    findings
}

/// Parse one `--graph-reachability` spec:
/// `from-actor=<ref>` XOR `from-capability=<ref>` plus `to=<ref>`,
/// comma-separated. Unknown keys, missing values, zero or two starts,
/// and an empty target are parse errors (a malformed request is a hard
/// doctor error, never a silent answer).
pub fn parse_reach_spec(spec: &str) -> std::result::Result<ReachQuery, String> {
    let mut query = ReachQuery {
        from_actor: None,
        from_capability: None,
        to: String::new(),
    };
    for part in spec.split(',') {
        let part = part.trim();
        let Some((key, value)) = part.split_once('=') else {
            return Err(format!(
                "invalid --graph-reachability spec '{spec}': '{part}' is not key=value"
            ));
        };
        let value = value.trim();
        if value.is_empty() {
            return Err(format!(
                "invalid --graph-reachability spec '{spec}': '{key}' has an empty value"
            ));
        }
        match key.trim() {
            "from-actor" => query.from_actor = Some(value.to_string()),
            "from-capability" => query.from_capability = Some(value.to_string()),
            "to" => query.to = value.to_string(),
            other => {
                return Err(format!(
                    "invalid --graph-reachability spec '{spec}': unknown key '{other}' \
                     (expected from-actor | from-capability | to)"
                ));
            }
        }
    }
    if query.from_actor.is_some() && query.from_capability.is_some() {
        return Err(format!(
            "invalid --graph-reachability spec '{spec}': exactly one of \
             from-actor/from-capability"
        ));
    }
    if query.from_actor.is_none() && query.from_capability.is_none() {
        return Err(format!(
            "invalid --graph-reachability spec '{spec}': one of \
             from-actor/from-capability is required"
        ));
    }
    if query.to.is_empty() {
        return Err(format!(
            "invalid --graph-reachability spec '{spec}': 'to' is required"
        ));
    }
    Ok(query)
}

/// Run one reachability query over the artifact. Unresolvable references
/// are analytics ERRORS naming the reference (with the resolved
/// candidates) — never findings, never silence.
pub fn run_reachability(
    model: &GraphModel,
    query: &ReachQuery,
) -> std::result::Result<ReachOutcome, String> {
    let index = GraphIndex::new(model);
    let options = ReachabilityOptions {
        from_actor: query.from_actor.as_deref(),
        from_capability: query.from_capability.as_deref(),
        to: &query.to,
    };
    let envelope = reachability(&index, &options).map_err(render_analytics_error)?;
    let json = envelope
        .to_json(true)
        .map_err(|e| format!("reachability envelope serialization failed: {e}"))?;
    let findings = envelope
        .results
        .iter()
        .filter(|result| result.is_finding())
        .map(|result| GraphFinding {
            analytic: "reachability",
            subject: result.target.clone(),
            detail: "unreachable from the queried authority".to_string(),
        })
        .collect();
    Ok(ReachOutcome {
        reachable: envelope.results.first().is_some_and(|r| r.reachable),
        envelope_json: json,
        findings,
    })
}

/// Render an analytics error the way upstream names it: the unresolved
/// reference plus the sorted candidates.
fn render_analytics_error(error: AnalyticsError) -> String {
    let candidates = error.candidates();
    let reference = match &error {
        AnalyticsError::Resolve(resolve) => resolve.reference().to_string(),
        AnalyticsError::NotAnActor { reference, .. }
        | AnalyticsError::NotACapability { reference, .. }
        | AnalyticsError::NotAClass { reference, .. } => reference.clone(),
        AnalyticsError::MissingStart => {
            return "reachability needs exactly one start (from-actor or \
                    from-capability)"
                .to_string();
        }
    };
    if candidates.is_empty() {
        format!("'{reference}' does not resolve in the graph artifact")
    } else {
        format!(
            "'{reference}' does not resolve in the graph artifact — candidates: {}",
            candidates.join(", ")
        )
    }
}

/// The doctor wiring. Returns the (hard_failures, soft_warnings)
/// contributed, in the house severity contract:
///
/// - **default**: every definite finding is one `WARN graph-analytics —`
///   line + one soft warning; a clean run prints one PASS line.
/// - **`--fail-on-findings`**: each finding is a hard failure (the
///   doctor's stable exit-1 contract).
/// - malformed reach specs, an unreadable/invalid `--graph-artifact`,
///   and unresolvable reachability references are HARD errors always —
///   malformed requests, not policy findings.
///
/// Presence-gated: no policy artifacts, no reachability queries, no
/// artifact mode, and no `--emit-graph` ⇒ the stage is silent (the
/// existing doctor output is byte-identical).
pub(crate) fn check_graph_plane(
    args: &DoctorArgs,
    mox: &MoxSources,
    cross: &UnionGather,
) -> (usize, usize) {
    // The sweeps need policy artifacts (or a committed artifact that
    // carries them); the queries/emit flags demand the plane on their own.
    let artifact_mode = args.graph_artifact.is_some();
    let sweep_inputs = artifact_mode || cross.has_policy_inputs();
    if !sweep_inputs && args.graph_reachability.is_empty() && args.emit_graph.is_none() {
        return (0, 0);
    }

    let mut hard = 0usize;
    let mut soft = 0usize;

    let model = if let Some(artifact) = &args.graph_artifact {
        match load_graph_artifact(artifact) {
            Ok(model) => model,
            Err(reason) => {
                hard += 1;
                println!("FAIL graph-artifact — {reason}");
                println!(
                    "     hint: regenerate with --emit-graph (or pherolang graph build) \
                     and commit the fresh artifact"
                );
                return (hard, soft);
            }
        }
    } else {
        match build_model_from(&full_gather(mox, cross)) {
            Ok(model) => model,
            Err(reason) => {
                hard += 1;
                println!("FAIL graph-analytics — {reason}");
                return (hard, soft);
            }
        }
    };

    let node_count = model.nodes.len();
    let edge_count = model.edges.len();

    if let Some(out) = &args.emit_graph {
        match model.to_json_pretty() {
            Ok(json) => match std::fs::write(out, &json) {
                Ok(()) => println!("INFO graph-analytics — emitted {}", out.display()),
                Err(e) => {
                    hard += 1;
                    println!("FAIL graph-analytics — cannot write {}: {e}", out.display());
                }
            },
            Err(e) => {
                hard += 1;
                println!("FAIL graph-analytics — {e}");
            }
        }
    }

    let mut findings: Vec<GraphFinding> = Vec::new();
    if sweep_inputs {
        findings.extend(analyze_sweeps(&model));
    }

    for spec in &args.graph_reachability {
        let query = match parse_reach_spec(spec) {
            Ok(query) => query,
            Err(reason) => {
                hard += 1;
                println!("FAIL graph-reachability — {reason}");
                continue;
            }
        };
        match run_reachability(&model, &query) {
            Ok(outcome) => {
                println!("{}", outcome.envelope_json);
                findings.extend(outcome.findings);
            }
            Err(reason) => {
                hard += 1;
                println!("FAIL graph-reachability — {reason}");
            }
        }
    }

    if findings.is_empty() {
        if sweep_inputs {
            println!(
                "PASS graph-analytics — {node_count} node(s), {edge_count} edge(s); \
                 no separation-of-duty, monotonicity, or dead-capability findings \
                 (pherolang graph analytics)"
            );
        }
        return (hard, soft);
    }

    for finding in &findings {
        if args.fail_on_findings {
            hard += 1;
            println!(
                "FAIL graph-analytics — {}: {} {}",
                finding.analytic, finding.subject, finding.detail
            );
        } else {
            soft += 1;
            println!(
                "WARN graph-analytics — {}: {} {}",
                finding.analytic, finding.subject, finding.detail
            );
        }
    }
    if args.fail_on_findings {
        println!(
            "     hint: fix the policy findings above (the upstream analytic names \
             the actor/capability ids), or drop --fail-on-findings to keep them advisory"
        );
    } else {
        println!("     hint: pass --fail-on-findings to gate CI on policy findings");
    }
    (hard, soft)
}

/// The full gather: the cross-surface artifacts plus the mox union model
/// split per source file — ONE mox compile for the graph plane.
pub(crate) fn full_gather(mox: &MoxSources, cross: &UnionGather) -> UnionGather {
    let mut full = cross.clone();
    if !mox.sources.is_empty() {
        let compilation = phero_driver::compile_files(
            &mox.sources,
            &phero_driver::DomainImports {
                schemas: mox.schema_imports.clone(),
                sigil: phero_driver::SigilImports::new(),
            },
        );
        if let Some(model) = &compilation.model {
            full.domains = split_domains(mox, model);
        }
    }
    full
}

/// The run path's protected-op WARN, in doctor (issue #619): a `protected`
/// operation is warned about ONLY when an `.ifml` action binds it — the
/// model contradicts itself — and then exactly once, naming the action.
///
/// Doctor holds no graph, but the cross-surface gather already compiled
/// both surfaces: the `.ddd` designs carry every `protected` operation and
/// the `.ifml` flows every action declaration — the same two artifact sets
/// upstream's `check_action_bindings` joins — so the join is the run
/// path's own [`codegraph_generate::ddd::service_ops::protected_binding`]
/// over compiled models: no graph, nothing compiled twice. A flow that
/// failed its compile is absent from the gather (doctor already reported
/// it), so nothing it declares counts as bound. Returns the warning texts
/// in flow order, actions in declaration order.
fn protected_op_dead_triggers(cross: &UnionGather) -> Vec<String> {
    use codegraph_generate::ddd::service_ops::{OpRef, dead_trigger_message, protected_binding};

    let service_ops = || {
        cross.designs.iter().flat_map(|file| {
            file.model
                .modules
                .iter()
                .flat_map(|module| &module.services)
                .flat_map(|service| {
                    service.operations.iter().map(move |op| OpRef {
                        owner: service.name.as_str(),
                        name: op.name.as_str(),
                        protected: op.is_protected,
                    })
                })
        })
    };
    let child_ops = || {
        cross.designs.iter().flat_map(|file| {
            file.model
                .modules
                .iter()
                .flat_map(|module| &module.designs)
                .flat_map(|design| {
                    // DD3: only a class-bearing design can own a `children`
                    // block; a class-less (standalone dto) design has none.
                    let owner = design.class.as_deref().unwrap_or_default();
                    design.child_operations.iter().map(move |op| OpRef {
                        owner,
                        name: op.name.as_str(),
                        protected: op.is_protected,
                    })
                })
        })
    };
    let mut warnings = Vec::new();
    for flow in &cross.flows {
        for action in &flow.model.actions {
            if let Some((op, reason)) = protected_binding(&action.name, service_ops(), child_ops())
            {
                warnings.push(dead_trigger_message(&action.name, &op, reason));
            }
        }
    }
    warnings
}

/// Print and count [`protected_op_dead_triggers`] — the doctor-side WARN
/// lines, each one soft warning (the `check_graph_plane` shape: the stage
/// prints its own lines and returns its contribution).
pub(crate) fn report_protected_op_dead_triggers(cross: &UnionGather) -> usize {
    let warnings = protected_op_dead_triggers(cross);
    for warning in &warnings {
        println!("WARN ddd — {warning}");
    }
    warnings.len()
}

#[cfg(test)]
mod tests {
    use super::gather_cross_surface;
    use super::protected_op_dead_triggers;
    use std::fs;
    use tempfile::TempDir;

    /// Issue #619 (doctor parity): the protected-op WARN is the JOIN of
    /// the compiled designs and flows the union gather already holds —
    /// a `protected` op is reported only when an `.ifml` action binds it,
    /// once, naming the action. Exercised through the real
    /// `gather_cross_surface` compile path over temp sources.
    #[test]
    fn protected_op_dead_triggers_join_compiled_designs_with_flows() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(
            root.join("payroll.mox"),
            "package payroll\n\nclass PayRun {\n    String [1] reference\n    refers PayLine[] lines\n}\n\n\
             class PayLine {\n    String [1] note\n}\n",
        )
        .unwrap();
        let ddd = root.join("payroll.ddd");
        fs::write(
            &ddd,
            r#"import "payroll.mox";

application Payroll {
    base payroll
    module ops {
        service PayRunService {
            inject PayRunRepository;
            protected recordStatus => PayRunRepository.save;
            approveRun => PayRunRepository.save;
        }
        entity PayRun repository PayRunRepository {
            findById;
            findAll;
            save;
            delete;
        }
        children PayRun {
            protected boolean removeLine(String lineId) => lines.remove;
        }
    }
}
"#,
        )
        .unwrap();

        // No flow input ⇒ nothing is bound ⇒ no protected-op warning.
        let (cross, _) = gather_cross_surface(std::slice::from_ref(&ddd), &[], &[], &[]);
        assert_eq!(cross.designs.len(), 1, "the design must compile");
        assert!(
            protected_op_dead_triggers(&cross).is_empty(),
            "an unbound protected op is by design"
        );

        // One flow binding a protected service op, a protected child op, a
        // public op, and nothing at all.
        let ifml = root.join("payroll.ifml");
        fs::write(
            &ifml,
            r#"domain "payroll" { schema "payroll"; }

view "PayRunBoard" {
    label "Pay Run Board";

    component "board" {
        type: details;
        data: PayRun;
        fields: [reference];

        on record -> action("RecordStatus");
        on remove -> action("RemoveLine");
        on approve -> action("ApproveRun");
        on stray -> action("Unrelated");
    }
}

action "RecordStatus" {
    target: PayRun;
}

action "RemoveLine" {
    target: PayRun;
}

action "ApproveRun" {
    target: PayRun;
}

action "Unrelated" {
    target: PayRun;
}
"#,
        )
        .unwrap();
        let (cross, _) = gather_cross_surface(&[ddd], &[], &[], &[ifml]);
        assert_eq!(cross.flows.len(), 1, "the flow must compile");
        assert_eq!(
            protected_op_dead_triggers(&cross),
            vec![
                "action 'RecordStatus' targets op 'PayRunService::recordStatus' which emits no \
                 API surface (the op is `protected` (Sculptor visibility)); the trigger is dead",
                "action 'RemoveLine' targets op 'PayRun::removeLine' which emits no API surface \
                 (the child operation is `protected` (Sculptor visibility)); the trigger is dead",
            ],
            "exactly one line per bound protected op, naming the action — the run path's line"
        );
    }
}
