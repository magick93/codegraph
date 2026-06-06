use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use crate::generate::ProjectConfig;
use codegraph_config::DomainConfig;

use super::proto_context::ProtoContext;

pub struct GrpcProtoGenerator {
    output_dir: PathBuf,
}

impl GrpcProtoGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for GrpcProtoGenerator {
    fn name(&self) -> &str {
        "grpc_proto"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let ctx = ProtoContext::build(db, schema_title, domain, config).await?;

        if ctx.is_empty() {
            return Ok(Vec::new());
        }

        let mut files = Vec::new();

        // Render proto_message.tera for the entity message + CRUD messages
        let msg_content = render_template_with_project(tera, "grpc/proto_message.tera", &ctx, project)?;
        let proto_dir = self.output_dir.join("proto").join(&ctx.package);
        files.push(GeneratedFile {
            path: proto_dir.join(&ctx.proto_file_name),
            content: msg_content,
        });

        // Render proto_service.tera for the service definition (appended to the same .proto)
        let svc_content = render_template_with_project(tera, "grpc/proto_service.tera", &ctx, project)?;
        // Append service definition to the same file
        if let Some(existing) = files.last_mut() {
            existing.content.push_str("\n");
            existing.content.push_str(&svc_content);
        }

        Ok(files)
    }
}
