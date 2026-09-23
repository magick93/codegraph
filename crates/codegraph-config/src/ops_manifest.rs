//! `OpsManifest` — project configuration for the codegraph-ops test & deploy
//! harness.
//!
//! This is the "project struct" shared between codegraph (which seeds it from
//! `ProjectConfig`/`BuildPlan` at generation time) and `codegraph-ops` (which
//! loads it at test time). Consumers edit the generated `codegraph-ops.toml`
//! to add project-specific hooks (rsync syncing, integration migrations, ...)
//! and external test extensions (Xero, Stripe, IRD, ...).
//!
//! Codegraph itself is agnostic to consumer-specific integrations: they are
//! expressed as `[[hooks]]` exec steps and `[[extensions]]` entries.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level ops manifest, serialized as `codegraph-ops.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsManifest {
    /// Generated application name (e.g. `hr-app`).
    pub app_name: String,
    /// Name of the codegen binary crate in the consumer workspace (e.g.
    /// `hr-graph`). Used by the `generate` step. Optional — if unset the
    /// harness skips generation.
    #[serde(default)]
    pub graph_binary: Option<String>,
    /// Directory of JSON schemas passed to the codegen binary.
    #[serde(default)]
    pub schemas_dir: Option<PathBuf>,
    /// `.mox` model files passed to the codegen binary (one `--mox-files`
    /// flag per entry, manifest order). When non-empty these are the model
    /// source for harness-driven regeneration and `schemas_dir`/`classifier`
    /// are ignored for generation — the mox pipeline needs no classifier.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mox_files: Vec<String>,
    /// Rosetta (Rune DSL) `.rosetta` model files passed to the codegen
    /// binary (one `--rosetta-files` flag per entry, manifest order).
    /// Rosetta-first manifests carry one `model/<domain>.rosetta` entry per
    /// domain and, like mox-first ones, need no `schemas_dir`/`classifier`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rosetta_files: Vec<String>,
    /// Classifier config file passed to the codegen binary.
    #[serde(default)]
    pub classifier: Option<PathBuf>,
    /// Domain config file (`domains.toml`) passed to the codegen binary.
    #[serde(default)]
    pub domain_config: Option<PathBuf>,
    /// Profile name passed to the codegen binary.
    #[serde(default)]
    pub profile: Option<String>,
    /// Output directory for generated code (default: `generated-app`).
    #[serde(default = "default_output_dir")]
    pub output_dir: PathBuf,
    /// UI directory used by e2e (default: `{output_dir}/ui`). Consumers that
    /// sync generated UI into a monorepo can point this at their web app dir.
    #[serde(default)]
    pub ui_dir: Option<PathBuf>,
    /// Smoke-test entity used by the api suite's curl CRUD checks.
    #[serde(default)]
    pub smoke: Option<OpsSmoke>,
    /// API version prefix used by the generated routes (`v1` → `/api/v1/...`).
    #[serde(default = "default_api_version")]
    pub api_version: String,
    /// Server ports / bind configuration.
    #[serde(default)]
    pub servers: OpsServers,
    /// Database targets for the API and (optional) E2E runs.
    pub database: OpsDatabase,
    /// Supabase local stack configuration (required for `e2e`).
    #[serde(default)]
    pub supabase: Option<OpsSupabase>,
    /// Which generated capabilities exist (drives which suites run).
    #[serde(default)]
    pub capabilities: OpsCapabilities,
    /// hurl contract-test configuration (used by the `api` suite).
    #[serde(default)]
    pub hurl: Option<OpsHurl>,
    /// Project-specific hook steps executed at named points of the pipeline.
    #[serde(default)]
    pub hooks: Vec<OpsHook>,
    /// External test extensions (consumer-specific integrations).
    #[serde(default)]
    pub extensions: Vec<OpsExtension>,
}

fn default_output_dir() -> PathBuf {
    PathBuf::from("generated-app")
}

fn default_api_version() -> String {
    "v1".to_string()
}

/// Ports and bind address for the API and UI servers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpsServers {
    pub api_port: u16,
    pub ui_port: u16,
    pub bind_addr: String,
}

impl Default for OpsServers {
    fn default() -> Self {
        Self {
            api_port: 3000,
            ui_port: 5173,
            bind_addr: "0.0.0.0".to_string(),
        }
    }
}

/// A postgres target: host/port/user/password/database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsDbTarget {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
    /// SQL file that resets the DB to a clean state before API tests.
    #[serde(default)]
    pub reset_sql: Option<PathBuf>,
    /// SQL file that seeds workflow definitions / API keys after migrate.
    #[serde(default)]
    pub seed_sql: Option<PathBuf>,
    /// Postgres role the generated API connects as (default `app_user`). The
    /// harness grants it DML on domain tables after migration and verifies the
    /// grants exist.
    #[serde(default)]
    pub grant_role: Option<String>,
    /// Fail the `api` suite when the grant role is missing DML on any domain
    /// table after migration (default `false` → warn instead).
    #[serde(default)]
    pub grant_strict: Option<bool>,
}

