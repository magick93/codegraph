use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{DomainGenerator, EntityGenerator, GeneratedFile};
use crate::generate::ProjectConfig;
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
struct XrpcQueryContext {
    lexicon: LexiconMeta,
    module_name: String,
    entity_name: String,
    fields: Vec<XrpcField>,
    imports: Vec<String>,
}

#[derive(Debug, Serialize)]
struct XrpcProcedureContext {
    lexicon: LexiconMeta,
    module_name: String,
    entity_name: String,
    imports: Vec<String>,
}

#[derive(Debug, Serialize)]
struct XrpcRouterContext {
    domain: String,
    endpoints: Vec<RouterEndpoint>,
    procedures: Vec<RouterEndpoint>,
}

#[derive(Debug, Serialize)]
struct RouterEndpoint {
    nsid: String,
    module_name: String,
}

#[derive(Debug, Serialize)]
struct LexiconMeta {
    nsid: String,
    description: String,
}

#[derive(Debug, Serialize)]
struct XrpcField {
    name: String,
    rust_type: String,
    column: String,
}

pub struct AtprotoXrpcEmitter {
    output_dir: PathBuf,
}

impl AtprotoXrpcEmitter {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for AtprotoXrpcEmitter {
    fn name(&self) -> &str {
        "atproto_xrpc"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        _config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        if project.atproto_authority.is_empty() {
            return Ok(Vec::new());
        }

        let lexicon = match db.get_lexicon_by_schema(schema_title).await? {
            Some(l) => l,
            None => return Ok(Vec::new()),
        };

        let schema = match db.get_schema_in_domain(schema_title, domain).await? {
            Some(s) => s,
            None => return Ok(Vec::new()),
        };

        let entity_name = schema.rust_type_name.clone();
        let module_name = schema.pg_table_name.clone();

        let properties = db
            .get_properties_in_domain(schema_title, domain)
            .await
            .unwrap_or_default();

        let fields: Vec<XrpcField> = properties
            .iter()
            .map(|p| XrpcField {
                name: p.name.clone(),
                rust_type: p.rust_field_type.clone(),
                column: p.rust_field_name.clone(),
            })
            .collect();

        let imports = vec![
            format!("crate::entity::{}", module_name),
        ];

        let mut files = Vec::new();

        let query_ctx = XrpcQueryContext {
            lexicon: LexiconMeta {
                nsid: lexicon.nsid.clone(),
                description: lexicon.description.clone().unwrap_or_default(),
            },
            module_name: module_name.clone(),
            entity_name: entity_name.clone(),
            fields,
            imports: imports.clone(),
        };

        let query_content = render_template_with_project(
            tera,
            "atproto/xrpc_query.tera",
            &query_ctx,
            project,
        )?;

        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("atproto")
                .join("xrpc")
                .join(format!("get_{}.rs", module_name)),
            content: query_content,
        });

        let proc_ctx = XrpcProcedureContext {
            lexicon: LexiconMeta {
                nsid: lexicon.nsid.clone(),
                description: lexicon.description.clone().unwrap_or_default(),
            },
            module_name: module_name.clone(),
            entity_name,
            imports,
        };

        let proc_content = render_template_with_project(
            tera,
            "atproto/xrpc_procedure.tera",
            &proc_ctx,
            project,
        )?;

        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("atproto")
                .join("xrpc")
                .join(format!("create_{}.rs", module_name)),
            content: proc_content,
        });

        Ok(files)
    }
}

