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
//! - namespaces: recorded per file (`rosetta_namespace` annotation +
//!   stats) but NOT mapped onto domains and NOT given nodes — namespaces
//!   are NOT domains; the first-class uplift is #267/#268. Until then the
//!   domain falls back to the mox bridge's `resolve_domain` semantics
//!   over the dotted namespace.
//! - out-of-data-plane elements (func/rules/reports/annotation decls/
//!   basic types/aliases/schemas/bodies/corpora/segments/meta types/
//!   external rule sources/library functions) are counted and named as
//!   `needs_review` — never silently dropped, never bridged (their node
//!   families are #263–#265).
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

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use sigil_model::{CardinalityMax, SemanticElement};
use sigil_resolve::Resolution;

use codegraph_config::config::DomainConfig;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{
    CodeList, ConditionKind, ConditionNode, EdgeProperties, EdgeType, EnumValue, PropertyNode,
    SchemaNode,
};
use codegraph_naming::{escape_rust_keyword, strip_suffix, to_kebab_case, to_snake_case};
use codegraph_type_contracts::{PgType, RefClassificationKind};

use crate::error::{Error, Result};
use crate::ingest::async_ingest::{sanitize_description, sanitize_rust_type_name};
use crate::ingest::mox_ingest::{build_projection, resolve_domain, strip_code_suffix};

/// Provenance marker on every rosetta-bridged node (`custom_annotations`
/// key `origin`, value `rosetta`) — distinct from the mox `source` key.
pub const ROSETTA_ORIGIN: &str = "rosetta";

