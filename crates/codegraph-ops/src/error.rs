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
