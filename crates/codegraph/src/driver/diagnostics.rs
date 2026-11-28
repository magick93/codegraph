//! User-facing notices, the generation report summary, and the opt-in
//! `CODEGRAPH_PERF_COUNTERS` lines (issues #469/#473). Everything here
//! prints; nothing here decides.

/// Stderr notice when `--schemas` is the only model source (issue #231).
pub fn schemas_deprecation_notice() -> &'static str {
    "WARN: --schemas is deprecated as the primary model source. Migrate to .mox files: codegraph migrate --schemas <dir> --output <dir>"
}

/// Stderr notice when both model sources are provided (issue #231).
pub fn mox_primary_notice() -> &'static str {
    "INFO: .mox files are the primary model source; --schemas fills gaps for types not authored in .mox"
}

/// Stderr notice when .rosetta is a provided primary source (issue #257).
pub fn rosetta_primary_notice() -> &'static str {
    "INFO: .rosetta files are a primary model source; --schemas fills gaps for types not authored in .rosetta"
}

/// The model-source notices `run` prints (stderr; stdout stays clean for
/// machine output). Only a schemas-bearing run says anything.
pub(super) fn print_model_source_notices(has_schemas: bool, has_mox: bool, has_rosetta: bool) {
    if has_schemas {
        if has_mox {
            eprintln!("{}", mox_primary_notice());
        } else if has_rosetta {
            eprintln!("{}", rosetta_primary_notice());
        } else {
            eprintln!("{}", schemas_deprecation_notice());
        }
    }
}

/// Print a generation report's summary plus the shared "completed with
/// errors" line every generating workflow emits.
pub(super) fn print_generation_report(report: &crate::generate::report::GenerationReport) {
    print!("{}", report.summary());
    if report.has_errors() {
        eprintln!("Generation completed with errors. Some entities were skipped.");
    }
}

fn perf_counters_enabled() -> bool {
    std::env::var_os("CODEGRAPH_PERF_COUNTERS").is_some_and(|v| v != "0")
}

fn format_cache_stats() -> String {
    let stats = codegraph_core::caching_querier::cache_stats_snapshot();
    let parts: Vec<String> = stats
        .iter()
        .filter(|(_, (hits, misses))| *hits > 0 || *misses > 0)
        .map(|(method, (hits, misses))| format!("{method}={hits}/{misses}"))
        .collect();
    if parts.is_empty() {
        "(none)".to_string()
    } else {
        parts.join(" ")
    }
}

/// One ingest summary line after the ingest passes (issues #469/#473) —
/// only when CODEGRAPH_PERF_COUNTERS is set.
pub(super) fn print_ingest_perf() {
    if perf_counters_enabled() {
        let perf = codegraph_grafeo::perf::perf_snapshot();
        eprintln!(
            "[perf] ingest: nodes={} edges={} gql_ingest={} gql_read={}",
            perf.ingest_node_inserts,
            perf.ingest_edge_inserts,
            perf.ingest_gql_calls,
            perf.read_gql_calls
        );
    }
}

/// One validate+generate line after generation, measured from `start` —
/// only when CODEGRAPH_PERF_COUNTERS is set.
pub(super) fn print_generate_perf(start: &codegraph_grafeo::perf::PerfSnapshot) {
    if perf_counters_enabled() {
        let perf_end = codegraph_grafeo::perf::perf_snapshot();
        eprintln!(
            "[perf] validate+generate: gql_read=+{} (total {}) | cache hits/misses: {}",
            perf_end.read_gql_calls - start.read_gql_calls,
            perf_end.read_gql_calls,
            format_cache_stats()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemas_deprecation_notice_has_exact_text() {
        assert_eq!(
            schemas_deprecation_notice(),
            "WARN: --schemas is deprecated as the primary model source. Migrate to .mox files: codegraph migrate --schemas <dir> --output <dir>"
        );
    }

    #[test]
    fn mox_primary_notice_states_mox_wins_and_schemas_fill_gaps() {
        let notice = mox_primary_notice();
        assert!(
            notice.starts_with("INFO: .mox files are the primary model source"),
            "{notice}"
        );
        assert!(notice.contains("--schemas fills gaps"), "{notice}");
    }
}
