//! `ops` global generator — emits a `codegraph-ops.toml` manifest plus a thin
//! `testkit/` crate into the generated output.
//!
//! The manifest seeds the [`codegraph_ops`](codegraph-ops) test & deploy harness
//! with project defaults (ports, database targets, capabilities). Consumers
//! extend it via `[[hooks]]` and `[[extensions]]` entries. The testkit crate is
//! a thin binary that calls into `codegraph_ops::cli` and exists so the harness
//! can be driven with `cargo run --manifest-path testkit/Cargo.toml`.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use crate::generate::ProjectConfig;
use codegraph_config::ops_manifest::{
    OpsCapabilities, OpsDatabase, OpsDbTarget, OpsManifest, OpsServers, OpsSmoke,
};
use codegraph_config::DomainConfig;

/// Derive the API path segment for an entity schema title
/// (`CandidateType` → `candidate`).
fn entity_segment(schema_title: &str) -> String {
    let stripped = schema_title.strip_suffix("Type").unwrap_or(schema_title);
    heck::ToSnakeCase::to_snake_case(stripped)
}

/// Context for the testkit Cargo.toml template.
#[derive(Debug, Serialize)]
pub struct TestkitContext {
    /// Relative path from `<output>/testkit` to the codegraph-ops crate.
    pub codegraph_ops_path: String,
}

/// Default plain-Postgres target used by the harness `api` suite.
fn default_api_db_target(port: u16) -> OpsDbTarget {
    OpsDbTarget {
        host: "localhost".into(),
        port,
        user: "postgres".into(),
        password: "postgres".into(),
        database: "postgres".into(),
        reset_sql: None,
        seed_sql: None,
    }
}

pub struct OpsManifestGenerator {
    output_dir: PathBuf,
    has_cli: bool,
    has_ui: bool,
    has_admin_cli: bool,
    has_grpc: bool,
}

impl OpsManifestGenerator {
    pub fn new(
        output_dir: &Path,
        has_cli: bool,
        has_ui: bool,
        has_admin_cli: bool,
        has_grpc: bool,
    ) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            has_cli,
            has_ui,
            has_admin_cli,
            has_grpc,
        }
    }

    /// Resolve the codegraph-ops crate location as a path relative to the
    /// testkit crate directory (where the generated Cargo.toml lives).
    ///
    /// Mirrors `scaffold/gen.rs::resolve_path` (pathdiff against the output
    /// dir). The codegraph-ops crate is a sibling of this crate
    /// (`crates/codegraph-ops`) inside the codegraph workspace, so the
    /// compiled-in `CARGO_MANIFEST_DIR` locates it regardless of the output
    /// directory; consumers can override by relocating the testkit crate or
    /// switching the dependency to a git/registry reference.
    fn resolve_ops_path(&self, abs_output: &Path) -> String {
        let testkit_dir = abs_output.join("testkit");
        let ops_abs = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.join("codegraph-ops"))
            .unwrap_or_else(|| PathBuf::from("codegraph-ops"));
        pathdiff::diff_paths(&ops_abs, &testkit_dir)
            .unwrap_or_else(|| PathBuf::from("codegraph-ops"))
            .to_string_lossy()
            .into_owned()
    }
}

#[async_trait]
impl GlobalGenerator for OpsManifestGenerator {
    fn name(&self) -> &str {
        "ops"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let smoke = generation_order.first().map(|entry| OpsSmoke {
            entity: format!("{}/{}", entry.domain, entity_segment(&entry.schema_title)),
            create_body: "{}".to_string(),
        });
        let manifest = OpsManifest {
            app_name: project.app_name.clone(),
            graph_binary: None,
            schemas_dir: None,
            classifier: None,
            domain_config: None,
            profile: None,
            // The manifest sits at the generated app root, so "." makes the
            // harness resolve app_dir == the directory containing the manifest.
            output_dir: PathBuf::from("."),
            ui_dir: None,
            smoke,
            api_version: _config.defaults.api_version.clone(),
            servers: OpsServers::default(),
            database: OpsDatabase {
                api: default_api_db_target(5432),
                e2e: Some(default_api_db_target(54322)),
                e2e_app: None,
            },
            supabase: None,
            capabilities: OpsCapabilities {
                has_cli: self.has_cli,
                has_ui: self.has_ui,
                has_admin_cli: self.has_admin_cli,
                has_grpc: self.has_grpc,
                database_target: project.database_target.clone(),
                // persistence_provider defaults to "sea_orm" in OpsCapabilities
                // (not yet on ProjectConfig in all codegraph versions).
                persistence_provider: "sea_orm".to_string(),
            },
            hurl: None,
            hooks: Vec::new(),
            extensions: Vec::new(),
        };

        let manifest_toml = toml::to_string_pretty(&manifest)
            .map_err(|e| Error::Template(format!("serialize ops manifest: {e}")))?;
        let manifest_content = format!(
            "# Generated by {}. DO NOT EDIT — extend via hooks/extensions.\n\n{}",
            project.generator_name, manifest_toml
        );

        // Compute the absolute output dir (mirrors scaffold/gen.rs).
        let abs_output = if self.output_dir.is_absolute() {
            self.output_dir.clone()
        } else {
            std::env::current_dir()
                .unwrap_or_default()
                .join(&self.output_dir)
        };
        let codegraph_ops_path = self.resolve_ops_path(&abs_output);
        let ctx = TestkitContext { codegraph_ops_path };

        let cargo_content =
            render_template_with_project(tera, "ops/testkit_cargo.tera", &ctx, project)?;
        let main_content =
            render_template_with_project(tera, "ops/testkit_main.tera", &ctx, project)?;

        tracing::info!(
            output_dir = %self.output_dir.display(),
            "ops: wrote codegraph-ops.toml manifest + testkit crate"
        );

        Ok(vec![
            GeneratedFile {
                path: self.output_dir.join("codegraph-ops.toml"),
                content: manifest_content,
            },
            GeneratedFile {
                path: self.output_dir.join("testkit").join("Cargo.toml"),
                content: cargo_content,
            },
            GeneratedFile {
                path: self.output_dir.join("testkit").join("src").join("main.rs"),
                content: main_content,
            },
        ])
    }
}