/// The Rosetta builtin simple types (issue #256) and their JSON-path
/// primitive mappings. `time` has no `PgType` variant and maps to TEXT
/// carrying an ISO-8601 time string (format hint `time`), mirroring how
/// the JSON path treats unmapped string formats.
fn builtin_mapping(name: &str) -> Option<(PgType, Option<&'static str>, &'static str)> {
    match name {
        "int" => Some((PgType::Integer, None, "integer")),
        "number" => Some((PgType::DoublePrecision, None, "number")),
        "string" => Some((PgType::Text, None, "string")),
        "boolean" => Some((PgType::Boolean, None, "boolean")),
        "date" => Some((PgType::Date, Some("date"), "string")),
        "dateTime" => Some((PgType::Timestamptz, Some("date-time"), "string")),
        "zonedDateTime" => Some((PgType::Timestamptz, Some("date-time"), "string")),
        "time" => Some((PgType::Text, Some("time"), "string")),
        _ => None,
    }
}

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
    pub namespaces: usize,
    pub namespace_imports: usize,
    /// Out-of-data-plane elements counted for review (never silently
    /// dropped, never bridged — #263–#265 own their node families).
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
             ({} nodes, {} one_of), {} namespaces ({} imports)",
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
    if parsed.is_empty() {
        return Ok(RosettaIngestOutcome { stats });
    }

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

    // `Resolution.files` = builtin files first, then user files in
    // submission order. Builtins contribute to the name universes but are
    // never bridged.
    let builtin_count = sigil_resolve::builtin_files().len();
    let user_files = resolution.files.get(builtin_count..).unwrap_or(&[]);

    // Name universes for reference classification (bare last segments —
    // v1 keeps Rosetta's per-namespace uniqueness; the title-suffix
    // collision rule matches the JSON skip-set contract).
    let mut enum_titles: HashSet<String> = HashSet::new();
    let mut type_titles: HashSet<String> = HashSet::new();
    for model in &resolution.files {
        collect_universes(model, &mut type_titles, &mut enum_titles);
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
    // everything else is out-of-data-plane and recorded for review.
    let mut data_index: Vec<(&sigil_model::Data, String, String, &str)> = Vec::new();
    let mut enum_index: Vec<(&sigil_model::Enumeration, String, String)> = Vec::new();
    let mut bridged_schema_ids: HashMap<String, String> = HashMap::new();
    let mut seen_elements: HashSet<String> = HashSet::new();
    let mut namespaces_seen: HashSet<String> = HashSet::new();
    for model in user_files {
        if !model.namespace.is_empty() {
            namespaces_seen.insert(model.namespace.clone());
        }
        // Namespaces are NOT domains (#267/#268): domain resolution
        // mirrors the mox package fallback until the namespace
        // assignment config lands.
        let (domain, _) = resolve_domain(domain_config, &model.namespace);
        for element in &model.elements {
            let name = element.name();
            if !seen_elements.insert(name.to_string()) {
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
                SemanticElement::Function(_)
                | SemanticElement::Rule(_)
                | SemanticElement::Report(_)
                | SemanticElement::Annotation(_)
                | SemanticElement::TypeAlias(_)
                | SemanticElement::BasicType(_)
                | SemanticElement::RecordType(_)
                | SemanticElement::LibraryFunction(_)
                | SemanticElement::ExternalRuleSource(_)
                | SemanticElement::Schema(_)
                | SemanticElement::Body(_)
                | SemanticElement::Corpus(_)
                | SemanticElement::Segment(_)
                | SemanticElement::MetaType(_) => {
                    stats.needs_review += 1;
                    stats
                        .needs_review_names
                        .push(format!("{} {name}", element_kind_label(element)));
                }
            }
        }
    }
    stats.namespaces = namespaces_seen.len();
    stats.namespace_imports = user_files.iter().map(|model| model.imports.len()).sum();

    // ── Bridge pass 1: codelist SchemaNodes + CodeLists + EnumValues.
    let mut bridged: HashSet<String> = HashSet::new();
    let mut bridged_enum_schema_ids: HashMap<String, String> = HashMap::new();
    let enum_by_name: HashMap<&str, &sigil_model::Enumeration> = enum_index
        .iter()
        .map(|(e, _, _)| (e.name.as_str(), *e))
        .collect();
    for (enumeration, domain, id) in &enum_index {
        if known_titles.contains(&enumeration.name) || bridged.contains(&enumeration.name) {
            continue;
        }
        let node = enum_schema_node(
            enumeration,
            domain,
            id,
            model_namespace(user_files, &enumeration.name),
            type_suffix,
        );
        ingestor.ingest_schema(&node).await.map_err(Error::Graph)?;
        bridged.insert(enumeration.name.clone());
        bridged_enum_schema_ids.insert(enumeration.name.clone(), id.clone());
        bridged_schema_ids.insert(enumeration.name.clone(), id.clone());
        stats.bridged_titles.push(enumeration.name.clone());
        stats.enum_schemas += 1;

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
        stats.enums += 1;
        for value in merged_enum_values(enumeration, &enum_by_name) {
            let ev = EnumValue {
                value: value.name.clone(),
                display_name: value.display.clone(),
                sort_order: stats.enum_values as i32,
            };
            ingestor
                .ingest_enum_value(&enumeration.name, &ev)
                .await
                .map_err(Error::Graph)?;
            stats.enum_values += 1;
        }
    }

    // ── Bridge pass 2: SchemaNodes for types matching no existing schema.
    for (data, domain, id, namespace) in &data_index {
        if known_titles.contains(&data.name) || bridged.contains(&data.name) {
            continue;
        }
        let node = data_schema_node(data, domain, id, namespace, type_suffix);
        ingestor.ingest_schema(&node).await.map_err(Error::Graph)?;
        bridged.insert(data.name.clone());
        bridged_schema_ids.insert(data.name.clone(), id.clone());
        stats.bridged_titles.push(data.name.clone());
        if data.is_choice {
            stats.choices += 1;
        }
        stats.conditions_recorded += data.conditions.len();

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
            stats.conditions_ingested += 1;
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
            stats.one_of_ingested += 1;
        }

        stats.types += 1;
    }

    // ── Bridge pass 3: properties + reference/extends edges per type, in
    // the JSON path's allOf-canonical order (ancestors first, each group
    // name-sorted, first occurrence wins — gap-analysis finding 1).
    let data_by_name: HashMap<&str, &sigil_model::Data> = data_index
        .iter()
        .map(|(d, _, _, _)| (d.name.as_str(), *d))
        .collect();
    for (data, _, id, _) in &data_index {
        if !bridged.contains(&data.name) {
            continue;
        }
        for attribute in ordered_bridge_attributes(data, &data_by_name) {
            let Some(prop) = attribute_property(
                attribute,
                &data.name,
                &enum_titles,
                &type_titles,
                &mut stats,
            ) else {
                continue;
            };
            ingestor
                .ingest_property(&data.name, id, &prop)
                .await
                .map_err(Error::Graph)?;
            stats.properties += 1;

            // Reference edges to bridged/existing schema nodes (the JSON
            // path's $ref shape). Builtin-typed attributes keep the
            // ref_target only — builtins have no Schema node.
            let target_name = referenced_title(&attribute.type_ref);
            let target_schema_id = json_schema_ids
                .get(&target_name)
                .cloned()
                .or_else(|| bridged_schema_ids.get(&target_name).cloned());
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
                stats.edges += 1;
            }
        }

        // `extends` → ExtendsSchema edge (allOf composition), mirroring
        // Pass 4 of the JSON path.
        if let Some(parent) = &data.super_type {
            let parent_name = referenced_title(parent);
            let parent_exists =
                known_titles.contains(&parent_name) || bridged.contains(&parent_name);
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
                stats.extends += 1;
                stats.edges += 1;
            }
        }
    }

    if stats.needs_review != 0 {
        eprintln!(
            "Warning: rosetta model has {} out-of-data-plane element(s) recorded as \
             needs_review (node families land in #263-#265): {}",
            stats.needs_review,
            stats.needs_review_names.join(", ")
        );
    }

    Ok(RosettaIngestOutcome { stats })
}

