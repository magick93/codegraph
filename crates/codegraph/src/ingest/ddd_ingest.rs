//! pherolang `.ddd` design ingest (issue #449): compile each design file
//! against its imported `.mox` domains with the pherolang driver, convert the
//! resulting `phero_ir::ddd::DddModel` into the graph-native
//! [`DddModelGraph`], resolve design class/search entity names against the
//! ingested schema titles, and land the model through
//! [`GraphIngestor::ingest_ddd_model`].
//!
//! Compilation failures are HARD errors ([`Error::DddModel`], the
//! [`Error::RosettaModel`] precedent) — a broken design never half-ingests.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use codegraph_config::config::DomainConfig;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{
    DddApplicationNode, DddChildRef, DddConditionPath, DddConditionScope, DddDesignFlags,
    DddDesignNode, DddDocumentField, DddModelGraph, DddModuleNode, DddPagination, DddParam,
    DddRepositoryNode, DddRepositoryOperation, DddSearchField, DddSearchNode, DddServiceNode,
    DddServiceOperation, DddTransitionRef,
};
use phero_driver::{CompileInputs, DomainImports, SchemaImports, SigilImports, compile_ddd_str};

use crate::error::{Error, Result};
use crate::ingest::phero_imports::{collect_domains, discover_rosetta_files, scan_sigil_imports};

/// Counters for one [`ingest_ddd_files`] run.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DddIngestStats {
    /// The number of `.ddd` files handed to the run.
    pub files: usize,
    /// Class designs ingested across all applications.
    pub designs: usize,
    /// Repositories ingested across all designs.
    pub repositories: usize,
    /// Application services ingested.
    pub services: usize,
    /// Search definitions ingested.
    pub searches: usize,
    /// Design class / search entity names that matched no ingested schema
    /// title (exact or +`type_suffix`); the node keeps `None` and the miss
    /// warns on stderr.
    pub unresolved_classes: usize,
}

impl std::fmt::Display for DddIngestStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} files, {} designs, {} repositories, {} services, {} searches, {} unresolved classes",
            self.files,
            self.designs,
            self.repositories,
            self.services,
            self.searches,
            self.unresolved_classes
        )
    }
}

/// Ingest pherolang `.ddd` design files into the graph's DDD design plane.
///
/// Per file: read the source (a missing file is a hard error), collect the
/// transitively imported `.mox` domains and their `import schema`/`import
/// sigil` content, compile with `phero_driver::compile_ddd_str`, hard-error
/// on any error-severity diagnostic (rendered per file into the reason),
/// resolve design class / search entity names against the graph's schema
/// titles (exact → title + `type_suffix` → warn + count), and ingest one
/// [`DddModelGraph`] per application.
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub async fn ingest_ddd_files(
    ingestor: &dyn GraphIngestor,
    querier: &dyn GraphQuerier,
    ddd_paths: &[PathBuf],
    domain_config: &DomainConfig,
) -> Result<DddIngestStats> {
    let mut stats = DddIngestStats {
        files: ddd_paths.len(),
        ..Default::default()
    };
    let type_suffix = &domain_config.defaults.type_suffix;

    // Schema titles available for class resolution (the wire_alias_refs
    // chain: exact title first, then title + the configured type suffix).
    let schemas = querier.list_schemas(None).await.map_err(Error::Graph)?;
    let titles: HashSet<String> = schemas.iter().map(|s| s.title.clone()).collect();

    for path in ddd_paths {
        let compiled = read_and_compile_ddd(path)?;
        let has_errors = compiled
            .compilation
            .diagnostics
            .iter()
            .any(|(_, d)| d.is_error());
        if has_errors || compiled.compilation.model.is_none() {
            let reason = render_ddd_diagnostics(&compiled);
            return Err(Error::DddModel {
                file: compiled.path.clone(),
                reason: if reason.is_empty() {
                    "compilation produced no design artifact".to_string()
                } else {
                    reason
                },
            });
        }
        let Some(model) = compiled.compilation.model.as_ref() else {
            return Err(Error::DddModel {
                file: compiled.path.clone(),
                reason: "compilation produced no design artifact".to_string(),
            });
        };

        // Class/entity resolution warns and counts on miss; the design
        // node keeps `resolved_title: None` for the mapping plane to
        // degrade on.
        let file_path = compiled.path.clone();
        let unresolved = std::cell::Cell::new(0usize);
        let graph =
            ddd_model_graph_from_phero(model, &file_path, &|name: &str| match resolve_with_suffix(
                name,
                type_suffix,
                &titles,
            ) {
                Some(title) => Some(title),
                None => {
                    let suffixed = format!("{name}{type_suffix}");
                    eprintln!(
                        "Warning: ddd design '{file_path}' class '{name}' matches no ingested \
                         schema title — tried '{name}' and '{suffixed}'; the design node keeps \
                         no resolved title"
                    );
                    unresolved.set(unresolved.get() + 1);
                    None
                }
            });
        stats.unresolved_classes += unresolved.get();
        stats.designs += graph.designs.len();
        stats.repositories += graph.repositories.len();
        stats.services += graph.services.len();
        stats.searches += graph.searches.len();
        ingestor
            .ingest_ddd_model(&graph)
            .await
            .map_err(Error::Graph)?;
    }
    Ok(stats)
}

