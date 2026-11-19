//! The rosetta bridge passes behind [`super::ingest_rosetta_files`] (issue
//! #751 extraction): one scan building the element universe, then the
//! historical pass sequence — namespaces (#268), regulatory nodes (#265),
//! functions (#263), rules (#264), codelist schemas, data schemas, rule
//! edges, properties/refs, regulatory structural edges — each moved
//! verbatim from the former single function. `RosettaUniverse` carries the
//! borrowed indices; `RosettaBridgeState` the mutable pass state.

use std::collections::{BTreeMap, HashMap, HashSet};

use sigil_model::SemanticElement;
use sigil_resolve::Resolution;

use codegraph_config::config::DomainConfig;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{
    CodeList, ConditionKind, ConditionNode, EdgeProperties, EdgeType, EnumValue, NamespaceImport,
    RegulatoryEdgeKind, RegulatoryKind, RegulatoryNode, RegulatoryOwner,
};
use codegraph_naming::{strip_suffix, to_snake_case};

use crate::error::{Error, Result};
use crate::ingest::async_ingest::sanitize_description;
use crate::ingest::mox_ingest::{
    DISCOVERED_NAMESPACE_SOURCE, emit_schema_namespace_edge, ingest_namespaces_deduped,
    resolve_domain,
};

use super::bridge::{
    attribute_property, data_schema_node, enum_schema_node, ordered_bridge_attributes,
};
use super::naming::{
    collect_universes, detect_function_extends_cycle, element_dedup_key, element_kind_label,
    function_node, merged_enum_values, model_namespace, named_ref_title, referenced_title,
    referenced_title_str, rule_node, schema_id, synthesize_report_name,
};
use super::{ROSETTA_NAMESPACE_SOURCE, ROSETTA_ORIGIN, emit_doc_reference_edges, link_regulatory};

