//! codegraph-ops — Rust test & deploy harness for codegraph-generated apps.
//!
//! Re-imagines the hand-written bash test harness (test.sh, lib/common.sh,
//! lib/migrate.sh, deploy/smoke-test.sh, scripts/quality-check.sh) as a
//! composable Rust library driven by a generated [`OpsManifest`] config file.
//!
//! Consumers add a thin binary (see the `ops` generator emitting
//! `testkit/Cargo.toml` + `testkit/src/main.rs`) and run:
//!
//! ```text
//! cargo run -p testkit -- api
//! cargo run -p testkit -- e2e
//! cargo run -p testkit -- smoke
//! ```
//!
//! Subcommands: `api`, `cli`, `e2e`, `ui`, `full`, `clean`, `smoke`,
//! `quality`, `ext <name>`.
//!
//! External integrations (Xero, Stripe, IRD, ...) plug in via the
//! [`ext::TestExtension`] trait or manifest `[[extensions]]` exec entries —
//! codegraph itself stays agnostic to consumer-specific integrations.

pub mod cli;
pub mod config;
pub mod db;
pub mod env;
pub mod error;
pub mod ext;
pub mod metrics;
pub mod migrate;
pub mod output;
pub mod pg;
pub mod proc;
pub mod suites;
pub mod wait;

pub use config::OpsConfig;
pub use error::{OpsError, OpsResult};
pub use pg::PgTarget;