/// One `.ddd` file read, its imports collected, and compiled — the shared
/// helper both [`ingest_ddd_files`] and the doctor `--ddd-files` check
/// call so their import resolution cannot drift.
pub struct DddCompiled {
    /// The design file's driver path (as given on the command line).
    pub path: String,
    /// The design file's source text.
    pub source: String,
    /// The imported `.mox` domain sources as `(driver path, source text)`.
    pub mox_sources: Vec<(String, String)>,
    /// Every provided rosetta source as `(tag path, source text)` — the
    /// named `import sigil` files (tagged with the import path as written)
    /// plus the discovered sibling candidates (tagged with their resolved
    /// paths).
    pub sigil_sources: Vec<(String, String)>,
    /// The compile result.
    pub compilation: phero_driver::DddCompilation,
}

impl DddCompiled {
    /// The source text of the file with the given driver path, if it is
    /// the design file, an imported domain, or a rosetta source (the
    /// phero-cli `DddDesignPair::source_of` shape).
    fn source_of(&self, path: &str) -> Option<&str> {
        if path == self.path {
            return Some(&self.source);
        }
        self.mox_sources
            .iter()
            .find(|(name, _)| name == path)
            .map(|(_, source)| source.as_str())
            .or_else(|| {
                self.sigil_sources
                    .iter()
                    .find(|(name, _)| name == path)
                    .map(|(_, source)| source.as_str())
            })
    }
}

