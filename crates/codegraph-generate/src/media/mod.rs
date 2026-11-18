//! Media storage plane (issue #625, PR-A): plan builder + the
//! `media_storage` global generator.
//!
//! Layering: `plan.rs` resolves `[domains.X.media]` against the graph into
//! `MediaPlan`s (core IR) and enforces the provider capability matrix +
//! key-template syntax (the config crate has no production dependency on
//! core — the enforcement lives here and rides both the driver's
//! pre-generation validation slot and doctor). `mod.rs` holds the
//! presence-gated global generator emitting `src/media/{mod,noop}.rs`.

pub mod keys;
pub mod migration;
pub mod plan;

/// The fallback presign TTL when the provider settings carry none (the r2
/// config default; supabase presign expiry comes from the caller).
pub const DEFAULT_PRESIGN_TTL_SECS: u64 = 900;

/// A well-formed id no row can carry: the hurl contract addresses it to pin
/// the PROVIDER-FREE 404 (the #690 parent gate answers before the storage
/// slot, so the leg needs no bucket). Deterministic, so the emitted file is
/// byte-stable across runs.
pub const MEDIA_CONTRACT_MISSING_ID: &str = "11111111-1111-4111-8111-111111111111";

/// The domain's configured accept list — the hurl context's mime sample.
fn media_cfg_accept(config: &DomainConfig, domain_name: &str) -> String {
    config
        .domains
        .get(domain_name)
        .and_then(|d| d.media.as_ref())
        .and_then(|m| m.accept.first().cloned())
        .unwrap_or_else(|| "application/octet-stream".to_string())
}

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::traits::{GeneratedFile, GlobalGenerator, GlobalGeneratorKind};
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{MediaPlan, MediaProvider, UploadMode};
use serde_json::json;

use crate::error::Result;
use crate::output::render_template_with_project;
use crate::project_config::{GenerationEntry, ProjectConfig};

/// Validate every domain's `[media]` declaration against the graph: a
/// broken media config is a hard `Error::Config` naming the domain — it
/// never half-generates (the function-bindings precedent). No-op when no
/// domain declares media. Rides the driver's pre-generation validation slot
/// and doctor.
pub async fn validate_media_configs(db: &dyn GraphQuerier, config: &DomainConfig) -> Result<()> {
    plan::build_media_plans(db, config).await.map(|_| ())
}

