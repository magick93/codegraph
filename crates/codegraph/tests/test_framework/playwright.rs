//! Playwright plumbing for full-stack UI tests: chromium install, spec runs
//! with the generated-app env contract (preview port, JSON report path, API
//! origin + key), and JSON-report parsing into a [`SpecReport`].

use std::fs;
use std::path::Path;
use std::time::Duration;

use super::node_project::NodeProject;

/// Ensure the exact chromium build the pinned playwright version expects is
/// installed. `npx playwright install chromium` is a fast no-op when the
/// cache already has the right revision (a present-but-mismatched build does
/// not count).
pub fn install_chromium(project: &NodeProject) -> Result<(), String> {
    let ok = project.npx(
        &["playwright", "install", "chromium"],
        &[],
        Duration::from_secs(600),
    )?;
    if !ok {
        return Err("npx playwright install chromium failed".to_string());
    }
    Ok(())
}

/// Parsed playwright JSON report.
pub struct SpecReport {
    pub failed: usize,
    pub passed: usize,
    pub titles: Vec<String>,
    /// Titles of the failed specs (expected-failure accounting in the
    /// gate: known pre-existing surface failures vs real regressions).
    pub failures: Vec<String>,
}

/// Parse a playwright JSON report (the `json` reporter's document) into a
/// [`SpecReport`].
pub fn parse_report(report_path: &Path) -> Result<SpecReport, String> {
    let raw = fs::read_to_string(report_path)
        .map_err(|e| format!("playwright JSON report missing ({e})"))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("parse playwright report: {e}"))?;
    let mut titles = Vec::new();
    let mut failures = Vec::new();
    let mut failed = 0usize;
    let mut passed = 0usize;
    collect_specs(&value, &mut titles, &mut failed, &mut passed, &mut failures);
    Ok(SpecReport {
        failures,
        failed,
        passed,
        titles,
    })
}

/// Run playwright with CALLER-SPECIFIED args and env, writing the `json`
/// reporter's document to `report_path` (`PLAYWRIGHT_JSON_OUTPUT_NAME`) and
/// parsing it into a [`SpecReport`]. The entity gate's emitted config reads
/// the `PUBLIC_*` contract (not the IFML gate's), so the IFML `run_specs`
/// env block does not generalize — this is the shared runner underneath.
pub fn run_specs_with(
    project: &NodeProject,
    report_path: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<SpecReport, String> {
    let _ = fs::remove_file(report_path);
    let report_env = report_path.to_string_lossy().to_string();
    let mut all_envs: Vec<(&str, &str)> = vec![
        ("CI", "1"),
        // The `json` reporter's output-file contract (playwright reads both
        // vars; the name alone with an absolute path suffices).
        ("PLAYWRIGHT_JSON_OUTPUT_NAME", report_env.as_str()),
    ];
    all_envs.extend(envs.iter().copied());
    let mut all_args: Vec<&str> = vec!["playwright", "test"];
    all_args.extend(args.iter().copied());
    let ok = project.npx(&all_args, &all_envs, Duration::from_secs(900))?;
    parse_report(report_path).map_err(|e| format!("{e} — playwright ok={ok}"))
}

/// Run the project's playwright specs against a booted API. `report_path`
/// receives the JSON report (wired through `IFML_GATE_REPORT`); the spec
/// suite's `vite preview` webServer picks its port from `IFML_PREVIEW_PORT`.
pub fn run_specs(
    project: &NodeProject,
    report_path: &Path,
    api_port: u16,
    preview_port: u16,
    api_key: &str,
) -> Result<SpecReport, String> {
    let preview_port_env = preview_port.to_string();
    let api_origin_env = format!("http://127.0.0.1:{api_port}");
    let gate_report_env = report_path.to_string_lossy().to_string();
    run_specs_with(
        project,
        report_path,
        // One retry per failing spec: the 16-worker suite contends for the
        // single vite preview + axum pair, and a first-attempt loss (a
        // 5s navigation window under load) is noise, not a regression —
        // the ops harness carries the same knob (--pw-retries).
        &["--retries=1"],
        &[
            ("IFML_PREVIEW_PORT", preview_port_env.as_str()),
            ("IFML_GATE_REPORT", gate_report_env.as_str()),
            // The neutral server contract (issue #574 S1): the helper and
            // the vite proxy read API_ORIGIN/API_KEY (the deprecated
            // IFML_* spellings are fallbacks only).
            ("API_ORIGIN", api_origin_env.as_str()),
            ("GATE_API_KEY", api_key),
            // The remote-functions plane (#557): the SvelteKit server's
            // fallback credential (distinct from the harness's GATE_API_KEY
            // proxy injection).
            ("API_KEY", api_key),
        ],
    )
}

fn collect_specs(
    node: &serde_json::Value,
    titles: &mut Vec<String>,
    failed: &mut usize,
    passed: &mut usize,
    failures: &mut Vec<String>,
) {
    if let Some(specs) = node.get("specs").and_then(|s| s.as_array()) {
        for spec in specs {
            let title = spec
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or_default()
                .to_string();
            let ok = spec.get("ok").and_then(|o| o.as_bool()).unwrap_or(false);
            titles.push(title.clone());
            if ok {
                *passed += 1;
            } else {
                *failed += 1;
                failures.push(title);
            }
        }
    }
    if let Some(suites) = node.get("suites").and_then(|s| s.as_array()) {
        for suite in suites {
            collect_specs(suite, titles, failed, passed, failures);
        }
    }
}
