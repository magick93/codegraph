use std::collections::HashSet;

use super::traits::GeneratedFile;
use super::GenerationEntry;
use crate::error::Error;

#[derive(Debug, Default)]
pub struct GenerationReport {
    pub files: Vec<GeneratedFile>,
    pub errors: Vec<GenerationError>,
    pub warnings: Vec<GenerationWarning>,
}

#[derive(Debug)]
pub struct GenerationError {
    pub entity: String,
    pub generator: String,
    pub source: Error,
}

#[derive(Debug)]
pub struct GenerationWarning {
    pub entity: String,
    pub generator: String,
    pub check: &'static str,
    pub message: String,
}

impl GenerationReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Check that every entity in the generation order has at least one
    /// corresponding file in the output. Entities present in scaffold/router
    /// files but missing entity-specific files indicate an ingestion or
    /// generation mismatch.
    pub fn validate_consistency(&mut self, generation_order: &[GenerationEntry], suffix: &str) {
        let generated_paths: HashSet<String> = self
            .files
            .iter()
            .map(|f| f.path.to_string_lossy().to_string())
            .collect();

        for entry in generation_order {
            let snake =
                codegraph_naming::to_snake_case(&codegraph_naming::strip_suffix(&entry.schema_title, suffix));
            // Check if any generated file path contains this entity's module name
            // under the entity's domain directory (e.g. "recruiting/candidate")
            let has_entity_file = generated_paths
                .iter()
                .any(|p| p.contains(&format!("/{}/{}", entry.domain, snake)));
            if !has_entity_file {
                self.warnings.push(GenerationWarning {
                    entity: entry.schema_title.clone(),
                    generator: "consistency_check".to_string(),
                    check: "missing_entity_files",
                    message: format!(
                        "Entity '{}' in domain '{}' is in the generation order but no entity-specific files were generated",
                        entry.schema_title, entry.domain
                    ),
                });
            }
        }
    }

    pub fn summary(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Generated {} files | {} errors | {} warnings\n",
            self.files.len(),
            self.errors.len(),
            self.warnings.len()
        ));
        for w in &self.warnings {
            out.push_str(&format!(
                "  WARN {}/{} [{}]: {}\n",
                w.entity, w.generator, w.check, w.message
            ));
        }
        for e in &self.errors {
            out.push_str(&format!(
                "  ERR  {}/{}: {}\n",
                e.entity, e.generator, e.source
            ));
        }
        out
    }
}
