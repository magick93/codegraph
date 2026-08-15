//! Staged performance metrics with TSV export.
//!
//! Mirrors the bash `stage_start`/`stage_end`/`print_metrics` behaviour:
//! each named stage records its duration; a summary table is printed and,
//! optionally, appended to a TSV file for CI aggregation
//! (`timestamp\tsubcommand\tstage\tduration_secs`).
//!
//! All methods take `&self` (interior mutability) so suites can share a
//! single `Metrics` behind a `&OpsConfig`.

use std::cell::RefCell;
use std::path::Path;
use std::time::Instant;

use crate::error::{OpsError, OpsResult};
use crate::output;

#[derive(Debug, Clone)]
pub struct Stage {
    pub name: String,
    pub duration_secs: u64,
}

/// A stage currently being timed.
#[derive(Debug)]
struct ActiveStage {
    name: String,
    start: Instant,
}

/// Tracks stage durations for a single subcommand run.
#[derive(Debug, Default)]
pub struct Metrics {
    total_start: RefCell<Option<Instant>>,
    stages: RefCell<Vec<Stage>>,
    current: RefCell<Option<ActiveStage>>,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            total_start: RefCell::new(Some(Instant::now())),
            stages: RefCell::new(Vec::new()),
            current: RefCell::new(None),
        }
    }

    /// Begin a named stage. Any in-flight stage is silently ended first.
    pub fn begin(&self, name: impl Into<String>) {
        if self.current.borrow().is_some() {
            self.end();
        }
        *self.current.borrow_mut() = Some(ActiveStage {
            name: name.into(),
            start: Instant::now(),
        });
    }

    /// End the current stage, recording its duration and printing it.
    pub fn end(&self) {
        if let Some(active) = self.current.borrow_mut().take() {
            let duration_secs = active.start.elapsed().as_secs();
            let name = active.name.clone();
            self.stages.borrow_mut().push(Stage {
                name: name.clone(),
                duration_secs,
            });
            output::ok(format!(
                "{name} {}",
                output::dim(format!("({})", format_duration(duration_secs)))
            ));
        }
    }

    /// End the current stage with a skip reason (records 0s).
    pub fn skip(&self, reason: impl Into<String>) {
        if let Some(active) = self.current.borrow_mut().take() {
            let name = active.name.clone();
            let reason = reason.into();
            self.stages.borrow_mut().push(Stage {
                name: name.clone(),
                duration_secs: 0,
            });
            output::ok(format!("{name} {}", output::dim(format!("({reason})"))));
        }
    }

    /// Mark the current stage as complete with an explicit label.
    pub fn end_with(&self, label: impl Into<String>) {
        if let Some(active) = self.current.borrow_mut().take() {
            let name = active.name.clone();
            let label = label.into();
            self.stages.borrow_mut().push(Stage {
                name: name.clone(),
                duration_secs: 0,
            });
            output::ok(format!("{name} {}", output::dim(format!("({label})"))));
        }
    }

    pub fn stages(&self) -> Vec<Stage> {
        self.stages.borrow().clone()
    }

    pub fn total_elapsed_secs(&self) -> u64 {
        self.total_start
            .borrow()
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0)
    }

    /// Print the summary table (durations + % of total per stage).
    pub fn print_summary(&self) {
        let stages = self.stages.borrow();
        let total = self.total_elapsed_secs().max(1);
        output::section("─── Performance ───");
        println!();
        println!(
            "  {}",
            output::dim(format!("{:<52} {:>10} {:>6}", "Stage", "Duration", "% Tot"))
        );
        for stage in stages.iter() {
            let pct = stage.duration_secs * 100 / total;
            println!(
                "  {:<52} {:>10} {:>5}%",
                stage.name,
                format_duration(stage.duration_secs),
                pct
            );
        }
        println!();
        println!(
            "  {}{:<52} {:>10}{}",
            output::bold(""),
            "Total",
            format_duration(self.total_elapsed_secs()),
            output::bold("")
        );
        println!();
    }

    /// Append all stages as TSV rows (creating the file with a header first).
    pub fn append_tsv(&self, path: &Path, subcommand: &str) -> OpsResult<()> {
        use std::io::Write;
        let ts = chrono::Local::now().to_rfc3339();
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        if file.metadata()?.len() == 0 {
            writeln!(file, "timestamp\tsubcommand\tstage\tduration_secs")?;
        }
        for stage in self.stages.borrow().iter() {
            writeln!(
                file,
                "{}\t{}\t{}\t{}",
                ts, subcommand, stage.name, stage.duration_secs
            )?;
        }
        writeln!(
            file,
            "{}\t{}\tTOTAL\t{}",
            ts,
            subcommand,
            self.total_elapsed_secs()
        )?;
        output::info(format!("Metrics appended to {}", path.display()));
        Ok(())
    }

    /// Append one run-level object to a JSON metrics file as an array:
    ///
    /// ```json
    /// {"subcommand": "api", "stage": "TOTAL", "duration_secs": 12, "total": 30}
    /// ```
    ///
    /// where `duration_secs` is the sum of recorded stage durations and
    /// `total` the wall-clock elapsed. If the file does not exist it is
    /// created as `[]`; an existing file must parse as a JSON array (or be
    /// empty), and the new object is pushed onto it.
    pub fn append_json(&self, path: &Path, subcommand: &str) -> OpsResult<()> {
        let duration_secs: u64 = self.stages.borrow().iter().map(|s| s.duration_secs).sum();
        let total = self.total_elapsed_secs();
        let obj = serde_json::json!({
            "subcommand": subcommand,
            "stage": "TOTAL",
            "duration_secs": duration_secs,
            "total": total,
        });
        let mut array: serde_json::Value = if path.is_file() {
            let content = std::fs::read_to_string(path)?;
            if content.trim().is_empty() {
                serde_json::json!([])
            } else {
                serde_json::from_str(&content).map_err(|e| {
                    OpsError::Config(format!(
                        "metrics file {} is not valid JSON: {e}",
                        path.display()
                    ))
                })?
            }
        } else {
            serde_json::json!([])
        };
        let items = array.as_array_mut().ok_or_else(|| {
            OpsError::Config(format!(
                "metrics file {} is not a JSON array",
                path.display()
            ))
        })?;
        items.push(obj);
        std::fs::write(
            path,
            serde_json::to_string_pretty(&array)
                .map_err(|e| OpsError::Config(format!("cannot serialize metrics: {e}")))?,
        )?;
        output::info(format!("Metrics appended to {}", path.display()));
        Ok(())
    }
}

