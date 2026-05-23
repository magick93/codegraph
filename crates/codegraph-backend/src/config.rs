use crate::kind::BackendKind;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct BackendConfig {
    pub kind: BackendKind,
    pub connection_url: Option<String>,
    pub data_dir: Option<PathBuf>,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            kind: BackendKind::Grafeo,
            connection_url: None,
            data_dir: None,
        }
    }
}
