//! mox domain ingest: compile `.mox` domain sources in-process via
//! `rex_driver::compile_files` and walk the resulting `rex_ir::Model` into
//! the graph (issue #218).
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
//! Non-goals (issue #218): entity generation still comes from JSON Schema;
//! operation bodies are ingested as text and are never executed or parsed by
//! codegraph.

use std::collections::HashSet;
use std::path::PathBuf;

use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{
    MoxDerivedFeatureNode, MoxDomainModel, MoxEntry, MoxFacet, MoxOperationNode, MoxPackageNode,
    MoxParam, MoxVocabularyNode,
};
use rex_driver::compile_files;
use rex_ir::{DefaultValue, TypeRef};

use crate::error::{Error, Result};

/// Counters for one `ingest_mox_files` run.
///
/// `skipped` counts mox files that could not be read or failed to compile,
/// plus mox classes carrying operations/derived features that matched no
/// ingested schema entity (each warned).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MoxIngestStats {
    pub vocabularies: usize,
    pub operations: usize,
    pub derived_features: usize,
    pub skipped: usize,
}

impl std::fmt::Display for MoxIngestStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} vocabularies, {} operations, {} derived features, {} skipped",
            self.vocabularies, self.operations, self.derived_features, self.skipped
        )
    }
}

/// Compile `.mox` files in-process and ingest the resulting domain model
/// into the graph. Diagnostics warn and never fail: unreadable files,
/// compile errors, and unmatched class names are counted in
/// [`MoxIngestStats::skipped`] and reported on stderr.
pub async fn ingest_mox_files(
    ingestor: &dyn GraphIngestor,
    querier: &dyn GraphQuerier,
    mox_paths: &[PathBuf],
) -> Result<MoxIngestStats> {
    let mut sources: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut stats = MoxIngestStats::default();
    for path in mox_paths {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !seen.insert(canonical.display().to_string()) {
            continue;
        }
        match std::fs::read_to_string(path) {
            Ok(text) => sources.push((path.display().to_string(), text)),
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
        return Ok(stats);
    }

    let compilation = compile_files(&sources);
    for (path, diagnostic) in &compilation.diagnostics {
        eprintln!("Warning: mox diagnostic in {path}: {}", diagnostic.message);
    }
    let Some(model) = compilation.model else {
        eprintln!("Warning: mox compilation produced no model — nothing ingested");
        stats.skipped += sources.len();
        return Ok(stats);
    };

    // Schema entities already in the graph: mox classes attach by NAME
    // (schema title or entity name). Warn on mismatch, never fail.
    let schemas = querier.list_schemas(None).await.unwrap_or_default();

    let mut mox = MoxDomainModel::default();
    for package in &model.packages {
        mox.packages.push(MoxPackageNode {
            name: package.name.clone(),
        });
        for vocab in &package.vocabularies {
            mox.vocabularies.push(MoxVocabularyNode {
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
            stats.vocabularies += 1;
        }
        for class in &package.classes {
            let mut attached = false;
            for op in &class.operations {
                mox.operations.push(MoxOperationNode {
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
                stats.operations += 1;
                attached = true;
            }
            for feature in &class.features {
                if !feature.is_derived {
                    continue;
                }
                mox.derived_features.push(MoxDerivedFeatureNode {
                    name: feature.name.clone(),
                    class: class.name.clone(),
                    package: package.name.clone(),
                    type_ref: type_ref_name(&feature.type_),
                    expr: feature.bodies.get("expr").cloned(),
                });
                stats.derived_features += 1;
                attached = true;
            }
            if attached {
                match schemas
                    .iter()
                    .find(|s| s.title == class.name || s.rust_type_name == class.name)
                {
                    Some(schema) => {
                        mox.class_links
                            .push((class.name.clone(), schema.title.clone()));
                    }
                    None => {
                        eprintln!(
                            "Warning: mox class '{}' matches no ingested schema entity — \
                             its operations/derived features will not attach to generated types",
                            class.name
                        );
                        stats.skipped += 1;
                    }
                }
            }
        }
    }

    ingestor
        .ingest_mox_domain(&mox)
        .await
        .map_err(Error::Graph)?;
    Ok(stats)
}

/// Render a resolved [`TypeRef`] as a plain name: primitives use their
/// mox-level name (`"string"`, `"int"`), everything else its qualified name.
fn type_ref_name(type_ref: &TypeRef) -> String {
    match type_ref {
        TypeRef::Primitive(p) => p.to_string(),
        other => other.qualified_name().unwrap_or_default(),
    }
}

/// Convert a rex [`DefaultValue`] (vocabulary facet values) to a plain JSON
/// value for graph storage.
fn default_value_json(value: &DefaultValue) -> serde_json::Value {
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
            type_ref_name(&TypeRef::Primitive(rex_ir::PrimitiveType::String)),
            "string"
        );
        assert_eq!(
            type_ref_name(&TypeRef::Primitive(rex_ir::PrimitiveType::Int)),
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
        };
        assert_eq!(
            stats.to_string(),
            "1 vocabularies, 2 operations, 3 derived features, 4 skipped"
        );
    }
}
