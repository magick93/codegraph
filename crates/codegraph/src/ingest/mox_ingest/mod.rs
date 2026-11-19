//! mox domain ingest: compile `.mox` domain sources in-process via
//! `phero_driver::compile_files` and walk the resulting `phero_ir::Model` into
//! the graph (issues #218, #229).
//!
//! What lands in the graph:
//! - `Vocabulary` nodes (with typed facets and vendored entries) plus
//!   `MoxPackage` carriers and `VocabularyInPackage` edges;
//! - `Operation` nodes with verbatim per-target bodies as text props;
//! - `DerivedFeature` nodes (computed, never stored) carrying the neutral
//!   `expr` body text;
//! - `BelongsToClass` edges linking operations/derived features to
//!   schema-ingested entities whose names match the mox class name (schema
//!   title or entity name). Mismatches warn and count as skipped — they
//!   never fail ingestion.
//!
//! Class bridge (#229): mox `class`/`enum`/`datatype` definitions additionally
//! produce the same graph shapes the JSON Schema path produces — a
//! `SchemaNode` with `PropertyNode`s per class, a `CodeList` with
//! `EnumValue`s per enum (plus a codelist `SchemaNode` with
//! `classification=codelist`, the shape the JSON codelist path produces and
//! that codelist DDL, FK resolution, and link generation key on),
//! `ReferencesSchema`/`ItemsOf` edges for refers/contains/enum features and
//! `ExtendsSchema` edges for `extends`. Classes whose name already exists as
//! an ingested schema keep the JSON-authored node (the bridge never
//! overrides); classes matching no schema get their own nodes, carrying the
//! `source=mox` provenance in `custom_annotations` so the auto-classifier
//! treats their entity/value-object nature as author-declared. `container`
//! back-pointers and derived features are skipped here (the former has no
//! stored column; the latter ingests as `DerivedFeature` nodes above).
//!
//! Property ordering: properties are ingested in the JSON path's effective
//! canonical order — allOf-canonical per class (inherited features before
//! own features, each group name-sorted), mirroring how
//! `ingest_properties_from_schema` orders its prop_blocks. The
//! `CachingQuerier` warm path serves properties in insertion order (no
//! `ORDER BY` in `list_all_properties`), so insertion order IS output field
//! order and must match the JSON path byte-for-byte (issue #233).
//!
//! Storage-name and register mapping (issue #668):
//! - `class X table "…"` (upstream pherolang#177) carries `ClassDef::table_name`;
//!   the declared spelling is the effective `pg_table_name` (snake_case
//!   normalized, a changing normalization warns), so the DDL, SeaORM entity,
//!   repository SQL, filters and hurl/openapi naming all agree by
//!   construction. URLs stay title-derived (`api_path_segment`).
//! - `register X { … }` (upstream pherolang#181, `ClassKind::Register`) is an
//!   org/role noun space — attributes only, no relations, no `extends`, no
//!   table annotation — so it bridges onto the REFERENCE-DATA
//!   (codelist/seed-table) plane: a `CodeList` node plus a codelist-classified
//!   `SchemaNode` carrying the `mox_class_kind = "register"` marker.
//!   `codegraph_generate::ordering` skips register schemas, which is what
//!   keeps them off the entity plane: no entity module, no repository, no
//!   handlers, no routes — only the `common.{table}` seed migration and the
//!   codelist Rust enum the codelist generators own. The register's declared
//!   attributes still ingest as properties (the model graph stays non-lossy;
//!   projecting them into the seed table is a tracked follow-up).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use codegraph_classifier::projection_builder::ProjectionBuilder;
use codegraph_config::config::DomainConfig;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{
    EdgeProperties, EdgeType, Incompleteness, NamespaceNode, PropertyNode, SchemaNode,
};
use codegraph_naming::{escape_rust_keyword, strip_suffix, to_kebab_case, to_snake_case};
use codegraph_type_contracts::{DddFieldProjection, PgType, RefClassificationKind, TypeExpr};
use phero_driver::{DomainImports, SchemaImports, compile_files};
use phero_ir::{ClassKind, DefaultValue, FeatureKind, PrimitiveType, TypeRef};

use crate::error::{Error, Result};
use crate::ingest::async_ingest::{sanitize_description, sanitize_rust_type_name};

mod bridge;

use bridge::MoxBridge;

/// Counters for one `ingest_mox_files` run.
///
/// `skipped` counts mox files that could not be read or failed to compile,
/// plus mox classes carrying operations/derived features that matched no
/// pre-existing ingested schema entity (each warned).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MoxIngestStats {
    pub vocabularies: usize,
    pub operations: usize,
    pub derived_features: usize,
    pub skipped: usize,
    pub classes: usize,
    pub properties: usize,
    pub edges: usize,
    pub enums: usize,
    /// Codelist `SchemaNode`s the enum bridge created this run (issue #233).
    /// Mirrors the JSON path, where an enum-valued schema is itself a
    /// `Schema` node that codelist DDL, FK resolution, and link generation
    /// key on.
    pub enum_schemas: usize,
    /// Titles of the schema nodes the class bridge created this run. Under
    /// mox-first pipeline ordering (issue #231) the JSON schema pass skips
    /// every title listed here, so `.mox` wins title conflicts.
    pub bridged_titles: Vec<String>,
    /// Unique JSON files imported via `import schema` declarations,
    /// validated and queued for the schema pipeline (issue #230).
    pub imported_files: usize,
    /// Alias-typed features whose graph edge resolved to a real schema
    /// title in the post-schema-pass wiring step.
    pub resolved_aliases: usize,
    /// Alias-typed features that matched no schema title (exact or with the
    /// type suffix); each warned, never a hard error.
    pub unresolved_aliases: usize,
    /// Namespace nodes ingested from `package <dotted.name>` declarations
    /// (issue #268), including the dotted parent chain. Namespace-less
    /// models (no package line) contribute zero.
    pub namespaces: usize,
    /// `register` classes bridged to the reference-data (codelist/seed-table)
    /// plane (issue #668). Counter surfaces only when non-zero, so
    /// register-free models keep byte-identical output.
    pub registers: usize,
    /// Structured incompleteness findings (issue #279): unresolvable import
    /// aliases and the like, as `UnresolvedReference { target }` holes.
    /// Empty for fully-resolved models.
    pub incompleteness: Vec<Incompleteness>,
}

