//! Project initialization: `codegraph init` / `doctor` / `add domain`.

pub mod commands;
pub mod context;
pub mod model_starter;

pub use context::{DomainSeed, ProjectFeatures, ProjectTemplateContext};
