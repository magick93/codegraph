//! Extension protocol + pipeline hooks.
//!
//! External integrations (Xero, Stripe, IRD, ...) plug into the harness either
//! as manifest `[[extensions]]` exec entries (out-of-process, run via
//! `sh -c`) or as in-process trait implementations registered via
//! [`register_extension`]. Hooks are the lighter-weight sibling: named `sh -c`
//! steps run at pipeline points (`pre_generate`, `post_generate`, ...).

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use crate::config::OpsConfig;
use crate::error::{OpsError, OpsResult};
use crate::output;

/// Context passed to a [`TestExtension`].
pub struct OpsContext<'a> {
    pub config: &'a OpsConfig,
    /// Extra args passed after `ext <name>` on the CLI.
    pub args: &'a [String],
}

/// Boxed future returned by [`TestExtension::run`] (keeps the trait
/// dyn-compatible so extensions can be registered as `Box<dyn TestExtension>`).
pub type ExtensionFuture<'a> = Pin<Box<dyn Future<Output = OpsResult<()>> + Send + 'a>>;

/// A pluggable test extension (Xero, Stripe, IRD, ...). Consumers implement
/// this trait in their own crates and register via [`register_extension`].
pub trait TestExtension: Send + Sync {
    fn name(&self) -> &str;
    /// Whether this extension needs the API running first (checked at run).
    fn requires_api_running(&self) -> bool {
        false
    }
    fn run(&self, ctx: &OpsContext<'_>) -> ExtensionFuture<'_>;
}

/// Global registry of in-process extensions. `std::sync::Mutex` on purpose:
/// [`register_extension`] may be called before the tokio runtime exists.
static REGISTRY: OnceLock<Mutex<Vec<Box<dyn TestExtension>>>> = OnceLock::new();

fn registry() -> &'static Mutex<Vec<Box<dyn TestExtension>>> {
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

/// Register an in-process extension (call before entering the async runtime).
/// Registering a name that already exists replaces the earlier entry.
pub fn register_extension(ext: Box<dyn TestExtension>) {
    let mut reg = registry().lock().expect("extension registry poisoned");
    if let Some(existing) = reg.iter_mut().find(|e| e.name() == ext.name()) {
        *existing = ext;
        return;
    }
    reg.push(ext);
}

/// Run extension `name`. Resolution order:
/// 1. Manifest `[[extensions]]` entry with matching name → run its `exec`
///    (if set) via `sh -c "{exec} {args...}"` in config.root_dir.
/// 2. Trait-registered extensions.
///
/// Returns Err(Config) if unknown.
pub async fn run_extension(name: &str, config: &OpsConfig, args: &[String]) -> OpsResult<()> {
    if let Some(entry) = config.manifest.extensions.iter().find(|e| e.name == name) {
        if let Some(exec) = &entry.exec {
            if entry.requires_api && !api_running(config).await {
                output::warn(format!(
                    "extension {name} requires the API running — run 'api --keep' first"
                ));
                return Err(OpsError::Config(format!(
                    "extension {name} requires the API running"
                )));
            }
            return run_exec(exec, &entry.args, &config.root_dir).await;
        }
        // No exec: fall through to the in-process registry.
    }

    let ctx = OpsContext { config, args };
    let ext = {
        let mut reg = registry().lock().expect("extension registry poisoned");
        reg.iter()
            .position(|e| e.name() == name)
            .map(|idx| reg.remove(idx))
    };
    let Some(ext) = ext else {
        return Err(OpsError::Config(format!("unknown extension '{name}'")));
    };
    if ext.requires_api_running() && !api_running(config).await {
        output::warn(format!(
            "extension {name} requires the API running — run 'api --keep' first"
        ));
        registry()
            .lock()
            .expect("extension registry poisoned")
            .push(ext);
        return Err(OpsError::Config(format!(
            "extension {name} requires the API running"
        )));
    }
    let result = ext.run(&ctx).await;
    registry()
        .lock()
        .expect("extension registry poisoned")
        .push(ext);
    result
}

/// Run all manifest hooks whose `on` matches `point`.
/// Points: pre_generate, post_generate, post_migrate, pre_e2e, post_e2e,
/// pre_api, post_api, pre_playwright.
/// Each hook: `sh -c "{exec} {args...}"` in config.root_dir. Failures abort
/// with Err(Command) including stderr/stdout tail. If no hooks match, Ok.
pub async fn run_hooks(config: &OpsConfig, point: &str) -> OpsResult<()> {
    let mut ran = 0usize;
    for hook in config
        .hooks
        .iter()
        .filter(|h| h.on.as_deref() == Some(point))
    {
        output::info(format!("hook {} ({point})", hook.name));
        run_exec(&hook.exec, &hook.args, &config.root_dir).await?;
        ran += 1;
    }
    if ran > 0 {
        output::ok(format!("{ran} hook(s) ran at '{point}'"));
    }
    Ok(())
}

/// List registered extension names (for `ext --list`).
pub fn extension_names() -> Vec<String> {
    registry()
        .lock()
        .expect("extension registry poisoned")
        .iter()
        .map(|e| e.name().to_string())
        .collect()
}

/// Run `sh -c "{exec} {args...}"` in `cwd`, capturing output. Non-zero exit
/// yields `OpsError::Command` with stdout/stderr tails.
async fn run_exec(exec: &str, args: &[String], cwd: &Path) -> OpsResult<()> {
    let script = if args.is_empty() {
        exec.to_string()
    } else {
        format!("{exec} {}", args.join(" "))
    };
    let output = Command::new("sh")
        .arg("-c")
        .arg(&script)
        .current_dir(cwd)
        .output()
        .map_err(|e| OpsError::Command(format!("failed to spawn hook '{script}': {e}")))?;
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(OpsError::Command(format!(
        "hook failed (exit {:?}): {script}\nstdout: {}\nstderr: {}",
        output.status.code(),
        tail(&stdout, 400),
        tail(&stderr, 400)
    )))
}

/// True when `{api_url}/health` responds (curl -sf).
async fn api_running(config: &OpsConfig) -> bool {
    Command::new("curl")
        .args(["-sf", "--max-time", "5"])
        .arg(format!("{}/health", config.api_url()))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Last `max` chars of `s`, prefixed with a truncation marker (UTF-8 safe).
fn tail(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    let skipped = chars.len() - max;
    let rest: String = chars[skipped..].iter().collect();
    format!("…[truncated {skipped} chars]\n{rest}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_config::{OpsDatabase, OpsDbTarget, OpsExtension, OpsHook, OpsManifest};

    use std::sync::atomic::{AtomicBool, Ordering};

    struct FakeExtension {
        name: String,
        flag: &'static AtomicBool,
        requires_api: bool,
    }

    impl TestExtension for FakeExtension {
        fn name(&self) -> &str {
            &self.name
        }

        fn requires_api_running(&self) -> bool {
            self.requires_api
        }

        fn run(&self, _ctx: &OpsContext<'_>) -> ExtensionFuture<'_> {
            let flag = self.flag;
            Box::pin(async move {
                flag.store(true, Ordering::SeqCst);
                Ok(())
            })
        }
    }

    fn minimal_manifest() -> OpsManifest {
        OpsManifest {
            app_name: "demo-app".into(),
            graph_binary: None,
            schemas_dir: None,
            classifier: None,
            domain_config: None,
            profile: None,
            output_dir: "generated-app".into(),
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
        }
    }

    fn config_with(manifest: OpsManifest, root: &Path) -> OpsConfig {
        OpsConfig::from_manifest(manifest, root.to_path_buf()).unwrap()
    }

    #[tokio::test]
    async fn registered_extension_runs_and_sets_flag() {
        static FLAG: AtomicBool = AtomicBool::new(false);
        register_extension(Box::new(FakeExtension {
            name: "ext-flag".into(),
            flag: &FLAG,
            requires_api: false,
        }));
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(minimal_manifest(), dir.path());
        run_extension("ext-flag", &cfg, &[]).await.unwrap();
        assert!(FLAG.load(Ordering::SeqCst));
        assert!(extension_names().contains(&"ext-flag".to_string()));
    }

    #[tokio::test]
    async fn unknown_extension_is_config_error() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(minimal_manifest(), dir.path());
        let err = run_extension("does-not-exist", &cfg, &[])
            .await
            .unwrap_err();
        assert!(matches!(err, OpsError::Config(_)), "got {err:?}");
        assert!(err.to_string().contains("does-not-exist"));
    }

    #[tokio::test]
    async fn manifest_extension_runs_exec() {
        let mut manifest = minimal_manifest();
        manifest.extensions.push(OpsExtension {
            name: "echo-ext".into(),
            exec: Some("echo hi".into()),
            requires_api: false,
            args: vec![],
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        run_extension("echo-ext", &cfg, &[]).await.unwrap();
    }

    #[tokio::test]
    async fn manifest_extension_appends_args() {
        let mut manifest = minimal_manifest();
        manifest.extensions.push(OpsExtension {
            name: "printf-ext".into(),
            exec: Some("printf %s".into()),
            requires_api: false,
            args: vec!["hello".into()],
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        run_extension("printf-ext", &cfg, &[]).await.unwrap();
    }

    #[tokio::test]
    async fn failing_exec_is_command_error_with_tail() {
        let mut manifest = minimal_manifest();
        manifest.extensions.push(OpsExtension {
            name: "failing-ext".into(),
            exec: Some("echo boom; exit 3".into()),
            requires_api: false,
            args: vec![],
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        let err = run_extension("failing-ext", &cfg, &[]).await.unwrap_err();
        assert!(matches!(err, OpsError::Command(_)), "got {err:?}");
        assert!(
            err.to_string().contains("boom"),
            "tail should show stdout: {err}"
        );
    }

    #[tokio::test]
    async fn requires_api_extension_without_api_is_config_error() {
        let mut manifest = minimal_manifest();
        manifest.servers.api_port = 1; // unreachable — API cannot be running
        manifest.extensions.push(OpsExtension {
            name: "needs-api".into(),
            exec: Some("echo hi".into()),
            requires_api: true,
            args: vec![],
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        let err = run_extension("needs-api", &cfg, &[]).await.unwrap_err();
        assert!(matches!(err, OpsError::Config(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn trait_extension_requiring_api_is_config_error_when_down() {
        static FLAG: AtomicBool = AtomicBool::new(false);
        register_extension(Box::new(FakeExtension {
            name: "ext-needs-api".into(),
            flag: &FLAG,
            requires_api: true,
        }));
        let mut manifest = minimal_manifest();
        manifest.servers.api_port = 1; // unreachable
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        let err = run_extension("ext-needs-api", &cfg, &[]).await.unwrap_err();
        assert!(matches!(err, OpsError::Config(_)), "got {err:?}");
        assert!(!FLAG.load(Ordering::SeqCst), "run() must not be called");
    }

    #[tokio::test]
    async fn run_hooks_runs_matching_hooks_only() {
        let mut manifest = minimal_manifest();
        manifest.hooks.push(OpsHook {
            name: "hi".into(),
            exec: "echo hi".into(),
            args: vec![],
            on: Some("pre_e2e".into()),
        });
        manifest.hooks.push(OpsHook {
            name: "other".into(),
            exec: "echo other".into(),
            args: vec![],
            on: Some("post_generate".into()),
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        run_hooks(&cfg, "pre_e2e").await.unwrap();
        run_hooks(&cfg, "never-called-point").await.unwrap();
    }

    #[tokio::test]
    async fn run_hooks_appends_hook_args() {
        let mut manifest = minimal_manifest();
        manifest.hooks.push(OpsHook {
            name: "printf".into(),
            exec: "printf".into(),
            args: vec!["arg-from-hook".into()],
            on: Some("pre_e2e".into()),
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        run_hooks(&cfg, "pre_e2e").await.unwrap();
    }

    #[tokio::test]
    async fn failing_hook_aborts_with_command_error() {
        let mut manifest = minimal_manifest();
        manifest.hooks.push(OpsHook {
            name: "boom".into(),
            exec: "false".into(),
            args: vec![],
            on: Some("pre_e2e".into()),
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        let err = run_hooks(&cfg, "pre_e2e").await.unwrap_err();
        assert!(matches!(err, OpsError::Command(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn run_hooks_ignores_hooks_without_on() {
        let mut manifest = minimal_manifest();
        manifest.hooks.push(OpsHook {
            name: "no-point".into(),
            exec: "false".into(),
            args: vec![],
            on: None,
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        run_hooks(&cfg, "pre_e2e").await.unwrap();
    }

    #[test]
    fn extension_names_reflects_registry() {
        static FLAG: AtomicBool = AtomicBool::new(false);
        register_extension(Box::new(FakeExtension {
            name: "ext-names-test".into(),
            flag: &FLAG,
            requires_api: false,
        }));
        let names = extension_names();
        assert!(names.contains(&"ext-names-test".to_string()));
    }

    #[test]
    fn tail_truncates_chars_safely() {
        let long = "a".repeat(1000);
        let t = tail(&long, 100);
        assert!(t.contains("[truncated 900 chars]"));
        assert!(t.ends_with(&"a".repeat(100)));
        assert_eq!(tail("short", 100), "short");
        let unicode = "é".repeat(200);
        let t = tail(&unicode, 50);
        assert!(t.starts_with('…'));
        assert_eq!(t.chars().last(), Some('é'));
        assert!(t.chars().count() < 100);
    }
}
