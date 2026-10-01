//! Captured process/HTTP/hurl output helpers for the API suite.

use std::path::Path;
use std::process::Command;

use crate::config::OpsConfig;
use crate::error::{OpsError, OpsResult};

pub(super) fn extract_json_field(body: &str, path: &str) -> String {
    let Some(value) = parse_json(body) else {
        return String::new();
    };
    let mut cur = value;
    for key in path.split('.') {
        if let Some(v) = cur.get(key) {
            cur = v.clone();
        } else {
            return String::new();
        }
    }
    cur.as_str().map(|s| s.to_string()).unwrap_or_default()
}

pub(super) fn parse_json(body: &str) -> Option<serde_json::Value> {
    serde_json::from_str(body).ok().or_else(|| {
        // Fallback: extract the first {...} block from the body (curl tail).
        let start = body.find('{')?;
        let end = body.rfind('}')?;
        serde_json::from_str(&body[start..=end]).ok()
    })
}

pub(super) fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for esc in chars.by_ref() {
                if esc.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub(super) fn run_capture(binary: &Path, args: &[&str], cwd: &Path) -> CapturedOutput {
    run_capture_env(binary, args, cwd, &[])
}

pub(super) struct CapturedOutput {
    pub(super) stdout: String,
}

impl CapturedOutput {
    pub(super) fn output_contains(&self, needle: &str) -> bool {
        self.stdout.contains(needle)
    }
}

pub(super) fn run_capture_env(
    binary: &Path,
    args: &[&str],
    cwd: &Path,
    envs: &[(&str, &str)],
) -> CapturedOutput {
    let mut cmd = Command::new(binary);
    cmd.args(args).current_dir(cwd);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let stdout = cmd
        .output()
        .map(|o| {
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            )
        })
        .unwrap_or_default();
    CapturedOutput { stdout }
}

/// Whether a `hurl --test` run passed, based on its summary block.
///
/// Hurl prints a summary like:
///
/// ```text
/// Executed files:    2
/// Executed requests: 10 (333.3/s)
/// Succeeded files:   2 (100.0%)
/// Failed files:      0 (0.0%)
/// ```
///
/// A naive `contains("100.0%")` check is a false positive: a fully-failed run
/// prints `Failed files: 2 (100.0%)`. The authoritative signal is the
/// `Failed files:` count — the suite passed iff it is 0.
pub(crate) fn hurl_suite_passed(stdout: &str) -> bool {
    for line in stdout.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("Failed files:") else {
            continue;
        };
        return rest
            .split_whitespace()
            .next()
            .and_then(|n| n.trim().parse::<u32>().ok())
            == Some(0);
    }
    // No `Failed files:` summary at all (unexpected output) — never claim pass.
    false
}

/// Full-output log path for one hurl file: `{root_dir}/test-results/hurl/{name}.log`.
pub(crate) fn hurl_log_path(config: &OpsConfig, name: &str) -> std::path::PathBuf {
    config
        .root_dir
        .join("test-results")
        .join("hurl")
        .join(format!("{name}.log"))
}

/// Persist a hurl run's combined stdout+stderr to `path` (parent created).
pub(crate) fn write_hurl_log(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)
}

/// Interesting excerpt of a failed hurl run: every line containing `error:`,
/// plus up to 10 following lines each — that's where hurl prints the
/// `actual:` / `expected:` / locator context that makes failures debuggable.
pub(crate) fn hurl_error_excerpt(output: &str) -> Vec<String> {
    let mut excerpt = Vec::new();
    let mut keep = 0usize;
    for line in output.lines() {
        if line.contains("error:") {
            excerpt.push(line.to_string());
            keep = 10;
        } else if keep > 0 {
            excerpt.push(line.to_string());
            keep -= 1;
        }
    }
    excerpt
}

/// Pluralize the last path segment of a smoke entity route using the same
/// simple rules as the codegen `pluralize` Tera filter (append `s`; `s`-final
/// → `es`; `y`-final not preceded by ey/ay/oy → `ies`). Used when the
/// manifest doesn't carry the resolved plural route.
pub(crate) fn pluralize_entity_route(entity: &str) -> String {
    let (prefix, seg) = match entity.rsplit_once('/') {
        Some((p, s)) => (Some(p), s),
        None => (None, entity),
    };
    let plural = if seg.ends_with('s') {
        format!("{seg}es")
    } else if seg.ends_with('y')
        && !seg.ends_with("ey")
        && !seg.ends_with("ay")
        && !seg.ends_with("oy")
    {
        format!("{}ies", &seg[..seg.len() - 1])
    } else {
        format!("{seg}s")
    };
    match prefix {
        Some(p) => format!("{p}/{plural}"),
        None => plural,
    }
}

/// Split a `curl -w "\n%{http_code}"` response into (body, status).
pub(super) fn split_status_body(text: &str) -> (String, String) {
    let mut lines: Vec<&str> = text.split('\n').collect();
    let status = lines
        .pop()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    (lines.join("\n"), status)
}

/// GET an HTTP URL, returning (status, body). Body excludes the status tail.
pub(crate) async fn http_get_body(
    url: &str,
    headers: &[(&str, &str)],
) -> OpsResult<(String, String)> {
    let mut cmd = Command::new("curl");
    cmd.arg("-s").arg("-w").arg("\n%{http_code}");
    for (k, v) in headers {
        cmd.arg("-H").arg(format!("{k}: {v}"));
    }
    cmd.arg(url);
    let out = cmd.output()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (body, status) = split_status_body(&text);
    Ok((status, body))
}

/// POST an HTTP URL with a JSON body, returning (status, body).
pub(crate) async fn http_post_body(
    url: &str,
    data: &str,
    headers: &[(&str, &str)],
) -> OpsResult<(String, String)> {
    let mut cmd = Command::new("curl");
    cmd.arg("-s")
        .arg("-X")
        .arg("POST")
        .arg("-w")
        .arg("\n%{http_code}");
    for (k, v) in headers {
        cmd.arg("-H").arg(format!("{k}: {v}"));
    }
    cmd.arg("-d").arg(data).arg(url);
    let out = cmd.output()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (body, status) = split_status_body(&text);
    Ok((status, body))
}

/// GET an HTTP URL, returning just the status code.
pub(crate) async fn http_status(url: &str, headers: &[(&str, &str)]) -> OpsResult<u16> {
    let mut cmd = Command::new("curl");
    cmd.arg("-s")
        .arg("-o")
        .arg("/dev/null")
        .arg("-w")
        .arg("%{http_code}");
    for (k, v) in headers {
        cmd.arg("-H").arg(format!("{k}: {v}"));
    }
    cmd.arg(url);
    let out = cmd.output()?;
    let code = String::from_utf8_lossy(&out.stdout).trim().to_string();
    code.parse::<u16>()
        .map_err(|e| OpsError::Http(format!("bad status {code:?}: {e}")))
}

pub(crate) fn parse_requests(hurl_output: &str) -> usize {
    hurl_output
        .lines()
        .find_map(|l| {
            let t = l.trim();
            if t.starts_with("Executed requests:") {
                return t
                    .strip_prefix("Executed requests:")
                    .and_then(|rest| rest.split_whitespace().next())
                    .and_then(|n| n.trim().parse::<usize>().ok());
            }
            t.strip_suffix(" request")
                .and_then(|n| n.rsplit(' ').next())
                .and_then(|n| n.trim().parse::<usize>().ok())
        })
        .unwrap_or(0)
}

/// Last `n` lines of `text` joined with newlines — failure diagnostics for
/// captured command output.
pub(super) fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}
