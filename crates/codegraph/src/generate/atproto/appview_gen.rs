use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{DomainGenerator, GeneratedFile};
use crate::generate::ProjectConfig;
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
struct AppviewIngestorContext {
    domain: String,
    lexicons: Vec<LexiconRef>,
}

#[derive(Debug, Serialize)]
struct LexiconRef {
    nsid: String,
    module_name: String,
}

#[derive(Debug, Serialize)]
struct AppviewIndexContext {
    domains: Vec<String>,
}

pub struct AtprotoAppviewEmitter {
    output_dir: PathBuf,
}

impl AtprotoAppviewEmitter {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl DomainGenerator for AtprotoAppviewEmitter {
    fn name(&self) -> &str {
        "atproto_appview"
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

        let mut lexicons = Vec::new();

        for title in entity_titles {
            if let Ok(Some(lexicon)) = db.get_lexicon_by_schema(title).await {
                lexicons.push(LexiconRef {
                    nsid: lexicon.nsid.clone(),
                    module_name: codegraph_naming::to_snake_case(title),
                });
            }
        }

        if lexicons.is_empty() {
            return Ok(Vec::new());
        }

        let mut files = Vec::new();

        let ingestor_ctx = AppviewIngestorContext {
            domain: domain.to_string(),
            lexicons,
        };

        let ingestor_content = render_template_with_project(
            tera,
            "atproto/appview_ingestor.tera",
            &ingestor_ctx,
            project,
        )?;

        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("atproto")
                .join("appview")
                .join(format!("ingest_{}.rs", domain)),
            content: ingestor_content,
        });

        // The index rebuild is a global file — only emit once (when domain == first domain with lexicons).
        // However DomainGenerator runs per domain, so we emit per domain and let the file system
        // overwrite. To avoid duplicates, we only emit for the first domain with lexicons.
        let all_domains: Vec<String> = {
            let schemas = db
                .list_schemas(None)
                .await
                .map_err(|e| crate::error::Error::Config(e.to_string()))?;
            let mut domains = std::collections::BTreeSet::new();
            for schema in &schemas {
                if let Some(ref d) = schema.domain {
                    if !d.is_empty() {
                        domains.insert(d.clone());
                    }
                }
            }
            domains.into_iter().collect()
        };

        let index_ctx = AppviewIndexContext {
            domains: all_domains,
        };

        let index_content = render_template_with_project(
            tera,
            "atproto/appview_index.tera",
            &index_ctx,
            project,
        )?;

        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("atproto")
                .join("appview")
                .join("reindex.rs"),
            content: index_content,
        });

        Ok(files)
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
    use crate::generate::traits::DomainGenerator;

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
            "atproto/appview_ingestor.tera",
            r#"use gravy_atproto::FirehoseClient;{% for l in lexicons %}"{{l.nsid}}"{% endfor %}fn handle_commit_{{domain}}(){}"#,
        )
        .unwrap();
        tera.add_raw_template(
            "atproto/appview_index.tera",
            r#"mod ingestor;{% for d in domains %}"{{d}}"{% endfor %}"#,
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
    async fn test_appview_ingestor_contains_gravy_atproto_imports() {
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
        let emitter = AtprotoAppviewEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(
                &engine,
                "grants",
                &["Grant".to_string()],
                &make_domain_config(),
                &tera,
                &project,
            )
            .await
            .expect("generation should succeed");

        let ingestor = result
            .iter()
            .find(|f| f.path.to_string_lossy().contains("ingest_grants.rs"))
            .expect("should produce ingestor file");

        assert!(
            ingestor.content.contains("use gravy_atproto::"),
            "ingestor should contain gravy_atproto imports"
        );
        assert!(
            ingestor.content.contains("nz.gravy.grants.grant"),
            "ingestor should contain collection NSID matching"
        );
        assert!(
            ingestor.content.contains("handle_commit_grants"),
            "ingestor should contain handle_commit function"
        );
    }

    #[tokio::test]
    async fn test_appview_skips_empty_authority() {
        let engine = MockEngine::builder().build();
        let tera = make_tera();
        let project = ProjectConfig {
            atproto_authority: "".to_string(),
            ..Default::default()
        };
        let emitter = AtprotoAppviewEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(
                &engine,
                "grants",
                &[],
                &make_domain_config(),
                &tera,
                &project,
            )
            .await
            .expect("generation should succeed");

        assert!(result.is_empty(), "should return empty when authority is blank");
    }

    #[tokio::test]
    async fn test_appview_index_rebuild_contains_domains() {
        let engine = MockEngine::builder()
            .with_schema(make_schema("Grant", "grants"))
            .with_schema(make_schema("Application", "grants"))
            .with_lexicon_mapping("Grant", "nz.gravy.grants.grant")
            .with_lexicon_mapping("Application", "nz.gravy.grants.application")
            .build();

        let ns = NamespaceNode {
            authority: "nz.gravy".to_string(),
            segment: "".to_string(),
            domain: "grants".to_string(),
        };
        engine.ingest_namespace(&ns).await.unwrap();

        let lex1 = LexiconNode {
            nsid: "nz.gravy.grants.grant".to_string(),
            lex_type: "record".to_string(),
            key_strategy: "tid".to_string(),
            revision: Some(1),
            description: Some("A grant".to_string()),
            domain: "grants".to_string(),
        };
        engine.ingest_lexicon(&lex1).await.unwrap();

        let lex2 = LexiconNode {
            nsid: "nz.gravy.grants.application".to_string(),
            lex_type: "record".to_string(),
            key_strategy: "tid".to_string(),
            revision: Some(1),
            description: Some("An application".to_string()),
            domain: "grants".to_string(),
        };
        engine.ingest_lexicon(&lex2).await.unwrap();

        let tera = make_tera();
        let project = make_project();
        let emitter = AtprotoAppviewEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(
                &engine,
                "grants",
                &["Grant".to_string(), "Application".to_string()],
                &make_domain_config(),
                &tera,
                &project,
            )
            .await
            .expect("generation should succeed");

        let reindex = result
            .iter()
            .find(|f| f.path.to_string_lossy().ends_with("reindex.rs"))
            .expect("should produce reindex.rs");

        assert!(
            reindex.content.contains("grants"),
            "reindex should contain domain reference"
        );
    }
}