/// Read a `.ddd` file, collect its transitively imported `.mox` domains
/// and their `import schema`/`import sigil` content (sigil named imports
/// plus the sibling candidate pool), and compile. An unreadable design
/// file is a hard [`Error::DddModel`]; unreadable import files warn and
/// the compile reports the unsatisfied import as a diagnostic (which the
/// callers turn into the hard error).
pub fn read_and_compile_ddd(path: &Path) -> Result<DddCompiled> {
    let source = std::fs::read_to_string(path).map_err(|e| Error::DddModel {
        file: path.display().to_string(),
        reason: format!("could not be read: {e}"),
    })?;
    let ddd_path = path.display().to_string();
    let dir = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    // Transitive `import "x.mox"` collection (the shared .actor plumbing).
    let domains = collect_domains(dir, &source);
    let domain_pairs: Vec<(String, String)> = domains
        .iter()
        .map(|d| (d.path.clone(), d.source.clone()))
        .collect();

    let mut schema_imports = SchemaImports::new();
    let mut sigil = SigilImports::new();
    let mut sigil_sources: Vec<(String, String)> = Vec::new();
    for domain in &domains {
        // `import schema "<path>" as <Alias>`: JSON content keyed
        // (mox path, import path as written) — the compile contract.
        for decl in crate::ingest::mox_ingest::scan_schema_imports(&domain.source) {
            let abs_path = domain.dir.join(&decl.path);
            match std::fs::read_to_string(&abs_path) {
                Ok(json) => {
                    if let Err(e) = serde_json::from_str::<serde_json::Value>(&json) {
                        eprintln!(
                            "Warning: ddd design '{ddd_path}' domain '{}' import schema '{}' \
                             is not valid JSON ({e}) — the compile reports the unsatisfied import",
                            domain.path, decl.path
                        );
                        continue;
                    }
                    schema_imports.insert(&domain.path, &decl.path, json);
                }
                Err(e) => eprintln!(
                    "Warning: ddd design '{ddd_path}' domain '{}' import schema '{}' \
                     could not be read: {e}",
                    domain.path, decl.path
                ),
            }
        }
        // `import sigil "<path>"`: the named rosetta file keyed
        // (mox path, import path as written), plus the candidate pool of
        // every `*.rosetta` next to it (rosetta-internal namespace imports
        // resolve transitively; the driver lowers only the needed closure).
        for import_path in scan_sigil_imports(&domain.source) {
            let resolved = domain.dir.join(&import_path);
            let text = match std::fs::read_to_string(&resolved) {
                Ok(text) => text,
                Err(e) => {
                    eprintln!(
                        "Warning: ddd design '{ddd_path}' domain '{}' import sigil '{}' \
                         could not be read: {e}",
                        domain.path, import_path
                    );
                    continue;
                }
            };
            sigil.insert(&domain.path, &import_path, text.clone());
            if !sigil_sources.iter().any(|(tag, _)| tag == &import_path) {
                sigil_sources.push((import_path.clone(), text));
            }
            for candidate in discover_rosetta_files(&resolved) {
                let candidate_path = candidate.display().to_string();
                if sigil_sources.iter().any(|(tag, _)| tag == &candidate_path) {
                    continue;
                }
                match std::fs::read_to_string(&candidate) {
                    Ok(text) => {
                        sigil.insert(&domain.path, &candidate_path, text.clone());
                        sigil_sources.push((candidate_path, text));
                    }
                    Err(e) => eprintln!(
                        "Warning: ddd design '{ddd_path}' discovered sigil '{}' \
                         could not be read: {e}",
                        candidate.display()
                    ),
                }
            }
        }
    }

    let imports = DomainImports {
        schemas: schema_imports,
        sigil,
    };
    let inputs = CompileInputs::from_domains(&domain_pairs).with_imports(imports);
    let compilation = compile_ddd_str(&ddd_path, &source, &inputs);
    Ok(DddCompiled {
        path: ddd_path,
        source,
        mox_sources: domain_pairs,
        sigil_sources,
        compilation,
    })
}

/// Render a compilation's diagnostics per file with the pherolang renderer:
/// diagnostics are grouped by their file path (first-appearance order) and
/// each group is rendered against its own source text (design file, mox
/// domain, or rosetta source). Paths with no retained source fall back to
/// plain `path: severity: message` lines.
pub fn render_ddd_diagnostics(compiled: &DddCompiled) -> String {
    let mut groups: Vec<(String, Vec<&phero_driver::Diagnostic>)> = Vec::new();
    for (path, diagnostic) in &compiled.compilation.diagnostics {
        match groups.iter_mut().find(|(name, _)| name == path) {
            Some((_, group)) => group.push(diagnostic),
            None => groups.push((path.clone(), vec![diagnostic])),
        }
    }
    let mut blocks: Vec<String> = Vec::new();
    for (path, group) in groups {
        match compiled.source_of(&path) {
            Some(source) => {
                let owned: Vec<phero_driver::Diagnostic> =
                    group.iter().map(|d| (*d).clone()).collect();
                blocks.push(phero_driver::render(&path, source, &owned));
            }
            None => {
                for diagnostic in group {
                    let severity = if diagnostic.is_error() {
                        "error"
                    } else {
                        "warning"
                    };
                    blocks.push(format!("{path}: {severity}: {}", diagnostic.message));
                }
            }
        }
    }
    blocks.join("\n")
}

/// Resolve a design class / search entity name against the graph's schema
/// titles: exact match first, then title + `type_suffix` (the
/// `Customer` → `CustomerType` convention — the `wire_alias_refs` chain).
/// `None` means the class is not (yet) in the schema graph.
pub fn resolve_with_suffix(
    name: &str,
    type_suffix: &str,
    titles: &HashSet<String>,
) -> Option<String> {
    if titles.contains(name) {
        return Some(name.to_string());
    }
    let suffixed = format!("{name}{type_suffix}");
    titles.contains(&suffixed).then_some(suffixed)
}

