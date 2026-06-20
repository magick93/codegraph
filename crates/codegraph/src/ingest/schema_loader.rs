use std::collections::HashMap;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::error::{Error, Result};

/// A loaded JSON schema entry with metadata.
#[derive(Debug, Clone)]
pub struct SchemaEntry {
    pub schema: serde_json::Value,
    pub rel_path: String,
    pub full_path: String,
    pub stem: String,
    pub domain: String,
}

/// Loads all JSON schemas from a directory tree and provides $ref resolution.
///
/// Directory structure expected: `<schema_dir>/<domain>/json/*.json`
/// with optional `codelist/` subdirectory.
pub struct SchemaLoader {
    /// Cache keyed by multiple aliases: $id, rel_path, stem, full_path
    cache: HashMap<String, SchemaEntry>,
    /// Only top-level schema URIs (not inline defs)
    top_level_uris: Vec<String>,
}

impl SchemaLoader {
    /// Walk a schema directory and load all JSON files.
    pub fn load(schema_dir: &Path) -> Result<Self> {
        let mut cache = HashMap::new();
        let mut top_level_uris = Vec::new();
        let schema_dir = schema_dir.canonicalize().map_err(|e| {
            Error::Config(format!(
                "Cannot canonicalize schema dir {:?}: {}",
                schema_dir, e
            ))
        })?;

        let mut walk_entries: Vec<_> = WalkDir::new(&schema_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                if !e.file_type().is_file() {
                    return false;
                }
                if !e
                    .path()
                    .extension()
                    .map(|ext| ext == "json")
                    .unwrap_or(false)
                {
                    return false;
                }
                // Skip sample, meta, and search directories — these are not schema
                // definitions and can collide with real entity schemas (e.g.
                // screening/json/samples/Position.json overwrites common/PositionType).
                let path_str = e.path().to_string_lossy();
                !path_str.contains("/samples/")
                    && !path_str.contains("/meta/")
                    && !path_str.contains("/search/")
            })
            .collect();
        walk_entries.sort_by(|a, b| a.path().cmp(b.path()));

        for entry in walk_entries {
            let full_path = entry.path().canonicalize()?;
            let rel_path = full_path
                .strip_prefix(&schema_dir)
                .unwrap_or(full_path.as_path())
                .to_string_lossy()
                .replace('\\', "/");

            // Extract domain from path: the segment immediately before `/json/`.
            // Handles both `<domain>/json/...` and `<version>/<domain>/json/...`.
            let domain = extract_domain_from_path(&rel_path);

            let stem = full_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            let content = std::fs::read_to_string(&full_path)?;
            let schema: serde_json::Value = serde_json::from_str(&content)?;

            let entry = SchemaEntry {
                schema: schema.clone(),
                rel_path: rel_path.clone(),
                full_path: full_path.to_string_lossy().to_string(),
                stem: stem.clone(),
                domain,
            };

            // Index under multiple keys for flexible lookup
            let schema_id = schema
                .get("$id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            top_level_uris.push(rel_path.clone());
            cache.insert(rel_path.clone(), entry.clone());
            cache.insert(stem.clone(), entry.clone());
            if !schema_id.is_empty() {
                cache.entry(schema_id).or_insert(entry.clone());
            }

            // Also extract inline definitions
            Self::extract_inline_defs(&schema, &rel_path, &entry.domain, &mut cache);
        }

        top_level_uris.sort();

        Ok(Self {
            cache,
            top_level_uris,
        })
    }

    /// Extract `#/definitions/*` and `#/$defs/*` entries.
    fn extract_inline_defs(
        schema: &serde_json::Value,
        parent_uri: &str,
        domain: &str,
        cache: &mut HashMap<String, SchemaEntry>,
    ) {
        for key in &["definitions", "$defs"] {
            if let Some(serde_json::Value::Object(defs)) = schema.get(*key) {
                for (def_name, def_schema) in defs {
                    let def_uri = format!("{}#/{}/{}", parent_uri, key, def_name);
                    let stem = def_schema
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or(def_name)
                        .to_string();

                    let entry = SchemaEntry {
                        schema: def_schema.clone(),
                        rel_path: def_uri.clone(),
                        full_path: String::new(),
                        stem: stem.clone(),
                        domain: domain.to_string(),
                    };

                    cache.insert(def_uri, entry.clone());
                    // Also index by stem for #/definitions/QualificationType lookups
                    cache.entry(stem).or_insert(entry);
                }
            }
        }
    }

