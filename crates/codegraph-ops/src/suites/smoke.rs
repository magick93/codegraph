//! Remote deployment smoke test (port of deploy/smoke-test.sh).
//!
//! All HTTP goes through `curl` subprocesses; JSON responses are parsed with
//! serde_json. Hard checks fail the run; soft checks log warnings.

use std::process::Command;

use crate::error::{OpsError, OpsResult};
use crate::output;

/// Arguments for the remote smoke test.
#[derive(Debug, Clone)]
pub struct SmokeArgs {
    /// Base URL of the deployed API (default `https://api.example.com`).
    pub api_url: String,
    /// Base URL of the deployed web app (default `https://app.example.com`).
    pub web_url: String,
    /// Expected `git_commit` reported by the version endpoint. A mismatch is
    /// a warning, not a failure.
    pub expected_commit: Option<String>,
    /// Optional Supabase auth health URL (e.g. `https://auth.example.com/auth/v1/health`).
    pub auth_health_url: Option<String>,
    /// Worker base URLs pinged via `POST {base}/events`.
    pub workers: Vec<String>,
}

impl Default for SmokeArgs {
    fn default() -> Self {
        Self {
            api_url: "https://api.example.com".to_string(),
            web_url: "https://app.example.com".to_string(),
            expected_commit: None,
            auth_health_url: None,
            workers: Vec::new(),
        }
    }
}

/// JSON body posted to worker `/events` for the ping check.
pub fn ping_body() -> String {
    serde_json::json!({
        "event": "ping",
        "entity_id": "test",
        "org_id": "test",
        "installation_id": "test",
        "correlation_id": "test",
    })
    .to_string()
}

/// `{worker_base}/events` — tolerates a trailing slash on the base URL.
pub fn ping_url(worker_base: &str) -> String {
    format!("{}/events", worker_base.trim_end_matches('/'))
}

/// Run the remote smoke test. Returns `Err(TestFailure)` if any hard check
/// fails; soft checks log warnings. Prints "=== Smoke test complete ===" at
/// the end.
pub async fn run_smoke(args: &SmokeArgs) -> OpsResult<()> {
    output::section("=== Remote Smoke Test ===");

    output::info("1/9 API health check");
    expect_200("API health", &format!("{}/health", args.api_url)).await?;

    output::info("2/9 health/ready");
    match get_json(&format!("{}/health/ready", args.api_url)).await {
        Ok(Some(body)) => {
            let status = body
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            if status == "ok" {
                output::ok("health/ready — ok");
            } else {
                output::warn(format!("health/ready — status {status:?}"));
            }
        }
        Ok(None) => output::warn("health/ready — endpoint unavailable"),
        Err(e) => output::warn(format!("health/ready — fetch failed: {e}")),
    }

    output::info("3/9 Swagger UI");
    expect_200("Swagger UI", &format!("{}/swagger-ui/", args.api_url)).await?;

    output::info("4/9 OpenAPI spec");
    expect_200(
        "OpenAPI spec",
        &format!("{}/api-docs/openapi.json", args.api_url),
    )
    .await?;

    output::info("5/9 Frontend loads");
    expect_200("Frontend", &args.web_url).await?;

    if let Some(auth_url) = &args.auth_health_url {
        output::info("6/9 Supabase auth health");
        soft_200("Supabase auth", auth_url).await;
    }

    output::info("7/9 Integration workers");
    let body = ping_body();
    for worker in &args.workers {
        let url = ping_url(worker);
        match http_status(&url, "POST", Some(&body)).await {
            Ok(status) if status < 500 => output::ok(format!("worker {worker} — {status}")),
            Ok(status) => output::warn(format!("worker {worker} — HTTP {status} (>= 500)")),
            Err(e) => output::warn(format!("worker {worker} — request failed: {e}")),
        }
    }

    output::info("8/9 Metrics endpoint");
    expect_200("Metrics", &format!("{}/metrics", args.api_url)).await?;

    output::info("9/9 Version endpoint");
    match get_json(&format!("{}/version", args.api_url)).await {
        Ok(Some(body)) => {
            let commit = body
                .get("git_commit")
                .and_then(|c| c.as_str())
                .unwrap_or("unknown");
            match &args.expected_commit {
                Some(expected) if expected != commit => {
                    output::warn(format!("expected commit {expected} but got {commit}"))
                }
                Some(_) => output::ok(format!("version — commit {commit}")),
                None => output::ok(format!("version — commit {commit}")),
            }
        }
        Ok(None) => output::warn("version — endpoint unavailable"),
        Err(e) => output::warn(format!("version — fetch failed: {e}")),
    }

    output::section("=== Smoke test complete ===");
    Ok(())
}

