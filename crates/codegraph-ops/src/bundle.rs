//! Failure artifact bundles (#358) — one "everything you need to triage"
//! directory per failed run.
//!
//! Failure artifacts used to be scattered: hurl logs durable under
//! `{root}/test-results/hurl/`, server logs in `/tmp/codegraph-ops-*.log`
//! (deleted by `clean`, never copied on failure), Playwright `test-results/`
//! unmanaged, and the summary printing only short tails. On any suite failure
//! (and on demand via `testkit bundle`) this module assembles
//! `{root}/test-results/artifacts-<utc-timestamp>/`:
//!
//! - `logs/` — server logs (`/tmp/codegraph-ops-*.log`, COPIED not moved —
//!   `clean` still owns deletion)
//! - `hurl/` — the per-file hurl logs (the api suite writes one per executed
//!   file, pass or fail, so failures are always among them)
//! - `playwright/` — `{ui_dir}/test-results/` SUMMARY: the last-run file and
//!   error-context `.md` files; heavy binary artifacts (traces, screenshots)
//!   are skipped with the skip recorded in `bundle.json`
//! - the harness's own `--results` JSON + `--metrics` file when given
//! - `bundle.json` — the manifest: every copied/skipped source + reason
//!
//! Hard constraints: assembly is BEST-EFFORT (never masks the real failure —
//! all errors degrade into recorded skips), the total is capped
//! (`[bundle].max_mb`, default 200 MB — oversized sources are tail-copied or
//! skipped), and `artifacts-*` dirs beyond `[bundle].keep` (default 3) are
//! pruned oldest-first (dir names embed a UTC timestamp, so lexicographic
//! order is chronological).

use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;

use codegraph_config::OpsManifest;

/// Server-log copy sources (basenames under the temp dir). Production uses
/// `/tmp`; `clean` still owns deleting the originals.
pub const SERVER_LOG_NAMES: &[&str] = &[
    "codegraph-ops-app.log",
    "codegraph-ops-e2e-app.log",
    "codegraph-ops-sveltekit-e2e.log",
    "codegraph-ops-sveltekit.log",
    "codegraph-ops-cli.log",
];

/// The smallest tail worth copying when a source exceeds the remaining
/// budget; below this the file is skipped entirely (a 3-line sliver of a
/// 300 MB log triages nobody).
const MIN_TAIL_BYTES: u64 = 1024 * 1024;

/// Bundle knobs from the manifest (`[bundle]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundleOptions {
    /// Total bundle size cap in bytes (`max_mb` × 1 MB).
    pub max_bytes: u64,
    /// `artifacts-*` dirs to keep before pruning the oldest.
    pub keep: usize,
}

impl BundleOptions {
    pub fn from_manifest(manifest: &OpsManifest) -> Self {
        Self {
            max_bytes: manifest.bundle.max_mb.saturating_mul(1024 * 1024),
            keep: manifest.bundle.keep.max(1) as usize,
        }
    }
}

/// Everything the assembler reads, injectable for hermetic tempdir tests
/// (server logs live under /tmp in production).
pub struct BundleSources<'a> {
    pub root_dir: &'a Path,
    pub ui_dir: &'a Path,
    pub server_logs: Vec<PathBuf>,
    /// The run's own artifacts (`--results` JSON, `--metrics` file), copied
    /// into the bundle root when they exist.
    pub extra_files: Vec<PathBuf>,
}

/// One copied or skipped source in the bundle manifest.
#[derive(Debug, Clone, Serialize)]
pub struct BundleEntry {
    pub source: String,
    /// Bundle-relative destination.
    pub dest: String,
    /// Set when the file was copied (possibly tail-truncated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copied_bytes: Option<u64>,
    /// Set when the file was NOT copied, with the reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
}

/// Result of one assembly, serialized as `bundle.json` inside the dir.
#[derive(Debug, Clone, Serialize)]
pub struct BundleReport {
    pub suite: String,
    pub created: String,
    /// The assembled `artifacts-*` directory (best-effort root placeholder
    /// when the dir could not be created).
    pub dir: PathBuf,
    pub max_bytes: u64,
    pub total_bytes: u64,
    pub copied: usize,
    pub skipped: usize,
    pub entries: Vec<BundleEntry>,
}

