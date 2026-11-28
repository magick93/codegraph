//! IFML ingestion — the ONE implementation both the full pipeline (`run`)
//! and the IFML-only path (`ifml_generate`) call, so a change to how a flow
//! lands in the graph cannot reach one path and miss the other.
//!
//! Each file goes through the front door (issue #591): the import closure
//! is gathered, the compilation resolves module composition, the
//! interaction-reference + authorization checker runs (E0104–E0108 — every
//! action/navigate/refresh target and `requires:`/`roles:` name is proven
//! before anything lands in the graph), and the declaration-backed manifest
//! is derived. A compile or ERROR-severity checker diagnostic is a hard
//! error (the EvtModel precedent): the #559 defect class cannot
//! half-ingest. The lint channel (W0150+) renders as warnings.

use std::path::PathBuf;

use codegraph_backend::Backend;

use crate::error::Result;
use crate::ingest::ifml_ingest::IfmlIngestStats;
use phero_ir::ifml_manifest::IfmlManifest;

/// Compile, check and ingest every `.ifml` file (plus its `.actor`
/// imports), fold the per-file stats + manifests, and persist the folded
/// manifest. Prints the `Pass 1b` lines.
pub(in crate::driver) async fn ingest_ifml_files(
    be: &Backend,
    ifml_files: &[PathBuf],
) -> Result<()> {
    println!("Pass 1b: {} IFML files to ingest", ifml_files.len());
    let mut total_stats = IfmlIngestStats::default();
    let mut total_manifest = IfmlManifest::new();
    for ifml_path in ifml_files {
        let compiled = crate::ifml_compile::compile_and_check_ifml(ifml_path)?;
        for lint in &compiled.lints {
            eprintln!("warning: ifml: {lint}");
        }
        let mut stats = crate::ingest::ifml_ingest::ingest_ifml_model(
            be.ingestor(),
            &compiled.compilation.model,
        )
        .await?;
        stats.imported_policies = crate::ifml_actor_import::ingest_actor_imports(
            be.ingestor(),
            be.querier(),
            &compiled.compilation.model,
            ifml_path,
        )
        .await?;
        accumulate_stats(&mut total_stats, &stats);
        fold_manifest(&mut total_manifest, compiled.manifest);
    }
    println!("Pass 1b complete: {total_stats}");
    println!(
        "Pass 1b manifest: {} routes, {} controls, {} edges, {} actions, {} commands \
         (declaration-backed — pherolang #118)",
        total_manifest.views.len(),
        total_manifest.controls.len(),
        total_manifest.edges.len(),
        total_manifest.actions.len(),
        total_manifest.commands.len(),
    );
    // WS6: the folded manifest persists as the run's singleton
    // IfmlManifestNode — the e2e action plane consumes it (it used to be
    // dropped after the counts printed).
    let source_paths: Vec<String> = ifml_files.iter().map(|p| p.display().to_string()).collect();
    crate::ingest::ifml_ingest::ingest_ifml_manifest(be.ingestor(), &total_manifest, &source_paths)
        .await
}

fn accumulate_stats(total: &mut IfmlIngestStats, stats: &IfmlIngestStats) {
    total.view_containers += stats.view_containers;
    total.containers += stats.containers;
    total.components += stats.components;
    total.events += stats.events;
    total.parameters += stats.parameters;
    total.actions += stats.actions;
    total.module_uses += stats.module_uses;
    total.actors += stats.actors;
    total.imported_policies += stats.imported_policies;
}

/// Fold one file's manifest into the run's: declaration order across files
/// (file order, then each file's own order) — the manifest is ordered data.
fn fold_manifest(total: &mut IfmlManifest, manifest: IfmlManifest) {
    total.views.extend(manifest.views);
    total.controls.extend(manifest.controls);
    total.edges.extend(manifest.edges);
    total.actions.extend(manifest.actions);
    total.commands.extend(manifest.commands);
}
