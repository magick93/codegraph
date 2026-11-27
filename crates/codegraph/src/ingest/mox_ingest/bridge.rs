//! The `MoxBridge` driver behind [`super::ingest_mox_files`] (issue #751
//! extraction): scan universes, then run the bridge passes in the historical
//! order — namespaces, enum codelists, register codelists, the
//! `MoxDomainModel` carrier, class/enum schema nodes, InNamespace edges,
//! properties + reference/extends edges — over the shared mutable state.

use std::collections::{BTreeMap, HashMap, HashSet};

use phero_ir::{FeatureKind, TypeRef};

use codegraph_config::config::DomainConfig;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{
    CodeList, EdgeProperties, EdgeType, EnumValue, MoxDerivedFeatureNode, MoxDomainModel, MoxEntry,
    MoxFacet, MoxOperationNode, MoxPackageNode, MoxParam, MoxVocabularyNode,
};
use codegraph_naming::{strip_suffix, to_snake_case};

use crate::error::{Error, Result};
use crate::ingest::async_ingest::sanitize_description;
use crate::ingest::mox_ingest::{
    ClassBridge, ClassEntry, EnumEntry, MOX_NAMESPACE_SOURCE, MoxIngestOutcome, MoxIngestStats,
    MoxSchemaImport, PendingAliasRef, class_schema_node, class_table_name, default_value_json,
    emit_schema_namespace_edge, enum_schema_node, feature_property, ingest_namespaces_deduped,
    non_empty_namespace, ordered_bridge_features, resolve_domain, type_ref_name,
};

pub(super) struct MoxBridge<'a> {
    model: &'a phero_ir::Model,
    ingestor: &'a dyn GraphIngestor,
    domain_config: &'a DomainConfig,
    type_suffix: &'a str,
    import_names: HashSet<String>,
    known_titles: HashSet<String>,
    json_schema_ids: HashMap<String, String>,
    // Universes (built by [`MoxBridge::scan_universes`]).
    class_index: Vec<ClassEntry<'a>>,
    enum_index: Vec<EnumEntry<'a>>,
    class_schema_ids: HashMap<String, String>,
    datatype_formats: HashMap<(String, String), String>,
    contains_targets: HashSet<String>,
    refers_targets: HashSet<String>,
    // Mutable pass state.
    stats: MoxIngestStats,
    bridged: HashSet<String>,
    bridged_enum_schema_ids: HashMap<String, String>,
    mox: MoxDomainModel,
    unmatched_with_behavior: Vec<String>,
    pending_alias_refs: Vec<PendingAliasRef>,
}