/// Production assembly from an [`OpsConfig`](crate::config::OpsConfig).
pub fn assemble_for_config(
    config: &crate::config::OpsConfig,
    suite: &str,
    extra_files: &[&Path],
) -> BundleReport {
    let opts = BundleOptions::from_manifest(&config.manifest);
    let sources = BundleSources {
        root_dir: &config.root_dir,
        ui_dir: &config.ui_dir,
        server_logs: server_logs(Path::new("/tmp")),
        extra_files: extra_files.iter().map(|p| p.to_path_buf()).collect(),
    };
    assemble_with(&sources, &opts, suite)
}

/// Server-log copy sources under `tmp_dir` (production: `/tmp`).
pub fn server_logs(tmp_dir: &Path) -> Vec<PathBuf> {
    SERVER_LOG_NAMES.iter().map(|n| tmp_dir.join(n)).collect()
}

/// Core assembler — injectable sources/options for hermetic tests.
/// Never fails: every error degrades into a recorded skip.
pub fn assemble_with(sources: &BundleSources, opts: &BundleOptions, suite: &str) -> BundleReport {
    let created = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let fallback_dir = sources.root_dir.join("test-results");
    let Some(dir) = fresh_bundle_dir(sources.root_dir) else {
        crate::output::warn(format!(
            "could not create an artifacts dir under {} — no bundle assembled",
            fallback_dir.display()
        ));
        return BundleReport {
            suite: suite.to_string(),
            created,
            dir: fallback_dir,
            max_bytes: opts.max_bytes,
            total_bytes: 0,
            copied: 0,
            skipped: 0,
            entries: Vec::new(),
        };
    };

    let mut builder = Builder {
        dir: dir.clone(),
        budget: opts.max_bytes,
        report: BundleReport {
            suite: suite.to_string(),
            created,
            dir: dir.clone(),
            max_bytes: opts.max_bytes,
            total_bytes: 0,
            copied: 0,
            skipped: 0,
            entries: Vec::new(),
        },
    };
    for log in &sources.server_logs {
        builder.copy_file(log, "logs");
    }
    // Hurl logs: per-file, small, rewritten each run — copied wholesale
    // (failures are always among them after a failing run).
    let hurl_dir = sources.root_dir.join("test-results").join("hurl");
    for file in sorted_files(&hurl_dir) {
        builder.copy_file(&file, "hurl");
    }
    // Playwright summary: last-run + error-context markdown; binary
    // artifacts (traces, screenshots, videos) are skipped with a recorded
    // reason — they dwarf the budget and trace.zip needs the full Playwright
    // UI anyway.
    let pw_dir = sources.ui_dir.join("test-results");
    for file in walk_files(&pw_dir) {
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let relative = file
            .strip_prefix(&pw_dir)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| name.clone());
        if name == ".last-run.json" || name.ends_with(".md") {
            builder.copy_file_nested(&file, "playwright", &relative);
        } else {
            builder.skip_nested(
                &file,
                "playwright",
                &relative,
                "binary artifact skipped (summary bundle)",
            );
        }
    }
    for extra in &sources.extra_files {
        builder.copy_file(extra, "");
    }

    builder.report.entries.sort_by(|a, b| a.dest.cmp(&b.dest));
    let report = builder.report.clone();
    write_bundle_json(&dir, &report);
    let pruned = prune_artifacts(sources.root_dir, opts.keep);
    crate::output::info(format!(
        "artifacts: {} ({} copied, {} skipped, {}){}",
        dir.display(),
        report.copied,
        report.skipped,
        crate::disk::human_bytes(report.total_bytes),
        if pruned > 0 {
            format!(" — pruned {pruned} old bundle(s)")
        } else {
            String::new()
        }
    ));
    report
}

