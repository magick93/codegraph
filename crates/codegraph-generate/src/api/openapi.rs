use crate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::GenerationEntry;
use crate::error::Result;
use crate::render_template_with_project;
use crate::traits::{GeneratedFile, GlobalGenerator, GlobalGeneratorKind};
use codegraph_config::DomainConfig;
use codegraph_naming;

use super::api_model::{resolve_entity_operations, resolve_path_segment};

/// Context for the combined "All" OpenAPI spec (`openapi_all.tera`).
#[derive(Debug, Serialize)]
pub struct OpenApiContext {
    pub title: String,
    pub version: String,
    pub domains: Vec<OpenApiDomain>,
    /// Modeled DTO boundaries (DD3, issue #667) — registered as component
    /// schemas beside the entity DTOs. (NO skip_serializing_if — the Tera
    /// context needs the empty vec.)
    pub dto_boundaries: Vec<OpenApiBoundary>,
}

/// One modeled DTO boundary registered as a utoipa component schema.
#[derive(Debug, Clone, Serialize)]
pub struct OpenApiBoundary {
    /// The boundary name (the generated Rust type).
    pub name: String,
    /// The `.ddd` module's snake_case file stem in `src/dto/`.
    pub module: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenApiDomain {
    pub name: String,
    pub label: String,
    pub tier: String,
    pub entities: Vec<OpenApiEntity>,
    /// HTTP-exposed modeled functions (issue #337 B1) — registered as
    /// utoipa paths + schemas from `functions_api.rs`. Empty unless a
    /// `[functions]` binding sets `exposure = "http"` (byte-identity).
    /// (NO skip_serializing_if — the Tera context needs the empty vec.)
    pub functions: Vec<OpenApiFunction>,
    /// Service-op action endpoints (issue #590). Child entities register
    /// too — their utoipa paths nest under the parent. (NO
    /// skip_serializing_if — the Tera context needs the empty vec.)
    pub service_ops: Vec<OpenApiServiceOp>,
}

/// One HTTP-exposed function's OpenAPI registration context.
#[derive(Debug, Clone, Serialize)]
pub struct OpenApiFunction {
    /// The handler fn name (snake_case) in `functions_api.rs`.
    pub fn_name: String,
    /// The `{Fn}Request` schema struct.
    pub request: String,
    /// The `{Fn}Response` schema struct.
    pub response: String,
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
    /// Media route fields that emit an app-plane handler module for this
    /// entity — `src/api/{domain}/media_{module_name}_{field}.rs` (issue
    /// #704). Resolved through the router's own `media_route_surface`, so
    /// the documented paths and the mounted ones cannot drift. Empty when
    /// the domain declares no `[media]`, or is `client-direct` (no
    /// app-plane route). (NO skip_serializing_if — the Tera context needs
    /// the empty vec.)
    pub media_fields: Vec<String>,
    /// The machine read-model route (issue #817): the entity binds an
    /// artifact machine, so the `get_machine` path + its schemas register
    /// in this document. Resolved through the router's own helper, so the
    /// documented paths and the mounted ones cannot drift.
    pub has_machine: bool,
    /// `upload_mode = "presigned"` ⇒ the media module exposes `presign`;
    /// otherwise (`through-app`) it exposes the multipart `upload`. Both
    /// always expose `display`/`remove`. Resolved through
    /// `media_upload_mode`, the helper the router template branches on.
    pub media_mode_presigned: bool,
}

/// Context for a single-domain OpenAPI spec (`openapi_domain.tera`).
#[derive(Debug, Serialize)]
pub struct PerDomainOpenApiContext {
    pub title: String,
    pub version: String,
    pub domain_name: String,
    pub domain_label: String,
    pub entities: Vec<OpenApiEntity>,
    /// Modeled DTO boundaries (DD3, issue #667) — documented in every
    /// domain document because a boundary is application-scoped, not
    /// domain-scoped. (NO skip_serializing_if.)
    pub dto_boundaries: Vec<OpenApiBoundary>,
    /// HTTP-exposed modeled functions (issue #337 B1). Empty unless a
    /// `[functions]` binding opts in.
    /// (NO skip_serializing_if — the Tera context needs the empty vec.)
    pub functions: Vec<OpenApiFunction>,
    /// Service-op action endpoints (issue #590). Child entities register
    /// too — their utoipa paths nest under the parent. (NO
    /// skip_serializing_if — the Tera context needs the empty vec.)
    pub service_ops: Vec<OpenApiServiceOp>,
}

/// One service-op action endpoint registered in the per-domain OpenAPI
/// spec (paths only — the request body is empty; identity rides the path).
#[derive(Debug, Clone, Serialize)]
pub struct OpenApiServiceOp {
    /// Handler fn name (`service_op_{snake}`).
    pub fn_name: String,
    /// The owning entity's module (`pg_table_name`) — the handler module.
    pub module: String,
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
    boundaries: crate::ddd::dto_boundaries::BoundaryIndexCache,
}

/// The media route surface an entity DOCUMENTS (issue #704): the publisher's
/// counterpart to `router.tera`'s mount. Both sides read the same two plan
/// helpers — `media_route_surface` (the mode-gated field list) and
/// `media_upload_mode` (presign vs multipart) — so the documented paths and
/// the served routes cannot drift. The run path already hard-failed a broken
/// media config in the driver's pre-generation pass, and the router treats a
/// plan error the same way (skip the surface), so this follows suit.
async fn resolve_media_surface(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain_name: &str,
    entity_title: &str,
) -> (Vec<String>, bool) {
    let surface = crate::media::plan::media_route_surface(db, config, domain_name, entity_title)
        .await
        .unwrap_or_default();
    let presigned = crate::media::plan::media_upload_mode(config, domain_name)
        .is_some_and(|mode| mode == codegraph_core::types::UploadMode::Presigned);
    (surface.fields, presigned)
}

impl OpenApiGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            boundaries: crate::ddd::dto_boundaries::BoundaryIndexCache::new(),
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for OpenApiGenerator {
    fn kind(&self) -> GlobalGeneratorKind {
        GlobalGeneratorKind::OpenApi
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

        // HTTP-exposed functions per domain (issue #337 B1). Empty unless a
        // `[functions]` binding opts in — the standing byte-identity default.
        let mut functions_per_domain: std::collections::BTreeMap<String, Vec<OpenApiFunction>> =
            Default::default();
        // Service-op action endpoints per domain (issue #590). Iterated in
        // GENERATION ORDER — not the config `entities` lists, which are
        // empty in mox-first runs (author-declarative mox), silently
        // registering no service ops there. Child entities register too
        // (their utoipa paths nest under the parent). Lowering refusals
        // propagate as config errors.
        let mut service_ops_per_domain: std::collections::BTreeMap<String, Vec<OpenApiServiceOp>> =
            Default::default();
        for entry in generation_order {
            if !config.domains.contains_key(entry.domain.as_str()) {
                continue;
            }
            for op in crate::ddd::service_ops::routes_for_entity(
                db,
                config,
                &entry.domain,
                &entry.schema_title,
                &crate::ddd::service_ops::BodyRenderContext::from_project(project)
                    .with_boundary_cache(self.boundaries.clone()),
            )
            .await
            .map_err(codegraph_core::error::Error::Config)?
            {
                let module = db
                    .get_schema_in_domain(&entry.schema_title, &entry.domain)
                    .await
                    .ok()
                    .flatten()
                    .map(|schema| schema.pg_table_name)
                    .unwrap_or_default();
                service_ops_per_domain
                    .entry(entry.domain.clone())
                    .or_default()
                    .push(OpenApiServiceOp {
                        fn_name: op.fn_name,
                        module,
                    });
            }
        }

        for bound in crate::ddd::functions_api::bound_functions(db, config).await? {
            if bound.is_http()
                && let Some(domain) = &bound.node.domain
            {
                functions_per_domain
                    .entry(domain.clone())
                    .or_default()
                    .push(OpenApiFunction {
                        fn_name: crate::ddd::functions_api::fn_snake(&bound.node.name),
                        request: crate::ddd::functions_api::request_struct(&bound.node.name),
                        response: crate::ddd::functions_api::response_struct(&bound.node.name),
                    });
            }
        }

        for (domain_name, domain_entry) in &config.domains {
            let domain_entities = entities_per_domain
                .get(domain_name.as_str())
                .cloned()
                .unwrap_or_default();
            let mut entities = Vec::new();
            let norm = |s: &str| -> String { s.chars().filter(|c| !c.is_whitespace()).collect() };
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            for entity_name in &domain_entry.entities {
                // The generation-order titles may differ from the domains.toml
                // entity names only by whitespace (e.g. title "Campaign Type"
                // vs entity "CampaignType"). Match on a normalized key so every
                // configured entity gets an OpenAPI spec, and resolve the schema
                // by its real title (get_schema_in_domain is title-keyed).
                let Some(title) = domain_entities
                    .iter()
                    .find(|t| norm(t) == norm(entity_name.as_str()))
                else {
                    continue;
                };
                seen.insert(norm(title));
                if let Ok(Some(schema)) = db.get_schema_in_domain(title, domain_name).await {
                    let entity_cfg = domain_entry.get_entity_config(entity_name);
                    let operations =
                        resolve_entity_operations(db, config, domain_name, entity_name).await;
                    let (media_fields, media_mode_presigned) =
                        resolve_media_surface(db, config, domain_name, title).await;

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
                            if let Ok(Some(parent_schema)) =
                                db.get_schema_in_domain(pname, domain_name).await
                            {
                                (
                                    Some(resolve_path_segment(None, &parent_schema)),
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
                        path_segment: resolve_path_segment(entity_cfg, &schema),
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
                        media_fields,
                        // The router's own expression (issues #817 + #824):
                        // the `/machine` route is monolith-only and exists
                        // only where the read model has a loader, so the
                        // document never lists a path the router does not
                        // mount.
                        has_machine: !project.is_workers_topology()
                            && crate::machines::machine_read_model_surface_for_entity(
                                db,
                                config,
                                domain_name,
                                title,
                                entity_cfg
                                    .and_then(|ec| ec.workflow.as_ref())
                                    .map(|wf| wf.status_field.as_str()),
                            )
                            .await
                            .is_some(),
                        media_mode_presigned,
                    });
                }
            }

            // Domains whose schemas are not listed in domains.toml `entities`
            // (e.g. VO/codelist-only domains like a `common` domain with
            // `entities = []`) still get scaffold routes, handlers and — per
            // server.tera's per-domain loop — an OpenAPI doc reference. Emit
            // entities for them from the generation order so the referenced
            // `{Domain}ApiDoc` actually exists; they run with default
            // entity config, mirroring how their handlers are generated.
            let mut unlisted: Vec<&str> = domain_entities
                .iter()
                .filter(|t| !seen.contains(&norm(t)))
                .copied()
                .collect();
            unlisted.sort_unstable();
            for title in unlisted {
                let Ok(Some(schema)) = db.get_schema_in_domain(title, domain_name).await else {
                    continue;
                };
                let operations = resolve_entity_operations(db, config, domain_name, title).await;
                let (media_fields, media_mode_presigned) =
                    resolve_media_surface(db, config, domain_name, title).await;
                entities.push(OpenApiEntity {
                    entity_name: schema.rust_type_name.clone(),
                    module_name: schema.pg_table_name.clone(),
                    path_segment: resolve_path_segment(None, &schema),
                    tag: schema.rust_type_name.clone(),
                    has_create: operations.contains(&"create".to_string()),
                    has_read: operations.contains(&"read".to_string()),
                    has_update: operations.contains(&"update".to_string()),
                    has_delete: operations.contains(&"delete".to_string()),
                    has_list: operations.contains(&"list".to_string()),
                    role: "root".to_string(),
                    parent_path_segment: None,
                    parent_entity: None,
                    parent_domain: None,
                    media_fields,
                    // The router's own expression (issues #817 + #824) — see
                    // the configured-entity arm above.
                    has_machine: !project.is_workers_topology()
                        && crate::machines::machine_read_model_surface_for_entity(
                            db,
                            config,
                            domain_name,
                            title,
                            domain_entry
                                .get_entity_config(title)
                                .and_then(|ec| ec.workflow.as_ref())
                                .map(|wf| wf.status_field.as_str()),
                        )
                        .await
                        .is_some(),
                    media_mode_presigned,
                });
            }

            let functions = functions_per_domain
                .remove(domain_name.as_str())
                .unwrap_or_default();
            let service_ops = service_ops_per_domain
                .remove(domain_name.as_str())
                .unwrap_or_default();
            if !entities.is_empty() || !functions.is_empty() || !service_ops.is_empty() {
                domains.push(OpenApiDomain {
                    name: domain_name.clone(),
                    label: domain_entry.label.clone(),
                    tier: domain_entry.tier.clone(),
                    entities,
                    functions,
                    service_ops,
                });
            }
        }

        // Sort domains for deterministic output
        domains.sort_by(|a, b| a.name.cmp(&b.name));

        // Modeled DTO boundaries (DD3, issue #667): application-scoped, so
        // every document registers the same component schemas.
        let dto_boundaries: Vec<OpenApiBoundary> =
            crate::ddd::dto_boundaries::DtoBoundaryIndex::from_models(&db.get_ddd_models().await?)
                .shapes()
                .map(|shape| OpenApiBoundary {
                    name: shape.name.clone(),
                    module: codegraph_naming::to_snake_case(&shape.module),
                })
                .collect();

        let openapi_dir = self.output_dir.join("src").join("api").join("openapi");
        let mut files = Vec::new();

        // 1. Shared security modifier
        let empty_ctx: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let security_content =
            render_template_with_project(tera, "api/openapi_security.tera", &empty_ctx, project)?;
        files.push(GeneratedFile {
            path: openapi_dir.join("security.rs"),
            content: security_content,
        });

        // 2. Combined "All" spec
        let all_ctx = OpenApiContext {
            title: project.identity.api_title.clone(),
            version: project.identity.api_version.clone(),
            domains: domains.clone(),
            dto_boundaries: dto_boundaries.clone(),
        };
        let all_content =
            render_template_with_project(tera, "api/openapi_all.tera", &all_ctx, project)?;
        files.push(GeneratedFile {
            path: openapi_dir.join("all.rs"),
            content: all_content,
        });

        // 3. Per-domain specs
        for domain in &domains {
            let ctx = PerDomainOpenApiContext {
                title: format!("{} — {}", project.identity.api_title, domain.label),
                version: project.identity.api_version.clone(),
                domain_name: domain.name.clone(),
                domain_label: domain.label.clone(),
                entities: domain.entities.clone(),
                functions: domain.functions.clone(),
                service_ops: domain.service_ops.clone(),
                dto_boundaries: dto_boundaries.clone(),
            };
            let content =
                render_template_with_project(tera, "api/openapi_domain.tera", &ctx, project)?;
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
        let catalog_content =
            render_template_with_project(tera, "api/openapi_catalog.tera", &catalog_ctx, project)?;
        files.push(GeneratedFile {
            path: openapi_dir.join("catalog.rs"),
            content: catalog_content,
        });

        // 5. Module declaration file (mod.rs)
        // Explicit rather than relying on generate_mod_files_recursive so the
        // generator is self-documenting about what it produces.
        let mut mod_lines = vec![
            "//! Per-domain OpenAPI specifications and API catalog.".to_string(),
            format!(
                "//! Generated by {} . DO NOT EDIT.",
                project.identity.generator_name
            ),
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
