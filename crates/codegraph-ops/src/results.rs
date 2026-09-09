//! Machine-readable suite results (`--results FILE`).
//!
//! One JSON object per suite run — pass/fail counts, the exact failing
//! check/test titles, stage timings, and the exit code — so agents and CI
//! can verify outcomes without grepping styled human output.
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

use std::path::Path;

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
    pub stages: Vec<StageTiming>,
    pub exit: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StageTiming {
    pub name: String,
    pub duration_secs: u64,
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
    /// Suites call this on success AND failure.
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
    }

    #[test]
    fn report_defaults_profile_when_unset() {
        let metrics = Metrics::new();
        let report = ResultsReport::new("e2e", &PathBuf::from("m.toml"), None, &metrics);
        assert_eq!(report.profile, "default");
        assert!(report.failures.is_empty());
        assert!(report.stages.is_empty());
    }
}
