//! Generated-CLI e2e suite (port of hr-platform/test-cli.sh, genericised:
//! the harness cannot know a consumer's CLI shape, so all checks are driven
//! by the manifest's smoke entity plus generic flag conventions).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::OpsConfig;
use crate::error::{OpsError, OpsResult};
use crate::output;

/// Append-only log capturing every CLI invocation (also used by `--verbose`).
const CLI_LOG: &str = "/tmp/codegraph-ops-cli.log";

/// Arguments for the CLI e2e suite.
pub struct CliArgs {
    pub skip_build: bool,
    pub verbose: bool,
}

/// Pass/fail counters for the CLI suite (private copy of the api suite's
/// counter pattern — kept local to this file to avoid cross-agent coupling).
#[derive(Debug)]
struct CliCounters {
    passes: usize,
    failures: usize,
    verbose: bool,
}

impl CliCounters {
    fn new(verbose: bool) -> Self {
        Self {
            passes: 0,
            failures: 0,
            verbose,
        }
    }

    fn pass(&mut self, msg: impl AsRef<str>) {
        self.passes += 1;
        output::ok(format!("PASS {}", msg.as_ref()));
    }

    fn fail(&mut self, msg: impl AsRef<str>) {
        self.failures += 1;
        output::fail(format!("FAIL {}", msg.as_ref()));
        if self.verbose {
            if let Ok(contents) = std::fs::read_to_string(CLI_LOG) {
                let t = tail(&contents, 1200);
                if !t.is_empty() {
                    output::warn(format!("--- {CLI_LOG} (tail) ---\n{t}"));
                }
            }
        }
    }

    fn summary(&self) -> String {
        format!(
            "{} passed, {} failed ({} total)",
            self.passes,
            self.failures,
            self.passes + self.failures
        )
    }
}

/// Run the generated CLI e2e suite. Requires the API running (callers check).
/// CLI binary: `{app_dir}/cli/target/{release|debug}/{app_name}` (from
/// config.app_binary_name()). If capabilities.has_cli is false, warn + Ok.
pub async fn run_cli(config: &OpsConfig, args: &CliArgs) -> OpsResult<()> {
    output::section("=== CLI E2E Suite ===");
    let mut counters = CliCounters::new(args.verbose);

    if !config.manifest.capabilities.has_cli {
        output::warn("CLI capability not generated — skipping CLI suite");
        return Ok(());
    }

    // 1. Preflight
    let api_url = config.api_url();
    if !http_ok(&format!("{api_url}/health")).await {
        return Err(OpsError::TestFailure(format!(
            "API server not running at {api_url}/health — run 'api --keep' first"
        )));
    }
    counters.pass("API server reachable");

    let binary = match prepare_cli_binary(config, args.skip_build) {
        Ok(b) => b,
        Err(e) => return Err(e),
    };

    let api_key = super::ui::read_or_provision_api_key(config).await?;
    let token = api_key.as_deref().unwrap_or("test-token");
    output::info(format!(
        "CLI: {} (token: {})",
        binary.display(),
        if api_key.is_some() {
            "provisioned"
        } else {
            "test-token"
        }
    ));

    // Clear the log between runs.
    let _ = std::fs::remove_file(CLI_LOG);

    // 2. Help & version
    output::section("Help & version");
    let (help_out, code) = invoke_cli(&binary, &api_url, token, &["--help"], false);
    if code == 0 && contains_any(&help_out, &["usage", "commands"]) {
        counters.pass("--help shows usage info");
    } else {
        counters.fail("--help output unexpected");
    }

    let (version_out, code) = invoke_cli(&binary, &api_url, token, &["--version"], false);
    if code == 0 && !version_out.trim().is_empty() {
        counters.pass("--version returns version string");
    } else {
        counters.fail("--version returned empty");
    }

    // Domain subcommand help, derived from the smoke entity route.
    if let Some(smoke) = &config.manifest.smoke {
        if let Some(domain) = domain_from_entity(&smoke.entity) {
            let (sub_out, code) = invoke_cli(&binary, &api_url, token, &[domain, "--help"], false);
            if code == 0 && !sub_out.trim().is_empty() {
                counters.pass(format!("{domain} --help lists subcommands"));
            } else {
                counters.fail(format!("{domain} --help unexpected output"));
            }
        }
    }

    // 3. Config commands
    output::section("Config commands");
    let (_, code) = invoke_cli(&binary, &api_url, token, &["config", "show"], false);
    if code == 0 {
        counters.pass("config show");
    } else {
        counters.fail("config show");
    }
    let (_, code) = invoke_cli(
        &binary,
        &api_url,
        token,
        &["config", "set-url", api_url.as_str()],
        false,
    );
    if code == 0 {
        counters.pass("config set-url");
    } else {
        counters.fail("config set-url");
    }
    let (_, code) = invoke_cli(
        &binary,
        &api_url,
        token,
        &["config", "set-token", "test-token"],
        false,
    );
    if code == 0 {
        counters.pass("config set-token");
    } else {
        counters.fail("config set-token");
    }

    // 4. CRUD lifecycle (driven by the manifest smoke entity).
    let Some(smoke) = &config.manifest.smoke else {
        output::info("no smoke entity configured — skipping CRUD lifecycle");
        return finish(config, counters, binary);
    };
    let Some(domain) = domain_from_entity(&smoke.entity) else {
        output::warn(format!(
            "smoke.entity {:?} has no domain — skipping CRUD",
            smoke.entity
        ));
        return finish(config, counters, binary);
    };
    let Some(entity) = entity_from_path(&smoke.entity) else {
        output::warn(format!(
            "smoke.entity {:?} has no entity — skipping CRUD",
            smoke.entity
        ));
        return finish(config, counters, binary);
    };
    output::section("CRUD lifecycle");
    crud_cycle(
        &mut counters,
        &binary,
        &api_url,
        token,
        domain,
        entity,
        &smoke.create_body,
    );

    // 5. Error handling
    output::section("Error handling");
    let (_, code) = invoke_cli(
        &binary,
        &api_url,
        token,
        &[
            domain,
            entity,
            "get",
            "00000000-0000-0000-0000-000000000000",
        ],
        true,
    );
    if code != 0 {
        counters.pass("404 for non-existent id");
    } else {
        counters.fail("should fail for non-existent id");
    }
    let (_, code) = invoke_cli(
        &binary,
        &api_url,
        token,
        &[domain, entity, "create", "--json", "not-valid-json"],
        true,
    );
    if code != 0 {
        counters.pass("rejects invalid JSON input");
    } else {
        counters.fail("should reject invalid JSON");
    }

    // 6. Subcommand listing (generic cross-domain proxy).
    output::section("Subcommand listing");
    let (out, code) = invoke_cli(&binary, &api_url, token, &["--help"], false);
    if code == 0 && contains_any(&out, &["usage", "commands"]) {
        counters.pass("--help lists subcommands");
    } else {
        counters.fail("--help does not list subcommands");
    }

    finish(config, counters, binary)
}

