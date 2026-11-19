//! Rosetta (Rune DSL / `.rosetta`) data-plane bridge (issue #256).
//!
//! The [`crate::ingest::mox_ingest`] bridge pattern applied to `.rosetta`:
//! parse → lower → resolve via the sigil crates (ALL user files in ONE
//! resolve call, builtins automatic), then walk the resolved
//! [`sigil_model::SemanticElement`]s into the same graph shapes the JSON
//! Schema path produces — the dispositions pinned by
//! `docs/rosetta-gap-analysis.md` (issue #254):
//!
//! - `type`/`choice` ([`sigil_model::Data`]) → `SchemaNode` (+ properties,
//!   `ExtendsSchema` for `extends` with the JSON path's ingest-time
//!   attribute merge: ancestors first, each group name-sorted, first
//!   occurrence wins; Rosetta `override` attributes REPLACE the inherited
//!   slot in place). Choices keep their options as `(0..1)` attributes and
//!   carry a `rosetta_choice` annotation. Rosetta types are AUTO-SCORED by
//!   the classifier (#258) — the bridge never declares entity/VO (unlike
//!   the mox bridge, where that is author-declarative).
//! - attributes → `PropertyNode` with `ReferencesSchema`/`ItemsOf` edges
//!   (`ref_target` from the referenced titles); builtin
//!   `int/number/string/date/dateTime/time/boolean` → the JSON path's
//!   primitive mappings.
//! - `enum` ([`sigil_model::Enumeration`]) → `CodeList` + `EnumValue`s
//!   (value + display_name) + the codelist `SchemaNode` the DDL/FK/link
//!   machinery keys on; `enum extends` merges the parent's values first.
//! - `[metadata]`/labels/`[ruleReference]`/`[docReference]` →
//!   `custom_annotations` payloads (gap-analysis finding 4): schema-level
//!   annotations land on the SchemaNode, attribute-level ones under a
//!   property-keyed `rosetta_attribute_annotations` map on the owning
//!   SchemaNode (a serde-defaulted `PropertyNode.custom_annotations` field
//!   is the flagged follow-up uplift — 60+ literal construction sites make
//!   it more than a bridge-sized change).
//! - conditions ([`sigil_model::Condition`]) → `ConditionNode`s (issue
//!   #261): each named condition lands as `kind: Condition` carrying the
//!   canonical `Expr::to_json()` payload (the embedding contract pinned by
//!   WP1.6: deterministic, serialization-stable, write-once — `Expr` is
//!   Serialize-only), linked to its schema via a `HasCondition` edge;
//!   `choice` types derive ONE `kind: OneOf` node whose options are the
//!   option attributes' referenced titles. Transpilation is #262. The
//!   `rosetta_conditions` annotation payload on the SchemaNode remains as
//!   provenance (#259 snapshot).
//! - namespaces (issue #268): declaring files get NamespaceNodes
//!   (`source = "rosetta"`, `InNamespace` + dotted `NamespaceParent`);
//!   import-only targets land as `source = "discovered"` so
//!   `NamespaceImports` edges resolve. Namespaces are still NOT domains —
//!   the domain falls back to the mox bridge's `resolve_domain` semantics
//!   over the dotted namespace (#267 config `domain=` wins when declared).
//! - regulatory reference elements (issue #265) → `RegulatoryNode`s:
//!   reports/bodies/corpora/segments/rule sources/rule schemas/meta types
//!   land as ONE parameterized node family (kind-tagged), with the
//!   associations the model actually expresses as edges — `[docReference]`
//!   metadata and report regulatory refs as `RegulatoryReference` edges
//!   (Schema/Condition/Function/Report → body/corpus/segment), `with
//!   source S` as `HasRuleSource`, a corpus' parent body as
//!   `CorpusInBody`, and rule-source `extends` as `DerivesFrom`.
//!   Doc-reference targets no declared element backs are skipped (sigil
//!   itself does not validate them).
//! - functions (issue #263) → `FunctionNode`s: each sigil `Function`
//!   lands as ONE structured node carrying its dispatch head
//!   (`(attr: Enum->VALUE)`), typed inputs/output, aliases (`alias n: e`),
//!   `set`/`add` operations with their `->` paths, and post-conditions —
//!   every expression persisted as the canonical `Expr::to_json()` payload
//!   for the generation-time transpiler. `extends` resolves within the
//!   run (functions sorted by name per domain; an extends CYCLE is a hard
//!   error naming the cycle) and becomes a `FunctionExtends` edge plus the
//!   denormalized `extends` field. `[transform]` annotations land on the
//!   node (typed) AND keep riding onto the named rule-schema node's
//!   properties (`transform_annotations`, the #265 surface). Function
//!   `[docReference ...]` metadata emits `RegulatoryReference` edges owned
//!   by `RegulatoryOwner::Function`.
//! - rules (issue #264) → `RuleNode`s: each sigil `Rule` lands as ONE
//!   structured node (`kind`: reporting/eligibility from the `eligibility`
//!   bool, `input_type` from the `from TypeCall` clause, canonical
//!   `Expr::to_json` body payload). Rules do NOT extend (sigil has no
//!   rule parent ref). A resolvable input type becomes a `RuleAppliesTo`
//!   edge (Rule → Schema, written after the schema bridging passes);
//!   rule `[docReference ...]` metadata emits `RegulatoryReference` edges
//!   owned by `RegulatoryOwner::Rule`; `[ruleReference R]` entries on
//!   `rule source` class attributes promote to `RuleReference` edges
//!   (Schema → Rule, carrying the attribute + source name) when R names a
//!   rule of this run — unresolvable references stay documented skips.
//! - out-of-data-plane elements (annotation decls/basic types/aliases/
//!   library functions) are counted and named as `needs_review` — never
//!   silently dropped, never bridged.
//!
//! Provenance: every bridged node carries `custom_annotations["origin"]
//! = "rosetta"` (issue #255) — deliberately DISTINCT from the mox
//! `source` key, so `is_mox_sourced` keeps applying to mox only and the
//! classifier's priority-0 bypass does not pick rosetta schemas up
//! (rosetta stays auto-scored).
//!
//! Title coordination (issue #257 wires the pipeline): `bridged_titles`
//! feeds the JSON-schema pass's skip-set (`ingest_schemas_with_skips`),
//! mirroring mox-first ordering — `.rosetta` wins title conflicts by
//! passing first.