    /// Resolve a $ref string relative to a base URI.
    pub fn resolve_ref<'a>(
        &'a self,
        ref_str: &str,
        base_uri: &str,
    ) -> Result<(String, &'a SchemaEntry)> {
        // Handle inline refs: #/definitions/Foo
        if ref_str.starts_with("#/") {
            let def_uri = format!("{}{}", base_uri, ref_str);
            if let Some(entry) = self.cache.get(&def_uri) {
                return Ok((def_uri, entry));
            }
            // Try just the def name
            let def_name = ref_str.rsplit('/').next().unwrap_or(ref_str);
            if let Some(entry) = self.cache.get(def_name) {
                return Ok((def_name.to_string(), entry));
            }
            return Err(Error::RefResolution(format!(
                "Cannot resolve inline ref '{}' from '{}'",
                ref_str, base_uri
            )));
        }

        // Relative path resolution
        let base_dir = Path::new(base_uri)
            .parent()
            .unwrap_or(Path::new(""))
            .to_path_buf();
        let resolved = normalize_path(&base_dir.join(ref_str));
        let resolved_str = resolved.to_string_lossy().replace('\\', "/");

        if let Some(entry) = self.cache.get(&resolved_str) {
            return Ok((resolved_str, entry));
        }

        // Try just the stem
        let stem = Path::new(ref_str)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if let Some(entry) = self.cache.get(&stem) {
            return Ok((stem, entry));
        }

        // Try the ref_str as-is
        if let Some(entry) = self.cache.get(ref_str) {
            return Ok((ref_str.to_string(), entry));
        }

        Err(Error::RefResolution(format!(
            "Cannot resolve ref '{}' from base '{}'",
            ref_str, base_uri
        )))
    }

    /// Get a schema entry by URI.
    pub fn get(&self, uri: &str) -> Option<&SchemaEntry> {
        self.cache.get(uri)
    }

    /// Number of distinct schemas loaded (including inline defs).
    pub fn schema_count(&self) -> usize {
        // Count unique entries by rel_path to avoid alias duplicates
        let unique: std::collections::HashSet<&str> =
            self.cache.values().map(|e| e.rel_path.as_str()).collect();
        unique.len()
    }

    /// Iterate over top-level schema URIs (not inline defs).
    pub fn iter_top_level(&self) -> impl Iterator<Item = (&str, &SchemaEntry)> {
        self.top_level_uris
            .iter()
            .filter_map(move |uri| self.cache.get(uri).map(|e| (uri.as_str(), e)))
    }

    /// Iterate all unique schema entries (top-level + inline definitions),
    /// deduplicating by stem. Each entry appears once even though the cache
    /// indexes it under multiple aliases (stem, rel_path, uri).
    pub fn iter_all_unique(&self) -> Vec<&SchemaEntry> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for entry in self.cache.values() {
            let stem = &entry.stem;
            if seen.insert(stem.clone()) {
                result.push(entry);
            }
        }
        result
    }
}

/// Extract the domain name from a relative schema path.
///
/// Looks for the path segment immediately before `/json/` in the path.
/// For `recruiting/json/CandidateType.json` → `recruiting`.
/// For `4.5RC1-build-2025-05-06/recruiting/json/CandidateType.json` → `recruiting`.
/// Falls back to the first segment if no `/json/` marker is found.
fn extract_domain_from_path(rel_path: &str) -> String {
    let segments: Vec<&str> = rel_path.split('/').collect();
    // Find the segment right before "json"
    for (i, seg) in segments.iter().enumerate() {
        if *seg == "json" && i > 0 {
            return segments[i - 1].to_string();
        }
    }
    // Fallback: first segment
    segments.first().unwrap_or(&"unknown").to_string()
}

/// Normalize a path by resolving `..` and `.` components without filesystem access.
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            other => components.push(other),
        }
    }
    components.iter().collect()
}
