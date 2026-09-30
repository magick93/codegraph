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
use crate::proc::run_streaming;

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
            return run_exec(&format!("ext:{name}"), exec, &entry.args, &config.root_dir).await;
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

/// Failure policy for a hook point (#357 made hook fatality explicit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookPolicy {
    /// Each hook's manifest `fatal` flag decides; a MISSING flag means fatal
    /// (failures abort the suite). Consumers with warn-only expectations
    /// (e.g. hr-specs' api `post_migrate`) set `fatal = false` explicitly in
    /// their manifests.
    PerHook,
    /// Failures only warn, regardless of the flag (and `fatal = true` cannot
    /// escalate past this). Used exclusively by `post_e2e`: cleanup hooks
    /// must always run and never mask the real failure — the documented
    /// best-effort contract.
    WarnOnly,
}

/// Run all manifest hooks whose `on` matches `point`.
/// Points: pre_generate, post_generate, post_migrate, pre_e2e, post_e2e,
/// pre_api, post_api, pre_playwright.
/// Each hook: `sh -c "{exec} {args...}"` in config.root_dir, streamed under
/// `[hook:<name>]` and timed (`hook <name> (3s)` via config.metrics, without
/// splitting an enclosing suite stage).
///
/// Failure handling follows `policy`: a failing fatal hook aborts with
/// Err(Command) including an output tail; a failing non-fatal hook warns and
/// its name is collected into the returned Vec (suites surface these in the
/// results JSON's `hook_failures`). If no hooks match, Ok(empty).
pub async fn run_hooks(
    config: &OpsConfig,
    point: &str,
    policy: HookPolicy,
) -> OpsResult<Vec<String>> {
    let mut ran = 0usize;
    let mut non_fatal_failures: Vec<String> = Vec::new();
    for hook in config
        .hooks
        .iter()
        .filter(|h| h.on.as_deref() == Some(point))
    {
        output::info(format!("hook {} ({point})", hook.name));
        let paused = config.metrics.pause();
        config.metrics.begin(format!("hook {}", hook.name));
        let result = run_exec(
            &format!("hook:{}", hook.name),
            &hook.exec,
            &hook.args,
            &config.root_dir,
        )
        .await;
        config.metrics.end();
        config.metrics.resume(paused);
        if let Err(e) = result {
            let fatal = policy == HookPolicy::PerHook && hook.fatal_or(true);
            if !fatal {
                output::warn(format!(
                    "hook {} ({point}) failed (fatal = false — continuing): {e}",
                    hook.name
                ));
                non_fatal_failures.push(hook.name.clone());
            } else {
                return Err(e);
            }
        }
        ran += 1;
    }
    if ran > 0 {
        output::ok(format!("{ran} hook(s) ran at '{point}'"));
    }
    Ok(non_fatal_failures)
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

/// Run `sh -c "{exec} {args...}"` in `cwd`, streaming output under
/// `[label]` (success output stays visible instead of being discarded).
/// Non-zero exit yields `OpsError::Command` with an output tail.
async fn run_exec(label: &str, exec: &str, args: &[String], cwd: &Path) -> OpsResult<()> {
    let script = if args.is_empty() {
        exec.to_string()
    } else {
        format!("{exec} {}", args.join(" "))
    };
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(&script).current_dir(cwd);
    let out = run_streaming(&mut cmd, label)?;
    if out.success() {
        return Ok(());
    }
    Err(OpsError::Command(format!(
        "{label}: `{script}` failed (exit {:?}):\n{}",
        out.status.code(),
        tail(&out.captured, 400)
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
            mox_files: Vec::new(),
            rosetta_files: Vec::new(),
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
                    grant_role: None,
                    grant_strict: None,
                },
                e2e: None,
                e2e_app: None,
            },
            supabase: None,
            capabilities: Default::default(),
            hurl: None,
            hooks: vec![],
            extensions: vec![],
            doctor: Default::default(),
            bundle: Default::default(),
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
            fatal: None,
        });
        manifest.hooks.push(OpsHook {
            name: "other".into(),
            exec: "echo other".into(),
            args: vec![],
            on: Some("post_generate".into()),
            fatal: None,
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        run_hooks(&cfg, "pre_e2e", HookPolicy::PerHook)
            .await
            .unwrap();
        run_hooks(&cfg, "never-called-point", HookPolicy::PerHook)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn run_hooks_appends_hook_args() {
        let mut manifest = minimal_manifest();
        manifest.hooks.push(OpsHook {
            name: "printf".into(),
            exec: "printf".into(),
            args: vec!["arg-from-hook".into()],
            on: Some("pre_e2e".into()),
            fatal: None,
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        run_hooks(&cfg, "pre_e2e", HookPolicy::PerHook)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn failing_hook_aborts_with_command_error() {
        let mut manifest = minimal_manifest();
        manifest.hooks.push(OpsHook {
            name: "boom".into(),
            exec: "false".into(),
            args: vec![],
            on: Some("pre_e2e".into()),
            fatal: None,
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        let err = run_hooks(&cfg, "pre_e2e", HookPolicy::PerHook)
            .await
            .unwrap_err();
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
            fatal: None,
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        run_hooks(&cfg, "pre_e2e", HookPolicy::PerHook)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn run_hooks_records_hook_timing() {
        let mut manifest = minimal_manifest();
        manifest.hooks.push(OpsHook {
            name: "timed".into(),
            exec: "true".into(),
            args: vec![],
            on: Some("pre_api".into()),
            fatal: None,
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        run_hooks(&cfg, "pre_api", HookPolicy::PerHook)
            .await
            .unwrap();
        let stages = cfg.metrics.stages();
        assert!(
            stages.iter().any(|s| s.name == "hook timed"),
            "hook duration must be recorded: {stages:?}"
        );
    }

    #[tokio::test]
    async fn run_hooks_preserves_enclosing_stage() {
        let mut manifest = minimal_manifest();
        manifest.hooks.push(OpsHook {
            name: "nested".into(),
            exec: "true".into(),
            args: vec![],
            on: Some("pre_api".into()),
            fatal: None,
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        cfg.metrics.begin("Generate + build");
        run_hooks(&cfg, "pre_api", HookPolicy::PerHook)
            .await
            .unwrap();
        cfg.metrics.end();
        let stages = cfg.metrics.stages();
        assert_eq!(stages.len(), 2, "hook + enclosing stage: {stages:?}");
        assert_eq!(stages[0].name, "hook nested");
        assert_eq!(stages[1].name, "Generate + build");
    }

    #[tokio::test]
    async fn missing_fatal_flag_defaults_to_fatal() {
        let mut manifest = minimal_manifest();
        manifest.hooks.push(OpsHook {
            name: "gate".into(),
            exec: "false".into(),
            args: vec![],
            on: Some("post_migrate".into()),
            fatal: None,
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        let err = run_hooks(&cfg, "post_migrate", HookPolicy::PerHook)
            .await
            .unwrap_err();
        assert!(matches!(err, OpsError::Command(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn fatal_false_opts_out_and_collects_the_name() {
        let mut manifest = minimal_manifest();
        manifest.hooks.push(OpsHook {
            name: "warn-only".into(),
            exec: "echo boom; exit 3".into(),
            args: vec![],
            on: Some("post_migrate".into()),
            fatal: Some(false),
        });
        manifest.hooks.push(OpsHook {
            name: "after".into(),
            exec: "true".into(),
            args: vec![],
            on: Some("post_migrate".into()),
            fatal: None,
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        let failed = run_hooks(&cfg, "post_migrate", HookPolicy::PerHook)
            .await
            .unwrap();
        assert_eq!(failed, vec!["warn-only".to_string()]);
    }

    #[tokio::test]
    async fn warn_only_policy_never_aborts_even_for_fatal_hooks() {
        let mut manifest = minimal_manifest();
        manifest.hooks.push(OpsHook {
            name: "cleanup".into(),
            exec: "false".into(),
            args: vec![],
            on: Some("post_e2e".into()),
            fatal: Some(true),
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        let failed = run_hooks(&cfg, "post_e2e", HookPolicy::WarnOnly)
            .await
            .unwrap();
        assert_eq!(failed, vec!["cleanup".to_string()]);
    }

    #[tokio::test]
    async fn per_hook_returns_empty_on_success() {
        let mut manifest = minimal_manifest();
        manifest.hooks.push(OpsHook {
            name: "fine".into(),
            exec: "true".into(),
            args: vec![],
            on: Some("post_api".into()),
            fatal: None,
        });
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with(manifest, dir.path());
        let failed = run_hooks(&cfg, "post_api", HookPolicy::PerHook)
            .await
            .unwrap();
        assert!(failed.is_empty());
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