fn finish(config: &OpsConfig, counters: CliCounters, _binary: PathBuf) -> OpsResult<()> {
    output::section("=== CLI E2E Summary ===");
    output::info(counters.summary());
    config.metrics.print_summary();
    if counters.failures > 0 {
        return Err(OpsError::TestFailure(format!(
            "CLI E2E suite failed: {} failure(s)",
            counters.failures
        )));
    }
    output::ok("All CLI E2E tests passed");
    Ok(())
}

/// CRUD lifecycle for one entity: create → get → list → delete → get-after-
/// delete (must fail). All driven by the manifest's smoke entity.
fn crud_cycle(
    counters: &mut CliCounters,
    binary: &Path,
    api_url: &str,
    token: &str,
    domain: &str,
    entity: &str,
    create_body: &str,
) {
    let (out, code) = invoke_cli(
        binary,
        api_url,
        token,
        &[domain, entity, "create", "--json", create_body],
        true,
    );
    let id = extract_id(&out);
    if code == 0 && id.is_some() {
        counters.pass("create");
    } else {
        counters.fail("create");
        return;
    }
    let id = id.expect("id is Some after successful create");

    let (out, code) = invoke_cli(
        binary,
        api_url,
        token,
        &[domain, entity, "get", id.as_str()],
        true,
    );
    if code == 0 && extract_id(&out).as_deref() == Some(id.as_str()) {
        counters.pass("get by id");
    } else {
        counters.fail("get by id");
    }

    let (out, code) = invoke_cli(
        binary,
        api_url,
        token,
        &[domain, entity, "list", "--limit", "10"],
        true,
    );
    if code == 0 && data_is_array(&out) {
        counters.pass("list returns data array");
    } else {
        counters.fail("list should return a data array");
    }

    let (_, code) = invoke_cli(
        binary,
        api_url,
        token,
        &[domain, entity, "delete", id.as_str()],
        false,
    );
    if code == 0 {
        counters.pass("delete");
    } else {
        counters.fail("delete");
    }

    let (_, code) = invoke_cli(
        binary,
        api_url,
        token,
        &[domain, entity, "get", id.as_str()],
        true,
    );
    if code != 0 {
        counters.pass("get after delete fails (expected)");
    } else {
        counters.fail("get after delete should fail");
    }
}