impl std::fmt::Display for MoxIngestStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} vocabularies, {} operations, {} derived features, {} skipped, \
             {} classes, {} properties, {} edges, {} enums, {} enum schemas",
            self.vocabularies,
            self.operations,
            self.derived_features,
            self.skipped,
            self.classes,
            self.properties,
            self.edges,
            self.enums,
            self.enum_schemas
        )?;
        // Import counters only surface when imports exist, so import-free
        // runs keep their byte-identical output.
        if self.imported_files != 0 || self.resolved_aliases != 0 || self.unresolved_aliases != 0 {
            write!(
                f,
                ", {} imported files, {} aliases resolved, {} unresolved",
                self.imported_files, self.resolved_aliases, self.unresolved_aliases
            )?;
        }
        // Namespace counter only surfaces when namespaces exist (#268).
        if self.namespaces != 0 {
            write!(f, ", {} namespaces", self.namespaces)?;
        }
        // Register counter only surfaces when register classes exist (#668).
        if self.registers != 0 {
            write!(f, ", {} registers", self.registers)?;
        }
        // Incompleteness counter only surfaces when partial authoring was
        // detected (#279) — fully-resolved models keep byte-identical output.
        if !self.incompleteness.is_empty() {
            write!(f, ", {} incomplete", self.incompleteness.len())?;
        }
        Ok(())
    }
}

/// One `import schema "<path>" (as <Alias>)?` declaration scanned from a
/// `.mox` source (issue #230). A line-oriented scan stands in for the
/// phero-syntax AST here (the lowered IR drops import declarations); limits
/// are pinned on [`scan_schema_imports`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaImportDecl {
    /// The quoted path exactly as written — the key the pherolang compiler's
    /// `SchemaImports` provider matches on.
    pub path: String,
    /// `as <Alias>` when present; the file stem otherwise (mirroring the
    /// upstream `imported_name` resolution).
    pub alias: Option<String>,
}

/// A scanned import resolved to a readable file, ready for the JSON schema
/// pipeline and the pherolang compile's `SchemaImports` provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoxSchemaImport {
    /// The .mox file declaring the import — the compile path it must match.
    pub mox_path: String,
    /// The import path exactly as written in the declaration.
    pub import_path: String,
    /// Absolute path resolved against the .mox file's directory.
    pub abs_path: PathBuf,
    /// The namespace name the import joins (alias or file stem).
    pub alias: String,
    /// Package name of the importing .mox file.
    pub importing_package: String,
    /// Domain the importing package resolves to (resolved at scan time so
    /// the schema pipeline can own the node without re-deriving it).
    pub domain: String,
}

/// A feature typed with an import alias, awaiting the post-schema-pass
/// wiring step (the imported schema's graph node does not exist while the
/// mox pass runs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingAliasRef {
    /// Schema title of the class declaring the feature.
    pub schema_title: String,
    /// Feature (property) name.
    pub prop_name: String,
    /// The alias exactly as the lowered IR names it.
    pub alias: String,
    /// `true` for many-features (ItemsOf edge); `false` → ReferencesSchema.
    pub is_array: bool,
}

/// An alias that matched no ingested schema title (exact or suffixed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedAlias {
    pub alias: String,
    pub schema_title: String,
    pub prop_name: String,
    /// The configured `defaults.type_suffix` used for the second try.
    pub type_suffix: String,
}

/// The warning line for an unresolvable alias — names both the alias and
/// the titles that were tried.
pub fn unresolved_alias_warning(u: &UnresolvedAlias) -> String {
    let suffixed = format!("{}{}", u.alias, u.type_suffix);
    format!(
        "Warning: mox import alias '{}' (feature '{}.{}') matches no ingested schema title — \
         tried '{}' and '{}'; the reference edge is skipped",
        u.alias, u.schema_title, u.prop_name, u.alias, suffixed
    )
}

/// Everything one `ingest_mox_files` run hands to the driver: the usual
/// stats, the imported schema files to ingest through the JSON pipeline,
/// and the alias-typed features to wire after the schema pass.
#[derive(Debug, Clone, Default)]
pub struct MoxIngestOutcome {
    pub stats: MoxIngestStats,
    pub imported_files: Vec<MoxSchemaImport>,
    pub pending_alias_refs: Vec<PendingAliasRef>,
}

/// Render the error diagnostics of a compile that produced no model:
/// one `pherolang render`-ed block per source file (spanned, with the
/// offending source), falling back to `path: message` for diagnostics
/// attributed to a file we do not hold the text of (an import).
fn render_mox_compile_failure(
    sources: &[(String, String)],
    diagnostics: &[(String, phero_driver::Diagnostic)],
) -> String {
    let mut blocks: Vec<String> = Vec::new();
    for (path, text) in sources {
        let owned: Vec<phero_driver::Diagnostic> = diagnostics
            .iter()
            .filter(|(file, diagnostic)| file == path && diagnostic.is_error())
            .map(|(_, diagnostic)| diagnostic.clone())
            .collect();
        if owned.is_empty() {
            continue;
        }
        blocks.push(phero_driver::render(path, text, &owned));
    }
    for (path, diagnostic) in diagnostics {
        if !diagnostic.is_error() || sources.iter().any(|(known, _)| known == path) {
            continue;
        }
        blocks.push(format!("{path}: {}", diagnostic.message));
    }
    if blocks.is_empty() {
        "compilation produced no model".to_string()
    } else {
        blocks.join("\n")
    }
}

/// Scan `.mox` source text for `import schema "<path>" (as <Alias>)?`
/// declarations. A line-oriented scan like the `.actor` import scanner: the
/// lowered pherolang IR does not carry import declarations, and a full phero-syntax
/// parse here would duplicate the compile. Pinned v1 limits: the
/// declaration must start its (trimmed) line with `import schema`, the path
/// is a plain double-quoted string without escapes, and the optional alias
/// is a bare identifier after `as`. Anything else is invisible to the scan
/// (the compile itself still validates the declaration upstream).
pub(crate) fn scan_schema_imports(source: &str) -> Vec<SchemaImportDecl> {
    source
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("import schema ")?.trim();
            let open = rest.strip_prefix('"')?;
            let close = open.find('"')?;
            let path = &open[..close];
            if path.is_empty() {
                return None;
            }
            let alias = open[close + 1..]
                .trim()
                .strip_prefix("as ")
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty());
            Some(SchemaImportDecl {
                path: path.to_string(),
                alias,
            })
        })
        .collect()
}

/// Scan the `package <name>` declaration of a `.mox` source (same
/// line-scan contract as [`scan_schema_imports`]; the first match wins —
/// the grammar allows one package declaration per file).
pub(crate) fn scan_package_name(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("package ")?;
        let name: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
        (!name.is_empty()).then_some(name)
    })
}