/// Format seconds as "1h 2m 3s" / "2m 3s" / "3s".
pub fn format_duration(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}h {}m {}s", secs / 3600, (secs % 3600) / 60, secs % 60)
    } else if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(3), "3s");
        assert_eq!(format_duration(65), "1m 5s");
        assert_eq!(format_duration(3661), "1h 1m 1s");
    }

    #[test]
    fn tracks_stages_and_total() {
        let m = Metrics::new();
        m.begin("first");
        std::thread::sleep(std::time::Duration::from_millis(10));
        m.end();
        m.begin("second");
        m.skip("not needed");
        assert_eq!(m.stages().len(), 2);
        assert_eq!(m.stages()[0].name, "first");
        assert_eq!(m.stages()[1].duration_secs, 0);
    }

    #[test]
    fn tsv_output_writes_header_once() {
        let m = Metrics::new();
        m.begin("a");
        m.end();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m.tsv");
        m.append_tsv(&path, "api").unwrap();
        m.append_tsv(&path, "api").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content.matches("timestamp\tsubcommand").count(), 1);
        assert_eq!(content.matches("\tapi\t").count(), 4); // 2 runs x (1 stage + TOTAL)
    }

    #[test]
    fn json_output_appends_one_object_per_run() {
        let m = Metrics::new();
        m.begin("first");
        m.end();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m.json");
        m.append_json(&path, "api").unwrap();
        m.append_json(&path, "api").unwrap();
        let array: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let items = array.as_array().unwrap();
        assert_eq!(items.len(), 2);
        for item in items {
            assert_eq!(item.get("subcommand").unwrap(), "api");
            assert_eq!(item.get("stage").unwrap(), "TOTAL");
            assert!(item.get("duration_secs").unwrap().is_u64());
            assert!(item.get("total").unwrap().is_u64());
        }
        // A non-array file is rejected rather than silently rewritten.
        std::fs::write(&path, "{\"not\": \"an array\"}").unwrap();
        assert!(m.append_json(&path, "api").is_err());
    }
}