/// Hard check: the URL must return HTTP 200 or the run fails.
async fn expect_200(name: &str, url: &str) -> OpsResult<()> {
    match http_status(url, "GET", None).await {
        Ok(200) => {
            output::ok(format!("{name} — 200"));
            Ok(())
        }
        Ok(status) => Err(OpsError::TestFailure(format!(
            "{name} failed: HTTP {status} for {url}"
        ))),
        Err(e) => Err(OpsError::TestFailure(format!(
            "{name} failed for {url}: {e}"
        ))),
    }
}

/// Soft check: HTTP 200 is OK, anything else warns and continues.
async fn soft_200(name: &str, url: &str) {
    match http_status(url, "GET", None).await {
        Ok(200) => output::ok(format!("{name} — 200")),
        Ok(status) => output::warn(format!("{name} — HTTP {status} for {url}")),
        Err(e) => output::warn(format!("{name} — request failed: {e}")),
    }
}

/// HTTP status code for a request via `curl -s -o /dev/null -w "%{http_code}"`.
/// Unreachable hosts surface as status 0 ("000").
async fn http_status(url: &str, method: &str, body: Option<&str>) -> OpsResult<u16> {
    let mut cmd = Command::new("curl");
    cmd.arg("-s")
        .arg("-o")
        .arg("/dev/null")
        .arg("-w")
        .arg("%{http_code}")
        .arg("--max-time")
        .arg("30");
    if !method.is_empty() && method != "GET" {
        cmd.arg("-X").arg(method);
    }
    if let Some(body) = body {
        cmd.arg("-H")
            .arg("Content-Type: application/json")
            .arg("-d")
            .arg(body);
    }
    cmd.arg(url);
    let output = cmd
        .output()
        .map_err(|e| OpsError::Http(format!("curl failed for {url}: {e}")))?;
    let code = String::from_utf8_lossy(&output.stdout).trim().to_string();
    code.parse::<u16>()
        .map_err(|e| OpsError::Http(format!("bad status code {code:?} from {url}: {e}")))
}

/// Fetch a JSON document via `curl -sf`. `Ok(None)` when the endpoint is
/// unavailable (non-2xx, connection failure, or unparseable body).
async fn get_json(url: &str) -> OpsResult<Option<serde_json::Value>> {
    let output = Command::new("curl")
        .args(["-sf", "--max-time", "30"])
        .arg(url)
        .output()
        .map_err(|e| OpsError::Http(format!("curl failed for {url}: {e}")))?;
    if !output.status.success() || output.stdout.is_empty() {
        return Ok(None);
    }
    match serde_json::from_slice(&output.stdout) {
        Ok(value) => Ok(Some(value)),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_body_has_expected_fields() {
        let v: serde_json::Value = serde_json::from_str(&ping_body()).unwrap();
        assert_eq!(v["event"], "ping");
        assert_eq!(v["entity_id"], "test");
        assert_eq!(v["org_id"], "test");
        assert_eq!(v["installation_id"], "test");
        assert_eq!(v["correlation_id"], "test");
    }

    #[test]
    fn ping_url_appends_events_and_strips_trailing_slash() {
        assert_eq!(
            ping_url("https://payments.example.com"),
            "https://payments.example.com/events"
        );
        assert_eq!(
            ping_url("https://payments.example.com/"),
            "https://payments.example.com/events"
        );
    }

    #[test]
    fn smoke_args_defaults() {
        let a = SmokeArgs::default();
        assert_eq!(a.api_url, "https://api.example.com");
        assert_eq!(a.web_url, "https://app.example.com");
        assert!(a.expected_commit.is_none());
        assert!(a.auth_health_url.is_none());
        assert!(a.workers.is_empty());
    }
}
