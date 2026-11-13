//! Media storage plane (issue #625, PR-A): plan builder + the
//! `media_storage` global generator.
//!
//! Layering: `plan.rs` resolves `[domains.X.media]` against the graph into
//! `MediaPlan`s (core IR) and enforces the provider capability matrix +
//! key-template syntax (the config crate has no production dependency on
//! core — the enforcement lives here and rides both the driver's
//! pre-generation validation slot and doctor). `mod.rs` holds the
//! presence-gated global generator emitting `src/media/{mod,noop}.rs`.

pub mod plan;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::MediaPlan;
use crate::traits::{GeneratedFile, GlobalGenerator, GlobalGeneratorKind};
use serde_json::json;

use crate::error::Result;
use crate::output::render_template_with_project;
use crate::project_config::{GenerationEntry, ProjectConfig};

/// Validate every domain's `[media]` declaration against the graph: a
/// broken media config is a hard `Error::Config` naming the domain — it
/// never half-generates (the function-bindings precedent). No-op when no
/// domain declares media. Rides the driver's pre-generation validation slot
/// and doctor.
pub async fn validate_media_configs(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<()> {
    plan::build_media_plans(db, config).await.map(|_| ())
}

/// Build the media plan for every domain that declares `[media]`, keyed by
/// domain name (BTreeMap — deterministic iteration, the charts precedent).
pub async fn build_media_plans(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<BTreeMap<String, MediaPlan>> {
    plan::build_media_plans(db, config).await
}

/// The `media_storage` global generator (issue #625 PR-A): emits
/// `src/media/{mod,noop}.rs` — the provider-neutral trait + fail-closed
/// NoopStorage — when any domain declares `[media]`. Presence-gated
/// internally: no media config ⇒ zero files (the charts precedent). Adapter
/// modules land with their provider slices (PR-B Supabase, PR-C R2); PR-A
/// emits none, so the `adapters` context loop stays empty.
pub struct MediaStorageGenerator {
    output_dir: PathBuf,
}

impl MediaStorageGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait::async_trait]
impl GlobalGenerator for MediaStorageGenerator {
    fn kind(&self) -> GlobalGeneratorKind {
        GlobalGeneratorKind::MediaStorage
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let plans: BTreeMap<String, MediaPlan> = plan::build_media_plans(db, config).await?;
        if plans.is_empty() {
            return Ok(Vec::new());
        }
        // Distinct configured providers would each contribute an adapter
        // module; PR-A emits none (the adapters context stays empty so the
        // mod.tera loop renders nothing — no `pub mod` without its file).
        let ctx = json!({ "adapters": [] });
        Ok(vec![
            GeneratedFile {
                path: self.output_dir.join("src").join("media").join("mod.rs"),
                content: render_template_with_project(tera, "media/mod.tera", &ctx, project)?,
            },
            GeneratedFile {
                path: self.output_dir.join("src").join("media").join("noop.rs"),
                content: render_template_with_project(tera, "media/noop.tera", &ctx, project)?,
            },
        ])
    }
}
