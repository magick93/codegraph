use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::{GenerationEntry, ProjectConfig};
use codegraph_config::DomainConfig;

/// Emits the Cornucopia code-generation scaffolding when the persistence
/// provider is `cornucopia`:
///
/// - `cornucopia-queries/cornucopia.toml` — Cornucopia codegen config
///   (query dir, destination, type mappings, generated-crate manifest)
/// - `cornucopia-queries/build.rs` — connects to the live Postgres and runs
///   `cornucopia::gen_live()` to produce the typed queries crate
/// - `cornucopia-queries/Cargo.toml` — initial manifest so the first `cargo
///   build` has a crate to build (subsequent builds regenerate it via
///   build.rs)
///
/// The generated crate is a path dependency of the app under the name
/// `cornucopia-queries`.
pub struct CornucopiaConfigGenerator {
    output_dir: PathBuf,
}

impl CornucopiaConfigGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for CornucopiaConfigGenerator {
    fn name(&self) -> &str {
        "cornucopia_config"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        _tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        if !project.is_cornucopia() {
            return Ok(Vec::new());
        }
        let base = self.output_dir.join("cornucopia-queries");
        Ok(vec![
            GeneratedFile {
                path: base.join("cornucopia.toml"),
                content: cornucopia_toml(),
            },
            GeneratedFile {
                path: base.join("build.rs"),
                content: BUILD_RS.to_string(),
            },
            GeneratedFile {
                path: base.join("Cargo.toml"),
                content: initial_manifest(),
            },
            // Placeholder lib target — cargo validates the manifest's targets
            // before build.rs runs, so the crate must have a lib.rs from the
            // very first build. `gen_live()` overwrites it with the real
            // generated module tree.
            GeneratedFile {
                path: base.join("src").join("lib.rs"),
                content: PLACEHOLDER_LIB_RS.to_string(),
            },
        ])
    }
}

const PLACEHOLDER_LIB_RS: &str = r#"//! Placeholder library target for the cornucopia-queries crate.
//! Overwritten on the first build by `cornucopia::gen_live()` in build.rs.
"#;

const BUILD_RS: &str = r#"//! Cornucopia code generation — connects to the live Postgres database,
//! introspects the query annotations in ../queries, and emits the typed
//! `cornucopia-queries` crate sources.
//!
//! Requires the `CORNUCOPIA_DATABASE_URL` environment variable to point at a
//! Postgres instance with the full (migrated) schema.

use std::path::PathBuf;

fn main() {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let manifest_dir = PathBuf::from(&manifest_dir);

    let config_path = manifest_dir.join("cornucopia.toml");
    // Snapshot the scaffolding before gen_live runs: cornucopia's persist
    // step wipes its destination directory (remove_dir_all) and the
    // destination IS this crate root, so build.rs and cornucopia.toml would
    // otherwise vanish — breaking every subsequent build (and racing hard
    // when wrangler dev builds several workers in parallel). Restore both
    // files after generation.
    let config_bytes = std::fs::read(&config_path).expect("failed to read cornucopia.toml");
    let self_source = std::fs::read_to_string(manifest_dir.join("build.rs"))
        .expect("failed to read build.rs");

    let mut config = cornucopia::config::Config::from_file(&config_path)
        .expect("failed to read cornucopia.toml");

    // Resolve paths relative to the crate root — cargo may run with any CWD.
    config.queries = manifest_dir.join("..").join("queries");
    config.destination = manifest_dir.clone();

    let db_url = std::env::var("CORNUCOPIA_DATABASE_URL")
        .expect("CORNUCOPIA_DATABASE_URL must be set to build cornucopia queries");

    let rt = tokio::runtime::Runtime::new().expect("failed to start tokio runtime");
    rt.block_on(async {
        let (client, conn) = tokio_postgres::connect(&db_url, tokio_postgres::NoTls)
            .await
            .expect("failed to connect to Postgres for cornucopia codegen");
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                eprintln!("cornucopia codegen connection error: {e}");
            }
        });
        // Serialize concurrent codegen runs (e.g. wrangler dev building
        // several workers in parallel) on a well-known advisory lock.
        client
            .execute("SELECT pg_advisory_lock(0x436f726e75436174)", &[])
            .await
            .expect("failed to acquire cornucopia codegen lock");
        let result = cornucopia::gen_live(&client, config);
        let _ = client
            .execute("SELECT pg_advisory_unlock(0x436f726e75436174)", &[])
            .await;
        result.expect("cornucopia codegen failed");
    });

    // Restore the codegen scaffolding wiped by gen_live's persist step.
    std::fs::write(&config_path, config_bytes).expect("failed to restore cornucopia.toml");
    std::fs::write(manifest_dir.join("build.rs"), self_source)
        .expect("failed to restore build.rs");

    println!("cargo:rerun-if-changed=cornucopia.toml");
    println!("cargo:rerun-if-changed=../queries");
    println!("cargo:rerun-if-env-changed=CORNUCOPIA_DATABASE_URL");
}
"#;

