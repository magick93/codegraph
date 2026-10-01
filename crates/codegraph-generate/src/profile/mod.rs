mod capabilities;
mod plan;
mod resolve;
mod types;

pub use capabilities::{CapabilityRegistry, GeneratorCapability, GeneratorKind, GeneratorTarget};
pub use plan::BuildPlan;
pub use resolve::{
    load_and_resolve_profile, IfmlFrameworkTarget, ProfileDef, ProfileIfmlConfig, ProfileMeta,
    ProfileScripts, ProfileSection, ProfileVariant, ProfilesConfig, ResolvedProfile,
    ResolvedSection,
};
pub use types::{DeploymentTopology, PersistenceProvider};

#[cfg(test)]
pub(crate) use resolve::resolve_profile;

#[cfg(test)]
use crate::error::Result;
#[cfg(test)]
use std::path::Path;

#[cfg(test)]
include!("tests.rs");