/// The name an import joins its package's namespace as: the `as` alias, or
/// the import path's file stem (mirroring the upstream `imported_name`).
fn import_name(decl: &SchemaImportDecl) -> String {
    decl.alias.clone().unwrap_or_else(|| {
        Path::new(&decl.path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    })
}

/// One scanned import resolved to a readable, JSON-validated target.
pub(crate) struct ScannedImport {
    /// The import path exactly as written (the compile key).
    pub import_path: String,
    /// The namespace name the import joins (alias or file stem).
    pub alias: String,
    /// Absolute path resolved against the .mox file's directory.
    pub abs_path: PathBuf,
    /// The validated JSON text.
    pub json: String,
    /// Package name of the importing .mox file.
    pub importing_package: String,
}

/// Scan one `.mox` source's `import schema` declarations, resolve each
/// against the .mox file's directory, and read + JSON-validate the target.
/// Unreadable or invalid files are a HARD error (variant
/// [`Error::MoxSchemaImport`]): the lowered model's structural references
/// depend on the imported content, so unlike `.actor` policy imports this
/// is not warn-and-continue.
pub(crate) fn collect_schema_imports(
    mox_path: &str,
    source: &str,
    base_dir: &Path,
) -> Result<Vec<ScannedImport>> {
    let mut out = Vec::new();
    for decl in scan_schema_imports(source) {
        let abs_path = base_dir.join(&decl.path);
        let json = std::fs::read_to_string(&abs_path).map_err(|e| Error::MoxSchemaImport {
            mox_path: mox_path.to_string(),
            import_path: decl.path.clone(),
            reason: format!("could not be read: {e}"),
        })?;
        if let Err(e) = serde_json::from_str::<serde_json::Value>(&json) {
            return Err(Error::MoxSchemaImport {
                mox_path: mox_path.to_string(),
                import_path: decl.path.clone(),
                reason: format!("is not valid JSON: {e}"),
            });
        }
        out.push(ScannedImport {
            import_path: decl.path.clone(),
            alias: import_name(&decl),
            abs_path,
            json,
            importing_package: scan_package_name(source).unwrap_or_default(),
        });
    }
    Ok(out)
}

/// Compile `.mox` files in-process and ingest the resulting domain model
/// into the graph. Diagnostics warn and never fail: unreadable files,
/// compile errors, and unmatched class names are counted in
/// [`MoxIngestStats::skipped`] and reported on stderr.
///
/// `import schema "<path>"` declarations are scanned before compiling,
/// resolved against the .mox file's directory, and their JSON content
/// validated and provided to the pherolang compiler ([`SchemaImports`]). A
/// missing or invalid import file is a HARD error ([`Error::MoxSchemaImport`])
/// naming both paths. The imported files themselves are returned on
/// [`MoxIngestOutcome::imported_files`] for the schema pipeline to ingest,
/// and alias-typed features on [`MoxIngestOutcome::pending_alias_refs`] for
/// the post-schema-pass wiring step ([`wire_alias_refs`]).
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub async fn ingest_mox_files(
    ingestor: &dyn GraphIngestor,
    querier: &dyn GraphQuerier,
    mox_paths: &[PathBuf],
    domain_config: &DomainConfig,
    type_suffix: &str,
) -> Result<MoxIngestOutcome> {
    let mut stats = MoxIngestStats::default();
    let Some(scanned) = scan_mox_files(mox_paths, domain_config, &mut stats)? else {
        return Ok(MoxIngestOutcome {
            stats,
            imported_files: Vec::new(),
            pending_alias_refs: Vec::new(),
        });
    };
    stats.imported_files = scanned
        .imported_files
        .iter()
        .map(|i| {
            i.abs_path
                .canonicalize()
                .unwrap_or_else(|_| i.abs_path.clone())
                .display()
                .to_string()
        })
        .collect::<HashSet<_>>()
        .len();
    let import_names: HashSet<String> = scanned
        .imported_files
        .iter()
        .map(|i| i.alias.clone())
        .collect();

    let imports = DomainImports {
        schemas: scanned.schema_imports,
        sigil: phero_driver::SigilImports::new(),
    };
    let compilation = compile_files(&scanned.sources, &imports);
    for (path, diagnostic) in &compilation.diagnostics {
        eprintln!("Warning: mox diagnostic in {path}: {}", diagnostic.message);
    }
    // A compile that lowers NO model is a hard error, not a skip: every
    // class, enum and vocabulary would silently vanish and the run would
    // happily generate an empty app (the WS5 failure mode). Individual
    // file-level problems that still leave a partial model keep the warn
    // path above — this is the total-failure case only.
    let Some(model) = compilation.model else {
        let reason = render_mox_compile_failure(&scanned.sources, &compilation.diagnostics);
        stats.skipped += scanned.sources.len();
        return Err(Error::MoxModel { reason });
    };

    // Schema entities already in the graph: mox classes attach by NAME
    // (schema title or entity name). Warn on mismatch, never fail.
    let schemas = querier.list_schemas(None).await.unwrap_or_default();
    let known_titles: HashSet<String> = schemas.iter().map(|s| s.title.clone()).collect();
    let json_schema_ids: HashMap<String, String> = schemas
        .iter()
        .map(|s| (s.title.clone(), s.schema_id.clone()))
        .collect();

    let mut bridge = MoxBridge::new(
        &model,
        ingestor,
        domain_config,
        type_suffix,
        import_names,
        known_titles,
        json_schema_ids,
        stats,
    );
    bridge.bridge_namespaces().await?;
    bridge.scan_class_universe();
    bridge.scan_enum_universe();
    bridge.scan_containment_targets();
    bridge.bridge_enum_codelists().await?;
    bridge.bridge_register_codelists().await?;
    bridge.build_domain_model();
    bridge.bridge_class_schemas().await?;
    bridge.bridge_enum_schemas().await?;
    bridge.emit_in_namespace_edges().await?;
    bridge.bridge_class_properties().await?;
    bridge.finish().await?;
    Ok(bridge.into_outcome(scanned.imported_files))
}

/// Read and dedupe the `.mox` sources, collecting each file's `import schema`
/// declarations (issue #230) along the way. Returns `None` when no source
/// could be read (the historical empty-run outcome).
/// The scanned `.mox` sources plus their collected `import schema` tables.
struct ScannedMoxFiles {
    sources: Vec<(String, String)>,
    imported_files: Vec<MoxSchemaImport>,
    schema_imports: SchemaImports,
}

fn scan_mox_files(
    mox_paths: &[PathBuf],
    domain_config: &DomainConfig,
    stats: &mut MoxIngestStats,
) -> Result<Option<ScannedMoxFiles>> {
    let mut sources: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut imported_files: Vec<MoxSchemaImport> = Vec::new();
    let mut schema_imports = SchemaImports::new();
    for path in mox_paths {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !seen.insert(canonical.display().to_string()) {
            continue;
        }
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let mox_path = path.display().to_string();
                // Imports resolve relative to the .mox file's directory.
                let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
                for scanned in collect_schema_imports(&mox_path, &text, base_dir)? {
                    schema_imports.insert(&mox_path, &scanned.import_path, scanned.json);
                    imported_files.push(MoxSchemaImport {
                        mox_path: mox_path.clone(),
                        import_path: scanned.import_path,
                        abs_path: scanned.abs_path,
                        alias: scanned.alias,
                        importing_package: scanned.importing_package.clone(),
                        domain: resolve_domain(domain_config, &scanned.importing_package).0,
                    });
                }
                sources.push((mox_path, text));
            }
            Err(e) => {
                eprintln!(
                    "Warning: mox file '{}' could not be read: {e}",
                    path.display()
                );
                stats.skipped += 1;
            }
        }
    }
    if sources.is_empty() {
        return Ok(None);
    }
    Ok(Some(ScannedMoxFiles {
        sources,
        imported_files,
        schema_imports,
    }))
}

