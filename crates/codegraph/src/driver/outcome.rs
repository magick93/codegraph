//! Result and format types of the driver workflows.

/// Output format for the `classify` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassifyFormat {
    Table,
    Json,
}

/// What a pipeline run did with the persisted-graph cache (issue #275).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutcome {
    /// True when the run reopened the persisted graph and skipped
    /// re-ingestion because the inputs hash was unchanged.
    pub graph_cache_reused: bool,
    /// The inputs hash (all model/config inputs + graph format version)
    /// when a graph cache dir was in play.
    pub inputs_hash: Option<String>,
}
