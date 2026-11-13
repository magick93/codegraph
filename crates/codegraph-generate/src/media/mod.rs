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

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::MediaPlan;

use crate::error::Error;

/// Validate every domain's `[media]` declaration against the graph: a
/// broken media config is a hard `Error::Config` naming the domain — it
/// never half-generates (the function-bindings precedent). No-op when no
/// domain declares media.
pub async fn validate_media_configs(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<(), Error> {
    build_media_plans(db, config).await.map(|_| ())
}

/// Build the media plan for every domain that declares `[media]`, keyed by
/// domain name (BTreeMap — deterministic iteration, the charts precedent).
pub async fn build_media_plans(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<BTreeMap<String, MediaPlan>, Error> {
    let _ = (db, config);
    todo!("M4 GREEN: plan builder")
}
