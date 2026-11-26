//! Doctor parity for the events-producer planner (issue #820 slice a).
//!
//! `codegraph doctor --ddd-files … --evt-files …` prints the SAME sentence
//! bodies the run path hard-errors on, as `WARN events producer — …`, so a
//! model can be aligned BEFORE the `events_producer` flag is flipped. Doctor
//! holds compiled phero artifacts but no codegraph graph, so this module
//! builds a SCRATCH in-memory graph from the same sources and runs the SAME
//! planner (`docs/events-producer.md` is the one contract; the planner reads
//! `{Entity}Response` through `build_dto_context`, never a second naming
//! rule).
//!
//! Mechanics: doctor is synchronous and `main` already runs inside a tokio
//! runtime, so the scratch ingest + plan run on a scoped worker thread with
//! its own current-thread runtime. That keeps `cmd_doctor`'s signature (and
//! its callers) untouched and cannot deadlock the main runtime.
//!
//! Presence-gated: nothing is printed unless a `.ddd` op emits an event that
//! a `.evt` contract declares. A scratch-ingest failure prints one
//! `INFO events producer — analysis skipped (…)` line and never fails.

use std::collections::HashSet;
use std::path::PathBuf;

use codegraph_config::DomainConfig;

use crate::doctor_graph::UnionGather;

/// The doctor producer lines (full `WARN …` / `INFO …` display strings).
pub(crate) fn producer_lines(
    args: &crate::init::commands::DoctorArgs,
    config: Option<&DomainConfig>,
    cross: &UnionGather,
    database_target: codegraph_generate::db::dialect::DatabaseTarget,
) -> Vec<String> {
    if !has_bound_emits(cross) {
        return Vec::new();
    }
    let Some(config) = config else {
        return vec![
            "INFO events producer — analysis skipped (domains.toml unparseable)".to_string(),
        ];
    };

    let mut mox_paths: Vec<PathBuf> = args.mox_files.clone();
    for path in &cross.mox_import_paths {
        if !mox_paths.contains(path) {
            mox_paths.push(path.clone());
        }
    }
    if mox_paths.is_empty() {
        return vec![
            "INFO events producer — analysis skipped (no .mox model; pass --mox-files)".to_string(),
        ];
    }

    let ddd = args.ddd_files.clone();
    let evt = args.evt_files.clone();
    let classifier = args.classifier.clone();
    let config = config.clone();

    std::thread::spawn(move || {
        match analyze(mox_paths, ddd, evt, config, classifier, database_target) {
            Ok(lines) => lines,
            Err(reason) => vec![format!(
                "INFO events producer — analysis skipped ({reason})"
            )],
        }
    })
    .join()
    .unwrap_or_else(|_| {
        vec!["INFO events producer — analysis skipped (worker thread panicked)".to_string()]
    })
}

/// At least one emitting operation names an event some contract declares.
fn has_bound_emits(cross: &UnionGather) -> bool {
    let declared: HashSet<&str> = cross
        .events
        .iter()
        .flat_map(|file| file.model.events.iter().map(|event| event.name.as_str()))
        .collect();
    cross.designs.iter().any(|file| {
        file.model.modules.iter().any(|module| {
            module
                .services
                .iter()
                .flat_map(|service| service.operations.iter())
                .chain(
                    module
                        .designs
                        .iter()
                        .flat_map(|design| design.child_operations.iter()),
                )
                .any(|op| {
                    op.emits
                        .iter()
                        .any(|event| declared.contains(event.as_str()))
                })
        })
    })
}