/// Locate the generated CLI binary: `{app_dir}/cli/target/{release|debug}/{name}`.
fn cli_binary_path(config: &OpsConfig) -> Option<PathBuf> {
    let cli_dir = config.app_dir.join("cli");
    let name = config.app_binary_name();
    for profile in ["release", "debug"] {
        let candidate = cli_dir.join("target").join(profile).join(&name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Build the CLI (`cargo build` in `{app_dir}/cli`) unless `skip_build`,
/// in which case an existing binary is required.
fn prepare_cli_binary(config: &OpsConfig, skip_build: bool) -> OpsResult<PathBuf> {
    let cli_dir = config.app_dir.join("cli");
    if skip_build {
        return cli_binary_path(config).ok_or_else(|| {
            OpsError::TestFailure(format!(
                "no CLI binary found under {} (--skip-build)",
                cli_dir.display()
            ))
        });
    }
    if !cli_dir.join("Cargo.toml").is_file() {
        return Err(OpsError::TestFailure(format!(
            "CLI not generated at {} — run code generation first",
            cli_dir.display()
        )));
    }
    output::info("Building CLI (cargo build)...");
    let out = Command::new("cargo")
        .arg("build")
        .current_dir(&cli_dir)
        .output()
        .map_err(|e| OpsError::Command(format!("failed to spawn cargo build: {e}")))?;
    if !out.status.success() {
        return Err(OpsError::TestFailure(format!(
            "CLI build failed\n{}",
            tail(
                &format!(
                    "{}{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                ),
                800
            )
        )));
    }
    cli_binary_path(config).ok_or_else(|| {
        OpsError::TestFailure(format!(
            "CLI binary missing after build in {}",
            cli_dir.display()
        ))
    })
}

/// Run `{binary} --url {url} --token {token} [--output json] {args...}`,
/// appending combined output to the suite log. Returns (output, exit code).
fn invoke_cli(
    binary: &Path,
    url: &str,
    token: &str,
    args: &[&str],
    json_output: bool,
) -> (String, i32) {
    let mut cmd = Command::new(binary);
    cmd.arg("--url").arg(url).arg("--token").arg(token);
    if json_output {
        cmd.arg("--output").arg("json");
    }
    cmd.args(args);
    let (text, code) = match cmd.output() {
        Ok(out) => (
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
            out.status.code().unwrap_or(1),
        ),
        Err(e) => (format!("spawn failed: {e}"), 127),
    };
    append_log(&text);
    (text, code)
}

fn append_log(text: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(CLI_LOG)
    {
        let _ = writeln!(f, "{text}");
    }
}

/// True when `curl -sf` succeeds against `url`.
async fn http_ok(url: &str) -> bool {
    Command::new("curl")
        .args(["-sf", "--max-time", "5"])
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Extract `data.id` from a JSON envelope string like
/// `{"data": {"id": "uuid", ...}, "meta": ...}`.
fn extract_id(envelope: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(envelope).ok()?;
    value
        .get("data")?
        .get("id")?
        .as_str()
        .map(|s| s.to_string())
}

/// True when the envelope's `data` member is an array (list responses).
fn data_is_array(envelope: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(envelope)
        .ok()
        .and_then(|v| v.get("data").map(|d| d.is_array()))
        .unwrap_or(false)
}

/// First path segment of an entity route, e.g. `recruiting/candidate` →
/// `recruiting`. `None` for empty input.
fn domain_from_entity(entity: &str) -> Option<&str> {
    entity.split('/').next().filter(|s| !s.is_empty())
}

/// Entity part after the domain, e.g. `recruiting/candidate` → `candidate`.
fn entity_from_path(entity: &str) -> Option<&str> {
    let mut parts = entity.split('/');
    let first = parts.next()?;
    if first.is_empty() {
        return None;
    }
    parts.next().filter(|s| !s.is_empty())
}

/// Case-insensitive substring check against any needle.
fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    let lower = haystack.to_lowercase();
    needles.iter().any(|n| lower.contains(&n.to_lowercase()))
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

    #[test]
    fn extract_id_from_envelope() {
        assert_eq!(
            extract_id(r#"{"data": {"id": "abc-123", "name": "x"}, "meta": {}}"#).as_deref(),
            Some("abc-123")
        );
    }

    #[test]
    fn extract_id_missing_or_malformed() {
        assert_eq!(extract_id(r#"{"data": {"name": "x"}}"#), None);
        assert_eq!(extract_id(r#"{"data": {"id": 5}}"#), None);
        assert_eq!(extract_id("not json"), None);
        assert_eq!(extract_id(""), None);
    }

    #[test]
    fn data_is_array_detects_list_responses() {
        assert!(data_is_array(r#"{"data": []}"#));
        assert!(data_is_array(r#"{"data": [{"id": "a"}, {"id": "b"}]}"#));
        assert!(!data_is_array(r#"{"data": {"id": "a"}}"#));
        assert!(!data_is_array("garbage"));
    }

    #[test]
    fn domain_and_entity_from_route() {
        assert_eq!(
            domain_from_entity("recruiting/candidate"),
            Some("recruiting")
        );
        assert_eq!(entity_from_path("recruiting/candidate"), Some("candidate"));
        assert_eq!(domain_from_entity("candidate"), Some("candidate"));
        assert_eq!(entity_from_path("candidate"), None);
        assert_eq!(domain_from_entity(""), None);
        assert_eq!(entity_from_path(""), None);
        assert_eq!(domain_from_entity("/candidate"), None);
    }

    #[test]
    fn contains_any_is_case_insensitive() {
        assert!(contains_any(
            "Usage: hr-app [OPTIONS]",
            &["usage", "commands"]
        ));
        assert!(contains_any("Commands:", &["usage", "commands"]));
        assert!(!contains_any("nothing here", &["usage", "commands"]));
    }
}