/// Database targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsDatabase {
    /// Plain-Postgres target used by the `api` suite.
    pub api: OpsDbTarget,
    /// Supabase postgres target (port 54322) used by the `e2e` suite.
    #[serde(default)]
    pub e2e: Option<OpsDbTarget>,
    /// App-role target (RLS-aware) used by the running API in `e2e`.
    #[serde(default)]
    pub e2e_app: Option<OpsDbTarget>,
}

/// Supabase local stack paths and standard local keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsSupabase {
    /// Directory containing the supabase project (with `supabase/` inside).
    pub dir: PathBuf,
    /// Health URL to probe for an already-running stack (default:
    /// `http://localhost:54321/auth/v1/health`).
    #[serde(default)]
    pub health_url: Option<String>,
    /// Standard local anon key. Seeded from the known Supabase demo key.
    #[serde(default)]
    pub anon_key: Option<String>,
    /// Standard local service-role key.
    #[serde(default)]
    pub service_key: Option<String>,
    /// JWT secret used for local auth tokens.
    #[serde(default)]
    pub jwt_secret: Option<String>,
}

/// Capabilities of the generated app (mirrors profile capabilities).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpsCapabilities {
    pub has_cli: bool,
    pub has_ui: bool,
    pub has_admin_cli: bool,
    pub has_grpc: bool,
    pub database_target: String,
    pub persistence_provider: String,
}

impl Default for OpsCapabilities {
    fn default() -> Self {
        Self {
            has_cli: false,
            has_ui: false,
            has_admin_cli: false,
            has_grpc: false,
            database_target: "postgres".to_string(),
            persistence_provider: "sea_orm".to_string(),
        }
    }
}

/// hurl contract-test configuration for the `api` suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpsHurl {
    /// Directory containing `*.hurl` contract tests.
    pub dir: PathBuf,
    /// Basenames of hurl files to skip in the main loop (run separately).
    pub skip: Vec<String>,
    /// Org A id used to provision API keys (default: `...0001`).
    pub org_id_a: Option<String>,
    /// Org B id used to provision the second tenant's API key (RLS tests).
    pub org_id_b: Option<String>,
    /// Provision a third, read-only API key for org A and expose it to hurl
    /// as the `api_key_limited` variable, so contract files can assert the
    /// scope-denial path (out-of-scope writes → 403 INSUFFICIENT_SCOPE, the
    /// HTTP mapping of the `scope_enforced_*` RLS policies).
    #[serde(default)]
    pub limited_key: bool,
}

impl Default for OpsHurl {
    fn default() -> Self {
        Self {
            dir: PathBuf::from("hurl"),
            skip: Vec::new(),
            org_id_a: Some("00000000-0000-0000-0000-000000000001".to_string()),
            org_id_b: Some("00000000-0000-0000-0000-000000000002".to_string()),
            limited_key: false,
        }
    }
}

/// Entity-level smoke config for the `api` suite's curl CRUD checks.
/// The generic harness cannot know a consumer's entities, so consumers
/// configure one here (e.g. `recruiting/candidate`) and the suite exercises
/// POST/GET/list against it, asserting the `data`/`meta` envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsSmoke {
    /// Entity route path, e.g. `recruiting/candidate`.
    pub entity: String,
    /// Resolved plural route path, e.g. `recruiting/candidates` — the exact
    /// path segment the generated routers nest under
    /// (`/api/{api_version}/{route}`). When absent the harness pluralizes
    /// `entity` with the codegen templates' simple pluralization rules.
    #[serde(default)]
    pub route: Option<String>,
    /// JSON body for the POST create check (can be `{}` for minimal fields).
    #[serde(default = "default_smoke_body")]
    pub create_body: String,
}

fn default_smoke_body() -> String {
    "{}".to_string()
}

/// A named hook executed with `sh -c` at a pipeline point.
/// `on` values: `pre_generate`, `post_generate`, `post_migrate`,
/// `pre_e2e`, `post_e2e`, `pre_api`, `post_api`, `pre_playwright`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsHook {
    pub name: String,
    /// Shell command (run via `sh -c`, args appended).
    pub exec: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub on: Option<String>,
}

/// An external test extension. Either a trait-backed extension registered by
/// the consumer's own crate (in-process) or an `exec` command run as a
/// subprocess.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsExtension {
    pub name: String,
    /// Optional shell command run for this extension (out-of-process).
    #[serde(default)]
    pub exec: Option<String>,
    /// Whether the extension needs the API running first.
    #[serde(default)]
    pub requires_api: bool,
    #[serde(default)]
    pub args: Vec<String>,
}

