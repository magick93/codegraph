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
    let _ = fs::remove_file(report_path);
    let report_env = report_path.to_string_lossy().to_string();
    let preview_port_env = preview_port.to_string();
    let api_origin_env = format!("http://127.0.0.1:{api_port}");
    let ok = project.npx(
        &["playwright", "test"],
        &[
            ("CI", "1"),
            ("IFML_PREVIEW_PORT", preview_port_env.as_str()),
            ("IFML_GATE_REPORT", report_env.as_str()),
            ("IFML_API_ORIGIN", api_origin_env.as_str()),
            ("IFML_GATE_API_KEY", api_key),
        ],
        Duration::from_secs(900),
    )?;
    let raw = fs::read_to_string(report_path)
        .map_err(|e| format!("playwright JSON report missing ({e}) — playwright ok={ok}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("parse playwright report: {e}"))?;
    let mut titles = Vec::new();
    let mut failed = 0usize;
    let mut passed = 0usize;
    collect_specs(&value, &mut titles, &mut failed, &mut passed);
    Ok(SpecReport {
        failed,
        passed,
        titles,
    })
}

fn collect_specs(
    node: &serde_json::Value,
    titles: &mut Vec<String>,
    failed: &mut usize,
    passed: &mut usize,
) {
    if let Some(specs) = node.get("specs").and_then(|s| s.as_array()) {
        for spec in specs {
            let title = spec
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or_default()
                .to_string();
            let ok = spec.get("ok").and_then(|o| o.as_bool()).unwrap_or(false);
            titles.push(title);
            if ok {
                *passed += 1;
            } else {
                *failed += 1;
            }
        }
    }
    if let Some(suites) = node.get("suites").and_then(|s| s.as_array()) {
        for suite in suites {
            collect_specs(suite, titles, failed, passed);
        }
    }
}
