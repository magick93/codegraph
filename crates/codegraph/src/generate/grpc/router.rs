use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{DomainGenerator, GeneratedFile};
use crate::generate::ProjectConfig;
use codegraph_config::DomainConfig;

use super::proto_context::ProtoContext;

#[derive(Debug, Serialize)]
pub struct GrpcRouterContext {
    pub domain: String,
    pub entities: Vec<GrpcRouterEntity>,
    pub proto_types_prefix: String,
}

#[derive(Debug, Serialize)]
pub struct GrpcRouterEntity {
    pub entity_name: String,
    pub module_name: String,
    pub proto_service_trait: String,
    pub proto_service_server: String,
    pub grpc_service_struct: String,
    pub repo_trait: String,
}

pub struct GrpcRouterGenerator {
    output_dir: PathBuf,
}

impl GrpcRouterGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl DomainGenerator for GrpcRouterGenerator {
    fn name(&self) -> &str {
        "grpc_router"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        domain: &str,
        entity_titles: &[String],
        config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let mut entities = Vec::new();

        for title in entity_titles {
            let ctx = ProtoContext::build(db, title, domain, config).await?;
            if ctx.is_empty() {
                continue;
            }

            let entity_name = ctx.entity_name.clone();
            let module_name = ctx.module_name.clone();

            entities.push(GrpcRouterEntity {
                entity_name,
                module_name,
                proto_service_trait: format!("{}Service", ctx.entity_name),
                proto_service_server: format!("{}ServiceServer", ctx.entity_name),
                grpc_service_struct: format!("{}GrpcService", ctx.entity_name),
                repo_trait: format!("{}Repository", ctx.entity_name),
            });
        }

        if entities.is_empty() {
            return Ok(Vec::new());
        }

        let router_ctx = GrpcRouterContext {
            domain: domain.to_string(),
            entities,
            proto_types_prefix: "crate::api::grpc::proto::".to_string(),
        };

        let content =
            render_template_with_project(tera, "grpc/tonic/domain_router.tera", &router_ctx, project)?;

        let grpc_dir = self.output_dir.join("src").join("api").join("grpc");
        Ok(vec![GeneratedFile {
            path: grpc_dir.join(format!("{domain}_router.rs")),
            content,
        }])
    }
}