/// `artifacts-<utc-timestamp>[-n]` under `{root}/test-results`, unique.
fn fresh_bundle_dir(root_dir: &Path) -> Option<PathBuf> {
    let base = root_dir.join("test-results");
    std::fs::create_dir_all(&base).ok()?;
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    for attempt in 0..100u32 {
        let name = if attempt == 0 {
            format!("artifacts-{ts}")
        } else {
            format!("artifacts-{ts}-{attempt}")
        };
        let dir = base.join(name);
        match std::fs::create_dir(&dir) {
            Ok(()) => return Some(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

struct Builder {
    dir: PathBuf,
    budget: u64,
    report: BundleReport,
}

impl Builder {
    fn record_copied(&mut self, source: &Path, dest: &str, bytes: u64) {
        self.report.total_bytes += bytes;
        self.report.copied += 1;
        self.report.entries.push(BundleEntry {
            source: source.display().to_string(),
            dest: dest.to_string(),
            copied_bytes: Some(bytes),
            skipped: None,
        });
    }

    fn record_skipped(&mut self, source: &Path, dest: &str, reason: &str) {
        self.report.skipped += 1;
        self.report.entries.push(BundleEntry {
            source: source.display().to_string(),
            dest: dest.to_string(),
            copied_bytes: None,
            skipped: Some(reason.to_string()),
        });
    }

    fn copy_file(&mut self, src: &Path, sub: &str) {
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.copy_file_nested(src, sub, &name);
    }

    /// Copy `src` into `{dir}/{sub}/{relative}` within the remaining budget:
    /// whole-file when it fits, tail-copy when oversized but worth it, skip
    /// otherwise. Every outcome lands in the manifest.
    fn copy_file_nested(&mut self, src: &Path, sub: &str, relative: &str) {
        let dest_rel = if sub.is_empty() {
            relative.to_string()
        } else {
            format!("{sub}/{relative}")
        };
        let Ok(meta) = std::fs::metadata(src) else {
            // Missing source (e.g. no server log for this suite): not a
            // skip-worthy surprise, just absent — record silently.
            return;
        };
        if !meta.is_file() {
            return;
        }
        let len = meta.len();
        if len <= self.budget {
            let dest = self.dir.join(&dest_rel);
            if copy_whole(src, &dest).is_ok() {
                self.budget -= len;
                self.record_copied(src, &dest_rel, len);
            } else {
                self.record_skipped(src, &dest_rel, "copy failed");
            }
            return;
        }
        if self.budget >= MIN_TAIL_BYTES {
            let dest = self.dir.join(&dest_rel);
            let cap = self.budget;
            match tail_copy(src, &dest, cap) {
                Ok(bytes) => {
                    self.budget = 0;
                    self.record_copied(src, &dest_rel, bytes);
                }
                Err(_) => self.record_skipped(src, &dest_rel, "copy failed"),
            }
            return;
        }
        self.record_skipped(src, &dest_rel, "bundle budget exhausted");
    }

    fn skip_nested(&mut self, src: &Path, sub: &str, relative: &str, reason: &str) {
        let dest_rel = format!("{sub}/{relative}");
        self.record_skipped(src, &dest_rel, reason);
    }
}

fn copy_whole(src: &Path, dest: &Path) -> std::io::Result<u64> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(src, dest)
}

/// Copy the LAST `cap` bytes of `src` into `dest`, prefixing a truncation
/// marker so readers know the head was dropped (logs are append-mostly — the
/// tail is the interesting part).
fn tail_copy(src: &Path, dest: &Path, cap: u64) -> std::io::Result<u64> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut reader = std::fs::File::open(src)?;
    let len = reader.metadata()?.len();
    let skip = len.saturating_sub(cap);
    reader.seek(SeekFrom::Start(skip))?;
    let mut writer = std::fs::File::create(dest)?;
    let mut copied = 0u64;
    if skip > 0 {
        let marker = format!("…[bundle: copied the last {cap} of {len} bytes]\n");
        writer.write_all(marker.as_bytes())?;
        copied += marker.len() as u64;
    }
    copied += std::io::copy(&mut reader, &mut writer)?;
    Ok(copied)
}

/// Write `bundle.json` (best-effort — a failed manifest write must not mask
/// the real failure).
fn write_bundle_json(dir: &Path, report: &BundleReport) {
    match serde_json::to_string_pretty(report) {
        Ok(json) => {
            if let Err(e) = std::fs::write(dir.join("bundle.json"), json + "\n") {
                crate::output::warn(format!("could not write bundle.json: {e}"));
            }
        }
        Err(e) => crate::output::warn(format!("could not serialize bundle.json: {e}")),
    }
}

/// Sorted `.log` files directly inside `dir` (hurl logs; non-recursive — the
/// api suite writes them flat).
fn sorted_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    files
}

/// Every file under `dir` (recursive, best-effort).
fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// Remove the oldest `artifacts-*` dirs beyond `keep` (dir names embed a UTC
/// timestamp, so lexicographic order is chronological). Returns the number
/// removed.
pub fn prune_artifacts(root_dir: &Path, keep: usize) -> usize {
    let base = root_dir.join("test-results");
    let Ok(entries) = std::fs::read_dir(&base) else {
        return 0;
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .map(|n| n.to_string_lossy().starts_with("artifacts-"))
                    .unwrap_or(false)
        })
        .collect();
    dirs.sort();
    let excess = dirs.len().saturating_sub(keep);
    for dir in &dirs[..excess] {
        if let Err(e) = std::fs::remove_dir_all(dir) {
            crate::output::warn(format!("could not prune old bundle {}: {e}", dir.display()));
        }
    }
    excess
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        #[allow(dead_code)]
        root: tempfile::TempDir,
        ui: tempfile::TempDir,
        tmp: tempfile::TempDir,
    }

    fn fixture() -> Fixture {
        Fixture {
            root: tempfile::tempdir().unwrap(),
            ui: tempfile::tempdir().unwrap(),
            tmp: tempfile::tempdir().unwrap(),
        }
    }

    fn sources<'a>(f: &'a Fixture, extras: &'a [PathBuf]) -> BundleSources<'a> {
        BundleSources {
            root_dir: f.root.path(),
            ui_dir: f.ui.path(),
            server_logs: server_logs(f.tmp.path()),
            extra_files: extras.to_vec(),
        }
    }

    fn opts(max_bytes: u64, keep: usize) -> BundleOptions {
        BundleOptions { max_bytes, keep }
    }

    fn entry_for<'a>(report: &'a BundleReport, needle: &str) -> &'a BundleEntry {
        report
            .entries
            .iter()
            .find(|e| e.dest.contains(needle) || e.source.contains(needle))
            .unwrap_or_else(|| panic!("no entry matching {needle}: {:?}", report.entries))
    }

    #[test]
    fn roundtrip_copies_server_logs_hurl_and_playwright_summary() {
        let f = fixture();
        std::fs::write(f.tmp.path().join("codegraph-ops-app.log"), "app server log").unwrap();
        std::fs::write(f.tmp.path().join("codegraph-ops-cli.log"), "cli log").unwrap();
        let hurl = f.root.path().join("test-results/hurl");
        std::fs::create_dir_all(&hurl).unwrap();
        std::fs::write(hurl.join("01_crud.hurl.log"), "hurl output").unwrap();
        let pw = f.ui.path().join("test-results/render-failed");
        std::fs::create_dir_all(&pw).unwrap();
        std::fs::write(pw.join("error-context.md"), "## Error").unwrap();
        std::fs::write(pw.join("trace.zip"), [0u8; 64]).unwrap();
        std::fs::write(f.ui.path().join("test-results/.last-run.json"), "{}").unwrap();
        let results = f.root.path().join("results.json");
        std::fs::write(&results, r#"{"suite":"api"}"#).unwrap();

        let report = assemble_with(&sources(&f, &[results]), &opts(10 * 1024 * 1024, 3), "api");

        assert_eq!(report.suite, "api");
        assert!(report.dir.starts_with(f.root.path().join("test-results")));
        assert!(
            report
                .dir
                .file_name()
                .map(|n| n.to_string_lossy().starts_with("artifacts-"))
                .unwrap_or(false),
            "bundle dir must be named artifacts-*: {:?}",
            report.dir
        );
        // Server logs.
        let app = entry_for(&report, "logs/codegraph-ops-app.log");
        assert_eq!(app.copied_bytes, Some("app server log".len() as u64));
        assert!(
            report.dir.join("logs/codegraph-ops-app.log").is_file(),
            "server log must be COPIED into the bundle"
        );
        assert!(
            f.tmp.path().join("codegraph-ops-app.log").is_file(),
            "source must survive (copy, not move)"
        );
        // Hurl log.
        assert!(report.dir.join("hurl/01_crud.hurl.log").is_file());
        // Playwright summary: md + last-run copied, binary skipped.
        let md = entry_for(&report, "playwright/render-failed/error-context.md");
        assert!(md.copied_bytes.is_some());
        assert!(
            report
                .dir
                .join("playwright/render-failed/error-context.md")
                .is_file()
        );
        assert!(report.dir.join("playwright/.last-run.json").is_file());
        let zip = entry_for(&report, "trace.zip");
        assert!(
            zip.skipped
                .as_deref()
                .unwrap_or_default()
                .contains("binary")
        );
        // Extra file (results JSON) at the bundle root.
        assert!(report.dir.join("results.json").is_file());
        // The manifest itself is written and parses.
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(report.dir.join("bundle.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["suite"], "api");
        assert_eq!(manifest["copied"], report.copied as u64);
        assert_eq!(manifest["skipped"], report.skipped as u64);
    }

    #[test]
    fn missing_sources_are_silently_absent() {
        let f = fixture();
        let report = assemble_with(&sources(&f, &[]), &opts(1024 * 1024, 3), "e2e");
        assert_eq!(report.copied, 0);
        assert_eq!(report.skipped, 0);
        assert!(report.dir.is_dir(), "the (empty) bundle dir still exists");
    }

    #[test]
    fn oversized_source_is_tail_copied_with_marker() {
        let f = fixture();
        let big = "x".repeat(3 * 1024 * 1024);
        std::fs::write(f.tmp.path().join("codegraph-ops-app.log"), &big).unwrap();
        let report = assemble_with(&sources(&f, &[]), &opts(2 * 1024 * 1024, 3), "api");
        let entry = entry_for(&report, "logs/codegraph-ops-app.log");
        let copied = entry
            .copied_bytes
            .expect("oversized log must be tail-copied");
        assert!(copied <= 2 * 1024 * 1024 + 128, "got {copied}");
        let contents =
            std::fs::read_to_string(report.dir.join("logs/codegraph-ops-app.log")).unwrap();
        assert!(
            contents.starts_with("…[bundle: copied the last"),
            "marker required: {contents:.80}"
        );
        assert!(contents.ends_with("x"), "the TAIL must be preserved");
    }

    #[test]
    fn tiny_budget_skips_oversized_sources_with_reason() {
        let f = fixture();
        std::fs::write(f.tmp.path().join("codegraph-ops-app.log"), "y".repeat(4096)).unwrap();
        let report = assemble_with(&sources(&f, &[]), &opts(16, 3), "api");
        let entry = entry_for(&report, "logs/codegraph-ops-app.log");
        assert_eq!(
            entry.skipped.as_deref(),
            Some("bundle budget exhausted"),
            "below MIN_TAIL the file must be skipped, not slivered"
        );
        assert_eq!(report.total_bytes, 0);
    }

    #[test]
    fn pruning_keeps_only_the_newest_keep_bundles() {
        let f = fixture();
        let base = f.root.path().join("test-results");
        for name in [
            "artifacts-20260101T000000Z",
            "artifacts-20260201T000000Z",
            "artifacts-20260301T000000Z",
        ] {
            std::fs::create_dir_all(base.join(name)).unwrap();
            std::fs::write(base.join(name).join("keep.txt"), "x").unwrap();
        }
        let report = assemble_with(&sources(&f, &[]), &opts(1024 * 1024, 2), "api");
        let mut left: Vec<String> = std::fs::read_dir(&base)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("artifacts-"))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left.len(), 2, "keep=2 must prune the oldest: {left:?}");
        assert!(
            !base.join("artifacts-20260101T000000Z").exists(),
            "the oldest bundle must go first"
        );
        assert!(
            report.dir.is_dir(),
            "the new bundle survives its own pruning"
        );
    }

    #[test]
    fn same_second_bundles_get_unique_dirs() {
        let f = fixture();
        let src = BundleSources {
            root_dir: f.root.path(),
            ui_dir: f.ui.path(),
            server_logs: vec![],
            extra_files: vec![],
        };
        let first = assemble_with(&src, &opts(1024 * 1024, 50), "api");
        let second = assemble_with(&src, &opts(1024 * 1024, 50), "api");
        assert_ne!(first.dir, second.dir, "bundle dirs must never collide");
    }

    #[test]
    fn options_read_the_manifest_bundle_section() {
        let raw = r#"
app_name = "demo-app"
database.api = { host = "localhost", port = 5432, user = "u", password = "p", database = "postgres" }

[bundle]
max_mb = 64
keep = 7
"#;
        let manifest: OpsManifest = toml::from_str(raw).unwrap();
        let opts = BundleOptions::from_manifest(&manifest);
        assert_eq!(opts.max_bytes, 64 * 1024 * 1024);
        assert_eq!(opts.keep, 7);
    }

    #[test]
    fn server_logs_list_covers_the_suite_log_family() {
        let logs = server_logs(Path::new("/tmp"));
        let names: Vec<String> = logs
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        for expected in [
            "codegraph-ops-app.log",
            "codegraph-ops-e2e-app.log",
            "codegraph-ops-sveltekit-e2e.log",
            "codegraph-ops-sveltekit.log",
            "codegraph-ops-cli.log",
        ] {
            assert!(names.contains(&expected.to_string()), "{expected} missing");
        }
    }
}
