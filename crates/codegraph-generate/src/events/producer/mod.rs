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
#[cfg(test)]
mod tests;

pub use collect::{GraphProducerSource, ProducerSource, StaticProducerSource};
pub use planner::{
    ContextFact, EmissionArm, EmissionSite, ProducerArm, ProducerBinding, ProducerDiagnostic,
    ProducerEntitySurface, ProducerInputs, ProducerPlan, ProducerReport, SurfaceChild,
    SurfaceField,
};

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

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
) -> Result<ProducerReport, String> {
    let source = GraphProducerSource::new(db, config, workers_topology);
    plan_with(&source).await
}
