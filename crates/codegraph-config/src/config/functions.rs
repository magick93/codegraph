//! Function bindings (issue #450 B0): per-function `implementation` /
//! `exposure` settings for modeled sigil `func`s, configured in
//! `domains.toml` under a top-level `[functions]` table.
//!
//! ```toml
//! [functions."PlaceBid"]
//! implementation = "manual"   # dsl (default) | manual | sql | external
//! exposure = "http"           # internal (default) | http | event
//! ```
//!
//! Function names are namespace-unique in Rosetta (the `FunctionNode`
//! natural key), so the bare name is the config key. Absent table (or a
//! function with no entry) ⇒ every function keeps today's `dsl` /
//! `internal` behavior — the standing byte-identity default.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::error::DomainConfigError;

/// Where the body of a modeled function lives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FunctionImplementation {
    /// Transpile `set`/`add` ops into generated Rust (today's behavior).
    #[default]
    Dsl,
    /// Hand-owned body — never transpiled, even when fully transpilable
    /// (kills the duplicate-impl trap of a later model edit).
    Manual,
    /// Postgres function lowered from the same `Expr::to_json` payloads
    /// (issue #450 — Postgres only; refused on `database_target = "sqlite"`).
    Sql,
    /// Typed external HTTP call — typed req/resp structs, a
    /// `{domain}_service_base_url` config slot, and a pre-wired write-once
    /// client hook (B3, issue #339).
    External,
}

impl FunctionImplementation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dsl => "dsl",
            Self::Manual => "manual",
            Self::Sql => "sql",
            Self::External => "external",
        }
    }
}

/// How a modeled function is invoked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FunctionExposure {
    /// Library call only (today's behavior).
    #[default]
    Internal,
    /// Authenticated `POST /api/v1/{domain}/{snake_fn}` + OpenAPI (B1,
    /// issue #337).
    Http,
    /// pgmq `events_{domain}` consumer routing to a regen-safe handler
    /// stub (B4, issue #340).
    Event,
}

impl FunctionExposure {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Internal => "internal",
            Self::Http => "http",
            Self::Event => "event",
        }
    }
}

/// One `[functions."Name"]` binding.
#[derive(Debug, Clone, Deserialize)]
pub struct FunctionBinding {
    #[serde(default)]
    pub implementation: FunctionImplementation,
    #[serde(default)]
    pub exposure: FunctionExposure,
}

/// The whole `[functions]` table, keyed by modeled function name.
/// `BTreeMap` keeps iteration deterministic for generation.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FunctionBindings(BTreeMap<String, FunctionBinding>);

impl FunctionBindings {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn get(&self, name: &str) -> Option<&FunctionBinding> {
        self.0.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &FunctionBinding)> {
        self.0.iter()
    }

    /// Strict validation for everything checkable WITHOUT the graph:
    /// `implementation = "sql"` requires the Postgres dialect (a PL/pgSQL
    /// function has no sqlite counterpart) — any other binding value is
    /// legal since the placement slices landed (issues #450/#339/#340).
    /// Graph-known function names are validated by the driver's
    /// `ValidationPass` (it is the only layer that has the graph).
    pub fn validate_for_target(&self, database_target: &str) -> Result<(), DomainConfigError> {
        for (name, binding) in &self.0 {
            if binding.implementation == FunctionImplementation::Sql
                && database_target.eq_ignore_ascii_case("sqlite")
            {
                return Err(DomainConfigError::Invalid(format!(
                    "functions.\"{name}\": implementation \"sql\" requires \
                     database_target = \"postgres\" — a Postgres function has \
                     no sqlite counterpart"
                )));
            }
        }
        Ok(())
    }
}

impl std::ops::Index<&str> for FunctionBindings {
    type Output = FunctionBinding;

    fn index(&self, name: &str) -> &Self::Output {
        self.0.get(name).unwrap_or_else(|| {
            panic!("function binding '{name}' not declared in [functions] table")
        })
    }
}