mod bridge;
mod mapping;
mod naming;
mod passes;

use std::collections::HashSet;
use std::path::PathBuf;

use sigil_resolve::Resolution;

use codegraph_config::config::DomainConfig;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{RegulatoryEdgeKind, RegulatoryKind, RegulatoryOwner};

use crate::error::{Error, Result};

use naming::referenced_title_str;
use passes::{
    RosettaBridgeState, bridge_codelist_schemas, bridge_data_schemas, bridge_functions,
    bridge_namespaces, bridge_properties, bridge_regulatory_edges, bridge_regulatory_nodes,
    bridge_rule_edges, bridge_rules, scan_elements,
};

/// Provenance marker on every rosetta-bridged node (`custom_annotations`
/// key `origin`, value `rosetta`) — distinct from the mox `source` key.
pub const ROSETTA_ORIGIN: &str = "rosetta";

/// `NamespaceNode.source` provenance for rosetta-declared namespaces.
pub(crate) const ROSETTA_NAMESPACE_SOURCE: &str = "rosetta";

/// Counters for one [`ingest_rosetta_files`] run. `bridged_titles` feeds
/// the JSON-schema pass's skip set (issue #257).
#[derive(Debug, Default)]
pub struct RosettaIngestStats {
    pub files: usize,
    pub types: usize,
    pub choices: usize,
    pub properties: usize,
    pub edges: usize,
    pub enums: usize,
    pub enum_values: usize,
    pub enum_schemas: usize,
    pub extends: usize,
    pub conditions_recorded: usize,
    /// Named conditions that landed as ConditionNodes (#261).
    pub conditions_ingested: usize,
    /// Bridge-derived one_of nodes (one per `choice` type).
    pub one_of_ingested: usize,
    /// Regulatory reference nodes ingested (issue #265): reports, bodies,
    /// corpora, segments, rule sources, rule schemas, meta types.
    pub regulatory_nodes: usize,
    /// Regulatory reference edges (doc references, report regulatory refs,
    /// rule sources, corpus parents, metaType/rule-source derivation).
    pub regulatory_edges: usize,
    /// Function nodes ingested (issue #263): sigil `Function` elements.
    pub functions_ingested: usize,
    /// Rule nodes ingested (issue #264): sigil `Rule` elements
    /// (reporting + eligibility).
    pub rules_ingested: usize,
    /// `RuleAppliesTo` edges written for rules whose `from` input type
    /// matches a schema of the graph (pre-existing or bridged this run).
    pub rule_applies_to: usize,
    /// Rule-source `[ruleReference R]` entries promoted to `RuleReference`
    /// edges (issue #264).
    pub rule_reference_edges: usize,
    /// `[ruleReference R]` entries naming no rule of this run — documented
    /// skips, never silent drops.
    pub rule_reference_skips: usize,
    pub namespaces: usize,
    pub namespace_imports: usize,
    /// Out-of-data-plane elements counted for review (never silently
    /// dropped, never bridged — #264 owns rule nodes).
    pub needs_review: usize,
    pub skipped: usize,
    pub bridged_titles: Vec<String>,
    /// Names of the out-of-data-plane elements (for #257's stderr notice).
    pub needs_review_names: Vec<String>,
}

