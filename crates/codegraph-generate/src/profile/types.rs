use std::fmt;

use crate::error::{Error, Result};

/// The persistence provider backend for code generation.
///
/// Selects which ORM/query framework generates entity models and repository
/// implementations. The DDL generator is provider-agnostic and always runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistenceProvider {
    #[default]
    SeaOrm,
    Cornucopia,
}

impl PersistenceProvider {
    pub fn from_config(s: &str) -> Self {
        match s {
            "cornucopia" => Self::Cornucopia,
            _ => Self::SeaOrm,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SeaOrm => "sea_orm",
            Self::Cornucopia => "cornucopia",
        }
    }
}

impl fmt::Display for PersistenceProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The deployment topology for the generated application.
///
/// Determines whether the generated backend is a single monolithic axum
/// server (today's default) or a set of Cloudflare Workers — one per
/// bounded-context domain — behind a gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentTopology {
    /// Today's single-crate axum server.
    #[default]
    Monolith,
    /// One Cloudflare Worker per domain + gateway.
    Workers,
}

impl DeploymentTopology {
    /// Parse from the profiles.toml `deployment_topology` feature value.
    ///
    /// Unknown values are a configuration error (unlike `PersistenceProvider`,
    /// which silently defaults) because a typo here would silently generate
    /// the wrong deployment shape.
    pub fn from_config(s: &str) -> Result<Self> {
        match s {
            "monolith" => Ok(Self::Monolith),
            "workers" => Ok(Self::Workers),
            other => Err(Error::Config(format!(
                "unknown deployment_topology \"{other}\" in profile features; \
                 expected \"monolith\" (default) or \"workers\""
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Monolith => "monolith",
            Self::Workers => "workers",
        }
    }
}

impl fmt::Display for DeploymentTopology {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
