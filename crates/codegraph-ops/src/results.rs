//! Machine-readable suite results (`--results FILE`).
//!
//! Two JSON shapes share the file path (#357):
//!
//! **Completed-run report** ([`ResultsReport`]) — written by the suites at
//! their summary stage, on success AND failure:
//!
//! ```json
//! {
//!   "suite": "api",
//!   "manifest": "codegraph-ops.toml",
//!   "profile": "default",
//!   "passed": 54,
//!   "failed": 0,
//!   "failures": [],
//!   "transient_resolved": 0,
//!   "stages": [{ "name": "DB migrate", "duration_secs": 511 }],
//!   "exit": 0
//! }
//! ```
//!
//! `hook_failures` (additive) lists hooks that failed non-fatally; it is
//! omitted when empty, so green runs' JSON is byte-identical to pre-#357.
//!
//! **Early-failure report** ([`EarlyFailureReport`]) — written by the CLI
//! level when a suite returns Err WITHOUT having written its summary report
//! (port conflicts, missing tools, stale binary, supabase reset failures —
//! exactly the failures that used to skip `--results`). Distinct flat shape:
//!
//! ```json
//! {
//!   "suite": "api",
//!   "manifest": "codegraph-ops.toml",
//!   "stage": "1. Preflight",
//!   "error": "port 3000 is not free (...)",
//!   "exit": 1
//! }
//! ```
//!
//! Double-write avoidance: [`ResultsReport::write`] records the suite in a
//! run-global registry; the CLI's early writer skips a suite that already
//! wrote its summary (the summary is the richer superset and wins). In `full`
//! runs each suite is tracked separately, and the LAST writer of the shared
//! file wins — a later suite's summary overwriting an earlier suite's early
//! report is intended (freshest state at the end).

use std::path::Path;
use std::sync::Mutex;

use crate::error::OpsResult;
use crate::metrics::Metrics;

/// One failing check or test.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SuiteFailure {
    /// Suite-local grouping: hurl file name, Playwright project, or the
    /// check's stage. Empty when unknown.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub context: String,
    /// The failing check message or test title.
    pub title: String,
}

/// A completed suite run, ready to serialize.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResultsReport {
    pub suite: String,
    pub manifest: String,
    pub profile: String,
    pub passed: usize,
    pub failed: usize,
    pub failures: Vec<SuiteFailure>,
    /// Failures that passed on a follow-up retry (transient flake count).
    #[serde(default)]
    pub transient_resolved: usize,
    /// Hooks that failed non-fatally (`fatal = false` in the manifest) at
    /// points the suite reports on. Additive; omitted when empty so green
    /// runs' JSON stays byte-identical to pre-#357 output.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hook_failures: Vec<String>,
    pub stages: Vec<StageTiming>,
    pub exit: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StageTiming {
    pub name: String,
    pub duration_secs: u64,
}

/// Suites that already wrote a completed-run report this process run (see
/// the module docs for the double-write contract).
static WRITTEN_SUITES: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn mark_written(suite: &str) {
    if let Ok(mut written) = WRITTEN_SUITES.lock() {
        if !written.iter().any(|s| s == suite) {
            written.push(suite.to_string());
        }
    }
}

/// True when `suite` already wrote its completed-run report this run — the
/// CLI-level early writer must not clobber it with the flat early shape.
pub fn report_written(suite: &str) -> bool {
    WRITTEN_SUITES
        .lock()
        .map(|written| written.iter().any(|s| s == suite))
        .unwrap_or(false)
}

impl ResultsReport {
    pub fn new(
        suite: impl Into<String>,
        manifest: &Path,
        profile: Option<&str>,
        metrics: &Metrics,
    ) -> Self {
        Self {
            suite: suite.into(),
            manifest: manifest.display().to_string(),
            profile: profile.unwrap_or("default").to_string(),
            passed: 0,
            failed: 0,
            failures: Vec::new(),
            transient_resolved: 0,
            hook_failures: Vec::new(),
            stages: metrics
                .stages()
                .into_iter()
                .map(|s| StageTiming {
                    name: s.name,
                    duration_secs: s.duration_secs,
                })
                .collect(),
            exit: 0,
        }
    }

    /// Overwrite `path` with this report (pretty-printed for diffability).
    /// Suites call this on success AND failure; the write marks the suite as
    /// reported for the CLI-level early writer.
    pub fn write(&self, path: &Path) -> OpsResult<()> {
        let json = serde_json::to_string_pretty(self).map_err(|e| {
            crate::error::OpsError::Config(format!("cannot serialize results: {e}"))
        })?;
        std::fs::write(path, json + "\n")?;
        mark_written(&self.suite);
        crate::output::info(format!("Results written to {}", path.display()));
        Ok(())
    }
}