/// Post-schema-pass wiring for alias-typed features (issue #230): resolve
/// each pending alias against the ingested schema titles — exact match
/// first, then title + `type_suffix` (the `Customer` → `CustomerType`
/// convention) — and create the ReferencesSchema/ItemsOf edge to the REAL
/// node. Unresolvable aliases warn (naming both names) and are counted in
/// `stats.unresolved_aliases`; they are never a hard error — the imported
/// file was valid, the mismatch is a naming miss.
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub async fn wire_alias_refs(
    ingestor: &dyn GraphIngestor,
    querier: &dyn GraphQuerier,
    pending: &[PendingAliasRef],
    type_suffix: &str,
    stats: &mut MoxIngestStats,
) -> Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let schemas = querier.list_schemas(None).await.map_err(Error::Graph)?;
    let title_to_schema_id: HashMap<String, String> = schemas
        .iter()
        .map(|s| (s.title.clone(), s.schema_id.clone()))
        .collect();

    for pending in pending {
        let resolved = if title_to_schema_id.contains_key(&pending.alias) {
            Some(pending.alias.clone())
        } else {
            let suffixed = format!("{}{}", pending.alias, type_suffix);
            title_to_schema_id
                .contains_key(&suffixed)
                .then_some(suffixed)
        };
        let Some(title) = resolved else {
            let unresolvable = UnresolvedAlias {
                alias: pending.alias.clone(),
                schema_title: pending.schema_title.clone(),
                prop_name: pending.prop_name.clone(),
                type_suffix: type_suffix.to_string(),
            };
            eprintln!("{}", unresolved_alias_warning(&unresolvable));
            stats.unresolved_aliases += 1;
            stats
                .incompleteness
                .push(Incompleteness::unresolved_reference(&pending.alias));
            continue;
        };
        // `title` is a key of the map by construction.
        let Some(target_schema_id) = title_to_schema_id.get(&title) else {
            continue;
        };
        let edge_type = if pending.is_array {
            EdgeType::ItemsOf
        } else {
            EdgeType::ReferencesSchema
        };
        ingestor
            .ingest_edge(
                &format!("{}::{}", pending.prop_name, pending.schema_title),
                target_schema_id,
                edge_type,
                Some(&EdgeProperties {
                    ref_path: Some(title),
                    ..Default::default()
                }),
            )
            .await
            .map_err(Error::Graph)?;
        stats.resolved_aliases += 1;
    }
    Ok(())
}

/// One mox class flattened for ingestion: its definition, owning package,
/// resolved domain, namespace (the package name — `None` when package-less),
/// and the `schema_id` used for its graph node.
pub(super) struct ClassEntry<'a> {
    pub(super) class: &'a phero_ir::ClassDef,
    pub(super) domain: String,
    pub(super) schema_id: String,
    pub(super) namespace: Option<String>,
}

impl ClassEntry<'_> {
    /// `true` for a `register` declaration (issue #668): a plain class on the
    /// wire carrying the upstream `ClassKind::Register` marker.
    pub(super) fn is_register(&self) -> bool {
        self.class.kind == ClassKind::Register
    }
}

/// One mox enum flattened for ingestion (same shape as [`ClassEntry`]).
pub(super) struct EnumEntry<'a> {
    pub(super) enum_def: &'a phero_ir::EnumDef,
    pub(super) domain: String,
    pub(super) schema_id: String,
    pub(super) namespace: Option<String>,
}

/// Map a mox package name to a domain: an exact `domains.toml` key wins,
/// then the last dot-segment; otherwise the snake_cased last segment is the
/// domain (and, by the domain-driven pg-schema convention, the namespace).
pub(crate) fn resolve_domain(domain_config: &DomainConfig, package: &str) -> (String, bool) {
    if domain_config.domains.contains_key(package) {
        return (package.to_string(), true);
    }
    let last = package.rsplit('.').next().unwrap_or(package);
    for name in domain_config.domains.keys() {
        if name == last {
            return (name.clone(), true);
        }
    }
    (to_snake_case(last), false)
}

/// The namespace FQN for a mox package: `None` when the model is
/// package-less (empty package name) — namespace-less models must stay
/// byte-identical (issue #268).
pub(super) fn non_empty_namespace(package: &str) -> Option<String> {
    (!package.trim().is_empty()).then(|| package.trim().to_string())
}

/// `NamespaceNode.source` provenance for mox-declared namespaces.
pub(crate) const MOX_NAMESPACE_SOURCE: &str = "mox";

/// `NamespaceNode.source` provenance for namespaces that only import
/// declarations reference (e.g. the sigil builtins' `com.rosetta.model`).
pub(crate) const DISCOVERED_NAMESPACE_SOURCE: &str = "discovered";

/// Expand one dotted FQN into its `(fqn, parent)` chain, root first
/// (issue #268): `a.b.c` → `[("a", None), ("a.b", "a"), ("a.b.c", "a.b")]`.
/// Empty segments are skipped; an empty FQN yields an empty chain.
pub(crate) fn namespace_chain(fqn: &str) -> Vec<(String, Option<String>)> {
    let segments: Vec<&str> = fqn.split('.').filter(|s| !s.is_empty()).collect();
    (0..segments.len())
        .map(|i| {
            (
                segments[..=i].join("."),
                (i != 0).then(|| segments[..i].join(".")),
            )
        })
        .collect()
}