/// The authored name's class part: phero-ir references are bare, `pkg.Name`,
/// or `pkg::Name` — resolution compares the LAST segment after both
/// qualifiers against the schema titles.
pub fn short_name(qualified: &str) -> &str {
    let after_ns = qualified.rsplit("::").next().unwrap_or(qualified);
    after_ns.rsplit('.').next().unwrap_or(after_ns)
}

/// Convert a compiled pherolang DDD design artifact into its graph-native
/// representation.
///
/// Module-major ordering contract (the [`DddModelGraph`] flattening the
/// querier's `get_ddd_models` restores): phero-ir modules are walked in
/// declaration order and, within each module, designs then services then
/// searches; every node's `ordinal` is its declaration index within its
/// module, so (module ordinal, own ordinal) reconstructs the authored
/// order. `title_resolver` receives the authored reference's short name
/// ([`short_name`]) and returns the schema title, or `None` for a class
/// not in the schema graph.
pub fn ddd_model_graph_from_phero(
    model: &phero_ir::ddd::DddModel,
    source_path: &str,
    title_resolver: &dyn Fn(&str) -> Option<String>,
) -> DddModelGraph {
    let app_name = model
        .application
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "app".to_string());
    let base = model.application.as_ref().and_then(|a| a.base.clone());

    let mut modules = Vec::new();
    let mut designs = Vec::new();
    let mut repositories = Vec::new();
    let mut services = Vec::new();
    let mut searches = Vec::new();

    for (module_ordinal, module) in model.modules.iter().enumerate() {
        modules.push(DddModuleNode {
            application: app_name.clone(),
            name: module.name.clone(),
            ordinal: module_ordinal,
        });

        for (ordinal, design) in module.designs.iter().enumerate() {
            // DD3: `class` is absent for a standalone DTO boundary (a `dto`
            // design with no `.mox` source). Those carry no domain class, so
            // there is nothing to map onto the graph: skip with a warning
            // naming the boundary instead of inventing an empty class.
            let Some(class) = design.class.clone() else {
                let boundary = design
                    .dto_shape
                    .as_ref()
                    .map(|shape| shape.name.as_str())
                    .unwrap_or("<unnamed>");
                eprintln!(
                    "Warning: ddd — module '{}' declares standalone DTO boundary \
                     '{boundary}' with no domain class; skipped (DTO boundaries \
                     are not mapped onto the entity plane)",
                    module.name
                );
                continue;
            };
            let resolved_title = title_resolver(short_name(&class));
            designs.push(DddDesignNode {
                application: app_name.clone(),
                module: module.name.clone(),
                class: class.clone(),
                resolved_title,
                stereotype: stereotype_label(design.stereotype),
                is_abstract: design.is_abstract,
                flags: DddDesignFlags {
                    scaffold: design.flags.scaffold,
                    auditable: design.flags.auditable,
                    optimistic_locking: design.flags.optimistic_locking,
                    non_persistent: design.flags.non_persistent,
                    cache: design.flags.cache,
                },
                machine_json: opt_json(design.machine.as_ref()),
                // Issue #661: the module's `children <Class>` blocks lower
                // onto the design (`Design::child_operations`); each op
                // carries its `child_scope`. The list is empty for an
                // extend-less design, so its node serializes as before.
                child_operations: design
                    .child_operations
                    .iter()
                    .enumerate()
                    .map(|(ordinal, op)| service_operation_node(op, ordinal))
                    .collect(),
                ordinal,
            });
            if let Some(repo) = &design.repository {
                let operations = repo
                    .operations
                    .iter()
                    .enumerate()
                    .map(|(ordinal, op)| DddRepositoryOperation {
                        name: op.name.clone(),
                        builtin: op.builtin.map(builtin_label),
                        return_type: opt_json(op.return_type.as_ref()),
                        return_multiplicity: opt_json(op.return_multiplicity.as_ref()),
                        params: op
                            .params
                            .iter()
                            .map(|p| DddParam {
                                name: p.name.clone(),
                                type_json: to_json(&p.type_),
                                multiplicity: opt_json(p.multiplicity.as_ref()),
                            })
                            .collect(),
                        ordinal,
                        is_protected: op.is_protected,
                        // DD9 (issue #659): the typed query block rides the
                        // graph as phero-ir `QuerySpec` JSON; the predicate
                        // stays verbatim source text (typing is proven
                        // upstream) and generation lowers it.
                        query_json: opt_json(op.query.as_ref()),
                        capabilities: op
                            .capabilities
                            .iter()
                            .map(|capability| capability.name.clone())
                            .collect(),
                    })
                    .collect();
                repositories.push(DddRepositoryNode {
                    application: app_name.clone(),
                    name: repo.name.clone(),
                    design_class: class.clone(),
                    operations,
                });
            }
        }

        for (ordinal, service) in module.services.iter().enumerate() {
            let operations = service
                .operations
                .iter()
                .enumerate()
                .map(|(ordinal, op)| service_operation_node(op, ordinal))
                .collect();
            services.push(DddServiceNode {
                application: app_name.clone(),
                module: module.name.clone(),
                name: service.name.clone(),
                description: service.description.clone(),
                dependencies: service.dependencies.clone(),
                decision_dependencies: service.decision_dependencies.clone(),
                operations,
                ordinal,
            });
        }

        for (ordinal, search) in module.searches.iter().enumerate() {
            let entity_class = search.entity.clone();
            let entity_title = title_resolver(short_name(&entity_class));
            searches.push(DddSearchNode {
                application: app_name.clone(),
                module: module.name.clone(),
                name: search.name.clone(),
                description: search.description.clone(),
                entity_class,
                entity_title,
                text: search
                    .text
                    .iter()
                    .map(|field| DddSearchField {
                        property: field.property.clone(),
                        boost: field.boost.map(phero_ir::ddd::Boost::value),
                        analyzer: field.analyzer.clone(),
                    })
                    .collect(),
                filters: search
                    .filters
                    .iter()
                    .map(|filter| filter.property.clone())
                    .collect(),
                sorts: search
                    .sort
                    .iter()
                    .map(|sort| sort.property.clone())
                    .collect(),
                document: search
                    .document
                    .iter()
                    .map(|field| DddDocumentField {
                        name: field.name.clone(),
                        expr: field.expr.clone(),
                    })
                    .collect(),
                ranking: search.ranking.as_ref().map(ranking_label),
                analyzer: search.analyzer.clone(),
                pagination: search.pagination.map(|p| DddPagination {
                    limit: p.limit,
                    max_limit: p.max_limit,
                    cursor: p.cursor,
                }),
                capabilities: search.capabilities.clone(),
                ordinal,
            });
        }
    }

    DddModelGraph {
        source_path: source_path.to_string(),
        application: DddApplicationNode {
            name: app_name,
            base,
            source_path: source_path.to_string(),
        },
        modules,
        designs,
        repositories,
        services,
        searches,
    }
}

