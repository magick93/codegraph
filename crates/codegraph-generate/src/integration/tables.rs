use crate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_ext_points::{ExtensionPointsConfig, IntegrationManifestSet};
use serde::Serialize;

use crate::GenerationEntry;
use crate::error::Result;
use crate::render_template_with_project;
use crate::traits::{GeneratedFile, GlobalGenerator, GlobalGeneratorKind};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
struct TablesContext {
    extension_points: Vec<ExtensionPointCtx>,
}

#[derive(Debug, Serialize)]
struct ExtensionPointCtx {
    id: String,
    name: String,
    description: String,
    cardinality: String,
    entities: Vec<String>,
    directions: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RlsContext {
    tenant_tables: Vec<String>,
}

/// Catalog-seed context (issue #552): one row per manifest entry plus the
/// optional demo installations. `worker_url` arrives ALREADY templated —
/// `{app_name}` substituted with the project app name — so no unresolved
/// placeholder can reach SQL.
#[derive(Debug, Serialize)]
struct CatalogSeedContext {
    catalog: Vec<CatalogRowCtx>,
    installations: Vec<InstallationCtx>,
}

#[derive(Debug, Serialize)]
struct CatalogRowCtx {
    id: String,
    name: String,
    vendor: String,
    version: String,
    description: String,
    tier: String,
    categories: Vec<String>,
    markets: Vec<String>,
    extension_point: String,
    worker_url: Option<String>,
    enabled: bool,
    manifest_hash: String,
}

#[derive(Debug, Serialize)]
struct InstallationCtx {
    catalog_id: String,
    organization_id: String,
    extension_point: String,
    status: String,
}

pub struct IntegrationTablesGenerator {
    output_dir: PathBuf,
    ext_config: ExtensionPointsConfig,
    manifests: Option<IntegrationManifestSet>,
}

impl IntegrationTablesGenerator {
    pub fn new(output_dir: &Path, ext_config: ExtensionPointsConfig) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            ext_config,
            manifests: None,
        }
    }

    /// Catalog manifests (issue #552). When present, `generate` emits
    /// `0010_integration_catalog_seed.sql` after the extension-point seed.
    /// Absent ⇒ no extra file (presence gating / byte-identity). The set
    /// is cloned into the generator (same treatment as `ext_config`) —
    /// global generators outlive the borrowed context.
    pub fn with_manifests(mut self, manifests: Option<&IntegrationManifestSet>) -> Self {
        self.manifests = manifests.cloned();
        self
    }

    fn build_context(&self) -> TablesContext {
        let mut points: Vec<ExtensionPointCtx> = self
            .ext_config
            .points
            .iter()
            .map(|(id, def)| ExtensionPointCtx {
                id: id.clone(),
                name: def.name.clone(),
                description: def.description.clone(),
                cardinality: format!("{:?}", def.cardinality).to_lowercase(),
                entities: def.entities.clone(),
                directions: def
                    .directions
                    .iter()
                    .map(|d| format!("{:?}", d).to_lowercase())
                    .collect(),
            })
            .collect();
        points.sort_by(|a, b| a.id.cmp(&b.id));
        TablesContext {
            extension_points: points,
        }
    }

    fn build_seed_context(&self, app_name: &str) -> CatalogSeedContext {
        let manifests = self
            .manifests
            .as_ref()
            .expect("seed context is only built when manifests are present");
        let catalog = manifests
            .catalog
            .iter()
            .map(|row| CatalogRowCtx {
                id: row.id.clone(),
                name: row.name.clone(),
                vendor: row.vendor.clone(),
                version: row.version.clone(),
                description: row.description.clone(),
                tier: row.tier.clone(),
                categories: row.categories.clone(),
                markets: row.markets.clone(),
                extension_point: row.extension_point.clone(),
                worker_url: row
                    .worker_url
                    .as_deref()
                    .map(|template| codegraph_ext_points::render_worker_url(template, app_name)),
                enabled: row.enabled,
                manifest_hash: row.manifest_hash.clone(),
            })
            .collect();
        let by_id: std::collections::HashMap<&str, &str> = manifests
            .catalog
            .iter()
            .map(|row| (row.id.as_str(), row.extension_point.as_str()))
            .collect();
        let installations = manifests
            .installations
            .iter()
            .map(|inst| InstallationCtx {
                catalog_id: inst.catalog_id.clone(),
                organization_id: inst.organization_id.clone(),
                // Validated against the catalog set pre-generation
                // (pipeline), so the lookup always resolves.
                extension_point: by_id
                    .get(inst.catalog_id.as_str())
                    .map(|ep| (*ep).to_string())
                    .unwrap_or_default(),
                status: inst.status.clone(),
            })
            .collect();
        CatalogSeedContext {
            catalog,
            installations,
        }
    }
}

#[async_trait]
impl GlobalGenerator for IntegrationTablesGenerator {
    fn kind(&self) -> GlobalGeneratorKind {
        GlobalGeneratorKind::IntegrationTables
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let ctx = self.build_context();
        let mut files = Vec::new();

        let ddl = render_template_with_project(tera, "integration/tables.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: crate::db::migrations_root(&self.output_dir).join("0007_integration_tables.sql"),
            content: ddl,
        });

        let rls_ctx = RlsContext {
            tenant_tables: vec![
                "installations".into(),
                "installation_settings".into(),
                "audit_log".into(),
            ],
        };
        let rls = render_template_with_project(tera, "integration/rls.tera", &rls_ctx, project)?;
        files.push(GeneratedFile {
            path: crate::db::migrations_root(&self.output_dir).join("0008_integration_rls.sql"),
            content: rls,
        });

        let seed = render_template_with_project(tera, "integration/seed.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: crate::db::migrations_root(&self.output_dir).join("0009_integration_seed.sql"),
            content: seed,
        });

        // Catalog seed (issue #552): only when manifests are provided —
        // absent manifests keep the migration set byte-identical.
        if self.manifests.is_some() {
            let seed_ctx = self.build_seed_context(&project.identity.app_name);
            let catalog_seed = render_template_with_project(
                tera,
                "integration/catalog_seed.tera",
                &seed_ctx,
                project,
            )?;
            files.push(GeneratedFile {
                path: crate::db::migrations_root(&self.output_dir)
                    .join("0010_integration_catalog_seed.sql"),
                content: catalog_seed,
            });
        }

        Ok(files)
    }
}