/// Ingest namespace nodes exactly once each: the pending `(fqn, source)`
/// set is expanded into full parent chains and deduplicated by FQN (first
/// source wins in sorted order — deterministic), then every node lands
/// with its `NamespaceParent` edge. Returns the number of nodes ingested.
pub(crate) async fn ingest_namespaces_deduped(
    ingestor: &dyn GraphIngestor,
    pending: &BTreeMap<String, String>,
) -> crate::error::Result<usize> {
    // fqn → (parent, source); BTreeMap iteration keeps ingestion order
    // deterministic (sorted by fqn).
    let mut nodes: BTreeMap<String, (Option<String>, String)> = BTreeMap::new();
    for (fqn, source) in pending {
        for (chain_fqn, parent) in namespace_chain(fqn) {
            nodes
                .entry(chain_fqn)
                .or_insert_with(|| (parent, source.clone()));
        }
    }
    for (fqn, (parent, source)) in &nodes {
        let node = NamespaceNode {
            fqn: fqn.clone(),
            parent: parent.clone(),
            source: Some(source.clone()),
        };
        ingestor
            .ingest_namespace(&node)
            .await
            .map_err(Error::Graph)?;
    }
    Ok(nodes.len())
}

/// Link one bridged schema to its namespace via an `InNamespace` edge
/// (issue #268). Namespace-less schemas contribute nothing (back-compat).
pub(crate) async fn emit_schema_namespace_edge(
    ingestor: &dyn GraphIngestor,
    schema_id: &str,
    namespace: Option<&str>,
) -> crate::error::Result<()> {
    let Some(ns) = namespace.filter(|n| !n.trim().is_empty()) else {
        return Ok(());
    };
    ingestor
        .ingest_edge(schema_id, ns, EdgeType::InNamespace, None)
        .await
        .map_err(Error::Graph)
}

/// How a bridged mox class lands on the graph (issue #668).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClassBridge {
    /// `class` — an entity or a contained value object (author-declared: a
    /// class targeted only by `contains` is a VO, `refers` wins).
    Class { is_entity: bool },
    /// `register` — the reference-data (codelist/seed-table) plane.
    Register,
}

/// The effective storage table of a mox class: a declared `table "…"`
/// override wins over the derived name (issue #668; upstream pherolang#177
/// `ClassDef::table_name` — "Some is the authoritative spelling").
///
/// The declared spelling is normalized to snake_case: `pg_table_name` is
/// BOTH the PostgreSQL table and the Rust module/file stem every plane keys
/// on (`crate::domain::{domain}::{module}`, migration file names), so a
/// declared `"PayRunRows"`/`"pay-run"` must land on one legal, consistent
/// identifier. A normalization that changes the spelling warns — never
/// silently.
pub(super) fn class_table_name(class: &phero_ir::ClassDef, type_suffix: &str) -> String {
    match class.table_name.as_deref() {
        Some(declared) => {
            let normalized = to_snake_case(declared);
            if normalized != declared {
                eprintln!(
                    "Warning: mox class '{}' declares table \"{declared}\" — normalized to \
                     '{normalized}' (the table name doubles as the module/table identifier, \
                     which is snake_case)",
                    class.name
                );
            }
            normalized
        }
        None => to_snake_case(&strip_suffix(&class.name, type_suffix)),
    }
}

/// Build the `SchemaNode` for a mox class, mirroring the JSON path's field
/// population (`ingest_schema_node` in async_ingest).
pub(super) fn class_schema_node(
    entry: &ClassEntry<'_>,
    bridge: ClassBridge,
    type_suffix: &str,
) -> SchemaNode {
    let class = entry.class;
    let stripped = strip_suffix(&class.name, type_suffix);
    let mut custom_annotations = HashMap::new();
    custom_annotations.insert(
        "source".to_string(),
        serde_json::Value::String(codegraph_core::types::MOX_SOURCE.to_string()),
    );
    if bridge == ClassBridge::Register {
        custom_annotations.insert(
            codegraph_core::types::MOX_CLASS_KIND.to_string(),
            serde_json::Value::String(codegraph_core::types::MOX_CLASS_KIND_REGISTER.to_string()),
        );
    }
    let is_entity = matches!(bridge, ClassBridge::Class { is_entity: true });
    let is_register = bridge == ClassBridge::Register;
    SchemaNode {
        namespace: entry.namespace.clone(),
        schema_id: entry.schema_id.clone(),
        title: class.name.clone(),
        description: class.description.as_deref().map(sanitize_description),
        schema_type: if is_register { "string" } else { "object" }.to_string(),
        classification: if is_register {
            "codelist".to_string()
        } else if is_entity {
            "entity_reference".to_string()
        } else {
            "value_object".to_string()
        },
        domain: Some(entry.domain.clone()),
        rel_path: entry.schema_id.clone(),
        pg_type: "UUID".to_string(),
        rust_type: stripped.clone(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: sanitize_rust_type_name(&stripped),
        pg_table_name: class_table_name(class, type_suffix),
        api_path_segment: to_kebab_case(&stripped),
        parent_schema: None,
        is_entity,
        is_codelist: is_register,
        is_primitive_wrapper: false,
        has_all_of: !class.extends.is_empty(),
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations,
        // phero-ir v1 (rev 77688ee) carries neither an access flag nor
        // structured annotations on `ClassDef` — `None` keeps the bridge
        // byte-identical (issue #279); producers connect upstream.
        access: None,
        annotations: None,
    }
}

/// Build the codelist `SchemaNode` for a mox enum, mirroring the JSON path's
/// enum-valued schema (`classification=codelist`, `is_codelist=true`): the
/// node codelist DDL, FK resolution, and link generation key on.
pub(super) fn enum_schema_node(entry: &EnumEntry<'_>, type_suffix: &str) -> SchemaNode {
    let enum_def = entry.enum_def;
    let stripped = strip_suffix(&enum_def.name, type_suffix);
    let mut custom_annotations = HashMap::new();
    custom_annotations.insert(
        "source".to_string(),
        serde_json::Value::String(codegraph_core::types::MOX_SOURCE.to_string()),
    );
    SchemaNode {
        namespace: entry.namespace.clone(),
        schema_id: entry.schema_id.clone(),
        title: enum_def.name.clone(),
        description: enum_def.description.as_deref().map(sanitize_description),
        schema_type: "string".to_string(),
        classification: "codelist".to_string(),
        domain: Some(entry.domain.clone()),
        rel_path: entry.schema_id.clone(),
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
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations,
        access: None,
        annotations: None,
    }
}

/// Features of a bridged class in the JSON path's allOf-canonical order:
/// inherited features before own features, each group name-sorted.
///
/// `ingest_properties_from_schema` orders its prop_blocks parent-$ref-blocks
/// first, inline-own-block last, and iterates each block in serde_json's
/// name-sorted map order. `list_all_properties` (the CachingQuerier warm
/// path) has no `ORDER BY`, so insertion order is output field order and
/// must match the JSON path byte-for-byte (issue #233).
pub(super) fn ordered_bridge_features<'a>(
    class: &'a phero_ir::ClassDef,
    class_map: &HashMap<&str, &'a phero_ir::ClassDef>,
    bridged: &HashSet<String>,
) -> Vec<&'a phero_ir::Feature> {
    let mut ordered = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    collect_ancestor_features(class, class_map, bridged, &mut visited, &mut ordered);
    let mut own: Vec<&phero_ir::Feature> = class.features.iter().collect();
    own.sort_by(|a, b| a.name.cmp(&b.name));
    ordered.extend(own);
    // First occurrence wins per name — the same semantics the generators'
    // query-time dedup applies to the JSON path's merged (and possibly
    // duplicated) property blocks.
    let mut seen_names: HashSet<String> = HashSet::new();
    ordered
        .into_iter()
        .filter(|f| seen_names.insert(f.name.clone()))
        .collect()
}

