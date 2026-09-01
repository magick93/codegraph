//! Error types for the ops harness.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum OpsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("config error: {0}")]
    Config(String),

    #[error("required tool '{0}' not found — {1}")]
    MissingTool(&'static str, &'static str),

    #[error("timeout: {0}")]
    Timeout(String),

    #[error("command failed: {0}")]
    Command(String),

    #[error("test failure: {0}")]
    TestFailure(String),

    #[error("path not found: {0}")]
    PathNotFound(PathBuf),

    #[error("http error: {0}")]
    Http(String),
}

pub type OpsResult<T> = Result<T, OpsError>;

impl OpsError {
    /// Exit code mapping: config/tooling errors are fatal (1); test failures
    /// are also 1 but distinct messages; timeouts 2 (matches bash `exit 2`).
    pub fn exit_code(&self) -> i32 {
        match self {
            OpsError::Timeout(_) => 2,
            _ => 1,
        }
    }
}

/// Remediation hint for known failure modes, printed after the error. The
/// hints are consumer-facing one-liners covering the common dogfooding
/// failure modes (missing tools, unreachable Postgres, browser deps).
pub fn hint(e: &OpsError) -> Option<&'static str> {
    match e {
        OpsError::MissingTool("hurl", _) => {
            Some("install hurl: https://hurl.dev/docs/installation.html (brew install hurl)")
        }
        OpsError::MissingTool("psql", _) => Some("install postgresql-client or set PSQL_PATH"),
        OpsError::TestFailure(msg) if msg.contains("Postgres not reachable") => Some(
            "start Postgres (docker compose / supabase) and set database.api in codegraph-ops.toml",
        ),
        OpsError::TestFailure(msg) if msg.contains("hurl") => {
            Some("install hurl or set hurl = none in the manifest")
        }
        OpsError::TestFailure(msg) if msg.contains("playwright") => {
            Some("run `npx playwright install chromium` in the ui dir")
        }
        OpsError::Timeout(_) => {
            Some("increase the wait via the suite's timeout or check the log file")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_map_known_failure_modes() {
        assert!(hint(&OpsError::MissingTool("hurl", "x"))
            .unwrap()
            .contains("hurl.dev"));
        assert!(hint(&OpsError::MissingTool("psql", "x"))
            .unwrap()
            .contains("PSQL_PATH"));
        assert!(hint(&OpsError::MissingTool("npx", "x")).is_none());
    }

    #[test]
    fn hints_match_test_failure_substrings() {
        assert!(hint(&OpsError::TestFailure(
            "preflight failed: Postgres not reachable".into()
        ))
        .unwrap()
        .contains("codegraph-ops.toml"));
        assert!(hint(&OpsError::TestFailure("hurl test failed".into())).is_some());
        assert!(hint(&OpsError::TestFailure("playwright crashed".into())).is_some());
        assert!(hint(&OpsError::TestFailure("generic failure".into())).is_none());
        // Specific Postgres hint wins over generic "hurl" matching.
        assert!(
            hint(&OpsError::TestFailure("Postgres not reachable".into()))
                .unwrap()
                .contains("start Postgres")
        );
    }

    #[test]
    fn hints_cover_timeout_and_default_to_none() {
        assert!(hint(&OpsError::Timeout("server did not respond".into())).is_some());
        assert!(hint(&OpsError::Config("bad config".into())).is_none());
        assert!(hint(&OpsError::Io(std::io::Error::other("boom"))).is_none());
    }
}