impl std::fmt::Display for RosettaIngestStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} files, {} types ({} choices), {} properties, {} edges, {} enums \
                 ({} values, {} codelist schemas), {} extends, {} conditions recorded \
                 ({} nodes, {} one_of), {} regulatory nodes ({} refs), {} functions, \
                 {} rules ({} applies-to, {} rule-source refs, {} skipped), \
                 {} namespaces ({} imports)",
            self.files,
            self.types,
            self.choices,
            self.properties,
            self.edges,
            self.enums,
            self.enum_values,
            self.enum_schemas,
            self.extends,
            self.conditions_recorded,
            self.conditions_ingested,
            self.one_of_ingested,
            self.regulatory_nodes,
            self.regulatory_edges,
            self.functions_ingested,
            self.rules_ingested,
            self.rule_applies_to,
            self.rule_reference_edges,
            self.rule_reference_skips,
            self.namespaces,
            self.namespace_imports,
        )?;
        if self.needs_review != 0 {
            write!(f, ", {} needs_review", self.needs_review)?;
        }
        Ok(())
    }
}

/// Result of one [`ingest_rosetta_files`] run.
#[derive(Debug, Default)]
pub struct RosettaIngestOutcome {
    pub stats: RosettaIngestStats,
}

/// Ingest `.rosetta` data-plane models into the graph.
///
/// Hard-errors on syntax or resolution failures via
/// [`Error::RosettaModel`] (codes/spans surfaced) — a broken model never
/// half-ingests, mirroring the `MoxSchemaImport` precedent.
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub async fn ingest_rosetta_files(
    ingestor: &dyn GraphIngestor,
    querier: &dyn GraphQuerier,
    rosetta_paths: &[PathBuf],
    domain_config: &DomainConfig,
    type_suffix: &str,
) -> Result<RosettaIngestOutcome> {
    let mut stats = RosettaIngestStats {
        files: rosetta_paths.len(),
        ..Default::default()
    };

    // Deterministic: parse each file in input order (deduped by canonical
    // path), then resolve ALL user files in ONE call (builtins automatic).
    let parsed = parse_rosetta_sources(rosetta_paths, &mut stats)?;
    if parsed.is_empty() {
        return Ok(RosettaIngestOutcome { stats });
    }

    let resolution = resolve_rosetta_workspace(parsed)?;

    // `Resolution.files` = builtin files first, then user files in
    // submission order. Builtins contribute to the name universes but are
    // never bridged.
    let builtin_count = sigil_resolve::builtin_files().len();
    let user_files = resolution.files.get(builtin_count..).unwrap_or(&[]);

    let mut state = RosettaBridgeState::new(stats);
    let universe = scan_elements(
        &resolution,
        user_files,
        domain_config,
        querier,
        &mut state.stats,
    )
    .await;

    bridge_namespaces(ingestor, &universe, &mut state).await?;
    bridge_regulatory_nodes(ingestor, &universe, &mut state).await?;
    bridge_functions(ingestor, &universe, &mut state).await?;
    bridge_rules(ingestor, &universe, &mut state).await?;
    bridge_codelist_schemas(ingestor, &universe, type_suffix, &mut state).await?;
    bridge_data_schemas(ingestor, &universe, type_suffix, &mut state).await?;
    bridge_rule_edges(ingestor, &universe, &mut state).await?;
    bridge_properties(ingestor, &universe, &mut state).await?;
    bridge_regulatory_edges(ingestor, &universe, &mut state).await?;

    if state.stats.needs_review != 0 {
        eprintln!(
            "Warning: rosetta model has {} out-of-data-plane element(s) recorded as \
             needs_review (annotation declarations, basic types, aliases, library \
             functions): {}",
            state.stats.needs_review,
            state.stats.needs_review_names.join(", ")
        );
    }

    Ok(RosettaIngestOutcome { stats: state.stats })
}

