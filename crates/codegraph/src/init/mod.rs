//! Project initialization: `codegraph init` / `doctor` / `add domain`.

pub mod commands;
pub mod context;
pub mod model_starter;
pub mod rosetta_model;

pub use context::{DomainSeed, ProjectFeatures, ProjectTemplateContext};