/// Recursively collect an ancestor chain's features (a parent's own parents
/// before its own features), skipping JSON-owned or unknown parents — the
/// schema pass merges those on the JSON side.
fn collect_ancestor_features<'a>(
    class: &'a phero_ir::ClassDef,
    class_map: &HashMap<&str, &'a phero_ir::ClassDef>,
    bridged: &HashSet<String>,
    visited: &mut HashSet<String>,
    out: &mut Vec<&'a phero_ir::Feature>,
) {
    for parent in &class.extends {
        let TypeRef::Class { name, .. } = parent else {
            continue;
        };
        if !visited.insert(name.clone()) || !bridged.contains(name) {
            continue;
        }
        let Some(parent_class) = class_map.get(name.as_str()).copied() else {
            continue;
        };
        collect_ancestor_features(parent_class, class_map, bridged, visited, out);
        let mut own: Vec<&phero_ir::Feature> = parent_class.features.iter().collect();
        own.sort_by(|a, b| a.name.cmp(&b.name));
        out.extend(own);
    }
}

/// Map one stored feature onto a `PropertyNode`, mirroring the JSON path's
/// classification and type-string conventions. Returns `None` for features
/// that produce no stored column (interfaces).
pub(super) fn feature_property(
    feature: &phero_ir::Feature,
    schema_title: &str,
    datatype_formats: &HashMap<(String, String), String>,
    sole_identity: bool,
    stats: &mut MoxIngestStats,
) -> Option<PropertyNode> {
    let is_array = feature.multiplicity.is_many();
    let is_required = feature.multiplicity.lower >= 1;

    // Classify the feature type into the JSON path's classification shapes.
    enum Mapped {
        Primitive {
            pg: PgType,
            format: Option<String>,
        },
        Codelist {
            target: String,
        },
        Reference {
            target: String,
            kind: RefClassificationKind,
            stored_fk: bool,
        },
    }
    let mapped = match (&feature.kind, &feature.type_) {
        (_, TypeRef::Class { name, .. }) => {
            // `refers` (and, defensively, an attribute typed with a class)
            // is an FK-style entity reference; `contains` a value object.
            let kind = if feature.kind == FeatureKind::Containment {
                RefClassificationKind::ValueObject
            } else {
                RefClassificationKind::EntityReference
            };
            // A single `refers` is a stored FK: project a Uuid `{name}_id`
            // column (resolve_field appends the suffix; mirrors
            // ProjectionBuilder::build_entity_ref on the JSON path). Array
            // references keep child-table/junction semantics — no direct
            // column, so the pg/rust bases stay empty.
            let stored_fk = kind == RefClassificationKind::EntityReference && !is_array;
            Mapped::Reference {
                target: name.clone(),
                kind,
                stored_fk,
            }
        }
        (_, TypeRef::Enum { name, .. }) | (_, TypeRef::Vocabulary { name, .. }) => {
            Mapped::Codelist {
                target: name.clone(),
            }
        }
        (_, TypeRef::Interface { name, .. }) => {
            eprintln!(
                "Warning: mox feature '{}.{}' targets interface '{}' — no stored column is produced",
                schema_title, feature.name, name
            );
            stats.skipped += 1;
            return None;
        }
        (_, TypeRef::Datatype { package, name }) => {
            let format = datatype_formats.get(&(package.clone(), name.clone()));
            match format.map(String::as_str) {
                Some("uuid") => Mapped::Primitive {
                    pg: PgType::Uuid,
                    format: Some("uuid".to_string()),
                },
                Some("date") => Mapped::Primitive {
                    pg: PgType::Date,
                    format: Some("date".to_string()),
                },
                Some("date-time") => Mapped::Primitive {
                    pg: PgType::Timestamptz,
                    format: Some("date-time".to_string()),
                },
                Some(s @ ("email" | "uri")) => Mapped::Primitive {
                    pg: PgType::Text,
                    format: Some(s.to_string()),
                },
                Some("json") => Mapped::Primitive {
                    pg: PgType::Jsonb,
                    format: Some("json".to_string()),
                },
                Some("decimal") => Mapped::Primitive {
                    pg: PgType::Numeric {
                        precision: 19,
                        scale: 4,
                    },
                    format: Some("decimal".to_string()),
                },
                Some(other) => {
                    eprintln!(
                        "Warning: mox datatype '{name}' has unknown format '{other}' — mapping to TEXT"
                    );
                    Mapped::Primitive {
                        pg: PgType::Text,
                        format: Some(other.to_string()),
                    }
                }
                None => {
                    eprintln!(
                        "Warning: mox datatype '{name}' declares no format — mapping to TEXT"
                    );
                    Mapped::Primitive {
                        pg: PgType::Text,
                        format: None,
                    }
                }
            }
        }
        (_, TypeRef::Primitive(p)) => Mapped::Primitive {
            pg: primitive_pg_type(*p),
            format: None,
        },
    };

    // Identity mapping rule (issue #522). A class with exactly ONE `id`
    // feature treats it as the surrogate identity: every layer must agree on
    // Uuid, so the declared datatype is overridden — the DDL and entity
    // generators already emit Uuid for these columns, and a TEXT/String
    // projection over a Uuid column breaks generated hydration (E0308:
    // expected `String`, found `Uuid`). Multiple `id` features are a
    // Sculptor composite natural key: every part keeps its declared mapping
    // (String→TEXT, wraps-String→TEXT, `format "uuid"`→UUID, …) and the
    // generated surrogate `id UUID` column stays the storage key.
    let mapped = if feature.is_id && sole_identity {
        match mapped {
            Mapped::Primitive { .. } => Mapped::Primitive {
                pg: PgType::Uuid,
                format: Some("uuid".to_string()),
            },
            other => other,
        }
    } else {
        mapped
    };

    let (kind, pg_base, rust_base, sea_base, ref_target, format_hint) = match mapped {
        Mapped::Primitive { pg, format } => (
            RefClassificationKind::PrimitiveWrapper,
            pg.pg_ddl(),
            pg.canonical_rust_type().as_rust_str().to_string(),
            pg.sea_orm_type().to_string(),
            None,
            format,
        ),
        Mapped::Codelist { target } => (
            RefClassificationKind::CodelistReference,
            "TEXT".to_string(),
            "String".to_string(),
            "Text".to_string(),
            Some(target),
            None,
        ),
        Mapped::Reference {
            target,
            kind,
            stored_fk,
        } => (
            kind,
            if stored_fk {
                PgType::Uuid.pg_ddl()
            } else {
                String::new()
            },
            if stored_fk {
                PgType::Uuid.canonical_rust_type().as_rust_str()
            } else {
                target.clone()
            },
            if stored_fk {
                PgType::Uuid.sea_orm_type().to_string()
            } else {
                String::new()
            },
            Some(target),
            None,
        ),
    };

    // Arrays of primitives wrap in Vec + pg [] suffix; entity/VO/codelist
    // arrays keep child-table semantics (mirrors the JSON path's array
    // branch in ingest_properties_from_schema).
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

    let sanitized_name = feature.name.replace(['@', '-'], "");
    let snake = to_snake_case(&sanitized_name);
    let projection = build_projection(&kind, &snake, &pg_type, &rust_type, &sea_base);
    // Stored single references carry the `_id` suffix on both the column
    // and the Rust field (resolve_field parity with the JSON path).
    let stored_fk_column = kind == RefClassificationKind::EntityReference && !is_array;
    let pg_column_name = if stored_fk_column {
        format!("{snake}_id")
    } else {
        snake.clone()
    };
    let mut rust_field_name = escape_rust_keyword(&if stored_fk_column {
        format!("{snake}_id")
    } else {
        snake.clone()
    });
    if matches!(
        kind,
        RefClassificationKind::CodelistReference | RefClassificationKind::CodelistCheck
    ) {
        rust_field_name = strip_code_suffix(&rust_field_name);
    }

    let prop_type = match &feature.type_ {
        TypeRef::Class { .. }
        | TypeRef::Interface { .. }
        | TypeRef::Enum { .. }
        | TypeRef::Vocabulary { .. } => "object".to_string(),
        TypeRef::Primitive(PrimitiveType::Int)
        | TypeRef::Primitive(PrimitiveType::Long)
        | TypeRef::Primitive(PrimitiveType::Short)
        | TypeRef::Primitive(PrimitiveType::Byte) => "integer".to_string(),
        TypeRef::Primitive(PrimitiveType::Float) | TypeRef::Primitive(PrimitiveType::Double) => {
            "number".to_string()
        }
        TypeRef::Primitive(PrimitiveType::Boolean) => "boolean".to_string(),
        _ => "string".to_string(),
    };

    // Express the frozen classification structurally (issue #277); the
    // legacy strings on the node stay populated either way.
    let type_expr = TypeExpr::from_frozen(&kind, ref_target.as_deref(), &rust_type);

    Some(PropertyNode {
        name: feature.name.clone(),
        prop_type,
        description: feature.description.as_deref().map(sanitize_description),
        format: format_hint,
        is_required,
        is_nullable: !is_required,
        is_id: feature.is_id,
        is_array,
        min_items: None,
        max_items: None,
        pattern: feature.constraints.pattern.clone(),
        min_length: feature.constraints.min_length,
        max_length: feature.constraints.max_length,
        minimum: feature.constraints.minimum.map(rust_decimal::Decimal::from),
        maximum: feature.constraints.maximum.map(rust_decimal::Decimal::from),
        pg_column_name,
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
        type_expr,
    })
}

