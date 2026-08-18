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
    /// Directory containing `codegraph-ops.toml` — the base for manifest-relative
    /// paths (schemas, classifier, domains, ui, supabase, hurl, sql, hooks).
    pub root_dir: PathBuf,
    /// Workspace root (the nearest ancestor `Cargo.toml` declaring `[workspace]`).
    /// Used for `cargo` invocations and `target/` paths, which must run from the
    /// workspace root even when the manifest lives in the generated output dir.
    pub workspace_root: PathBuf,
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
        let raw = std::fs::read_to_string(manifest_path).map_err(|e| {
            OpsError::Config(format!("cannot read {}: {e}", manifest_path.display()))
        })?;
        let manifest: OpsManifest = toml::from_str(&raw).map_err(|e| {
            OpsError::Config(format!("invalid manifest {}: {e}", manifest_path.display()))
        })?;
        let root_dir = manifest_path
            .parent()
            .map(|p| p.to_path_buf())
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        Self::from_manifest(manifest, root_dir)
    }

    /// Build a config from an already-parsed manifest.
    pub fn from_manifest(manifest: OpsManifest, root_dir: PathBuf) -> OpsResult<Self> {
        let workspace_root = discover_workspace_root(&root_dir);
        let app_dir = root_dir.join(&manifest.output_dir);
        let ui_dir = manifest
            .ui_dir
            .clone()
            .map(|d| root_dir.join(d))
            .unwrap_or_else(|| app_dir.join("ui"));
        let supabase_dir = manifest.supabase.as_ref().map(|s| root_dir.join(&s.dir));
        let hurl_dir = manifest.hurl.as_ref().map(|h| root_dir.join(&h.dir));

        let api_db = pg_target(&manifest.database.api, "api");
        let e2e_db = manifest.database.e2e.as_ref().map(|t| pg_target(t, "e2e"));
        let e2e_app_db = manifest
            .database
            .e2e_app
            .as_ref()
            .map(|t| pg_target(t, "e2e_app"));

        let supabase = manifest.supabase.as_ref();
        let anon_key = supabase
            .and_then(|s| s.anon_key.as_deref().map(resolve_env))
            .unwrap_or_else(|| DEMO_ANON_KEY.to_string());
        let service_key = supabase
            .and_then(|s| s.service_key.as_deref().map(resolve_env))
            .unwrap_or_else(|| DEMO_SERVICE_KEY.to_string());
        let jwt_secret = supabase
            .and_then(|s| s.jwt_secret.as_deref().map(resolve_env))
            .unwrap_or_else(|| DEMO_JWT_SECRET.to_string());

        let metrics = Metrics::new();

        Ok(Self {
            hooks: manifest.hooks.clone(),
            manifest,
            root_dir,
            workspace_root,
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

/// Find the outermost ancestor directory (starting at `start`) whose
/// `Cargo.toml` declares a `[workspace]` section — i.e. the consumer repo root
/// where `cargo build -p {graph_binary}` and `target/` live. The generated app
/// is itself often a nested workspace (`[workspace] members = [".", "cli"]`),
/// so we keep walking up and return the LAST match (closest to the filesystem
/// root). Falls back to `start` when none is found.
fn discover_workspace_root(start: &Path) -> PathBuf {
    let mut found: Option<PathBuf> = None;
    for dir in start.ancestors() {
        let cargo = dir.join("Cargo.toml");
        if cargo.is_file()
            && std::fs::read_to_string(&cargo)
                .map(|s| s.contains("[workspace]"))
                .unwrap_or(false)
        {
            found = Some(dir.to_path_buf());
        }
    }
    found.unwrap_or_else(|| start.to_path_buf())
}

fn pg_target(t: &OpsDbTarget, role: &str) -> PgTarget {
    PgTarget {
        host: t.host.clone(),
        port: t.port,
        user: t.user.clone(),
        password: resolve_env(&t.password),
        db: t.database.clone(),
        role: role.to_string(),
    }
}

/// Replace every `{env:NAME}` placeholder in `value` with the current value
/// of the `NAME` environment variable. An unset variable expands to an empty
/// string. Plain strings (no placeholders) pass through unchanged, so
/// generated manifests without indirection keep working as-is.
///
/// Example: `"{env:DB_PASSWORD}"` → the value of `DB_PASSWORD` (or "").
pub fn resolve_env(value: &str) -> String {
    const PREFIX: &str = "{env:";
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find(PREFIX) {
        out.push_str(&rest[..start]);
        let after = &rest[start + PREFIX.len()..];
        match after.find('}') {
            Some(end) => {
                let name = &after[..end];
                out.push_str(&std::env::var(name).unwrap_or_default());
                rest = &after[end + 1..];
            }
            None => {
                // Unterminated placeholder: keep it verbatim.
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
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
            api_version: "v1".to_string(),
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

    #[test]
    fn discover_workspace_root_walks_up_to_workspace_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("out/deep")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        std::fs::write(root.join("out/Cargo.toml"), "[package]\nname = \"app\"\n").unwrap();
        // From inside the generated output dir, resolve up to the workspace root.
        assert_eq!(
            discover_workspace_root(&root.join("out/deep")),
            root.to_path_buf()
        );
        // A non-workspace Cargo.toml (the generated app) is skipped.
        assert_eq!(discover_workspace_root(&root.join("out")), root.to_path_buf());
    }

    #[test]
    fn discover_workspace_root_skips_nested_app_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("out")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        // The generated app is itself a workspace — the repo root must win.
        std::fs::write(
            root.join("out/Cargo.toml"),
            "[workspace]\nmembers = [\".\", \"cli\"]\n",
        )
        .unwrap();
        assert_eq!(discover_workspace_root(&root.join("out")), root.to_path_buf());
    }

    #[test]
    fn discover_workspace_root_falls_back_to_start() {
        let dir = tempfile::tempdir().unwrap();
        // No Cargo.toml anywhere up the chain → return the start dir.
        assert_eq!(
            discover_workspace_root(&dir.path().join("a/b")),
            dir.path().join("a/b")
        );
    }

    #[test]
    fn resolve_env_expands_set_and_unset_vars() {
        std::env::set_var("CG_OPS_TEST_SET_VAR", "hunter2");
        std::env::remove_var("CG_OPS_TEST_UNSET_VAR");
        // Set variable expands; unset expands to empty.
        assert_eq!(
            resolve_env("user={env:CG_OPS_TEST_SET_VAR}"),
            "user=hunter2"
        );
        assert_eq!(
            resolve_env("prefix-{env:CG_OPS_TEST_UNSET_VAR}-suffix"),
            "prefix--suffix"
        );
        // Multiple placeholders are all replaced.
        assert_eq!(
            resolve_env("{env:CG_OPS_TEST_SET_VAR}:{env:CG_OPS_TEST_UNSET_VAR}"),
            "hunter2:"
        );
    }

    #[test]
    fn resolve_env_passes_plain_strings_through() {
        std::env::remove_var("CG_OPS_TEST_UNSET_VAR");
        assert_eq!(resolve_env("postgres"), "postgres");
        assert_eq!(resolve_env(""), "");
        // No placeholder → untouched even if it contains braces.
        assert_eq!(resolve_env("{not-env:VAR}"), "{not-env:VAR}");
        // Unterminated placeholder is kept verbatim.
        assert_eq!(resolve_env("x={env:UNCLOSED"), "x={env:UNCLOSED");
    }

    #[test]
    fn from_manifest_resolves_env_in_db_password_and_keys() {
        std::env::set_var("CG_OPS_TEST_DB_PW", "pw-from-env");
        std::env::set_var("CG_OPS_TEST_ANON", "anon-from-env");
        let manifest = OpsManifest {
            app_name: "demo-app".into(),
            graph_binary: None,
            schemas_dir: None,
            classifier: None,
            domain_config: None,
            profile: None,
            output_dir: "generated-candidate".into(),
            ui_dir: None,
            smoke: None,
            api_version: "v1".to_string(),
            servers: Default::default(),
            database: OpsDatabase {
                api: OpsDbTarget {
                    host: "localhost".into(),
                    port: 5432,
                    user: "u".into(),
                    password: "{env:CG_OPS_TEST_DB_PW}".into(),
                    database: "postgres".into(),
                    reset_sql: None,
                    seed_sql: None,
                },
                e2e: None,
                e2e_app: None,
            },
            supabase: Some(codegraph_config::OpsSupabase {
                dir: "supabase".into(),
                health_url: None,
                anon_key: Some("{env:CG_OPS_TEST_ANON}".into()),
                service_key: None,
                jwt_secret: None,
            }),
            capabilities: Default::default(),
            hurl: None,
            hooks: vec![],
            extensions: vec![],
        };
        let cfg = OpsConfig::from_manifest(manifest, PathBuf::from("/tmp/repo")).unwrap();
        assert_eq!(cfg.api_db.password, "pw-from-env");
        assert_eq!(cfg.anon_key, "anon-from-env");
    }
}