fn cornucopia_toml() -> String {
    let mut toml = String::with_capacity(2048);
    toml.push_str("# Cornucopia configuration generated by codegraph\n");
    toml.push_str("# persistence_provider = \"cornucopia\"\n\n");
    toml.push_str("queries = \"../queries\"\n");
    toml.push_str("destination = \".\"\n");
    toml.push_str("sync = false\n");
    toml.push_str("async = true\n\n");
    toml.push_str("[manifest.package]\n");
    toml.push_str("name = \"cornucopia-queries\"\n");
    toml.push_str("version = \"0.1.0\"\n");
    toml.push_str("publish = false\n\n");
    toml.push_str("[manifest.dependencies]\n");
    toml.push_str("pgvector = \"0.3\"\n\n");
    toml.push_str("[manifest.build-dependencies]\n");
    toml.push_str("cornucopia = \"1.0\"\n");
    toml.push_str("tokio = { version = \"1\", features = [\"rt\", \"macros\"] }\n");
    toml.push_str("tokio-postgres = \"0.7\"\n\n");
    toml.push_str("[types]\n");
    toml.push_str("derive-traits = [\"serde::Serialize\", \"serde::Deserialize\"]\n\n");
    toml.push_str("# Codegraph type mappings — covers all classified property types\n");
    toml.push_str("[types.mapping]\n");
    toml.push_str("\"pg_catalog.uuid\" = \"uuid::Uuid\"\n");
    toml.push_str(
        "\"pg_catalog.text\" = { rust-type = \"String\", is-copy = false }\n",
    );
    toml.push_str(
        "\"pg_catalog.varchar\" = { rust-type = \"String\", is-copy = false }\n",
    );
    toml.push_str("\"pg_catalog.timestamptz\" = \"chrono::DateTime<chrono::Utc>\"\n");
    toml.push_str("\"pg_catalog.timestamp\" = \"chrono::NaiveDateTime\"\n");
    toml.push_str("\"pg_catalog.bool\" = \"bool\"\n");
    toml.push_str("\"pg_catalog.int4\" = \"i32\"\n");
    toml.push_str("\"pg_catalog.int8\" = \"i64\"\n");
    toml.push_str("\"pg_catalog.float4\" = \"f32\"\n");
    toml.push_str("\"pg_catalog.float8\" = \"f64\"\n");
    toml.push_str(
        "\"pg_catalog.jsonb\" = { rust-type = \"serde_json::Value\", is-copy = false }\n",
    );
    toml.push_str(
        "\"pg_catalog.json\" = { rust-type = \"serde_json::Value\", is-copy = false }\n",
    );
    toml.push_str("\"pg_catalog.date\" = \"chrono::NaiveDate\"\n");
    toml.push_str("\"pg_catalog.bytea\" = { rust-type = \"Vec<u8>\", is-copy = false }\n");
    // Numeric travels as String through the SQL layer: rust_decimal's
    // postgres FromSql/ToSql impls are gated off on wasm32
    // (rust_decimal 1.x `mod postgres` is `cfg(not(target_arch = "wasm32"))`),
    // so mapping to Decimal would break Cloudflare Worker builds. The
    // repository adapters parse the String back to `rust_decimal::Decimal`
    // at the DTO boundary (same pattern as every typed column).
    toml.push_str(
        "\"pg_catalog.numeric\" = { rust-type = \"String\", is-copy = false }\n",
    );
    toml.push_str("\"pg_catalog.inet\" = \"std::net::IpAddr\"\n");
    toml.push_str("\"pg_catalog.tstzrange\" = { rust-type = \"String\", is-copy = false }\n");
    toml.push_str("\"pg_catalog.daterange\" = { rust-type = \"String\", is-copy = false }\n");
    toml.push_str("\"pg_catalog.tsrange\" = { rust-type = \"String\", is-copy = false }\n");
    toml.push_str("\"pg_catalog.int4range\" = { rust-type = \"String\", is-copy = false }\n");
    toml.push_str("\"pg_catalog.int8range\" = { rust-type = \"String\", is-copy = false }\n");
    toml.push_str("\"pg_catalog.numrange\" = { rust-type = \"String\", is-copy = false }\n");
    toml.push_str(
        "\"public.vector\" = { rust-type = \"pgvector::Vector\", is-copy = false }\n",
    );
    toml.push_str(
        "\"extensions.vector\" = { rust-type = \"pgvector::Vector\", is-copy = false }\n",
    );
    toml.push_str("\"public.geometry\" = { rust-type = \"String\", is-copy = false }\n");
    toml.push_str("\"extensions.geometry\" = { rust-type = \"String\", is-copy = false }\n");
    toml.push_str("\"public.geography\" = { rust-type = \"String\", is-copy = false }\n");
    toml.push_str("\"extensions.geography\" = { rust-type = \"String\", is-copy = false }\n");
    toml
}

/// Initial Cargo.toml for the generated queries crate. Cornucopia's
/// `gen_live()` regenerates this file on every build (from
/// `cornucopia.toml`'s `[manifest]` + its dependency analysis); this initial
/// copy just needs to be complete enough for the first build.
fn initial_manifest() -> String {
    r#"[package]
name = "cornucopia-queries"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
publish = false

[dependencies]
postgres-types = { version = "0.2", features = ["derive"] }
postgres-protocol = "0.6"
postgres = { version = "0.19", optional = true, features = ["with-chrono-0_4", "with-uuid-1", "with-serde_json-1"] }
tokio-postgres = { version = "0.7", default-features = false, features = ["with-chrono-0_4", "with-uuid-1", "with-serde_json-1"] }
futures = "0.3"
deadpool-postgres = { version = "0.14", optional = true }
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["serde"] }
rust_decimal = { version = "1", features = ["db-postgres"] }
serde = { version = "1", features = ["derive"] }
serde_json = { version = "1", features = ["raw_value"] }
pgvector = "0.3"

[features]
default = ["dep:postgres", "deadpool"]
deadpool = ["dep:deadpool-postgres", "tokio-postgres/default"]
wasm-async = ["tokio-postgres/js", "chrono/wasmbind"]

[build-dependencies]
cornucopia = "1.0"
tokio = { version = "1", features = ["rt", "macros"] }
tokio-postgres = "0.7"
"#
    .to_string()
}