/// The element universe of one rosetta run: per-kind indices into the
/// resolved user models plus the name universes and existing-graph lookups
/// the passes share. Borrowed from the `Resolution` owned by the caller.
pub(super) struct RosettaUniverse<'a> {
    pub(super) user_files: &'a [sigil_model::ModelFile],
    pub(super) known_titles: HashSet<String>,
    pub(super) json_schema_ids: HashMap<String, String>,
    pub(super) enum_titles: HashSet<String>,
    pub(super) type_titles: HashSet<String>,
    pub(super) alias_types: HashMap<String, sigil_model::TypeRef>,
    pub(super) data_index: Vec<(&'a sigil_model::Data, String, String, &'a str)>,
    pub(super) enum_index: Vec<(&'a sigil_model::Enumeration, String, String)>,
    pub(super) reg_bodies: Vec<(&'a sigil_model::Body, String)>,
    pub(super) reg_corpora: Vec<(&'a sigil_model::Corpus, String)>,
    pub(super) reg_segments: Vec<(&'a sigil_model::Segment, String)>,
    pub(super) reg_rule_schemas: Vec<(&'a sigil_model::Schema, String)>,
    pub(super) reg_meta_types: Vec<(&'a sigil_model::MetaType, String)>,
    pub(super) reg_rule_sources: Vec<(&'a sigil_model::ExternalRuleSource, String)>,
    /// (report, domain, synthesized name).
    pub(super) reg_reports: Vec<(&'a sigil_model::Report, String, String)>,
    /// (function, domain).
    pub(super) func_index: Vec<(&'a sigil_model::Function, String)>,
    /// (rule, domain).
    pub(super) rule_index: Vec<(&'a sigil_model::Rule, String)>,
    /// Function `[transform]` annotations awaiting their target rule-schema
    /// node: (bare reference, function name, transform kind).
    pub(super) function_transforms: Vec<(String, String, &'static str)>,
    pub(super) namespace_sources: BTreeMap<String, String>,
    pub(super) namespace_imports: Vec<NamespaceImport>,
}

/// Mutable state threaded through the passes: the run stats plus the
/// bridged-node registries later passes consult.
pub(super) struct RosettaBridgeState {
    pub(super) stats: super::RosettaIngestStats,
    /// (kind, name) of every regulatory node bridged this run — drives
    /// best-effort doc-reference/structural edge emission so stats stay
    /// backend-independent.
    pub(super) reg_known: HashSet<(String, String)>,
    pub(super) bridged: HashSet<String>,
    pub(super) bridged_schema_ids: HashMap<String, String>,
    #[allow(dead_code)]
    pub(super) bridged_enum_schema_ids: HashMap<String, String>,
}

impl RosettaBridgeState {
    pub(super) fn new(stats: super::RosettaIngestStats) -> Self {
        Self {
            stats,
            reg_known: HashSet::new(),
            bridged: HashSet::new(),
            bridged_schema_ids: HashMap::new(),
            bridged_enum_schema_ids: HashMap::new(),
        }
    }
}

/// Classify every user-file element into the per-kind indices and collect
/// the namespace/import maps (issue #268). The deterministic function/rule
/// sorts the bridge passes rely on happen here — nothing mutates the
/// indices between this scan and their passes, so the resulting order is
/// identical to the former in-pass sorts.
pub(super) async fn scan_elements<'a>(
    resolution: &'a Resolution,
    user_files: &'a [sigil_model::ModelFile],
    domain_config: &DomainConfig,
    querier: &dyn GraphQuerier,
    stats: &mut super::RosettaIngestStats,
) -> RosettaUniverse<'a> {
    // Name universes for reference classification (bare last segments —
    // v1 keeps Rosetta's per-namespace uniqueness; the title-suffix
    // collision rule matches the JSON skip-set contract).
    let mut enum_titles: HashSet<String> = HashSet::new();
    let mut type_titles: HashSet<String> = HashSet::new();
    for model in &resolution.files {
        collect_universes(model, &mut type_titles, &mut enum_titles);
    }

    // User-declared typeAliases (`typeAlias Money: number(fractionalDigits: 2)`)
    // lower onto primitive property types at attribute-bridge time: the alias
    // target's builtin name + arguments decide the PgType (gap doc, Data
    // plane "TypeAlias → bridge-side alias lowering"). Chain-safe up to a
    // small depth; aliases landing on data types fall through to the
    // entity-reference arm instead.
    let mut alias_types: HashMap<String, sigil_model::TypeRef> = HashMap::new();
    for model in &resolution.files {
        for element in &model.elements {
            if let SemanticElement::TypeAlias(alias) = element {
                alias_types.insert(alias.name.clone(), alias.type_ref.clone());
            }
        }
    }

    // Schema entities already in the graph: rosetta types attach by NAME
    // and never override an existing node (mox precedent).
    let schemas = querier.list_schemas(None).await.unwrap_or_default();
    let known_titles: HashSet<String> = schemas.iter().map(|s| s.title.clone()).collect();
    let json_schema_ids: HashMap<String, String> = schemas
        .iter()
        .map(|s| (s.title.clone(), s.schema_id.clone()))
        .collect();

    // ── Element universe (user files only): data types + enums to bridge;
    // regulatory reference elements for the #265 plane; everything else is
    // out-of-data-plane and recorded for review.
    let mut data_index: Vec<(&sigil_model::Data, String, String, &str)> = Vec::new();
    let mut enum_index: Vec<(&sigil_model::Enumeration, String, String)> = Vec::new();
    let mut reg_bodies: Vec<(&sigil_model::Body, String)> = Vec::new();
    let mut reg_corpora: Vec<(&sigil_model::Corpus, String)> = Vec::new();
    let mut reg_segments: Vec<(&sigil_model::Segment, String)> = Vec::new();
    let mut reg_rule_schemas: Vec<(&sigil_model::Schema, String)> = Vec::new();
    let mut reg_meta_types: Vec<(&sigil_model::MetaType, String)> = Vec::new();
    let mut reg_rule_sources: Vec<(&sigil_model::ExternalRuleSource, String)> = Vec::new();
    // (report, domain, synthesized name).
    let mut reg_reports: Vec<(&sigil_model::Report, String, String)> = Vec::new();
    // (function, domain).
    let mut func_index: Vec<(&sigil_model::Function, String)> = Vec::new();
    // (rule, domain).
    let mut rule_index: Vec<(&sigil_model::Rule, String)> = Vec::new();
    // Function `[transform]` annotations awaiting their target rule-schema
    // node: (bare reference, function name, transform kind).
    let mut function_transforms: Vec<(String, String, &'static str)> = Vec::new();
    let mut seen_elements: HashSet<String> = HashSet::new();
    // Namespace universe (issue #268): declaring-file namespaces get the
    // "rosetta" source; import-only targets (e.g. the sigil builtins'
    // `com.rosetta.model`) land as "discovered" so import edges resolve
    // and validation sees them.
    let mut namespace_sources: BTreeMap<String, String> = BTreeMap::new();
    let mut namespace_imports: Vec<NamespaceImport> = Vec::new();
    for model in user_files {
        if !model.namespace.is_empty() {
            namespace_sources
                .entry(model.namespace.clone())
                .or_insert_with(|| ROSETTA_NAMESPACE_SOURCE.to_string());
        }
        // Namespaces are NOT domains (#267/#268): domain resolution
        // mirrors the mox package fallback until the namespace
        // assignment config lands.
        let (domain, _) = resolve_domain(domain_config, &model.namespace);
        for element in &model.elements {
            let name = element.name();
            if !seen_elements.insert(element_dedup_key(element, name)) {
                continue;
            }
            match element {
                SemanticElement::Data(data) => {
                    let id = schema_id(&model.namespace, name);
                    data_index.push((data, domain.clone(), id.clone(), &model.namespace));
                }
                SemanticElement::Enumeration(enumeration) => {
                    let id = schema_id(&model.namespace, name);
                    enum_index.push((enumeration, domain.clone(), id));
                }
                SemanticElement::Body(body) => reg_bodies.push((body, domain.clone())),
                SemanticElement::Corpus(corpus) => reg_corpora.push((corpus, domain.clone())),
                SemanticElement::Segment(segment) => reg_segments.push((segment, domain.clone())),
                SemanticElement::Schema(schema) => reg_rule_schemas.push((schema, domain.clone())),
                SemanticElement::MetaType(meta_type) => {
                    reg_meta_types.push((meta_type, domain.clone()))
                }
                SemanticElement::ExternalRuleSource(source) => {
                    reg_rule_sources.push((source, domain.clone()))
                }
                SemanticElement::Report(report) => {
                    reg_reports.push((report, domain.clone(), synthesize_report_name(report)))
                }
                SemanticElement::Function(function) => {
                    func_index.push((function, domain.clone()));
                    // Transform annotations keep riding onto the targeted
                    // rule-schema node's properties (the #265 surface) in
                    // ADDITION to landing on the FunctionNode itself.
                    for transform in &function.transform {
                        if let Some(reference) = &transform.reference {
                            function_transforms.push((
                                referenced_title_str(reference),
                                function.name.clone(),
                                transform.kind.as_str(),
                            ));
                        }
                    }
                }
                SemanticElement::Rule(rule) => {
                    rule_index.push((rule, domain.clone()));
                }
                SemanticElement::Annotation(_)
                | SemanticElement::TypeAlias(_)
                | SemanticElement::BasicType(_)
                | SemanticElement::RecordType(_)
                | SemanticElement::LibraryFunction(_) => {
                    stats.needs_review += 1;
                    stats
                        .needs_review_names
                        .push(format!("{} {name}", element_kind_label(element)));
                }
            }
        }
    }
    // Import scan: `import a.b.*` / `import a.b as x` → NamespaceImports
    // edges (issue #268). The sigil lowered form appends `.*` to the
    // wildcard namespace string — strip it back to the bare FQN. Targets
    // outside the workspace's own namespaces join the node universe as
    // "discovered". Empty importing namespaces record no edges (nothing
    // to import FROM); dedup keeps repeat imports to one edge.
    let mut seen_imports: HashSet<(String, String, bool, Option<String>)> = HashSet::new();
    for model in user_files {
        if model.namespace.is_empty() {
            continue;
        }
        for imp in &model.imports {
            let target = imp
                .imported_namespace
                .trim_end_matches('*')
                .trim_end_matches('.');
            if target.is_empty() {
                continue;
            }
            namespace_sources
                .entry(target.to_string())
                .or_insert_with(|| DISCOVERED_NAMESPACE_SOURCE.to_string());
            let payload = (
                model.namespace.clone(),
                target.to_string(),
                imp.wildcard,
                imp.namespace_alias.clone(),
            );
            if seen_imports.insert(payload.clone()) {
                namespace_imports.push(NamespaceImport {
                    from_ns: payload.0,
                    to_ns: payload.1,
                    wildcard: payload.2,
                    alias: payload.3,
                });
            }
        }
    }
    // The bridge passes' deterministic orders — functions and rules sorted
    // by name within a domain — applied at scan time (the indices are not
    // touched between here and their passes).
    func_index.sort_by(|(a, da), (b, db)| (da, &a.name).cmp(&(db, &b.name)));
    rule_index.sort_by(|(a, da), (b, db)| (da, &a.name).cmp(&(db, &b.name)));

    RosettaUniverse {
        user_files,
        known_titles,
        json_schema_ids,
        enum_titles,
        type_titles,
        alias_types,
        data_index,
        enum_index,
        reg_bodies,
        reg_corpora,
        reg_segments,
        reg_rule_schemas,
        reg_meta_types,
        reg_rule_sources,
        reg_reports,
        func_index,
        rule_index,
        function_transforms,
        namespace_sources,
        namespace_imports,
    }
}

/// ── Namespace bridge (issue #268): namespace nodes with their dotted
/// parent chains, then the import edges (both endpoints exist by now).
pub(super) async fn bridge_namespaces(
    ingestor: &dyn GraphIngestor,
    universe: &RosettaUniverse<'_>,
    state: &mut RosettaBridgeState,
) -> Result<()> {
    state.stats.namespaces =
        ingest_namespaces_deduped(ingestor, &universe.namespace_sources).await?;
    for imp in &universe.namespace_imports {
        ingestor
            .ingest_namespace_import(imp)
            .await
            .map_err(Error::Graph)?;
        state.stats.namespace_imports += 1;
    }
    Ok(())
}

/// ── Bridge pass 0: regulatory reference nodes (#265). ONE parameterized
/// node family (kind-tagged); `reg_known` drives best-effort edge
/// emission below so stats stay backend-independent.
pub(super) async fn bridge_regulatory_nodes(
    ingestor: &dyn GraphIngestor,
    universe: &RosettaUniverse<'_>,
    state: &mut RosettaBridgeState,
) -> Result<()> {
    let transform_annotations: HashMap<&str, Vec<serde_json::Value>> = universe
        .function_transforms
        .iter()
        .fold(HashMap::new(), |mut acc, (reference, function, kind)| {
            acc.entry(reference.as_str())
                .or_insert_with(Vec::new)
                .push(serde_json::json!({ "function": function, "kind": kind }));
            acc
        });
    for (body, domain) in &universe.reg_bodies {
        let node = RegulatoryNode {
            name: body.name.clone(),
            kind: RegulatoryKind::Body,
            label: None,
            definition: body.definition.clone(),
            domain: Some(domain.clone()),
            properties: serde_json::json!({
                "origin": ROSETTA_ORIGIN,
                "body_type": body.body_type,
            }),
        };
        ingestor
            .ingest_regulatory(&node)
            .await
            .map_err(Error::Graph)?;
        state
            .reg_known
            .insert((RegulatoryKind::Body.as_str().to_string(), body.name.clone()));
        state.stats.regulatory_nodes += 1;
    }
    for (corpus, domain) in &universe.reg_corpora {
        let node = RegulatoryNode {
            name: corpus.name.clone(),
            kind: RegulatoryKind::Corpus,
            label: corpus.display_name.clone(),
            definition: corpus.definition.clone(),
            domain: Some(domain.clone()),
            properties: serde_json::json!({
                "origin": ROSETTA_ORIGIN,
                "corpus_type": corpus.corpus_type,
                "parent_body": corpus.body.as_deref().map(referenced_title_str),
            }),
        };
        ingestor
            .ingest_regulatory(&node)
            .await
            .map_err(Error::Graph)?;
        state.reg_known.insert((
            RegulatoryKind::Corpus.as_str().to_string(),
            corpus.name.clone(),
        ));
        state.stats.regulatory_nodes += 1;
    }
    for (segment, domain) in &universe.reg_segments {
        let node = RegulatoryNode {
            name: segment.name.clone(),
            kind: RegulatoryKind::Segment,
            label: None,
            definition: None,
            domain: Some(domain.clone()),
            properties: serde_json::json!({ "origin": ROSETTA_ORIGIN }),
        };
        ingestor
            .ingest_regulatory(&node)
            .await
            .map_err(Error::Graph)?;
        state.reg_known.insert((
            RegulatoryKind::Segment.as_str().to_string(),
            segment.name.clone(),
        ));
        state.stats.regulatory_nodes += 1;
    }
    for (schema, domain) in &universe.reg_rule_schemas {
        let transforms = transform_annotations
            .get(schema.name.as_str())
            .cloned()
            .unwrap_or_default();
        let mut properties = serde_json::json!({
            "origin": ROSETTA_ORIGIN,
            "format": schema.format,
            "transform_annotations": transforms,
        });
        if !schema.annotations.is_empty()
            && let Ok(annotations) = serde_json::to_value(&schema.annotations)
        {
            properties["annotations"] = annotations;
        }
        let node = RegulatoryNode {
            name: schema.name.clone(),
            kind: RegulatoryKind::RuleSchema,
            label: None,
            definition: schema.definition.clone(),
            domain: Some(domain.clone()),
            properties,
        };
        ingestor
            .ingest_regulatory(&node)
            .await
            .map_err(Error::Graph)?;
        state.reg_known.insert((
            RegulatoryKind::RuleSchema.as_str().to_string(),
            schema.name.clone(),
        ));
        state.stats.regulatory_nodes += 1;
    }
    for (meta_type, domain) in &universe.reg_meta_types {
        let node = RegulatoryNode {
            name: meta_type.name.clone(),
            kind: RegulatoryKind::MetaType,
            label: None,
            definition: None,
            domain: Some(domain.clone()),
            properties: serde_json::json!({
                "origin": ROSETTA_ORIGIN,
                "type_ref": referenced_title(&meta_type.type_ref),
            }),
        };
        ingestor
            .ingest_regulatory(&node)
            .await
            .map_err(Error::Graph)?;
        state.reg_known.insert((
            RegulatoryKind::MetaType.as_str().to_string(),
            meta_type.name.clone(),
        ));
        state.stats.regulatory_nodes += 1;
    }
    for (source, domain) in &universe.reg_rule_sources {
        let classes: Vec<serde_json::Value> = source
            .classes
            .iter()
            .map(|class| {
                serde_json::json!({
                    "data": referenced_title(&class.data),
                    "attributes": class.attributes.iter().map(|attribute| {
                        serde_json::json!({
                            "add": attribute.add,
                            "attribute": attribute.attribute,
                            "rule_references": attribute.rule_references.iter().map(|r| {
                                serde_json::json!({
                                    "rule": r.rule,
                                    "empty": r.empty,
                                })
                            }).collect::<Vec<_>>(),
                        })
                    }).collect::<Vec<_>>(),
                })
            })
            .collect();
        let node = RegulatoryNode {
            name: source.name.clone(),
            kind: RegulatoryKind::RuleSource,
            label: None,
            definition: None,
            domain: Some(domain.clone()),
            properties: serde_json::json!({
                "origin": ROSETTA_ORIGIN,
                "super_source": source.super_source.as_ref().map(referenced_title),
                "classes": classes,
            }),
        };
        ingestor
            .ingest_regulatory(&node)
            .await
            .map_err(Error::Graph)?;
        state.reg_known.insert((
            RegulatoryKind::RuleSource.as_str().to_string(),
            source.name.clone(),
        ));
        state.stats.regulatory_nodes += 1;
    }
    for (report, domain, name) in &universe.reg_reports {
        let node = RegulatoryNode {
            name: name.clone(),
            kind: RegulatoryKind::Report,
            label: None,
            definition: None,
            domain: Some(domain.clone()),
            properties: serde_json::json!({
                "origin": ROSETTA_ORIGIN,
                "timing": report.timing.as_str(),
                "input_type": referenced_title(&report.input_type),
                "report_type": named_ref_title(&report.report_type),
                "eligibility_rules": report.eligibility_rules.iter().map(named_ref_title)
                    .collect::<Vec<_>>(),
                "rule_source": report.rule_source.as_ref().map(named_ref_title),
                "regulatory": {
                    "body": named_ref_title(&report.regulatory.body),
                    "corpora": report.regulatory.corpora.iter().map(named_ref_title)
                        .collect::<Vec<_>>(),
                    "segments": report.regulatory.segments.iter().map(|segment| {
                        serde_json::json!({
                            "segment": named_ref_title(&segment.segment),
                            "reference": segment.reference,
                        })
                    }).collect::<Vec<_>>(),
                },
            }),
        };
        ingestor
            .ingest_regulatory(&node)
            .await
            .map_err(Error::Graph)?;
        state
            .reg_known
            .insert((RegulatoryKind::Report.as_str().to_string(), name.clone()));
        state.stats.regulatory_nodes += 1;
    }
    Ok(())
}

/// ── Bridge pass 0b: functions (issue #263). Deterministic order —
/// functions sorted by name within a domain (sorted at scan time) — drives
/// extends resolution: every child carries the parent's name (denormalized
/// `extends` field) and the backend links the `FunctionExtends` edge when
/// the parent node exists in this run. An extends CYCLE is a hard error
/// naming the cycle (a cyclic family could never be code-generated in
/// dependency order, so half-bridging it would be worse than failing).
pub(super) async fn bridge_functions(
    ingestor: &dyn GraphIngestor,
    universe: &RosettaUniverse<'_>,
    state: &mut RosettaBridgeState,
) -> Result<()> {
    if let Some(cycle) = detect_function_extends_cycle(&universe.func_index) {
        return Err(Error::RosettaModel {
            file: "rosetta workspace".to_string(),
            reason: format!("function extends cycle detected: {cycle}"),
        });
    }
    for (function, domain) in &universe.func_index {
        let node = function_node(function, domain);
        ingestor
            .ingest_function(&node)
            .await
            .map_err(Error::Graph)?;
        state.stats.functions_ingested += 1;
        // Doc-reference edges come after the node exists (backends match
        // the owner by name).
        for doc in &function.doc_references {
            emit_doc_reference_edges(
                ingestor,
                &RegulatoryOwner::Function(node.name.clone()),
                doc,
                &state.reg_known,
                &mut state.stats,
            )
            .await?;
        }
    }
    // The FunctionExtends edges come after every function node exists —
    // name-ordered ingestion is not parent-first. Only functions of this
    // run are linked (a parent outside the run, e.g. a builtin, is
    // unresolvable here); the child's `extends` field keeps the name.
    for (function, _) in &universe.func_index {
        let Some(parent) = function.super_function.as_ref().map(referenced_title) else {
            continue;
        };
        if universe.func_index.iter().any(|(f, _)| f.name == parent) {
            ingestor
                .ingest_edge(&function.name, &parent, EdgeType::FunctionExtends, None)
                .await
                .map_err(Error::Graph)?;
        }
    }
    Ok(())
}

/// ── Bridge pass 0c: rules (issue #264). Deterministic order — rules
/// sorted by name within a domain (sorted at scan time). Rules do NOT
/// extend (sigil has no rule parent ref), so there are no cycle semantics
/// to check.
pub(super) async fn bridge_rules(
    ingestor: &dyn GraphIngestor,
    universe: &RosettaUniverse<'_>,
    state: &mut RosettaBridgeState,
) -> Result<()> {
    for (rule, domain) in &universe.rule_index {
        let node = rule_node(rule, domain);
        ingestor.ingest_rule(&node).await.map_err(Error::Graph)?;
        state.stats.rules_ingested += 1;
        // Doc-reference edges come after the node exists (backends match
        // the owner by name).
        for doc in &rule.doc_references {
            emit_doc_reference_edges(
                ingestor,
                &RegulatoryOwner::Rule(node.name.clone()),
                doc,
                &state.reg_known,
                &mut state.stats,
            )
            .await?;
        }
    }
    Ok(())
}

/// ── Bridge pass 1: codelist SchemaNodes + CodeLists + EnumValues.
pub(super) async fn bridge_codelist_schemas(
    ingestor: &dyn GraphIngestor,
    universe: &RosettaUniverse<'_>,
    type_suffix: &str,
    state: &mut RosettaBridgeState,
) -> Result<()> {
    let enum_by_name: HashMap<&str, &sigil_model::Enumeration> = universe
        .enum_index
        .iter()
        .map(|(e, _, _)| (e.name.as_str(), *e))
        .collect();
    for (enumeration, domain, id) in &universe.enum_index {
        if universe.known_titles.contains(&enumeration.name)
            || state.bridged.contains(&enumeration.name)
        {
            continue;
        }
        let node = enum_schema_node(
            enumeration,
            domain,
            id,
            model_namespace(universe.user_files, &enumeration.name),
            type_suffix,
        );
        ingestor.ingest_schema(&node).await.map_err(Error::Graph)?;
        state.bridged.insert(enumeration.name.clone());
        state
            .bridged_enum_schema_ids
            .insert(enumeration.name.clone(), id.clone());
        state
            .bridged_schema_ids
            .insert(enumeration.name.clone(), id.clone());
        state.stats.bridged_titles.push(enumeration.name.clone());
        state.stats.enum_schemas += 1;
        emit_schema_namespace_edge(
            ingestor,
            id,
            Some(model_namespace(universe.user_files, &enumeration.name)),
        )
        .await?;

        let codelist = CodeList {
            name: enumeration.name.clone(),
            description: enumeration.definition.as_deref().map(sanitize_description),
            pg_table_name: to_snake_case(&strip_suffix(&enumeration.name, type_suffix)),
            render_as: "codelist".to_string(),
            check_expression: None,
        };
        ingestor
            .ingest_codelist(&codelist)
            .await
            .map_err(Error::Graph)?;
        state.stats.enums += 1;
        for value in merged_enum_values(enumeration, &enum_by_name) {
            let ev = EnumValue {
                value: value.name.clone(),
                display_name: value.display.clone(),
                sort_order: state.stats.enum_values as i32,
            };
            ingestor
                .ingest_enum_value(&enumeration.name, &ev)
                .await
                .map_err(Error::Graph)?;
            state.stats.enum_values += 1;
        }
    }
    Ok(())
}

/// ── Bridge pass 2: SchemaNodes for types matching no existing schema.
pub(super) async fn bridge_data_schemas(
    ingestor: &dyn GraphIngestor,
    universe: &RosettaUniverse<'_>,
    type_suffix: &str,
    state: &mut RosettaBridgeState,
) -> Result<()> {
    for (data, domain, id, namespace) in &universe.data_index {
        if universe.known_titles.contains(&data.name) || state.bridged.contains(&data.name) {
            continue;
        }
        let node = data_schema_node(data, domain, id, namespace, type_suffix);
        ingestor.ingest_schema(&node).await.map_err(Error::Graph)?;
        state.bridged.insert(data.name.clone());
        state
            .bridged_schema_ids
            .insert(data.name.clone(), id.clone());
        state.stats.bridged_titles.push(data.name.clone());
        emit_schema_namespace_edge(ingestor, id, Some(*namespace)).await?;
        if data.is_choice {
            state.stats.choices += 1;
        }
        state.stats.conditions_recorded += data.conditions.len();

        // Named conditions land as ConditionNodes (#261). The
        // `rosetta_conditions` annotation payload on the SchemaNode stays
        // untouched — it remains the provenance snapshot pinned by #259.
        for (idx, condition) in data.conditions.iter().enumerate() {
            let name = condition
                .name
                .clone()
                .unwrap_or_else(|| format!("{}_condition_{}", data.name, idx));
            let expr_json =
                serde_json::to_string(&condition.expression.to_json()).map_err(|e| {
                    Error::RosettaModel {
                        file: data.name.clone(),
                        reason: format!("condition '{name}' expression serialization failed: {e}"),
                    }
                })?;
            let condition_node = ConditionNode {
                name,
                owner_title: data.name.clone(),
                kind: ConditionKind::Condition,
                expr_json: Some(expr_json),
                options: Vec::new(),
                definition: condition.definition.clone(),
                domain: Some(domain.clone()),
            };
            ingestor
                .ingest_condition(&condition_node)
                .await
                .map_err(Error::Graph)?;
            state.stats.conditions_ingested += 1;

            // The condition's own `[docReference ...]` metadata →
            // RegulatoryReference edges (issue #265).
            for doc in &condition.doc_references {
                emit_doc_reference_edges(
                    ingestor,
                    &RegulatoryOwner::Condition(condition_node.name.clone()),
                    doc,
                    &state.reg_known,
                    &mut state.stats,
                )
                .await?;
            }
        }

        // Choices derive ONE one_of node whose options are the referenced
        // titles of the `(0..1)` option attributes (declaration order,
        // deduplicated). one_of is not a sigil Expr kind, so `expr_json`
        // stays empty — transpilation is #262.
        if data.is_choice {
            let mut seen: HashSet<String> = HashSet::new();
            let options: Vec<String> = data
                .attributes
                .iter()
                .map(|attribute| referenced_title(&attribute.type_ref))
                .filter(|title| seen.insert(title.clone()))
                .collect();
            let one_of = ConditionNode {
                name: format!("{}_one_of", data.name),
                owner_title: data.name.clone(),
                kind: ConditionKind::OneOf,
                expr_json: None,
                options,
                definition: None,
                domain: Some(domain.clone()),
            };
            ingestor
                .ingest_condition(&one_of)
                .await
                .map_err(Error::Graph)?;
            state.stats.one_of_ingested += 1;
        }

        state.stats.types += 1;
    }
    Ok(())
}

/// ── Bridge pass 2b: rule attachment edges (issue #264). Written after
/// the schema nodes exist — `ingest_edge` MATCHes the endpoints at
/// execute time.
///
/// RuleAppliesTo (Rule → Schema): the `from TypeCall` input, matched by
/// bare title against schemas pre-existing in the graph or bridged this
/// run. An input naming no schema is a documented no-op (the rule node
/// keeps its `input_type` name either way — mirroring FunctionExtends,
/// where only in-run parents link).
pub(super) async fn bridge_rule_edges(
    ingestor: &dyn GraphIngestor,
    universe: &RosettaUniverse<'_>,
    state: &mut RosettaBridgeState,
) -> Result<()> {
    for (rule, _) in &universe.rule_index {
        let Some(input) = rule.input.as_ref().map(referenced_title) else {
            continue;
        };
        if universe.known_titles.contains(&input) || state.bridged.contains(&input) {
            ingestor
                .ingest_edge(&rule.name, &input, EdgeType::RuleAppliesTo, None)
                .await
                .map_err(Error::Graph)?;
            state.stats.rule_applies_to += 1;
        }
    }
    // RuleReference (Schema → Rule): `[ruleReference R]` entries on rule
    // source class attributes, promoted to real edges when R names a rule
    // of this run. Unresolvable references stay documented skips (counted,
    // never silent).
    for (source, _) in &universe.reg_rule_sources {
        for class in &source.classes {
            let class_title = referenced_title(&class.data);
            for attribute in &class.attributes {
                for reference in &attribute.rule_references {
                    let Some(name) = reference.rule.as_deref().map(referenced_title_str) else {
                        continue;
                    };
                    if universe
                        .rule_index
                        .iter()
                        .any(|(rule, _)| rule.name == name)
                    {
                        ingestor
                            .ingest_edge(
                                &class_title,
                                &name,
                                EdgeType::RuleReference,
                                Some(&EdgeProperties {
                                    ref_path: Some(attribute.attribute.clone()),
                                    rule_source: Some(source.name.clone()),
                                    ..Default::default()
                                }),
                            )
                            .await
                            .map_err(Error::Graph)?;
                        state.stats.rule_reference_edges += 1;
                    } else {
                        state.stats.rule_reference_skips += 1;
                        eprintln!(
                            "Warning: rule source '{}' references rule '{}' (class {}, attribute \
                             '{}') — no such rule in this run; skipping the binding edge",
                            source.name, name, class_title, attribute.attribute
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

/// ── Bridge pass 3: properties + reference/extends edges per type, in
/// the JSON path's allOf-canonical order (ancestors first, each group
/// name-sorted, first occurrence wins — gap-analysis finding 1).
pub(super) async fn bridge_properties(
    ingestor: &dyn GraphIngestor,
    universe: &RosettaUniverse<'_>,
    state: &mut RosettaBridgeState,
) -> Result<()> {
    let data_by_name: HashMap<&str, &sigil_model::Data> = universe
        .data_index
        .iter()
        .map(|(d, _, _, _)| (d.name.as_str(), *d))
        .collect();
    for (data, _, id, _) in &universe.data_index {
        if !state.bridged.contains(&data.name) {
            continue;
        }
        for attribute in ordered_bridge_attributes(data, &data_by_name) {
            let Some(prop) = attribute_property(
                attribute,
                &data.name,
                &universe.enum_titles,
                &universe.type_titles,
                &universe.alias_types,
                &mut state.stats,
            ) else {
                continue;
            };
            ingestor
                .ingest_property(&data.name, id, &prop)
                .await
                .map_err(Error::Graph)?;
            state.stats.properties += 1;

            // Reference edges to bridged/existing schema nodes (the JSON
            // path's $ref shape). Builtin-typed attributes keep the
            // ref_target only — builtins have no Schema node.
            let target_name = referenced_title(&attribute.type_ref);
            let target_schema_id = universe
                .json_schema_ids
                .get(&target_name)
                .cloned()
                .or_else(|| state.bridged_schema_ids.get(&target_name).cloned());
            if let Some(target_schema_id) = target_schema_id {
                let edge_type = if prop.is_array {
                    EdgeType::ItemsOf
                } else {
                    EdgeType::ReferencesSchema
                };
                ingestor
                    .ingest_edge(
                        &format!("{}::{}", prop.name, data.name),
                        &target_schema_id,
                        edge_type,
                        Some(&EdgeProperties {
                            ref_path: Some(target_name.clone()),
                            ..Default::default()
                        }),
                    )
                    .await
                    .map_err(Error::Graph)?;
                state.stats.edges += 1;
            }
        }

        // `extends` → ExtendsSchema edge (allOf composition), mirroring
        // Pass 4 of the JSON path.
        if let Some(parent) = &data.super_type {
            let parent_name = referenced_title(parent);
            let parent_exists = universe.known_titles.contains(&parent_name)
                || state.bridged.contains(&parent_name);
            if parent_exists {
                ingestor
                    .ingest_edge(
                        &data.name,
                        &parent_name,
                        EdgeType::ExtendsSchema,
                        Some(&EdgeProperties {
                            composition_type: Some("allOf".to_string()),
                            ..Default::default()
                        }),
                    )
                    .await
                    .map_err(Error::Graph)?;
                state.stats.extends += 1;
                state.stats.edges += 1;
            }
        }

        // The type's `[docReference ...]` metadata → RegulatoryReference
        // edges (issue #265). Attribute-level doc references stay in the
        // SchemaNode's `rosetta_attribute_annotations` payload (attributes
        // are not nodes).
        for doc in &data.doc_references {
            emit_doc_reference_edges(
                ingestor,
                &RegulatoryOwner::Schema(data.name.clone()),
                doc,
                &state.reg_known,
                &mut state.stats,
            )
            .await?;
        }
    }
    Ok(())
}

/// ── Bridge pass 4: regulatory structural edges (issue #265).
/// Best-effort: a target no declared user-file element backs is skipped
/// (sigil passes doc references through unvalidated, and backends match
/// by name + kind).
pub(super) async fn bridge_regulatory_edges(
    ingestor: &dyn GraphIngestor,
    universe: &RosettaUniverse<'_>,
    state: &mut RosettaBridgeState,
) -> Result<()> {
    for (corpus, _) in &universe.reg_corpora {
        if let Some(parent) = &corpus.body {
            link_regulatory(
                ingestor,
                &RegulatoryOwner::Regulatory {
                    name: corpus.name.clone(),
                    kind: RegulatoryKind::Corpus,
                },
                &referenced_title_str(parent),
                RegulatoryKind::Body,
                RegulatoryEdgeKind::CorpusInBody,
                None,
                &state.reg_known,
                &mut state.stats,
            )
            .await?;
        }
    }
    for (source, _) in &universe.reg_rule_sources {
        if let Some(super_source) = &source.super_source {
            link_regulatory(
                ingestor,
                &RegulatoryOwner::Regulatory {
                    name: source.name.clone(),
                    kind: RegulatoryKind::RuleSource,
                },
                &referenced_title(super_source),
                RegulatoryKind::RuleSource,
                RegulatoryEdgeKind::DerivesFrom,
                None,
                &state.reg_known,
                &mut state.stats,
            )
            .await?;
        }
    }
    for (report, _, name) in &universe.reg_reports {
        let owner = RegulatoryOwner::Regulatory {
            name: name.clone(),
            kind: RegulatoryKind::Report,
        };
        link_regulatory(
            ingestor,
            &owner,
            &named_ref_title(&report.regulatory.body),
            RegulatoryKind::Body,
            RegulatoryEdgeKind::Reference,
            None,
            &state.reg_known,
            &mut state.stats,
        )
        .await?;
        for corpus in &report.regulatory.corpora {
            link_regulatory(
                ingestor,
                &owner,
                &named_ref_title(corpus),
                RegulatoryKind::Corpus,
                RegulatoryEdgeKind::Reference,
                None,
                &state.reg_known,
                &mut state.stats,
            )
            .await?;
        }
        for segment in &report.regulatory.segments {
            link_regulatory(
                ingestor,
                &owner,
                &named_ref_title(&segment.segment),
                RegulatoryKind::Segment,
                RegulatoryEdgeKind::Reference,
                Some(&segment.reference),
                &state.reg_known,
                &mut state.stats,
            )
            .await?;
        }
        if let Some(rule_source) = &report.rule_source {
            link_regulatory(
                ingestor,
                &owner,
                &named_ref_title(rule_source),
                RegulatoryKind::RuleSource,
                RegulatoryEdgeKind::RuleSource,
                None,
                &state.reg_known,
                &mut state.stats,
            )
            .await?;
        }
    }
    Ok(())
}