/// Graph-free provider-consistency check (one provider per app, v1) — the
/// doctor-parity half of the plan builder's validation.
pub fn validate_media_provider_consistency(config: &DomainConfig) -> Result<()> {
    plan::validate_media_provider_consistency(config)
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
        // One provider per app in v1 (validated in the plan builder — the
        // adapter set is derived from the single surviving provider).
        let provider = plans
            .values()
            .next()
            .map(|p| p.provider)
            .unwrap_or(MediaProvider::Supabase);
        let adapter = match provider {
            MediaProvider::Supabase => "supabase",
            MediaProvider::R2 => "r2",
            // The generic S3 slice is not landed; R2's adapter accepts plain
            // S3-compatible endpoints today (R2_S3_ENDPOINT). A named error,
            // never silent emission of a broken plane (#551 in spirit).
            MediaProvider::S3 => {
                return Err(crate::error::Error::Config(
                    "media provider \"s3\" adapter lands in a follow-up slice — use provider \
                     \"r2\" with R2_S3_ENDPOINT for S3-compatible stores today"
                        .to_string(),
                ));
            }
        };
        let ctx = json!({
            "adapters": [adapter],
            // Issue #722: the session module gains the nested parent gate
            // (`parent_exists_linked`) only when SOME media field is mounted
            // under a parent route — a root-only app keeps the exact
            // pre-#722 session module (byte identity).
            "has_nested_media": plans
                .values()
                .any(|p| p.fields.iter().any(|f| !f.route_parents.is_empty())),
        });
        let mut files = vec![
            GeneratedFile {
                path: self.output_dir.join("src").join("media").join("mod.rs"),
                content: render_template_with_project(tera, "media/mod.tera", &ctx, project)?,
            },
            GeneratedFile {
                path: self.output_dir.join("src").join("media").join("noop.rs"),
                content: render_template_with_project(tera, "media/noop.tera", &ctx, project)?,
            },
            GeneratedFile {
                path: self.output_dir.join("src").join("media").join("http.rs"),
                content: render_template_with_project(tera, "media/http.tera", &ctx, project)?,
            },
            // The #169 session bundle + tenant-scoped parent gate (issue
            // #690) the media routes ride. Emitted with the plane; the
            // module declaration is native-only (SeaORM cannot link wasm32).
            GeneratedFile {
                path: self.output_dir.join("src").join("media").join("session.rs"),
                content: render_template_with_project(tera, "media/session.tera", &ctx, project)?,
            },
            GeneratedFile {
                path: self
                    .output_dir
                    .join("src")
                    .join("media")
                    .join(format!("{adapter}.rs")),
                content: render_template_with_project(
                    tera,
                    &format!("media/{adapter}.tera"),
                    &ctx,
                    project,
                )?,
            },
        ];
        // The Supabase leg carries the SQL surface (bucket + defense-in-depth
        // storage RLS, issue #625 PR-B). Static band 023000; R2 buckets are
        // platform-side — no migration analogue. Postgres only (a sqlite
        // target has no Supabase Storage to bind).
        if provider == MediaProvider::Supabase {
            if project.database.database_target == crate::db::dialect::DatabaseTarget::Sqlite {
                return Err(crate::error::Error::Config(
                    "media provider \"supabase\" requires database_target \"postgres\" — \
                     Supabase Storage bootstrap (bucket + RLS policies) is Postgres-only"
                        .to_string(),
                ));
            }
            if let Some(sql) = migration::emit_media_storage_migration(&plans) {
                files.push(GeneratedFile {
                    path: crate::db::migrations_root(&self.output_dir).join(format!(
                        "{:06}_media_storage.sql",
                        migration::MEDIA_MIGRATION_BAND
                    )),
                    content: sql,
                });
            }
        }
        // The media hurl contract (issue #630): one file per run, anchored to
        // the FIRST route-mode domain's first media field. Client-direct runs
        // emit nothing (no media surface to contract).
        let contract = plans
            .values()
            .find_map(|p| plan::media_upload_mode(config, &p.domain).map(|mode| (p, mode)));
        if let Some((plan, mode)) = contract
            && let Some(field) = plan.fields.first()
        {
            let domain_name = plan.domain.clone();
            let kind = if mode == UploadMode::Presigned {
                "presigned"
            } else {
                "through_app"
            };
            // The URL must be the one the ROUTER serves. The prefix is carried
            // on the plan (issue #713): the entity's own path segment — config
            // `path_segment` → `api_path_segment` → lowercased title, NOT
            // `pg_table_name` (title-derived segments drop the underscores) —
            // plus the nesting chain `router.tera` mounts a child under
            // (`/{parent_segment}/{parent_id}/…/{child_segment}`). Both were
            // previously re-derived here, and a child media field emitted a
            // flat path no router mounts (#691, #713).
            let prefix = field.route_prefix_with_ids(
                &config.defaults.api_version,
                &domain_name,
                MEDIA_CONTRACT_MISSING_ID,
            );
            // `{{base_url}}` + the CONFIGURED api version: a scheme-less path
            // is not a valid hurl URL (`error: Invalid URL … Missing scheme`),
            // and every sibling contract (`hurl_contract.rs`) is built the
            // same way.
            let base = format!(
                "{{{{base_url}}}}{prefix}/{MEDIA_CONTRACT_MISSING_ID}/{}",
                field.field_snake
            );
            let ctx = json!({
                "kind": kind,
                "domain": domain_name,
                // The ops api suite supplies `{{api_key}}`; the contract's
                // authenticated legs reference the same var the entity
                // contracts use (hurl_contract.rs).
                "auth_header": "Authorization: Bearer {{api_key}}",
                "field": field.field_snake,
                "mime": media_cfg_accept(config, &domain_name),
                "prefix": prefix,
                "upload_url": base,
                // Display is the GET on the SAME path (both modes mount it),
                // so the ownership leg needs no body extractor.
                "display_url": base,
                "presign_url": format!("{base}/presign"),
            });
            files.push(GeneratedFile {
                path: self.output_dir.join("hurl").join("09_media.hurl"),
                content: render_template_with_project(tera, "hurl/media.tera", &ctx, project)?,
            });
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }
}