/// Convert one phero-ir [`phero_ir::ddd::ServiceOperation`] into its graph
/// mirror — the ONE conversion both service operations and the design-level
/// child operations (issue #661) ride, so the two planes cannot drift.
fn service_operation_node(
    op: &phero_ir::ddd::ServiceOperation,
    ordinal: usize,
) -> DddServiceOperation {
    DddServiceOperation {
        name: op.name.clone(),
        return_type: opt_json(op.return_type.as_ref()),
        return_multiplicity: opt_json(op.return_multiplicity.as_ref()),
        params: op
            .params
            .iter()
            .map(|p| DddParam {
                name: p.name.clone(),
                type_json: to_json(&p.type_),
                multiplicity: opt_json(p.multiplicity.as_ref()),
            })
            .collect(),
        delegation_target: op.delegation.as_ref().map(|d| d.target.clone()),
        delegation_operation: op.delegation.as_ref().map(|d| d.operation.clone()),
        capabilities: op.capabilities.clone(),
        is_protected: op.is_protected,
        condition: op.condition.clone(),
        emits: op.emits.clone(),
        invokes: op.invokes.clone(),
        // DD7 vocabulary + pherolang#178 bodies (issue #662):
        // the `throws` set rides as phero-ir `ErrorRef` JSON and
        // the checked body payload stays opaque (canonical
        // `StmtBody` JSON under the `"stmt"` target).
        throws: op.throws.iter().map(to_json).collect(),
        bodies: op.bodies.clone(),
        body_receiver: op.body_receiver.clone(),
        // Issue #660: the authored transition binding mirrors
        // as authored (`TransitionRef { trigger, machine }`).
        transition: op.transition.as_ref().map(|t| DddTransitionRef {
            trigger: t.trigger.clone(),
            machine: t.machine.clone(),
        }),
        // Issue #661: a `children <Class>` operation carries the root's
        // child feature, the resolved child class, and the validated
        // action (`add`/`remove`/`save`). Absent on every service op.
        child_scope: op.child_scope.as_ref().map(|scope| DddChildRef {
            feature: scope.feature.clone(),
            class: scope.class.clone(),
            action: scope.action.clone(),
        }),
        // Issue #663 (pherolang#180): the resolved field paths of the
        // `when` condition, mirrored typed→typed (the `DddTransitionRef`
        // discipline — codegraph-core may not name phero-ir types). The
        // field's declared type rides as opaque `TypeRef` JSON.
        condition_paths: op
            .condition_paths
            .iter()
            .map(|path| DddConditionPath {
                scope: match &path.scope {
                    phero_ir::ddd::ConditionScope::OwnRecord => DddConditionScope::OwnRecord,
                    phero_ir::ddd::ConditionScope::Injected { repository, class } => {
                        DddConditionScope::Injected {
                            repository: repository.clone(),
                            class: class.clone(),
                        }
                    }
                },
                field: path.field.clone(),
                field_type: to_json(&path.field_type),
            })
            .collect(),
        ordinal,
    }
}