/// Namespace of the file declaring `element_name` (lookup by bridged
/// element name; empty when not found — namespace recording is best-effort
/// until #268 gives namespaces nodes).
fn model_namespace<'a>(user_files: &'a [sigil_model::ModelFile], element_name: &str) -> &'a str {
    user_files
        .iter()
        .find(|model| {
            model
                .elements
                .iter()
                .any(|element| element.name() == element_name)
        })
        .map(|model| model.namespace.as_str())
        .unwrap_or("")
}

fn element_kind_label(element: &SemanticElement) -> &'static str {
    match element {
        SemanticElement::Data(_) => "type",
        SemanticElement::Enumeration(_) => "enum",
        SemanticElement::Annotation(_) => "annotation-decl",
        SemanticElement::TypeAlias(_) => "type-alias",
        SemanticElement::BasicType(_) => "basic-type",
        SemanticElement::RecordType(_) => "record-type",
        SemanticElement::LibraryFunction(_) => "library-function",
        SemanticElement::Function(_) => "func",
        SemanticElement::Rule(_) => "rule",
        SemanticElement::Report(_) => "report",
        SemanticElement::ExternalRuleSource(_) => "external-rule-source",
        SemanticElement::Schema(_) => "schema",
        SemanticElement::Body(_) => "body",
        SemanticElement::Corpus(_) => "corpus",
        SemanticElement::Segment(_) => "segment",
        SemanticElement::MetaType(_) => "meta-type",
    }
}

/// `<namespace>/<Name>` — the mox bridge's `package/class` convention.
fn schema_id(namespace: &str, name: &str) -> String {
    format!("{namespace}/{name}")
}

/// Bare title of a reference: the last segment of the written name (v1 —
/// Rosetta references are namespace-qualified but titles are bare; the
/// resolution pass already rejected unknown names).
fn referenced_title(type_ref: &sigil_model::TypeRef) -> String {
    type_ref
        .name
        .rsplit('.')
        .next()
        .unwrap_or(&type_ref.name)
        .to_string()
}

fn collect_universes(
    model: &sigil_model::ModelFile,
    types: &mut HashSet<String>,
    enums: &mut HashSet<String>,
) {
    for element in &model.elements {
        match element {
            SemanticElement::Data(data) => {
                types.insert(data.name.clone());
            }
            SemanticElement::Enumeration(enumeration) => {
                enums.insert(enumeration.name.clone());
            }
            _ => {}
        }
    }
}

/// Enum values with `extends` parents merged first (root ancestor first),
/// each level in declaration order; first occurrence of a value name wins.
fn merged_enum_values<'a>(
    enumeration: &'a sigil_model::Enumeration,
    enum_by_name: &HashMap<&str, &'a sigil_model::Enumeration>,
) -> Vec<&'a sigil_model::EnumValue> {
    let mut chain = Vec::new();
    let mut current = Some(enumeration);
    let mut visited: HashSet<String> = HashSet::new();
    while let Some(element) = current {
        if !visited.insert(element.name.clone()) {
            break;
        }
        chain.push(element);
        current = element
            .super_type
            .as_ref()
            .map(referenced_title)
            .and_then(|name| enum_by_name.get(name.as_str()).copied());
    }
    let mut ordered = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for element in chain.into_iter().rev() {
        for value in &element.values {
            if seen.insert(value.name.clone()) {
                ordered.push(value);
            }
        }
    }
    ordered
}

