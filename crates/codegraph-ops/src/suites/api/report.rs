//! `--results` report writing and filesystem inspection helpers for the API suite.

use std::path::Path;

use crate::config::OpsConfig;
use crate::output;
use crate::results::ResultsReport;

use super::{ApiArgs, TestCounters};

pub(super) fn write_api_report(
    config: &OpsConfig,
    args: &ApiArgs,
    counters: &TestCounters,
    ok: bool,
    hook_failures: &[String],
) {
    if let Some(results_file) = &args.results_file {
        let mut report = ResultsReport::new(
            "api",
            &config.manifest_path,
            config.manifest.profile.as_deref(),
            &config.metrics,
        );
        report.passed = counters.passes;
        report.failed = counters.failures;
        report.failures = counters.failure_log.clone();
        report.hook_failures = hook_failures.to_vec();
        report.exit = i32::from(!ok);
        let _ = report.write(Path::new(results_file));
    }
}

pub(super) fn print_log_tail(log_path: &Path, n: usize) {
    if let Ok(content) = std::fs::read_to_string(log_path) {
        let lines: Vec<&str> = content.lines().rev().take(n).collect();
        output::warn(format!("--- {} (last {n} lines) ---", log_path.display()));
        for line in lines.iter().rev() {
            println!("    {line}");
        }
        output::warn("--- end ---");
    }
}

pub(super) fn find_file(dir: &Path, needle: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        e.file_name()
            .to_string_lossy()
            .to_lowercase()
            .contains(needle)
    })
}

pub(super) fn count_files_with_suffix(dir: &Path, suffix: &str) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(suffix))
        .count()
}