/// Early-failure report: a suite returned Err before reaching its summary
/// stage. Distinct flat shape — see the module docs.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EarlyFailureReport {
    pub suite: String,
    pub manifest: String,
    /// The most recent ▸-level stage title when the failure happened
    /// (tracked by `output::section`); "" before the first section.
    pub stage: String,
    pub error: String,
    pub exit: i32,
}

impl EarlyFailureReport {
    /// Overwrite `path` with this report. Deliberately does NOT mark the
    /// suite as written: a later suite summary must still be able to win.
    pub fn write(&self, path: &Path) -> OpsResult<()> {
        let json = serde_json::to_string_pretty(self).map_err(|e| {
            crate::error::OpsError::Config(format!("cannot serialize results: {e}"))
        })?;
        std::fs::write(path, json + "\n")?;
        crate::output::info(format!("Results written to {}", path.display()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn report_serializes_the_documented_shape() {
        let metrics = Metrics::new();
        metrics.begin("DB migrate");
        metrics.skip("already done");
        let mut report = ResultsReport::new(
            "api",
            Path::new("codegraph-ops.toml"),
            Some("cornucopia"),
            &metrics,
        );
        report.passed = 51;
        report.failed = 2;
        report.failures = vec![
            SuiteFailure {
                context: "bulk_create.hurl".into(),
                title: "create candidate".into(),
            },
            SuiteFailure {
                context: String::new(),
                title: "no API key migration found".into(),
            },
        ];
        report.transient_resolved = 1;
        report.exit = 1;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("results.json");
        report.write(&path).unwrap();

        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["suite"], "api");
        assert_eq!(value["manifest"], "codegraph-ops.toml");
        assert_eq!(value["profile"], "cornucopia");
        assert_eq!(value["passed"], 51);
        assert_eq!(value["failed"], 2);
        assert_eq!(value["transient_resolved"], 1);
        assert_eq!(value["exit"], 1);
        assert_eq!(value["failures"][0]["context"], "bulk_create.hurl");
        assert_eq!(value["failures"][0]["title"], "create candidate");
        // Empty context is omitted rather than written as an empty string.
        assert!(value["failures"][1].get("context").is_none());
        assert_eq!(value["stages"][0]["name"], "DB migrate");
        assert_eq!(value["stages"][0]["duration_secs"], 0);
        assert!(value.get("hook_failures").is_none());
    }

    #[test]
    fn report_defaults_profile_when_unset() {
        let metrics = Metrics::new();
        let report = ResultsReport::new("e2e", &PathBuf::from("m.toml"), None, &metrics);
        assert_eq!(report.profile, "default");
        assert!(report.failures.is_empty());
        assert!(report.stages.is_empty());
    }

    #[test]
    fn hook_failures_serialize_when_present_and_omit_when_empty() {
        let metrics = Metrics::new();
        let mut report = ResultsReport::new("api", Path::new("m.toml"), None, &metrics);
        report.hook_failures = vec!["refresh-views".to_string()];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("results.json");
        report.write(&path).unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["hook_failures"], serde_json::json!(["refresh-views"]));
    }

    #[test]
    fn completed_write_marks_the_suite_for_the_early_writer() {
        let metrics = Metrics::new();
        let report = ResultsReport::new("api-marks", Path::new("m.toml"), None, &metrics);
        assert!(!report_written("api-marks"));
        let dir = tempfile::tempdir().unwrap();
        report.write(&dir.path().join("results.json")).unwrap();
        assert!(report_written("api-marks"));
        // Other suites are unaffected.
        assert!(!report_written("e2e"));
    }

    #[test]
    fn early_failure_report_writes_the_documented_flat_shape() {
        let report = EarlyFailureReport {
            suite: "e2e".to_string(),
            manifest: "codegraph-ops.toml".to_string(),
            stage: "E2E 3. Database".to_string(),
            error: "supabase db reset failed: exit 1".to_string(),
            exit: 1,
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("results.json");
        report.write(&path).unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["suite"], "e2e");
        assert_eq!(value["manifest"], "codegraph-ops.toml");
        assert_eq!(value["stage"], "E2E 3. Database");
        assert_eq!(value["error"], "supabase db reset failed: exit 1");
        assert_eq!(value["exit"], 1);
        // The early write must not block a later suite summary.
        assert!(!report_written("e2e"));
    }
}
