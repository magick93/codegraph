//! Events producer planning (issue #820 slice a): lower `.ddd` service-op
//! `emits` clauses onto `.evt` payloads at generation time.
//!
//! Slice (a) is the planner + the refusal contract — NO publishing yet. The
//! plan is a pure derivation over the existing graph (no new node/edge/
//! property, `GRAPH_FORMAT_VERSION` untouched) and is gated by the
//! `events_producer` feature:
//!
//! - flag ON: `ValidationPass::check_event_producers` hard-fails every
//!   refusal before any file is written;
//! - flag OFF (the default): zero diagnostics, byte-identical output;
//!   `codegraph doctor --ddd-files … --evt-files …` prints the same lines as
//!   `WARN events producer — …` so a model can be aligned first.
//!
//! `docs/events-producer.md` is the normative sentence catalogue. The pure
//! planner ([`planner::plan`]) consumes [`ProducerInputs`] assembled by a
//! [`ProducerSource`]; [`collect::GraphProducerSource`] is the production
//! source (run path + doctor) and [`collect::StaticProducerSource`] is the
//! KAT/future-compiled-source seam.

pub mod collect;
pub mod messages;
pub mod planner;
pub mod render;
#[cfg(test)]
mod tests;

pub use collect::{GraphProducerSource, ProducerSource, StaticProducerSource};
pub use planner::{
    ContextFact, EmissionArm, EmissionSite, ProducerArm, ProducerBinding, ProducerDiagnostic,
    ProducerEntitySurface, ProducerInputs, ProducerPlan, ProducerReport, SurfaceChild,
    SurfaceField,
};

use std::collections::BTreeMap;
use std::sync::Arc;

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

/// The run's publishable plans grouped by entity — built once in the
/// generation context and shared by the command/handler generators (issue
/// #820 slice b). Refused pairs never appear here (validation fail-closed
/// before generation).
#[derive(Debug, Clone, Default)]
pub struct ProducerPlanIndex {
    by_entity: BTreeMap<(String, String), Vec<ProducerPlan>>,
}

impl ProducerPlanIndex {
    #[must_use]
    pub fn from_report(report: &ProducerReport) -> Self {
        let mut by_entity: BTreeMap<(String, String), Vec<ProducerPlan>> = BTreeMap::new();
        for plan in &report.plans {
            by_entity
                .entry((plan.domain.clone(), plan.entity.clone()))
                .or_default()
                .push(plan.clone());
        }
        Self { by_entity }
    }

    /// The plans for one entity (`domain`, schema title), declaration order.
    #[must_use]
    pub fn for_entity(&self, domain: &str, title: &str) -> &[ProducerPlan] {
        self.by_entity
            .get(&(domain.to_string(), title.to_string()))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

/// Build the run's producer report (the flag gates the work).
///
/// ONE planner run feeds every consumer: the publishing plane derives its
/// [`ProducerPlanIndex`] from this report and the catalogue plane (#820
/// slice c) reads the SAME report for its runtime-publication marks — never
/// a second run, never a re-derivation. `None` when the analysis could not
/// run or yielded no publishable plan, which is exactly the pre-slice-(c)
/// index gate: a flag-ON run with nothing to publish renders no marks, and
/// a flag-OFF run never gets here.
pub async fn report_for_run(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    workers_topology: bool,
    database_target: crate::db::dialect::DatabaseTarget,
) -> Option<Arc<ProducerReport>> {
    let report = plan_for_graph(db, config, workers_topology, database_target)
        .await
        .ok()?;
    (!report.plans.is_empty()).then(|| Arc::new(report))
}

/// Run the planner over an arbitrary source. An error means the analysis
/// could not run (never a guessed diagnostic).
pub async fn plan_with(source: &dyn ProducerSource) -> Result<ProducerReport, String> {
    let inputs = source.collect().await?;
    Ok(planner::plan(&inputs))
}

/// The production entry point: plan over the live graph.
pub async fn plan_for_graph(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    workers_topology: bool,
    database_target: crate::db::dialect::DatabaseTarget,
) -> Result<ProducerReport, String> {
    let source = GraphProducerSource::new(db, config, workers_topology, database_target);
    plan_with(&source).await
}