#[async_trait]
impl DomainGenerator for AtprotoXrpcEmitter {
    fn name(&self) -> &str {
        "atproto_xrpc_router"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        domain: &str,
        entity_titles: &[String],
        _config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        if project.atproto_authority.is_empty() {
            return Ok(Vec::new());
        }

        let mut endpoints = Vec::new();
        let mut procedures = Vec::new();

        for title in entity_titles {
            let lexicon = match db.get_lexicon_by_schema(title).await {
                Ok(Some(l)) => l,
                _ => continue,
            };

            let schema = match db.get_schema_in_domain(title, domain).await {
                Ok(Some(s)) => s,
                _ => continue,
            };

            let module_name = schema.pg_table_name.clone();

            if lexicon.lex_type == "record" || lexicon.lex_type == "query" {
                endpoints.push(RouterEndpoint {
                    nsid: lexicon.nsid.clone(),
                    module_name: module_name.clone(),
                });
            }

            if lexicon.lex_type == "procedure" {
                procedures.push(RouterEndpoint {
                    nsid: lexicon.nsid.clone(),
                    module_name: module_name.clone(),
                });
            }
        }

        if endpoints.is_empty() && procedures.is_empty() {
            return Ok(Vec::new());
        }

        let router_ctx = XrpcRouterContext {
            domain: domain.to_string(),
            endpoints,
            procedures,
        };

        let router_content = render_template_with_project(
            tera,
            "atproto/xrpc_router.tera",
            &router_ctx,
            project,
        )?;

        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("atproto")
                .join("xrpc")
                .join(format!("{}_router.rs", domain)),
            content: router_content,
        }])
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use codegraph_core::mock::MockEngine;
    use codegraph_core::traits::GraphIngestor;
    use codegraph_core::types::{LexiconNode, NamespaceNode, SchemaNode};
    use tera::Tera;

    use super::*;
    use crate::generate::traits::{DomainGenerator, EntityGenerator};

    fn make_domain_config() -> codegraph_config::DomainConfig {
        let domains = std::collections::HashMap::new();
        codegraph_config::DomainConfig {
            defaults: Default::default(),
            domains,
        }
    }

    fn make_project() -> ProjectConfig {
        ProjectConfig {
            atproto_authority: "nz.gravy".to_string(),
            ..Default::default()
        }
    }

    fn make_tera() -> Tera {
        let mut tera = Tera::default();
        tera.add_raw_template(
            "atproto/xrpc_query.tera",
            r#"use axum::extract::Query;fn get_{{module_name}}(){}"#,
        )
        .unwrap();
        tera.add_raw_template(
            "atproto/xrpc_procedure.tera",
            r#"use gravy_atproto::AtprotoError;fn create_{{module_name}}(){}"#,
        )
        .unwrap();
        tera.add_raw_template(
            "atproto/xrpc_router.tera",
            r#"Router::new(){% for e in endpoints %}.route("/xrpc/{{e.nsid}}", get(...)){% endfor %}"#,
        )
        .unwrap();
        tera
    }

    fn make_schema(title: &str, domain: &str) -> SchemaNode {
        SchemaNode {
            schema_id: format!("id:{}", title),
            title: title.to_string(),
            description: Some(format!("The {} schema", title)),
            schema_type: "object".to_string(),
            classification: "entity".to_string(),
            domain: Some(domain.to_string()),
            rel_path: format!("{}.json", title),
            pg_type: "UUID".to_string(),
            rust_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            rust_type_name: title.to_string(),
            pg_table_name: codegraph_naming::to_snake_case(title),
            api_path_segment: codegraph_naming::to_kebab_case(title),
            parent_schema: None,
            is_entity: true,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
        }
    }

    #[tokio::test]
    async fn test_xrpc_query_contains_axum_extract() {
        let engine = MockEngine::builder()
            .with_schema(make_schema("Grant", "grants"))
            .with_lexicon_mapping("Grant", "nz.gravy.grants.grant")
            .build();

        let ns = NamespaceNode {
            authority: "nz.gravy".to_string(),
            segment: "".to_string(),
            domain: "grants".to_string(),
        };
        engine.ingest_namespace(&ns).await.unwrap();

        let lex = LexiconNode {
            nsid: "nz.gravy.grants.grant".to_string(),
            lex_type: "record".to_string(),
            key_strategy: "tid".to_string(),
            revision: Some(1),
            description: Some("A grant".to_string()),
            domain: "grants".to_string(),
        };
        engine.ingest_lexicon(&lex).await.unwrap();

        let tera = make_tera();
        let project = make_project();
        let emitter = AtprotoXrpcEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = EntityGenerator::generate(
                &emitter,
                &engine,
                "Grant",
                "grants",
                &make_domain_config(),
                &tera,
                &project,
            )
            .await
            .expect("generation should succeed");

        assert!(!result.is_empty(), "should produce files");

        let query_file = result
            .iter()
            .find(|f| f.path.to_string_lossy().contains("get_"))
            .expect("should produce get_ handler");

        assert!(
            query_file.content.contains("axum::extract::Query"),
            "query handler should contain axum extract Query"
        );
    }

    #[tokio::test]
    async fn test_xrpc_router_contains_xrpc_path() {
        let engine = MockEngine::builder()
            .with_schema(make_schema("Grant", "grants"))
            .with_lexicon_mapping("Grant", "nz.gravy.grants.grant")
            .build();

        let ns = NamespaceNode {
            authority: "nz.gravy".to_string(),
            segment: "".to_string(),
            domain: "grants".to_string(),
        };
        engine.ingest_namespace(&ns).await.unwrap();

        let lex = LexiconNode {
            nsid: "nz.gravy.grants.grant".to_string(),
            lex_type: "record".to_string(),
            key_strategy: "tid".to_string(),
            revision: Some(1),
            description: Some("A grant".to_string()),
            domain: "grants".to_string(),
        };
        engine.ingest_lexicon(&lex).await.unwrap();

        let tera = make_tera();
        let project = make_project();
        let emitter = AtprotoXrpcEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = DomainGenerator::generate(
                &emitter,
                &engine,
                "grants",
                &["Grant".to_string()],
                &make_domain_config(),
                &tera,
                &project,
            )
            .await
            .expect("domain generation should succeed");

        assert_eq!(result.len(), 1);
        let router_file = &result[0];

        assert!(
            router_file.content.contains("/xrpc/nz.gravy"),
            "router should contain /xrpc/nz.gravy path"
        );
    }

    #[tokio::test]
    async fn test_xrpc_procedure_uses_atproto_error() {
        let engine = MockEngine::builder()
            .with_schema(make_schema("Grant", "grants"))
            .with_lexicon_mapping("Grant", "nz.gravy.grants.grant")
            .build();

        let ns = NamespaceNode {
            authority: "nz.gravy".to_string(),
            segment: "".to_string(),
            domain: "grants".to_string(),
        };
        engine.ingest_namespace(&ns).await.unwrap();

        let lex = LexiconNode {
            nsid: "nz.gravy.grants.grant".to_string(),
            lex_type: "record".to_string(),
            key_strategy: "tid".to_string(),
            revision: Some(1),
            description: Some("A grant".to_string()),
            domain: "grants".to_string(),
        };
        engine.ingest_lexicon(&lex).await.unwrap();

        let tera = make_tera();
        let project = make_project();
        let emitter = AtprotoXrpcEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = EntityGenerator::generate(
            &emitter,
            &engine,
            "Grant",
            "grants",
            &make_domain_config(),
            &tera,
            &project,
        )
            .await
            .expect("generation should succeed");

        let proc_file = result
            .iter()
            .find(|f| f.path.to_string_lossy().contains("create_"))
            .expect("should produce create_ handler");

        assert!(
            proc_file.content.contains("gravy_atproto::AtprotoError"),
            "procedure handler should use AtprotoError"
        );
    }
}