/// Attributes in the JSON path's allOf-canonical order: inherited
/// attributes before own, each group name-sorted, first occurrence wins.
/// Rosetta `override` attributes REPLACE the inherited entry in place
/// (override semantics beat first-wins), keeping the inherited slot's
/// position so field order stays canonical.
fn ordered_bridge_attributes<'a>(
    data: &'a sigil_model::Data,
    data_by_name: &HashMap<&str, &'a sigil_model::Data>,
) -> Vec<&'a sigil_model::Attribute> {
    let mut ordered = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    collect_ancestor_attributes(data, data_by_name, &mut visited, &mut ordered);
    let mut own: Vec<&sigil_model::Attribute> = data.attributes.iter().collect();
    own.sort_by(|a, b| a.name.cmp(&b.name));
    ordered.extend(own);

    let mut positions: HashMap<String, usize> = HashMap::new();
    let mut result: Vec<&'a sigil_model::Attribute> = Vec::with_capacity(ordered.len());
    for attribute in ordered {
        match positions.get(&attribute.name) {
            Some(&index) => {
                if attribute.is_override {
                    result[index] = attribute;
                }
            }
            None => {
                positions.insert(attribute.name.clone(), result.len());
                result.push(attribute);
            }
        }
    }
    result
}

fn collect_ancestor_attributes<'a>(
    data: &'a sigil_model::Data,
    data_by_name: &HashMap<&str, &'a sigil_model::Data>,
    visited: &mut HashSet<String>,
    ordered: &mut Vec<&'a sigil_model::Attribute>,
) {
    if !visited.insert(data.name.clone()) {
        return;
    }
    if let Some(parent) = &data.super_type {
        let parent_name = referenced_title(parent);
        if let Some(parent_data) = data_by_name.get(parent_name.as_str()) {
            collect_ancestor_attributes(parent_data, data_by_name, visited, ordered);
        }
    }
    let mut group: Vec<&sigil_model::Attribute> = data.attributes.iter().collect();
    group.sort_by(|a, b| a.name.cmp(&b.name));
    ordered.extend(group);
}

fn rosetta_annotations() -> HashMap<String, serde_json::Value> {
    HashMap::from([(
        "origin".to_string(),
        serde_json::Value::String(ROSETTA_ORIGIN.to_string()),
    )])
}

/// JSON payload for one attribute's annotation surface (labels, rule
/// references, doc references, `[metadata]`-style annotation refs).
fn attribute_annotations_json(attribute: &sigil_model::Attribute) -> Option<serde_json::Value> {
    let mut payload = serde_json::Map::new();
    if !attribute.annotations.is_empty() {
        payload.insert(
            "annotations".to_string(),
            serde_json::to_value(&attribute.annotations).ok()?,
        );
    }
    if !attribute.labels.is_empty() {
        payload.insert(
            "labels".to_string(),
            serde_json::to_value(&attribute.labels).ok()?,
        );
    }
    if !attribute.rule_references.is_empty() {
        payload.insert(
            "rule_references".to_string(),
            serde_json::to_value(&attribute.rule_references).ok()?,
        );
    }
    if !attribute.doc_references.is_empty() {
        payload.insert(
            "doc_references".to_string(),
            serde_json::to_value(&attribute.doc_references).ok()?,
        );
    }
    (!payload.is_empty()).then_some(serde_json::Value::Object(payload))
}