/// Read, dedupe (by canonical path), and parse every `.mox`… rosetta
/// source in input order; any syntax error is a hard `RosettaModel` error.
fn parse_rosetta_sources(
    rosetta_paths: &[PathBuf],
    stats: &mut RosettaIngestStats,
) -> Result<Vec<sigil_model::ModelFile>> {
    let mut parsed: Vec<sigil_model::ModelFile> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for path in rosetta_paths {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !seen.insert(canonical.display().to_string()) {
            stats.files -= 1;
            continue;
        }
        let display = path.display().to_string();
        let text = std::fs::read_to_string(path).map_err(|e| Error::RosettaModel {
            file: display.clone(),
            reason: format!("could not be read: {e}"),
        })?;
        let source = sigil_diag::SourceFile::new(display.clone(), text);
        let (unit, diagnostics) = sigil_syntax::parse(&source);
        let errors: Vec<String> = diagnostics
            .iter()
            .filter(|d| d.severity == sigil_diag::Severity::Error)
            .map(|d| format!("[{}] {:?} {}", d.code, d.span, d.message))
            .collect();
        if !errors.is_empty() {
            return Err(Error::RosettaModel {
                file: display,
                reason: format!("syntax errors: {}", errors.join("; ")),
            });
        }
        let Some(unit) = unit else {
            return Err(Error::RosettaModel {
                file: display,
                reason: "parsing produced no syntax tree".to_string(),
            });
        };
        parsed.push(sigil_syntax::lower(&display, &unit));
    }
    Ok(parsed)
}

/// Resolve ALL user files in ONE call (builtins automatic); any
/// error-severity diagnostic is a hard `RosettaModel` error.
fn resolve_rosetta_workspace(parsed: Vec<sigil_model::ModelFile>) -> Result<Resolution> {
    let resolution: Resolution = sigil_resolve::resolve(parsed);
    let resolve_errors: Vec<String> = resolution
        .diagnostics
        .iter()
        .filter(|d| d.severity == sigil_diag::Severity::Error)
        .map(|d| format!("[{}] {}", d.code, d.message))
        .collect();
    if !resolve_errors.is_empty() {
        return Err(Error::RosettaModel {
            file: "rosetta workspace".to_string(),
            reason: format!("resolution errors: {}", resolve_errors.join("; ")),
        });
    }
    Ok(resolution)
}

/// Emit the `RegulatoryReference` edges for one `[docReference ...]`
/// payload: owner → body, owner → each corpus (carrying the provision as
/// the ref path), owner → each segment (carrying the segment reference).
pub(super) async fn emit_doc_reference_edges(
    ingestor: &dyn GraphIngestor,
    owner: &RegulatoryOwner,
    doc: &sigil_model::DocReference,
    reg_known: &HashSet<(String, String)>,
    stats: &mut RosettaIngestStats,
) -> crate::error::Result<()> {
    let mut edges: Vec<(String, RegulatoryKind, Option<String>)> = vec![(
        referenced_title_str(&doc.body),
        RegulatoryKind::Body,
        doc.provision.clone(),
    )];
    for corpus in &doc.corpora {
        edges.push((
            referenced_title_str(corpus),
            RegulatoryKind::Corpus,
            doc.provision.clone(),
        ));
    }
    for (segment, reference) in &doc.segments {
        edges.push((
            referenced_title_str(segment),
            RegulatoryKind::Segment,
            Some(reference.clone()),
        ));
    }
    for (target, target_kind, ref_path) in edges {
        link_regulatory(
            ingestor,
            owner,
            &target,
            target_kind,
            RegulatoryEdgeKind::Reference,
            ref_path.as_deref(),
            reg_known,
            stats,
        )
        .await?;
    }
    Ok(())
}

/// Link an owner to a regulatory node when the target was ingested this
/// run, counting the edge in the stats. Best-effort: a target no declared
/// user-file element backs is skipped (sigil does not validate
/// doc-reference targets either).
#[allow(clippy::too_many_arguments)]
pub(super) async fn link_regulatory(
    ingestor: &dyn GraphIngestor,
    owner: &RegulatoryOwner,
    target: &str,
    target_kind: RegulatoryKind,
    edge_kind: RegulatoryEdgeKind,
    ref_path: Option<&str>,
    reg_known: &HashSet<(String, String)>,
    stats: &mut RosettaIngestStats,
) -> crate::error::Result<()> {
    if !reg_known.contains(&(target_kind.as_str().to_string(), target.to_string())) {
        return Ok(());
    }
    ingestor
        .ingest_regulatory_reference(owner, target, target_kind, edge_kind, ref_path)
        .await
        .map_err(Error::Graph)?;
    stats.regulatory_edges += 1;
    Ok(())
}
