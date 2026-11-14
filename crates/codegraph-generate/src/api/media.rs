//! The media route family (issue #625 PR-C — the dormant plane revived).
//!
//! Plan-driven, mode-gated: routes fire for a domain's media fields when the
//! configured `upload_mode` needs an app-plane data path (`through-app` |
//! `presigned`). Client-direct (the Supabase default, R5) emits NO routes —
//! the browser speaks to the provider directly with its session. The old
//! MediaWrapper-classification trigger is retired: mox-first consumers could
//! never produce it, which is exactly why this plane went dormant.
//!
//! Handlers ride the monolith's AppState media slot (`state.{domain}_
//! media_storage`, PR-B) — never the hooks-crate registry slot. The emitted
//! module writes the CHILD row (the contained-value table), not parent
//! columns; keys render from the domain's key template (R7 — overwrite in
//! place).

use crate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::media::plan;
use crate::media::keys;
use crate::render_template_with_project;
use crate::traits::{EntityGenerator, EntityGeneratorKind, GeneratedFile};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
struct MediaRouteContext {
    entity_name: String,
    entity_snake: String,
    entity_module: String,
    domain: String,
    media_field_name: String,
    media_accept: Vec<String>,
    child_table: String,
    parent_fk_column: String,
    url_column: String,
    mime_column: String,
    /// Presigned mode ⇒ presign issuance route (no multipart upload).
    mode_presigned: bool,
    max_bytes: u64,
    key_format: String,
    display_ttl_secs: u64,
    presign_ttl_secs: u64,
}

pub struct MediaRouteGenerator {
    output_dir: PathBuf,
}

impl MediaRouteGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for MediaRouteGenerator {
    fn kind(&self) -> EntityGeneratorKind {
        EntityGeneratorKind::MediaRoute
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
        // Mode gate (R5): client-direct needs no app-plane route.
        let fields = plan::media_route_fields(db, config, domain, schema_title).await?;
        if fields.is_empty() {
            return Ok(Vec::new());
        }
        let Some(media_cfg) = config.domains.get(domain).and_then(|d| d.media.as_ref()) else {
            return Ok(Vec::new());
        };
        let titles = [schema_title.to_string()];
        let media_plan = plan::build_media_plan_for_domain(
            db,
            domain,
            media_cfg,
            &config.defaults.type_suffix,
            &titles,
        )
        .await?;

        let schema = match db.get_schema_in_domain(schema_title, domain).await? {
            Some(s) => s,
            None => return Ok(Vec::new()),
        };
        let entity_name = schema.rust_type_name.clone();
        let entity_snake = schema.pg_table_name.clone();
        let entity_module = format!("{}_{}", domain, entity_snake);
        if entity_snake.is_empty() {
            return Ok(Vec::new());
        }

        let mut files = Vec::new();
        for field in &media_plan.fields {
            let key_format = keys::key_format_for(
                &media_plan.key_template,
                &media_plan.domain,
                &field.entity_table,
                &field.field_snake,
            );
            let ctx = MediaRouteContext {
                entity_name: entity_name.clone(),
                entity_snake: entity_snake.clone(),
                entity_module: entity_module.clone(),
                domain: domain.to_string(),
                media_field_name: field.field_snake.clone(),
                media_accept: media_cfg.accept.clone(),
                child_table: field.child_table.clone(),
                parent_fk_column: field.parent_fk_column.clone(),
                url_column: field.url_column.clone(),
                mime_column: field.mime_column.clone(),
                mode_presigned: media_plan.upload_mode
                    == codegraph_core::types::UploadMode::Presigned,
                max_bytes: media_plan.max_bytes,
                key_format: key_format.clone(),
                display_ttl_secs: media_plan.display_ttl_secs,
                presign_ttl_secs: media_plan
                    .provider_settings
                    .presign_ttl_secs()
                    .unwrap_or(crate::media::DEFAULT_PRESIGN_TTL_SECS),
            };
            let content =
                render_template_with_project(tera, "api/media_route.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: self
                    .output_dir
                    .join("src")
                    .join("api")
                    .join(domain)
                    .join(format!("media_{}_{}.rs", entity_snake, field.field_snake)),
                content,
            });
        }

        Ok(files)
    }
}