fn data_schema_node(
    data: &sigil_model::Data,
    domain: &str,
    schema_id: &str,
    namespace: &str,
    type_suffix: &str,
) -> SchemaNode {
    // Choices KEEP their full name for code identifiers: the Type-suffix
    // strip assumes types carry the suffix, but in Rosetta it is the
    // choices that do (`choice ProductType` optioning `type Product`) —
    // stripping would collide both at pg_table_name `product`
    // (gap-analysis defect #9).
    let stripped = if data.is_choice {
        data.name.clone()
    } else {
        strip_suffix(&data.name, type_suffix)
    };
    let mut custom_annotations = rosetta_annotations();
    custom_annotations.insert(
        "rosetta_namespace".to_string(),
        serde_json::Value::String(namespace.to_string()),
    );
    if data.is_choice {
        custom_annotations.insert("rosetta_choice".to_string(), serde_json::Value::Bool(true));
    }
    if !data.annotations.is_empty() {
        if let Ok(value) = serde_json::to_value(&data.annotations) {
            custom_annotations.insert("rosetta_annotations".to_string(), value);
        }
    }
    if !data.doc_references.is_empty() {
        if let Ok(value) = serde_json::to_value(&data.doc_references) {
            custom_annotations.insert("rosetta_doc_references".to_string(), value);
        }
    }
    let attribute_annotations: serde_json::Map<String, serde_json::Value> = data
        .attributes
        .iter()
        .filter_map(|attribute| {
            attribute_annotations_json(attribute).map(|value| (attribute.name.clone(), value))
        })
        .collect();
    if !attribute_annotations.is_empty() {
        custom_annotations.insert(
            "rosetta_attribute_annotations".to_string(),
            serde_json::Value::Object(attribute_annotations),
        );
    }
    if !data.conditions.is_empty() {
        let conditions: Vec<serde_json::Value> = data
            .conditions
            .iter()
            .map(|condition| {
                serde_json::json!({
                    "name": condition.name,
                    "expression": condition.expression.to_json(),
                })
            })
            .collect();
        custom_annotations.insert(
            "rosetta_conditions".to_string(),
            serde_json::Value::Array(conditions),
        );
    }

    SchemaNode {
        schema_id: schema_id.to_string(),
        title: data.name.clone(),
        description: data.definition.as_deref().map(sanitize_description),
        schema_type: "object".to_string(),
        // Rosetta types are auto-scored by the classifier (#258) — the
        // bridge records a VO-shaped default and never declares entities.
        classification: "value_object".to_string(),
        domain: Some(domain.to_string()),
        rel_path: schema_id.to_string(),
        pg_type: "UUID".to_string(),
        rust_type: stripped.clone(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: sanitize_rust_type_name(&stripped),
        pg_table_name: to_snake_case(&stripped),
        api_path_segment: to_kebab_case(&stripped),
        parent_schema: None,
        is_entity: false,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: data.super_type.is_some(),
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations,
    }
}

/// Module-level counter hop removed: condition counts are added to stats
/// directly in the bridge pass that builds each schema node.
fn enum_schema_node(
    enumeration: &sigil_model::Enumeration,
    domain: &str,
    schema_id: &str,
    namespace: &str,
    type_suffix: &str,
) -> SchemaNode {
    let stripped = strip_suffix(&enumeration.name, type_suffix);
    let mut custom_annotations = rosetta_annotations();
    custom_annotations.insert(
        "rosetta_namespace".to_string(),
        serde_json::Value::String(namespace.to_string()),
    );
    SchemaNode {
        schema_id: schema_id.to_string(),
        title: enumeration.name.clone(),
        description: enumeration.definition.as_deref().map(sanitize_description),
        schema_type: "string".to_string(),
        classification: "codelist".to_string(),
        domain: Some(domain.to_string()),
        rel_path: schema_id.to_string(),
        pg_type: "UUID".to_string(),
        rust_type: stripped.clone(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: sanitize_rust_type_name(&stripped),
        pg_table_name: to_snake_case(&stripped),
        api_path_segment: to_kebab_case(&stripped),
        parent_schema: None,
        is_entity: false,
        is_codelist: true,
        is_primitive_wrapper: false,
        has_all_of: enumeration.super_type.is_some(),
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations,
    }
}

/// Rosetta cardinality → the graph's two multiplicity bits. A `(min..max)`
/// cardinality is required iff `min >= 1`, and an array iff max is
/// unbounded or greater than one.
///
/// Returns `(is_required, is_array)`.
fn cardinality_flags(cardinality: &sigil_model::Cardinality) -> (bool, bool) {
    let is_required = cardinality.min >= 1;
    let is_array = match cardinality.max {
        CardinalityMax::Unbounded => true,
        CardinalityMax::Finite(max) => max > 1,
    };
    (is_required, is_array)
}

/// Rosetta cardinality → JSON-Schema-equivalent array bounds (issue #261,
/// closing gap-analysis finding 2): `(2..10)` maps to
/// `min_items = 2 / max_items = 10`. Only arrays carry item counts — a
/// scalar attribute's `min` is requiredness, not an item bound.
fn cardinality_items(
    cardinality: &sigil_model::Cardinality,
    is_array: bool,
) -> (Option<u32>, Option<u32>) {
    if !is_array {
        return (None, None);
    }
    let min_items = (cardinality.min > 1).then_some(cardinality.min);
    let max_items = match cardinality.max {
        CardinalityMax::Finite(max) if max > 1 => Some(max),
        _ => None,
    };
    (min_items, max_items)
}

fn classification_str(kind: &RefClassificationKind) -> &'static str {
    match kind {
        RefClassificationKind::PrimitiveWrapper => "primitive_wrapper",
        RefClassificationKind::ArrayWrapper => "array_wrapper",
        RefClassificationKind::RangeWrapper => "range_wrapper",
        RefClassificationKind::CodelistReference => "codelist",
        RefClassificationKind::CodelistCheck => "codelist_check",
        RefClassificationKind::InlineEnum => "inline_enum",
        RefClassificationKind::EntityReference => "entity_reference",
        RefClassificationKind::ValueObject => "value_object",
        RefClassificationKind::CompositeWrapper => "composite_wrapper",
        RefClassificationKind::MediaWrapper => "media_wrapper",
        RefClassificationKind::StructuredWrapper => "structured_wrapper",
    }
}

fn attribute_property(
    attribute: &sigil_model::Attribute,
    schema_title: &str,
    enum_titles: &HashSet<String>,
    type_titles: &HashSet<String>,
    stats: &mut RosettaIngestStats,
) -> Option<PropertyNode> {
    let target_title = referenced_title(&attribute.type_ref);
    let (is_required, is_array) = cardinality_flags(&attribute.cardinality);
    let (min_items, max_items) = cardinality_items(&attribute.cardinality, is_array);

    let (kind, pg_base, rust_base, sea_base, ref_target, format_hint, prop_type) =
        if let Some((pg, format, json_type)) = builtin_mapping(&target_title) {
            (
                RefClassificationKind::PrimitiveWrapper,
                pg.pg_ddl().to_string(),
                pg.canonical_rust_type().as_rust_str().to_string(),
                pg.sea_orm_type().to_string(),
                None,
                format.map(str::to_string),
                json_type.to_string(),
            )
        } else if enum_titles.contains(&target_title) {
            (
                RefClassificationKind::CodelistReference,
                "TEXT".to_string(),
                "String".to_string(),
                "Text".to_string(),
                Some(target_title.clone()),
                None,
                "string".to_string(),
            )
        } else if type_titles.contains(&target_title) {
            (
                RefClassificationKind::EntityReference,
                String::new(),
                target_title.clone(),
                String::new(),
                Some(target_title.clone()),
                None,
                "object".to_string(),
            )
        } else {
            eprintln!(
                "Warning: rosetta attribute '{schema_title}.{}' references unknown type \
                 '{target_title}' — mapping to TEXT",
                attribute.name
            );
            stats.skipped += 1;
            (
                RefClassificationKind::PrimitiveWrapper,
                "TEXT".to_string(),
                "String".to_string(),
                "Text".to_string(),
                None,
                None,
                "string".to_string(),
            )
        };

    // Arrays of primitives wrap in Vec + pg [] suffix; entity/VO/codelist
    // arrays keep child-table semantics (mirrors the JSON path's array
    // branch and the mox bridge).
    let (render_strategy, pg_type, rust_type) = if is_array {
        match kind {
            RefClassificationKind::EntityReference
            | RefClassificationKind::ValueObject
            | RefClassificationKind::CodelistReference
            | RefClassificationKind::CodelistCheck => {
                ("child_table".to_string(), pg_base, rust_base)
            }
            _ => (
                classification_str(&kind).to_string(),
                format!("{pg_base}[]"),
                format!("Vec<{rust_base}>"),
            ),
        }
    } else {
        (classification_str(&kind).to_string(), pg_base, rust_base)
    };

    let sanitized_name = attribute.name.replace(['@', '-'], "");
    let snake = to_snake_case(&sanitized_name);
    let projection = build_projection(&kind, &snake, &pg_type, &rust_type, &sea_base);
    let mut rust_field_name = escape_rust_keyword(&snake);
    if matches!(
        kind,
        RefClassificationKind::CodelistReference | RefClassificationKind::CodelistCheck
    ) {
        rust_field_name = strip_code_suffix(&rust_field_name);
    }

    Some(PropertyNode {
        name: attribute.name.clone(),
        prop_type,
        description: attribute.definition.as_deref().map(sanitize_description),
        format: format_hint,
        is_required,
        is_nullable: !is_required,
        is_array,
        min_items,
        max_items,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: snake.clone(),
        pg_column_type: pg_type,
        rust_field_name,
        rust_field_type: rust_type,
        sea_orm_type: sea_base,
        render_strategy,
        ref_target,
        classification: None,
        projection: Some(projection),
        classification_kind: Some(kind),
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    })
}
