//! `MediaPlan` resolution: `[domains.X.media]` + graph → per-domain plans.
//!
//! Resolution is strict (the charts-binding precedent): unresolved media
//! classes, refers-wins targets, array media fields, and missing url/mime
//! columns are hard `Error::Config`s naming the domain/entity/field — never
//! bare names in templates.

use std::collections::{BTreeMap, BTreeSet};

use codegraph_config::{DomainConfig, MediaConfig};
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PropertyNode;
use codegraph_core::types::{
    KeyTemplate, MediaFieldPlan, MediaPlan, MediaProvider, MediaProviderSettings, MediaVisibility,
    UploadMode, provider_capabilities,
};
use codegraph_naming::{child_parent_fk_column, child_table_name, to_snake_case};
use codegraph_type_contracts::RefClassificationKind;

use crate::error::Error;

/// Build one domain's plan from explicit entity titles (the generator
/// passes `generation_order` titles; the validator derives them from the
/// graph).
pub async fn build_media_plan_for_domain(
    db: &dyn GraphQuerier,
    domain_name: &str,
    media: &MediaConfig,
    type_suffix: &str,
    entity_titles: &[String],
) -> Result<MediaPlan, Error> {
    let _ = (db, domain_name, media, type_suffix, entity_titles);
    todo!("M4 GREEN: per-domain plan builder")
}

#[cfg(test)]
#[allow(dead_code)]
fn find_column_fixture() {}
