//! Runtime configuration for the ops harness.
//!
//! [`OpsConfig`] resolves an [`OpsManifest`](codegraph_config::OpsManifest)
//! against the filesystem: paths are made absolute relative to the manifest's
//! directory (the repo root), database targets are wrapped in [`db::PgTarget`]
//! and the metrics timer is initialised.

use std::path::{Path, PathBuf};

use codegraph_config::{OpsDbTarget, OpsManifest};

use crate::error::{OpsError, OpsResult};
use crate::metrics::Metrics;
use crate::pg::PgTarget;

/// Resolved runtime config for one harness invocation.
#[derive(Debug)]
pub struct OpsConfig {
    pub manifest: OpsManifest,
    /// Directory containing `codegraph-ops.toml` (repo root).
    pub root_dir: PathBuf,
    /// Generated app directory.
    pub app_dir: PathBuf,
    /// UI directory used by e2e (defaults to `{app_dir}/ui`).
    pub ui_dir: PathBuf,
    /// Supabase project directory (if configured).
    pub supabase_dir: Option<PathBuf>,
    /// hurl contract-test directory (if configured).
    pub hurl_dir: Option<PathBuf>,
    /// Log file for the API server.
    pub log_file: PathBuf,
    /// Plain-Postgres API target.
    pub api_db: PgTarget,
    /// Supabase postgres target (e2e).
    pub e2e_db: Option<PgTarget>,
    /// App-role target (e2e).
    pub e2e_app_db: Option<PgTarget>,
    /// Stage metrics for the current run.
    pub metrics: Metrics,
    /// Standard local Supabase keys (only meaningful for local e2e).
    pub anon_key: String,
    pub service_key: String,
    pub jwt_secret: String,
    /// Commands run before/after pipeline stages (from manifest hooks).
    pub hooks: Vec<codegraph_config::OpsHook>,
}

impl OpsConfig {
    /// Load `codegraph-ops.toml` from `manifest_path` and resolve all paths
    /// relative to its parent directory.
    pub fn load(manifest_path: &Path) -> OpsResult<Self> {
        let raw = std::fs::read_to_string(manifest_path)
            .map_err(|e| OpsError::Config(format!("cannot read {}: {e}", manifest_path.display())))?;
        let manifest: OpsManifest = toml::from_str(&raw).map_err(|e| {
            OpsError::Config(format!("invalid manifest {}: {e}", manifest_path.display()))
        })?;
        let root_dir = manifest_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        Self::from_manifest(manifest, root_dir)
    }

    /// Build a config from an already-parsed manifest.
    pub fn from_manifest(manifest: OpsManifest, root_dir: PathBuf) -> OpsResult<Self> {
        let app_dir = root_dir.join(&manifest.output_dir);
        let ui_dir = manifest
            .ui_dir
            .clone()
            .map(|d| root_dir.join(d))
            .unwrap_or_else(|| app_dir.join("ui"));
        let supabase_dir = manifest
            .supabase
            .as_ref()
            .map(|s| root_dir.join(&s.dir));
        let hurl_dir = manifest
            .hurl
            .as_ref()
            .map(|h| root_dir.join(&h.dir));

        let api_db = pg_target(&manifest.database.api, "api");
        let e2e_db = manifest
            .database
            .e2e
            .as_ref()
            .map(|t| pg_target(t, "e2e"));
        let e2e_app_db = manifest
            .database
            .e2e_app
            .as_ref()
            .map(|t| pg_target(t, "e2e_app"));

        let supabase = manifest.supabase.as_ref();
        let anon_key = supabase
            .and_then(|s| s.anon_key.clone())
            .unwrap_or_else(|| DEMO_ANON_KEY.to_string());
        let service_key = supabase
            .and_then(|s| s.service_key.clone())
            .unwrap_or_else(|| DEMO_SERVICE_KEY.to_string());
        let jwt_secret = supabase
            .and_then(|s| s.jwt_secret.clone())
            .unwrap_or_else(|| DEMO_JWT_SECRET.to_string());

        let metrics = Metrics::new();

        Ok(Self {
            hooks: manifest.hooks.clone(),
            manifest,
            root_dir,
            app_dir,
            ui_dir,
            supabase_dir,
            hurl_dir,
            log_file: PathBuf::from("/tmp/codegraph-ops-app.log"),
            api_db,
            e2e_db,
            e2e_app_db,
            metrics,
            anon_key,
            service_key,
            jwt_secret,
        })
    }

    /// API base URL (localhost-form) derived from the bind addr + port.
    pub fn api_url(&self) -> String {
        let bind = if self.manifest.servers.bind_addr.starts_with("0.0.0.0") {
            "localhost".to_string()
        } else {
            self.manifest.servers.bind_addr.clone()
        };
        format!("http://{bind}:{}", self.manifest.servers.api_port)
    }

    pub fn ui_url(&self) -> String {
        format!("http://localhost:{}", self.manifest.servers.ui_port)
    }

    /// The generated app binary name (e.g. `hr-app`).
    pub fn app_binary_name(&self) -> String {
        self.manifest.app_name.clone()
    }
}

fn pg_target(t: &OpsDbTarget, role: &str) -> PgTarget {
    PgTarget {
        host: t.host.clone(),
        port: t.port,
        user: t.user.clone(),
        password: t.password.clone(),
        db: t.database.clone(),
        role: role.to_string(),
    }
}

/// Standard local Supabase demo keys (stable across local stacks).
pub const DEMO_ANON_KEY: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZS1kZW1vIiwicm9sZSI6ImFub24iLCJleHAiOjE5ODM4MTI5OTZ9.CRXP1A7WOeoJeXxjNni43kdQwgnWNReilDMblYTn_I0";
pub const DEMO_SERVICE_KEY: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZS1kZW1vIiwicm9sZSI6InNlcnZpY2Vfcm9sZSIsImV4cCI6MTk4MzgxMjk5Nn0.EGIM96RAZx35lJzdJsyH-qQwv8Hdp7fsn3W0YpN81IU";
pub const DEMO_JWT_SECRET: &str = "super-secret-jwt-token-with-at-least-32-characters-long";

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_config::OpsDatabase;

    #[test]
    fn resolves_paths_relative_to_manifest() {
        let mut manifest = OpsManifest {
            app_name: "demo-app".into(),
            graph_binary: None,
            schemas_dir: None,
            classifier: None,
            domain_config: None,
            profile: None,
            output_dir: "generated-candidate".into(),
            ui_dir: None,
            smoke: None,
            servers: Default::default(),
            database: OpsDatabase {
                api: OpsDbTarget {
                    host: "localhost".into(),
                    port: 5432,
                    user: "u".into(),
                    password: "p".into(),
                    database: "postgres".into(),
                    reset_sql: None,
                    seed_sql: None,
                },
                e2e: None,
                e2e_app: None,
            },
            supabase: None,
            capabilities: Default::default(),
            hurl: None,
            hooks: vec![],
            extensions: vec![],
        };
        manifest.output_dir = "generated-candidate".into();
        let cfg = OpsConfig::from_manifest(manifest, PathBuf::from("/tmp/repo")).unwrap();
        assert_eq!(cfg.app_dir, PathBuf::from("/tmp/repo/generated-candidate"));
        assert_eq!(cfg.api_db.port, 5432);
        assert_eq!(cfg.api_url(), "http://localhost:3000");
    }
}
