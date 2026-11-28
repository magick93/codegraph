//! The driver's application workflows. Each is a separate entry point that
//! composes the shared stages (config loading, ingestion, profile
//! resolution, generation preparation, validation, post-generation); none
//! re-implements a stage another workflow owns.

pub(super) mod classify;
pub(super) mod generate;
pub(super) mod ifml;
pub(super) mod run;