fn stereotype_label(stereotype: phero_ir::ddd::Stereotype) -> String {
    match stereotype {
        phero_ir::ddd::Stereotype::Entity => "entity",
        phero_ir::ddd::Stereotype::Value => "value",
        phero_ir::ddd::Stereotype::Dto => "dto",
    }
    .to_string()
}

fn builtin_label(builtin: phero_ir::ddd::BuiltinRepositoryOp) -> String {
    match builtin {
        phero_ir::ddd::BuiltinRepositoryOp::FindById => "findById",
        phero_ir::ddd::BuiltinRepositoryOp::FindAll => "findAll",
        phero_ir::ddd::BuiltinRepositoryOp::FindByExample => "findByExample",
        phero_ir::ddd::BuiltinRepositoryOp::FindByKeys => "findByKeys",
        phero_ir::ddd::BuiltinRepositoryOp::Save => "save",
        phero_ir::ddd::BuiltinRepositoryOp::Delete => "delete",
    }
    .to_string()
}

fn ranking_label(ranking: &phero_ir::ddd::RankingStrategy) -> String {
    match ranking {
        phero_ir::ddd::RankingStrategy::Bm25 => "bm25".to_string(),
        phero_ir::ddd::RankingStrategy::TfIdf => "tfIdf".to_string(),
        phero_ir::ddd::RankingStrategy::Exact => "exact".to_string(),
        phero_ir::ddd::RankingStrategy::Custom(name) => name.clone(),
    }
}

/// Serialize a phero-ir wire payload (`TypeRef`, `Multiplicity`) to its JSON
/// value; a serialization failure degrades to `Value::Null`, never a panic.
fn to_json<T: serde::Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
}

fn opt_json<T: serde::Serialize>(value: Option<&T>) -> Option<serde_json::Value> {
    value.map(to_json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_name_takes_the_last_segment_of_both_qualifier_styles() {
        assert_eq!(short_name("Book"), "Book");
        assert_eq!(short_name("nz.example.library::Book"), "Book");
        assert_eq!(short_name("nz.example.library.Book"), "Book");
        assert_eq!(short_name("shop.pricing.Money"), "Money");
    }

    #[test]
    fn resolve_with_suffix_prefers_exact_then_suffixed() {
        let titles: HashSet<String> = ["Book", "MoneyType"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert_eq!(
            resolve_with_suffix("Book", "Type", &titles),
            Some("Book".to_string())
        );
        assert_eq!(
            resolve_with_suffix("Money", "Type", &titles),
            Some("MoneyType".to_string())
        );
        assert_eq!(resolve_with_suffix("Ghost", "Type", &titles), None);
    }
}
