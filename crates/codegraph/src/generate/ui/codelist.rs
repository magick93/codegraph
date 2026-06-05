//! UI codelist data generator.
//!
//! Reads HR Open Standards codelist JSON schemas from
//! `{schema_base_dir}/common/json/codelist/` and produces TypeScript data files
//! under `{output_dir}/ui/src/lib/codelists/`.
//!
//! Each codelist schema becomes an individual `<Title>.ts` file, and a barrel
//! `index.ts` re-exports them all.

use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;

/// Template context for a single codelist.
#[derive(Debug, Serialize)]
pub struct CodelistContext {
    pub title: String,
    pub description: String,
    pub options: Vec<CodelistOption>,
}

/// A single option entry in a codelist.
#[derive(Debug, Serialize)]
pub struct CodelistOption {
    pub value: String,
    pub label: String,
}

/// Raw JSON schema shape for a codelist file.
#[derive(Debug, serde::Deserialize)]
struct RawCodelistSchema {
    title: Option<String>,
    description: Option<String>,
    #[serde(rename = "enum", default)]
    enum_values: Vec<String>,
    #[serde(rename = "enumNames", default)]
    enum_names: Vec<String>,
}

pub struct UiCodelistGenerator {
    output_dir: PathBuf,
    schema_base_dir: PathBuf,
}

impl UiCodelistGenerator {
    pub fn new(output_dir: &Path, schema_base_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            schema_base_dir: schema_base_dir.to_path_buf(),
        }
    }

    /// Return the `{schema_base_dir}/common/json/codelist/` directory.
    fn codelist_dir(&self) -> PathBuf {
        self.schema_base_dir
            .join("common")
            .join("json")
            .join("codelist")
    }

    /// Output directory for generated TypeScript files.
    fn ts_output_dir(&self) -> PathBuf {
        self.output_dir
            .join("ui")
            .join("src")
            .join("lib")
            .join("codelists")
    }
}

#[async_trait]
impl GlobalGenerator for UiCodelistGenerator {
    fn name(&self) -> &str {
        "ui-codelist"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let codelist_dir = self.codelist_dir();

        if !codelist_dir.exists() {
            tracing::warn!(
                path = %codelist_dir.display(),
                "ui-codelist: codelist directory not found — skipping"
            );
            return Ok(vec![]);
        }

        // Collect and sort JSON files alphabetically for deterministic output.
        let mut json_files: Vec<PathBuf> = std::fs::read_dir(&codelist_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect();
        json_files.sort();

        let mut files = Vec::new();
        let mut titles: Vec<String> = Vec::new();

        for path in &json_files {
            let raw = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        file = %path.display(),
                        error = %e,
                        "ui-codelist: failed to read file — skipping"
                    );
                    continue;
                }
            };

            let schema: RawCodelistSchema = match serde_json::from_str(&raw) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        file = %path.display(),
                        error = %e,
                        "ui-codelist: failed to parse JSON — skipping"
                    );
                    continue;
                }
            };

            let title = match schema.title {
                Some(t) if !t.is_empty() => t,
                _ => {
                    tracing::warn!(
                        file = %path.display(),
                        "ui-codelist: missing title — skipping"
                    );
                    continue;
                }
            };

            let description = schema.description.unwrap_or_default();

            // Build options: use enumNames as labels when available, else fall back to values.
            let options: Vec<CodelistOption> = schema
                .enum_values
                .iter()
                .enumerate()
                .map(|(i, value)| {
                    let label = schema
                        .enum_names
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| value.clone());
                    CodelistOption {
                        value: value.clone(),
                        label,
                    }
                })
                .collect();

            let ctx = CodelistContext {
                title: title.clone(),
                description,
                options,
            };

            let content = render_template_with_project(tera, "ui/codelist_data.tera", &ctx, project)?;

            files.push(GeneratedFile {
                path: self.ts_output_dir().join(format!("{}.ts", title)),
                content,
            });

            titles.push(title);
        }

        tracing::info!(
            count = titles.len(),
            "ui-codelist: generated {} TypeScript codelist files",
            titles.len()
        );

        // Generate barrel index.ts re-exporting all codelists.
        if !titles.is_empty() {
            let mut index_lines: Vec<String> = titles
                .iter()
                .map(|t| format!("export {{ {} }} from './{}.js';", t, t))
                .collect();
            index_lines.sort();
            let index_content = index_lines.join("\n") + "\n";

            files.push(GeneratedFile {
                path: self.ts_output_dir().join("index.ts"),
                content: index_content,
            });
        }

        Ok(files)
    }
}