fn primitive_pg_type(p: PrimitiveType) -> PgType {
    match p {
        PrimitiveType::String | PrimitiveType::Char => PgType::Text,
        PrimitiveType::Int => PgType::Integer,
        PrimitiveType::Long => PgType::BigInt,
        PrimitiveType::Short | PrimitiveType::Byte => PgType::SmallInt,
        PrimitiveType::Float => PgType::Real,
        PrimitiveType::Double => PgType::DoublePrecision,
        PrimitiveType::Boolean => PgType::Boolean,
        PrimitiveType::Date => PgType::Date,
    }
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

/// Build the cross-layer projection the same way the classifier's
/// `ProjectionBuilder` does for the JSON path.
pub(crate) fn build_projection(
    kind: &RefClassificationKind,
    field_name: &str,
    pg_type: &str,
    rust_type: &str,
    sea_orm_type: &str,
) -> DddFieldProjection {
    let pg = (kind == &RefClassificationKind::PrimitiveWrapper).then_some(pg_type);
    let rust = (kind == &RefClassificationKind::PrimitiveWrapper).then_some(rust_type);
    let sea = (kind == &RefClassificationKind::PrimitiveWrapper).then_some(sea_orm_type);
    ProjectionBuilder::from_classification(
        kind,
        field_name,
        pg,
        rust,
        sea,
        None,
        None,
        &HashMap::new(),
        None,
        None,
        None,
        false,
    )
}

/// Strip a trailing `_code` from a codelist field name (JSON-path parity:
/// the column keeps the suffix, the Rust field does not).
pub(crate) fn strip_code_suffix(name: &str) -> String {
    match name.strip_suffix("_code") {
        Some(stripped) if !stripped.is_empty() => stripped.to_string(),
        _ => name.to_string(),
    }
}

/// Render a resolved [`TypeRef`] as a plain name: primitives use their
/// mox-level name (`"string"`, `"int"`), everything else its qualified name.
pub(super) fn type_ref_name(type_ref: &TypeRef) -> String {
    match type_ref {
        TypeRef::Primitive(p) => p.to_string(),
        other => other.qualified_name().unwrap_or_default(),
    }
}

/// Convert a pherolang [`DefaultValue`] (vocabulary facet values) to a plain JSON
/// value for graph storage.
pub(super) fn default_value_json(value: &DefaultValue) -> serde_json::Value {
    match value {
        DefaultValue::String(s) => serde_json::Value::String(s.clone()),
        DefaultValue::Int(i) => serde_json::json!(i),
        DefaultValue::Bool(b) => serde_json::Value::Bool(*b),
        DefaultValue::EnumLiteral(name) => serde_json::Value::String(name.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_ref_name_resolves_primitives_and_qualified_names() {
        assert_eq!(
            type_ref_name(&TypeRef::Primitive(phero_ir::PrimitiveType::String)),
            "string"
        );
        assert_eq!(
            type_ref_name(&TypeRef::Primitive(phero_ir::PrimitiveType::Int)),
            "int"
        );
    }

    #[test]
    fn default_value_json_strips_the_wire_tags() {
        assert_eq!(
            default_value_json(&DefaultValue::Int(2)),
            serde_json::json!(2)
        );
        assert_eq!(
            default_value_json(&DefaultValue::String("€".into())),
            serde_json::json!("€")
        );
        assert_eq!(
            default_value_json(&DefaultValue::Bool(true)),
            serde_json::json!(true)
        );
    }

    #[test]
    fn stats_display_lists_all_counters() {
        let stats = MoxIngestStats {
            vocabularies: 1,
            operations: 2,
            derived_features: 3,
            skipped: 4,
            classes: 5,
            properties: 6,
            edges: 7,
            enums: 8,
            enum_schemas: 9,
            bridged_titles: Vec::new(),
            imported_files: 0,
            resolved_aliases: 0,
            unresolved_aliases: 0,
            namespaces: 0,
            registers: 0,
            incompleteness: Vec::new(),
        };
        assert_eq!(
            stats.to_string(),
            "1 vocabularies, 2 operations, 3 derived features, 4 skipped, \
             5 classes, 6 properties, 7 edges, 8 enums, 9 enum schemas"
        );
    }

    /// Issue #668: register-free models keep byte-identical stats lines —
    /// the counter is appended only when a register class was ingested.
    #[test]
    fn stats_display_appends_the_register_counter_only_when_present() {
        let with_registers = MoxIngestStats {
            registers: 2,
            ..Default::default()
        };
        assert!(
            with_registers.to_string().contains(", 2 registers"),
            "got: {with_registers}"
        );
        let without = MoxIngestStats::default();
        assert!(!without.to_string().contains("registers"), "got: {without}");
    }

    #[test]
    fn stats_display_appends_import_counters_only_when_present() {
        let stats = MoxIngestStats {
            imported_files: 2,
            resolved_aliases: 1,
            unresolved_aliases: 1,
            ..Default::default()
        };
        let display = stats.to_string();
        assert!(
            display.ends_with("2 imported files, 1 aliases resolved, 1 unresolved"),
            "{display}"
        );
    }

    #[test]
    fn scan_schema_imports_finds_paths_and_aliases() {
        let source = concat!(
            "package todo\n\n",
            "import schema \"schemas/todo_item.json\" as TodoItem\n",
            "import schema \"schemas/note.json\"\n",
            "class C { refers TodoItem[] items }\n"
        );
        assert_eq!(
            scan_schema_imports(source),
            vec![
                SchemaImportDecl {
                    path: "schemas/todo_item.json".to_string(),
                    alias: Some("TodoItem".to_string()),
                },
                SchemaImportDecl {
                    path: "schemas/note.json".to_string(),
                    alias: None,
                },
            ]
        );
    }

    #[test]
    fn scan_schema_imports_ignores_actor_imports_and_prose() {
        // `.actor`-style imports and ordinary lines must not match.
        assert!(scan_schema_imports("import \"support.mox\"\nclass C { String x }").is_empty());
        assert!(scan_schema_imports("import schema\nbroken").is_empty());
    }

    #[test]
    fn scan_package_name_reads_the_first_package_line() {
        assert_eq!(
            scan_package_name("package nz.example.todo\n\nclass C {}"),
            Some("nz.example.todo".to_string())
        );
        assert_eq!(scan_package_name("class C {}"), None);
    }

    #[test]
    fn import_name_prefers_alias_then_stem() {
        let aliased = SchemaImportDecl {
            path: "deep/dir/todo_item.json".to_string(),
            alias: Some("TodoItem".to_string()),
        };
        let stemmed = SchemaImportDecl {
            path: "deep/dir/todo_item.json".to_string(),
            alias: None,
        };
        assert_eq!(import_name(&aliased), "TodoItem");
        assert_eq!(import_name(&stemmed), "todo_item");
    }

    #[test]
    fn unresolved_alias_warning_names_both_tried_titles() {
        let warning = unresolved_alias_warning(&UnresolvedAlias {
            alias: "Widget".to_string(),
            schema_title: "TodoListType".to_string(),
            prop_name: "items".to_string(),
            type_suffix: "Type".to_string(),
        });
        assert!(warning.contains("'Widget'"), "{warning}");
        assert!(warning.contains("'WidgetType'"), "{warning}");
        assert!(warning.contains("TodoListType"), "{warning}");
        assert!(warning.contains("items"), "{warning}");
    }

    #[test]
    fn resolve_domain_prefers_exact_then_last_segment() {
        let config = toml::from_str::<DomainConfig>(
            r#"
[domains."nz.example.library"]
label = "Library"
schema_dir = "schemas/library"
postgres_schema = "library"

[domains.library]
label = "Library Short"
schema_dir = "schemas/library"
postgres_schema = "lib"
"#,
        )
        .unwrap();
        let (exact, matched) = resolve_domain(&config, "nz.example.library");
        assert_eq!(exact, "nz.example.library");
        assert!(matched);
        let (segment, matched) = resolve_domain(&config, "other.library");
        assert_eq!(segment, "library");
        assert!(matched);
        let (fallback, matched) = resolve_domain(&config, "totally.unknown");
        assert_eq!(fallback, "unknown");
        assert!(!matched);
    }

    #[test]
    fn primitive_pg_type_covers_every_pherolang_primitive() {
        assert_eq!(primitive_pg_type(PrimitiveType::String), PgType::Text);
        assert_eq!(primitive_pg_type(PrimitiveType::Char), PgType::Text);
        assert_eq!(primitive_pg_type(PrimitiveType::Int), PgType::Integer);
        assert_eq!(primitive_pg_type(PrimitiveType::Long), PgType::BigInt);
        assert_eq!(primitive_pg_type(PrimitiveType::Short), PgType::SmallInt);
        assert_eq!(primitive_pg_type(PrimitiveType::Byte), PgType::SmallInt);
        assert_eq!(primitive_pg_type(PrimitiveType::Float), PgType::Real);
        assert_eq!(
            primitive_pg_type(PrimitiveType::Double),
            PgType::DoublePrecision
        );
        assert_eq!(primitive_pg_type(PrimitiveType::Boolean), PgType::Boolean);
    }

    #[test]
    fn strip_code_suffix_strips_only_non_empty() {
        assert_eq!(strip_code_suffix("color_code"), "color");
        assert_eq!(strip_code_suffix("color"), "color");
        assert_eq!(strip_code_suffix("_code"), "_code");
    }
}