impl<'a> MoxBridge<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        model: &'a phero_ir::Model,
        ingestor: &'a dyn GraphIngestor,
        domain_config: &'a DomainConfig,
        type_suffix: &'a str,
        import_names: HashSet<String>,
        known_titles: HashSet<String>,
        json_schema_ids: HashMap<String, String>,
        stats: MoxIngestStats,
    ) -> Self {
        Self {
            model,
            ingestor,
            domain_config,
            type_suffix,
            import_names,
            known_titles,
            json_schema_ids,
            class_index: Vec::new(),
            enum_index: Vec::new(),
            class_schema_ids: HashMap::new(),
            datatype_formats: HashMap::new(),
            contains_targets: HashSet::new(),
            refers_targets: HashSet::new(),
            stats,
            bridged: HashSet::new(),
            bridged_enum_schema_ids: HashMap::new(),
            mox: MoxDomainModel::default(),
            unmatched_with_behavior: Vec::new(),
            pending_alias_refs: Vec::new(),
        }
    }

    /// ── Namespace bridge (issue #268): `package <dotted.name>` declarations
    /// become NamespaceNodes (source="mox") with the dotted parent chain as
    /// NamespaceParent edges. Namespace-less models contribute nothing, so
    /// their graph (and generated output) stays byte-identical.
    pub(super) async fn bridge_namespaces(&mut self) -> Result<()> {
        let mut namespace_fqns: BTreeMap<String, String> = BTreeMap::new();
        for package in &self.model.packages {
            if let Some(ns) = non_empty_namespace(&package.name) {
                namespace_fqns
                    .entry(ns)
                    .or_insert_with(|| MOX_NAMESPACE_SOURCE.to_string());
            }
        }
        self.stats.namespaces = ingest_namespaces_deduped(self.ingestor, &namespace_fqns).await?;
        Ok(())
    }

    /// ── Class bridge (#229): collect the class universe first so entity vs
    /// value-object is decided from the whole model, not per package.
    pub(super) fn scan_class_universe(&mut self) {
        let mut seen_classes: HashSet<String> = HashSet::new();
        let mut datatype_formats: HashMap<(String, String), String> = HashMap::new();
        for package in &self.model.packages {
            for datatype in &package.datatypes {
                if let Some(ref format) = datatype.format {
                    datatype_formats.insert(
                        (package.name.clone(), datatype.name.clone()),
                        format.clone(),
                    );
                }
            }
            for class in &package.classes {
                // Imported schemas lower to nominal feature-less classes in
                // their package (upstream v1 is opaque). The JSON pipeline owns
                // their graph nodes — the bridge never creates them (issue #230).
                if self.import_names.contains(&class.name) {
                    continue;
                }
                if !seen_classes.insert(class.name.clone()) {
                    eprintln!(
                        "Warning: mox class '{}' declared in multiple packages — keeping the first",
                        class.name
                    );
                    self.stats.skipped += 1;
                    continue;
                }
                let (domain, _) = resolve_domain(self.domain_config, &package.name);
                let schema_id = format!("{}/{}", package.name, class.name);
                self.class_schema_ids
                    .insert(class.name.clone(), schema_id.clone());
                self.class_index.push(ClassEntry {
                    class,
                    domain,
                    schema_id,
                    namespace: non_empty_namespace(&package.name),
                });
            }
        }
        self.datatype_formats = datatype_formats;
    }

    /// Enum universe: enums bridge to codelist Schema nodes (issue #233) so
    /// the graph carries the same shapes the JSON codelist path produces.
    pub(super) fn scan_enum_universe(&mut self) {
        let mut seen_enums: HashSet<String> = HashSet::new();
        for package in &self.model.packages {
            for enum_def in &package.enums {
                if !seen_enums.insert(enum_def.name.clone()) {
                    continue;
                }
                let (domain, _) = resolve_domain(self.domain_config, &package.name);
                let schema_id = format!("{}/{}", package.name, enum_def.name);
                self.enum_index.push(EnumEntry {
                    enum_def,
                    domain,
                    schema_id,
                    namespace: non_empty_namespace(&package.name),
                });
            }
        }
    }

    /// Contained-only classes become value objects: a class that is targeted
    /// exclusively by `contains` features has its rows nested in the owner's
    /// child tables, never referenced by FK. `refers` wins on conflict.
    pub(super) fn scan_containment_targets(&mut self) {
        for entry in &self.class_index {
            for feature in &entry.class.features {
                if feature.is_derived {
                    continue;
                }
                if let TypeRef::Class { name, .. } = &feature.type_ {
                    match feature.kind {
                        FeatureKind::Containment => {
                            self.contains_targets.insert(name.clone());
                        }
                        FeatureKind::CrossReference => {
                            self.refers_targets.insert(name.clone());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Enums → CodeList + EnumValue nodes (declaration order, per package).
    pub(super) async fn bridge_enum_codelists(&mut self) -> Result<()> {
        for package in &self.model.packages {
            for enum_def in &package.enums {
                let codelist = CodeList {
                    name: enum_def.name.clone(),
                    description: enum_def.description.as_deref().map(sanitize_description),
                    pg_table_name: to_snake_case(&strip_suffix(&enum_def.name, self.type_suffix)),
                    render_as: "codelist".to_string(),
                    check_expression: None,
                };
                self.ingestor
                    .ingest_codelist(&codelist)
                    .await
                    .map_err(Error::Graph)?;
                self.stats.enums += 1;
                for literal in &enum_def.literals {
                    let ev = EnumValue {
                        value: literal.name.clone(),
                        display_name: literal.label.clone(),
                        sort_order: literal.value.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
                    };
                    self.ingestor
                        .ingest_enum_value(&enum_def.name, &ev)
                        .await
                        .map_err(Error::Graph)?;
                }
            }
        }
        Ok(())
    }

    /// Register classes → the reference-data (codelist/seed-table) plane
    /// (issue #668). A `register` is a flat org/role noun space: attributes
    /// only, no relations, no `extends`, no table annotation. codegraph's
    /// reference-data plane is the codelist/seed-table one (a
    /// `common.{table}` seed migration + the codelist Rust enum, owned by the
    /// codelist generators, never a transactional aggregate), so a register
    /// gets a `CodeList` node here and a codelist-classified `SchemaNode` in
    /// the class-schema pass below. Generation ordering skips register schemas
    /// (`codegraph_generate::ordering`), which is what keeps them off the
    /// entity/CRUD plane — no table module, no repository, no handlers.
    /// Registers declare no literals, so there are no `EnumValue` rows: the
    /// rows are consumer data, seeded by migration/ops (the existing
    /// `empty_enum.tera` shape covers the zero-variant Rust enum).
    pub(super) async fn bridge_register_codelists(&mut self) -> Result<()> {
        for entry in &self.class_index {
            if !entry.is_register() {
                continue;
            }
            let codelist = CodeList {
                name: entry.class.name.clone(),
                description: entry.class.description.as_deref().map(sanitize_description),
                pg_table_name: class_table_name(entry.class, self.type_suffix),
                render_as: "codelist".to_string(),
                check_expression: None,
            };
            self.ingestor
                .ingest_codelist(&codelist)
                .await
                .map_err(Error::Graph)?;
            self.stats.registers += 1;
        }
        Ok(())
    }

    /// Vocabulary/operation/derived-feature carriers (`MoxDomainModel`) plus
    /// the class-link bookkeeping for behavior-bearing classes.
    pub(super) fn build_domain_model(&mut self) {
        for package in &self.model.packages {
            self.mox.packages.push(MoxPackageNode {
                name: package.name.clone(),
            });
            for vocab in &package.vocabularies {
                self.mox.vocabularies.push(MoxVocabularyNode {
                    name: vocab.name.clone(),
                    package: package.name.clone(),
                    source: vocab.source.clone(),
                    version: vocab.version.clone(),
                    key_facet: vocab.key.clone(),
                    facets: vocab
                        .facets
                        .iter()
                        .map(|f| MoxFacet {
                            name: f.name.clone(),
                            type_: f.type_.to_string(),
                        })
                        .collect(),
                    entries: vocab
                        .entries
                        .iter()
                        .map(|e| MoxEntry {
                            key: e.key.clone(),
                            facets: e
                                .facets
                                .iter()
                                .map(|(name, value)| (name.clone(), default_value_json(value)))
                                .collect(),
                        })
                        .collect(),
                });
                self.stats.vocabularies += 1;
            }
            for class in &package.classes {
                let mut attached = false;
                for op in &class.operations {
                    self.mox.operations.push(MoxOperationNode {
                        name: op.name.clone(),
                        class: class.name.clone(),
                        package: package.name.clone(),
                        description: op.description.clone(),
                        return_type: type_ref_name(&op.return_type),
                        params: op
                            .params
                            .iter()
                            .map(|p| MoxParam {
                                name: p.name.clone(),
                                type_: type_ref_name(&p.type_),
                            })
                            .collect(),
                        bodies: op.bodies.clone(),
                    });
                    self.stats.operations += 1;
                    attached = true;
                }
                for feature in &class.features {
                    if !feature.is_derived {
                        continue;
                    }
                    self.mox.derived_features.push(MoxDerivedFeatureNode {
                        name: feature.name.clone(),
                        class: class.name.clone(),
                        package: package.name.clone(),
                        type_ref: type_ref_name(&feature.type_),
                        expr: feature.bodies.get("expr").cloned(),
                    });
                    self.stats.derived_features += 1;
                    attached = true;
                }
                if attached {
                    if self.known_titles.contains(&class.name) {
                        self.mox
                            .class_links
                            .push((class.name.clone(), class.name.clone()));
                    } else {
                        eprintln!(
                            "Warning: mox class '{}' matches no ingested schema entity — \
                             its schema nodes are created from the mox definition and its \
                             operations/derived features attach there",
                            class.name
                        );
                        self.stats.skipped += 1;
                        // The bridge (below) creates the schema node for this
                        // class in this run; link once it exists.
                        self.unmatched_with_behavior.push(class.name.clone());
                    }
                }
            }
        }
    }

    /// ── Bridge pass 1: SchemaNodes for classes matching no ingested schema.
    pub(super) async fn bridge_class_schemas(&mut self) -> Result<()> {
        for entry in &self.class_index {
            let class = entry.class;
            if self.known_titles.contains(&class.name) {
                // The JSON node owns this title (the bridge never overrides it),
                // so a mox-only storage decision on the class cannot take effect.
                // Say so — a silently ignored declaration is the failure mode
                // this issue exists to delete.
                if let Some(declared) = &class.table_name {
                    eprintln!(
                        "Warning: mox class '{}' declares table \"{declared}\" but the title is \
                         owned by an ingested JSON schema — the declaration is ignored (the JSON \
                         node keeps its own table)",
                        class.name
                    );
                }
                if entry.is_register() {
                    eprintln!(
                        "Warning: mox register '{}' matches an ingested JSON schema — the JSON node \
                         wins, so the class stays an entity rather than a reference-data table",
                        class.name
                    );
                }
                continue;
            }
            let bridge = if entry.is_register() {
                ClassBridge::Register
            } else {
                ClassBridge::Class {
                    is_entity: !self.contains_targets.contains(&class.name)
                        || self.refers_targets.contains(&class.name),
                }
            };
            let node = class_schema_node(entry, bridge, self.type_suffix);
            self.ingestor
                .ingest_schema(&node)
                .await
                .map_err(Error::Graph)?;
            self.bridged.insert(class.name.clone());
            self.stats.bridged_titles.push(class.name.clone());
            self.stats.classes += 1;
        }
        Ok(())
    }

    /// ── Bridge pass 1b: codelist SchemaNodes for enums (issue #233). The
    /// JSON path models an enum-valued schema as a `Schema` node with
    /// `classification=codelist`; codelist DDL, codelist FK resolution
    /// (`resolve_fk_target` routes codelist targets to the `common` schema
    /// through `ts.is_codelist`), link generation, and the per-entity
    /// artifact set all key on that node. Titles already owned by JSON
    /// schemas (or colliding with a bridged class) keep the existing node.
    pub(super) async fn bridge_enum_schemas(&mut self) -> Result<()> {
        for entry in &self.enum_index {
            let enum_def = entry.enum_def;
            if self.known_titles.contains(&enum_def.name) || self.bridged.contains(&enum_def.name) {
                continue;
            }
            let node = enum_schema_node(entry, self.type_suffix);
            self.ingestor
                .ingest_schema(&node)
                .await
                .map_err(Error::Graph)?;
            self.bridged_enum_schema_ids
                .insert(enum_def.name.clone(), entry.schema_id.clone());
            self.stats.bridged_titles.push(enum_def.name.clone());
            self.stats.enum_schemas += 1;
        }
        Ok(())
    }

    /// ── Bridge pass 1c: InNamespace edges for bridged schemas (issue #268).
    /// Written after the nodes exist (endpoints MATCH at execute time).
    /// Namespace-less entries contribute nothing.
    pub(super) async fn emit_in_namespace_edges(&mut self) -> Result<()> {
        for entry in &self.class_index {
            if !self.bridged.contains(entry.class.name.as_str()) {
                continue;
            }
            emit_schema_namespace_edge(self.ingestor, &entry.schema_id, entry.namespace.as_deref())
                .await?;
        }
        for entry in &self.enum_index {
            if !self
                .bridged_enum_schema_ids
                .contains_key(&entry.enum_def.name)
            {
                continue;
            }
            emit_schema_namespace_edge(self.ingestor, &entry.schema_id, entry.namespace.as_deref())
                .await?;
        }
        Ok(())
    }

    /// ── Bridge pass 2: properties + reference edges per bridged class.
    /// Features are ingested in the JSON path's allOf-canonical order
    /// (inherited features before own features, each group name-sorted) so
    /// property insertion order — and thus the CachingQuerier's warm-cache
    /// ordering — matches the JSON path byte-for-byte (issue #233).
    pub(super) async fn bridge_class_properties(&mut self) -> Result<()> {
        let class_map: HashMap<&str, &phero_ir::ClassDef> = self
            .class_index
            .iter()
            .map(|entry| (entry.class.name.as_str(), entry.class))
            .collect();
        for entry in &self.class_index {
            let class = entry.class;
            if !self.bridged.contains(&class.name) {
                // Owned by a JSON schema (never overridden) or a duplicate
                // declaration that was dropped.
                continue;
            }
            // Identity mapping rule (issue #522): a class with exactly one `id`
            // feature keeps it as its own column forced to Uuid (below) — the
            // generated surrogate `id` stays the PK/FK target (issue #821);
            // multiple `id` features are a composite natural key whose parts
            // keep their declared type mappings. Counted over the same stored
            // feature set the loop below ingests (derived/container features
            // produce no column).
            let features = ordered_bridge_features(class, &class_map, &self.bridged);
            let sole_identity = features
                .iter()
                .filter(|f| f.is_id && !f.is_derived && f.kind != FeatureKind::Container)
                .count()
                == 1;
            for feature in features {
                if feature.is_derived || feature.kind == FeatureKind::Container {
                    continue;
                }
                let Some(prop) = feature_property(
                    feature,
                    &class.name,
                    &self.datatype_formats,
                    sole_identity,
                    &mut self.stats,
                ) else {
                    continue;
                };
                self.ingestor
                    .ingest_property(&class.name, &entry.schema_id, &prop)
                    .await
                    .map_err(Error::Graph)?;
                self.stats.properties += 1;

                // Class-targeted features carry a graph edge to the target
                // schema (enum/vocabulary targets keep only the ref_target —
                // they are CodeList/Vocabulary nodes, not Schema nodes — except
                // bridged enum schemas, which exist as codelist Schema nodes).
                if let (
                    FeatureKind::Containment | FeatureKind::CrossReference,
                    TypeRef::Class { name, .. },
                ) = (&feature.kind, &feature.type_)
                {
                    // JSON-owned targets keep their own schema_id; bridged
                    // targets use the mox-authored node's id.
                    let target_schema_id = self
                        .json_schema_ids
                        .get(name)
                        .cloned()
                        .or_else(|| self.class_schema_ids.get(name).cloned());
                    let Some(target_schema_id) = target_schema_id else {
                        // Import aliases resolve after the schema pass — their
                        // graph node does not exist yet (issue #230). Anything
                        // else keeps the master behavior: silently no edge.
                        if self.import_names.contains(name) {
                            self.pending_alias_refs.push(PendingAliasRef {
                                schema_title: class.name.clone(),
                                prop_name: prop.name.clone(),
                                alias: name.clone(),
                                is_array: prop.is_array,
                            });
                        }
                        continue;
                    };
                    let edge_type = if prop.is_array {
                        EdgeType::ItemsOf
                    } else {
                        EdgeType::ReferencesSchema
                    };
                    self.ingestor
                        .ingest_edge(
                            &format!("{}::{}", prop.name, class.name),
                            &target_schema_id,
                            edge_type,
                            Some(&EdgeProperties {
                                ref_path: Some(name.clone()),
                                ..Default::default()
                            }),
                        )
                        .await
                        .map_err(Error::Graph)?;
                    self.stats.edges += 1;
                } else if let TypeRef::Enum { name, .. } = &feature.type_ {
                    // Enum features reference the bridged codelist Schema node
                    // with the same ReferencesSchema/ItemsOf edges the JSON path
                    // creates for $ref properties — `resolve_fk_target` needs
                    // the edge to emit the `REFERENCES common.<codelist>(code)`
                    // constraint. Vocabulary targets have no Schema node (no
                    // JSON counterpart) and keep the ref_target only.
                    if let Some(target_schema_id) = self.bridged_enum_schema_ids.get(name) {
                        let edge_type = if prop.is_array {
                            EdgeType::ItemsOf
                        } else {
                            EdgeType::ReferencesSchema
                        };
                        self.ingestor
                            .ingest_edge(
                                &format!("{}::{}", prop.name, class.name),
                                target_schema_id,
                                edge_type,
                                Some(&EdgeProperties {
                                    ref_path: Some(name.clone()),
                                    ..Default::default()
                                }),
                            )
                            .await
                            .map_err(Error::Graph)?;
                        self.stats.edges += 1;
                    }
                }
            }

            // `extends` → ExtendsSchema edges (allOf composition), mirroring
            // Pass 4 of the JSON path.
            for parent in &class.extends {
                if let TypeRef::Class { name, .. } = parent {
                    let parent_exists =
                        self.known_titles.contains(name) || self.bridged.contains(name.as_str());
                    if !parent_exists {
                        continue;
                    }
                    self.ingestor
                        .ingest_edge(
                            &class.name,
                            name,
                            EdgeType::ExtendsSchema,
                            Some(&EdgeProperties {
                                composition_type: Some("allOf".to_string()),
                                ..Default::default()
                            }),
                        )
                        .await
                        .map_err(Error::Graph)?;
                    self.stats.edges += 1;
                }
            }
        }
        Ok(())
    }

    /// Operations/derived features of bridged classes attach to the freshly
    /// created schema nodes, then the domain-model carrier lands.
    pub(super) async fn finish(&mut self) -> Result<()> {
        for class in &self.unmatched_with_behavior {
            if self.bridged.contains(class) {
                self.mox.class_links.push((class.clone(), class.clone()));
            }
        }
        self.ingestor
            .ingest_mox_domain(&self.mox)
            .await
            .map_err(Error::Graph)?;
        Ok(())
    }

    pub(super) fn into_outcome(self, imported_files: Vec<MoxSchemaImport>) -> MoxIngestOutcome {
        MoxIngestOutcome {
            stats: self.stats,
            imported_files,
            pending_alias_refs: self.pending_alias_refs,
        }
    }
}