fn analyze(
    mox_paths: Vec<PathBuf>,
    ddd: Vec<PathBuf>,
    evt: Vec<PathBuf>,
    config: DomainConfig,
    classifier: Option<PathBuf>,
    database_target: codegraph_generate::db::dialect::DatabaseTarget,
) -> Result<Vec<String>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("cannot start the doctor runtime: {e}"))?;
    runtime.block_on(async move {
        let backend =
            codegraph_backend::create_backend(&codegraph_backend::BackendConfig::default())
                .await
                .map_err(|e| e.to_string())?;
        let suffix = config.defaults.type_suffix.clone();

        let outcome = crate::ingest::mox_ingest::ingest_mox_files(
            backend.ingestor(),
            backend.querier(),
            &mox_paths,
            &config,
            &suffix,
        )
        .await
        .map_err(|e| e.to_string())?;
        if !outcome.imported_files.is_empty() {
            let classifier_config = match &classifier {
                Some(path) => codegraph_classifier::config::parse_classifier_config(path)
                    .map_err(|e| e.to_string())?,
                None => codegraph_classifier::config::parse_classifier_config_str("")
                    .map_err(|e| e.to_string())?,
            };
            crate::ingest::async_ingest::ingest_imported_schemas(
                backend.ingestor(),
                &outcome.imported_files,
                &classifier_config,
                &codegraph_config::UiOverrideConfig::default(),
                &suffix,
            )
            .await
            .map_err(|e| e.to_string())?;
        }
        if !outcome.pending_alias_refs.is_empty() {
            let mut stats = crate::ingest::mox_ingest::MoxIngestStats::default();
            crate::ingest::mox_ingest::wire_alias_refs(
                backend.ingestor(),
                backend.querier(),
                &outcome.pending_alias_refs,
                &suffix,
                &mut stats,
            )
            .await
            .map_err(|e| e.to_string())?;
        }
        if !ddd.is_empty() {
            crate::ingest::ddd_ingest::ingest_ddd_files(
                backend.ingestor(),
                backend.querier(),
                &ddd,
                &config,
            )
            .await
            .map_err(|e| e.to_string())?;
        }
        if !evt.is_empty() {
            crate::ingest::evt_ingest::ingest_evt_files(
                backend.ingestor(),
                backend.querier(),
                &evt,
                &config,
            )
            .await
            .map_err(|e| e.to_string())?;
        }

        let report = codegraph_generate::events::producer::plan_for_graph(
            backend.querier(),
            &config,
            false,
            database_target,
        )
        .await?;
        let mut lines = Vec::with_capacity(report.refusals.len() + report.warnings.len());
        for refusal in &report.refusals {
            lines.push(refusal.doctor_line());
        }
        for warning in &report.warnings {
            lines.push(warning.doctor_line());
        }
        Ok(lines)
    })
}

/// Public helper for tests and embedders: run the doctor producer check
/// against the `--ddd-files` / `--evt-files` sources and return the display
/// lines (`WARN events producer — …`, or one `INFO` line when the analysis
/// cannot run). `cmd_doctor` prints exactly these.
#[must_use]
pub fn producer_diagnostics(
    args: &crate::init::commands::DoctorArgs,
    config: &DomainConfig,
    database_target: codegraph_generate::db::dialect::DatabaseTarget,
) -> Vec<String> {
    let (cross, _) =
        crate::doctor_graph::gather_cross_surface(&args.ddd_files, &args.evt_files, &[], &[]);
    producer_lines(args, Some(config), &cross, database_target)
}

/// Print the producer lines and return how many are WARNs (the doctor
/// soft-warning count). Kept here so `cmd_doctor` stays under the function
/// line budget (issue #751).
pub(crate) fn report(
    args: &crate::init::commands::DoctorArgs,
    config: Option<&DomainConfig>,
    cross: &UnionGather,
    database_target: codegraph_generate::db::dialect::DatabaseTarget,
) -> usize {
    let lines = producer_lines(args, config, cross, database_target);
    for line in &lines {
        println!("{line}");
    }
    lines
        .iter()
        .filter(|line| line.starts_with("WARN "))
        .count()
}

/// The doctor call-site wrapper: resolve the generation target from the
/// loaded build plan and print the producer lines.
pub(crate) fn report_for_run(
    args: &crate::init::commands::DoctorArgs,
    config: Option<&DomainConfig>,
    cross: &UnionGather,
    build_plan: Option<&crate::profile::BuildPlan>,
) -> usize {
    report(
        args,
        config,
        cross,
        build_plan
            .map(|plan| plan.database_target())
            .unwrap_or_default(),
    )
}
