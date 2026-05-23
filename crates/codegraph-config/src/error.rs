#[derive(Debug, thiserror::Error)]
pub enum DomainConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("Invalid config: {0}")]
    Invalid(String),
    #[error("Cycle in domain dependency graph involving: {0}")]
    CyclicDependency(String),
}
