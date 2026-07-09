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
use codegraph_naming;

/// Context for the combined "All" OpenAPI spec (`openapi_all.tera`).
#[derive(Debug, Serialize)]
pub struct OpenApiContext {
    pub title: String,
    pub version: String,
    pub domains: Vec<OpenApiDomain>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenApiDomain {
    pub name: String,
    pub label: String,
    pub tier: String,
    pub entities: Vec<OpenApiEntity>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenApiEntity {
    pub entity_name: String,
    pub module_name: String,
    pub path_segment: String,
    pub tag: String,
    pub has_create: bool,
    pub has_read: bool,
    pub has_update: bool,
    pub has_delete: bool,
    pub has_list: bool,
    /// "root" or "child" — mirrors HandlerContext::role
    pub role: String,
    /// For child entities: the parent's path segment (e.g. "candidates")
    pub parent_path_segment: Option<String>,
    /// For child entities: the parent entity name (e.g. "Candidate")
    pub parent_entity: Option<String>,
    /// For child entities: the domain the parent belongs to
    pub parent_domain: Option<String>,
}

/// Context for a single-domain OpenAPI spec (`openapi_domain.tera`).
#[derive(Debug, Serialize)]
pub struct PerDomainOpenApiContext {
    pub title: String,
    pub version: String,
    pub domain_name: String,
    pub domain_label: String,
    pub entities: Vec<OpenApiEntity>,
}

/// Context for the API catalog handler (`openapi_catalog.tera`).
#[derive(Debug, Serialize)]
pub struct ApiCatalogContext {
    pub domains: Vec<ApiCatalogDomain>,
}

#[derive(Debug, Serialize)]
pub struct ApiCatalogDomain {
    pub name: String,
    pub label: String,
    pub spec_url: String,
    pub docs_url: String,
    pub entity_count: usize,
}

pub struct OpenApiGenerator {
    output_dir: PathBuf,
}

impl OpenApiGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for OpenApiGenerator {
    fn name(&self) -> &str {
        "openapi"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        // Build a per-domain set of entities from the generation order.
        // An entity listed in domains.toml for domain X is only included if the
        // generation order also places it in domain X (the graph may assign it to
        // a different domain based on the schema file's directory).
        let mut entities_per_domain: std::collections::HashMap<
            &str,
            std::collections::HashSet<&str>,
        > = std::collections::HashMap::new();
        for entry in generation_order {
            entities_per_domain
                .entry(entry.domain.as_str())
                .or_default()
                .insert(entry.schema_title.as_str());
        }

        let mut domains = Vec::new();

        for (domain_name, domain_entry) in &config.domains {
            let domain_entities = entities_per_domain
                .get(domain_name.as_str())
                .cloned()
                .unwrap_or_default();
            let mut entities = Vec::new();
            for entity_name in &domain_entry.entities {
                // Skip entities not generated for this specific domain
                if !domain_entities.contains(entity_name.as_str()) {
                    continue;
                }
                if let Ok(Some(schema)) = db.get_schema_in_domain(entity_name, domain_name).await {
                    let entity_cfg = domain_entry.get_entity_config(entity_name);
                    let operations = entity_cfg
                        .and_then(|ec| ec.operations.clone())
                        .unwrap_or_else(|| config.defaults.operations.clone());

                    // Resolve role and parent relationship info from entity config.
                    // These mirror the fields populated by HandlerContext so that
                    // the OpenAPI template can reference nested paths if needed.
                    let role = entity_cfg
                        .and_then(|ec| ec.role.clone())
                        .unwrap_or_else(|| "root".to_string());
                    let parent_name = entity_cfg.and_then(|ec| ec.parent.clone());
                    let (parent_path_segment, parent_entity, parent_domain) =
                        if let Some(ref pname) = parent_name {
                            // Look up parent schema for accurate path segment and domain
                            if let Ok(Some(parent_schema)) = db.get_schema_in_domain(pname, domain_name).await {
                                (
                                    Some(parent_schema.api_path_segment.clone()),
                                    Some(parent_schema.rust_type_name.clone()),
                                    parent_schema.domain.clone(),
                                )
                            } else {
                                (
                                    Some(codegraph_naming::to_kebab_case(pname)),
                                    Some(pname.clone()),
                                    Some(domain_name.clone()),
                                )
                            }
                        } else {
                            (None, None, None)
                        };

                    entities.push(OpenApiEntity {
                        entity_name: schema.rust_type_name.clone(),
                        module_name: schema.pg_table_name.clone(),
                        path_segment: schema.api_path_segment.clone(),
                        tag: schema.rust_type_name.clone(),
                        has_create: operations.contains(&"create".to_string()),
                        has_read: operations.contains(&"read".to_string()),
                        has_update: operations.contains(&"update".to_string()),
                        has_delete: operations.contains(&"delete".to_string()),
                        has_list: operations.contains(&"list".to_string()),
                        role,
                        parent_path_segment,
                        parent_entity,
                        parent_domain,
                    });
                }
            }

            if !entities.is_empty() {
                domains.push(OpenApiDomain {
                    name: domain_name.clone(),
                    label: domain_entry.label.clone(),
                    tier: domain_entry.tier.clone(),
                    entities,
                });
            }
        }

        // Sort domains for deterministic output
        domains.sort_by(|a, b| a.name.cmp(&b.name));

        let openapi_dir = self.output_dir.join("src").join("api").join("openapi");
        let mut files = Vec::new();

        // 1. Shared security modifier
        let empty_ctx: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let security_content = render_template_with_project(tera, "api/openapi_security.tera", &empty_ctx, project)?;
        files.push(GeneratedFile {
            path: openapi_dir.join("security.rs"),
            content: security_content,
        });

        // 2. Combined "All" spec
        let all_ctx = OpenApiContext {
            title: crate::generate::get_project_config().api_title.clone(),
            version: "1.0.0".to_string(),
            domains: domains.clone(),
        };
        let all_content = render_template_with_project(tera, "api/openapi_all.tera", &all_ctx, project)?;
        files.push(GeneratedFile {
            path: openapi_dir.join("all.rs"),
            content: all_content,
        });

        // 3. Per-domain specs
        for domain in &domains {
            let ctx = PerDomainOpenApiContext {
                title: format!("{} — {}", crate::generate::get_project_config().api_title, domain.label),
                version: "1.0.0".to_string(),
                domain_name: domain.name.clone(),
                domain_label: domain.label.clone(),
                entities: domain.entities.clone(),
            };
            let content = render_template_with_project(tera, "api/openapi_domain.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: openapi_dir.join(format!("{}.rs", domain.name)),
                content,
            });
        }

        // 4. API catalog handler
        let catalog_ctx = ApiCatalogContext {
            domains: domains
                .iter()
                .map(|d| ApiCatalogDomain {
                    name: d.name.clone(),
                    label: d.label.clone(),
                    spec_url: format!("/api-docs/{}/openapi.json", d.name),
                    docs_url: format!("/swagger-ui/?urls.primaryName={}", d.label),
                    entity_count: d.entities.len(),
                })
                .collect(),
        };
        let catalog_content = render_template_with_project(tera, "api/openapi_catalog.tera", &catalog_ctx, project)?;
        files.push(GeneratedFile {
            path: openapi_dir.join("catalog.rs"),
            content: catalog_content,
        });

        // 5. Module declaration file (mod.rs)
        // Explicit rather than relying on generate_mod_files_recursive so the
        // generator is self-documenting about what it produces.
        let mut mod_lines = vec![
            "//! Per-domain OpenAPI specifications and API catalog.".to_string(),
            format!("//! Generated by {} . DO NOT EDIT.", crate::generate::get_project_config().generator_name),
            String::new(),
            "pub mod all;".to_string(),
            "pub mod catalog;".to_string(),
            "pub mod security;".to_string(),
        ];
        for domain in &domains {
            mod_lines.push(format!("pub mod {};", domain.name));
        }
        mod_lines.push(String::new());
        files.push(GeneratedFile {
            path: openapi_dir.join("mod.rs"),
            content: mod_lines.join("\n"),
        });

        Ok(files)
    }
}