impl OpsManifest {
    /// Load and parse a manifest file.
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        toml::from_str(&raw).map_err(|e| format!("invalid manifest {}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_roundtrip() {
        let m = OpsManifest {
            app_name: "hr-app".into(),
            graph_binary: Some("hr-graph".into()),
            schemas_dir: Some("4_5RC1".into()),
            mox_files: Vec::new(),
            rosetta_files: Vec::new(),
            classifier: Some("classifier.toml".into()),
            domain_config: Some("domains.toml".into()),
            profile: Some("default".into()),
            output_dir: PathBuf::from("generated-candidate"),
            ui_dir: None,
            smoke: None,
            api_version: "v1".to_string(),
            servers: OpsServers::default(),
            database: OpsDatabase {
                api: OpsDbTarget {
                    host: "localhost".into(),
                    port: 5432,
                    user: "nuvix_admin".into(),
                    password: "postgres".into(),
                    database: "postgres".into(),
                    reset_sql: None,
                    seed_sql: None,
                    grant_role: None,
                    grant_strict: None,
                },
                e2e: None,
                e2e_app: None,
            },
            supabase: None,
            capabilities: OpsCapabilities::default(),
            hurl: Some(OpsHurl::default()),
            hooks: vec![OpsHook {
                name: "sync-ui".into(),
                exec: "rsync -a src/ dst/".into(),
                args: vec![],
                on: Some("post_generate".into()),
            }],
            extensions: vec![OpsExtension {
                name: "xero".into(),
                exec: Some("hr-xero-test".into()),
                requires_api: true,
                args: vec![],
            }],
        };
        let toml_str = toml::to_string(&m).unwrap();
        let back: OpsManifest = toml::from_str(&toml_str).unwrap();
        assert_eq!(back.app_name, "hr-app");
        assert_eq!(back.hooks.len(), 1);
        assert_eq!(back.extensions[0].name, "xero");
    }

    #[test]
    fn defaults_apply_on_partial() {
        let raw = r#"
app_name = "demo-app"
database.api = { host = "localhost", port = 5432, user = "u", password = "p", database = "postgres" }
"#;
        let m: OpsManifest = toml::from_str(raw).unwrap();
        assert_eq!(m.servers.api_port, 3000);
        assert_eq!(m.output_dir, PathBuf::from("generated-app"));
        assert_eq!(m.capabilities.database_target, "postgres");
        assert!(m.hurl.is_none());
    }

    #[test]
    fn smoke_without_route_parses_backward_compat() {
        let raw = r#"
app_name = "demo-app"
database.api = { host = "localhost", port = 5432, user = "u", password = "p", database = "postgres" }

[smoke]
entity = "recruiting/candidate"
create_body = "{}"
"#;
        let m: OpsManifest = toml::from_str(raw).unwrap();
        let smoke = m.smoke.expect("smoke section should parse");
        assert_eq!(smoke.entity, "recruiting/candidate");
        assert!(
            smoke.route.is_none(),
            "legacy manifests without `route` must still parse"
        );
        assert_eq!(smoke.create_body, "{}");
    }

    #[test]
    fn smoke_route_roundtrips() {
        let raw = r#"
app_name = "demo-app"
database.api = { host = "localhost", port = 5432, user = "u", password = "p", database = "postgres" }

[smoke]
entity = "recruiting/candidate"
route = "recruiting/candidates"
"#;
        let m: OpsManifest = toml::from_str(raw).unwrap();
        assert_eq!(
            m.smoke.unwrap().route.as_deref(),
            Some("recruiting/candidates")
        );
    }

    #[test]
    fn hurl_limited_key_defaults_to_false() {
        let raw = r#"
app_name = "demo-app"
database.api = { host = "localhost", port = 5432, user = "u", password = "p", database = "postgres" }

[hurl]
dir = "hurl"
"#;
        let m: OpsManifest = toml::from_str(raw).unwrap();
        let hurl = m.hurl.expect("hurl section should parse");
        assert!(
            !hurl.limited_key,
            "limited_key must default to false so legacy manifests keep their behavior"
        );
    }

    #[test]
    fn mox_files_default_empty() {
        let raw = r#"
app_name = "demo-app"
database.api = { host = "localhost", port = 5432, user = "u", password = "p", database = "postgres" }
"#;
        let m: OpsManifest = toml::from_str(raw).unwrap();
        assert!(
            m.mox_files.is_empty(),
            "mox_files must default to empty so legacy manifests parse unchanged"
        );
    }

    #[test]
    fn mox_files_parse_from_toml_in_manifest_order() {
        let raw = r#"
app_name = "demo-app"
mox_files = ["model/common.mox", "model/billing.mox"]
database.api = { host = "localhost", port = 5432, user = "u", password = "p", database = "postgres" }
"#;
        let m: OpsManifest = toml::from_str(raw).unwrap();
        assert_eq!(
            m.mox_files,
            vec![
                "model/common.mox".to_string(),
                "model/billing.mox".to_string()
            ]
        );
    }

    #[test]
    fn mox_files_roundtrip_preserved() {
        let raw = r#"
app_name = "demo-app"
mox_files = ["model/common.mox", "model/billing.mox"]
database.api = { host = "localhost", port = 5432, user = "u", password = "p", database = "postgres" }
"#;
        let m: OpsManifest = toml::from_str(raw).unwrap();
        let serialized = toml::to_string(&m).unwrap();
        let back: OpsManifest = toml::from_str(&serialized).unwrap();
        assert_eq!(back.mox_files, m.mox_files);
    }

    #[test]
    fn mox_files_omitted_when_empty_on_serialize() {
        let raw = r#"
app_name = "demo-app"
database.api = { host = "localhost", port = 5432, user = "u", password = "p", database = "postgres" }
"#;
        let m: OpsManifest = toml::from_str(raw).unwrap();
        let serialized = toml::to_string(&m).unwrap();
        assert!(
            !serialized.contains("mox_files"),
            "empty mox_files must not appear in serialized manifests: {serialized}"
        );
    }

    #[test]
    fn mox_files_empty_array_accepted() {
        let raw = r#"
app_name = "demo-app"
mox_files = []
database.api = { host = "localhost", port = 5432, user = "u", password = "p", database = "postgres" }
"#;
        let m: OpsManifest = toml::from_str(raw).unwrap();
        assert!(m.mox_files.is_empty());
    }

    #[test]
    fn rosetta_files_default_empty() {
        let raw = r#"
app_name = "demo-app"
database.api = { host = "localhost", port = 5432, user = "u", password = "p", database = "postgres" }
"#;
        let m: OpsManifest = toml::from_str(raw).unwrap();
        assert!(
            m.rosetta_files.is_empty(),
            "rosetta_files must default to empty so legacy manifests parse unchanged"
        );
    }

    #[test]
    fn rosetta_files_parse_from_toml_in_manifest_order() {
        let raw = r#"
app_name = "demo-app"
rosetta_files = ["model/common.rosetta", "model/billing.rosetta"]
database.api = { host = "localhost", port = 5432, user = "u", password = "p", database = "postgres" }
"#;
        let m: OpsManifest = toml::from_str(raw).unwrap();
        assert_eq!(
            m.rosetta_files,
            vec![
                "model/common.rosetta".to_string(),
                "model/billing.rosetta".to_string()
            ]
        );
    }

    #[test]
    fn rosetta_files_roundtrip_preserved() {
        let raw = r#"
app_name = "demo-app"
rosetta_files = ["model/common.rosetta", "model/billing.rosetta"]
database.api = { host = "localhost", port = 5432, user = "u", password = "p", database = "postgres" }
"#;
        let m: OpsManifest = toml::from_str(raw).unwrap();
        let serialized = toml::to_string(&m).unwrap();
        let back: OpsManifest = toml::from_str(&serialized).unwrap();
        assert_eq!(back.rosetta_files, m.rosetta_files);
    }

    #[test]
    fn rosetta_files_omitted_when_empty_on_serialize() {
        let raw = r#"
app_name = "demo-app"
database.api = { host = "localhost", port = 5432, user = "u", password = "p", database = "postgres" }
"#;
        let m: OpsManifest = toml::from_str(raw).unwrap();
        let serialized = toml::to_string(&m).unwrap();
        assert!(
            !serialized.contains("rosetta_files"),
            "empty rosetta_files must not appear in serialized manifests: {serialized}"
        );
    }

    #[test]
    fn hurl_limited_key_parses_when_requested() {
        let raw = r#"
app_name = "demo-app"
database.api = { host = "localhost", port = 5432, user = "u", password = "p", database = "postgres" }

[hurl]
dir = "hurl"
skip = []
org_id_a = "00000000-0000-0000-0000-000000000001"
org_id_b = "00000000-0000-0000-0000-000000000002"
limited_key = true
"#;
        let m: OpsManifest = toml::from_str(raw).unwrap();
        let hurl = m.hurl.expect("hurl section should parse");
        assert!(hurl.limited_key);
        assert_eq!(hurl.dir, PathBuf::from("hurl"));
        assert!(hurl.skip.is_empty());
        assert_eq!(
            hurl.org_id_a.as_deref(),
            Some("00000000-0000-0000-0000-000000000001")
        );
        assert_eq!(
            hurl.org_id_b.as_deref(),
            Some("00000000-0000-0000-0000-000000000002")
        );
    }
}
