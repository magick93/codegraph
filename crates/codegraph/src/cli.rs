use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ClassifyFormat {
    Table,
    Json,
}

#[derive(Parser)]
#[command(name = "codegraph", about = "Graph-driven code generation from JSON schemas")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Generate code from JSON schemas
    Generate {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
        /// Path to extension-points.toml (optional)
        #[arg(long)]
        extension_points: Option<PathBuf>,
    },
    /// Classify all schemas and show entity/VO decisions
    Classify {
        /// Path to JSON schema directory
        #[arg(long)]
        schemas: PathBuf,

        /// Path to classifier.toml
        #[arg(long)]
        classifier: PathBuf,

        /// Path to domains.toml
        #[arg(long)]
        config: PathBuf,

        /// Filter to a single domain
        #[arg(long)]
        domain: Option<String>,

        /// Output format
        #[arg(long, default_value = "table")]
        format: ClassifyFormat,
    },
    /// Convenience: ingest + generate in one step
    Run {
        #[arg(long)]
        schemas: PathBuf,
        #[arg(long)]
        classifier: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
        /// Path to extension-points.toml (optional)
        #[arg(long)]
        extension_points: Option<PathBuf>,
        /// Profile name to use from profiles.toml (default: "default")
        #[arg(long, default_value = "default")]
        profile: String,
        /// Profile variant to apply (e.g. "lite", "enterprise")
        #[arg(long)]
        variant: Option<String>,
        /// Path to profiles.toml (default: profiles.toml in current directory)
        #[arg(long)]
        profiles_config: Option<PathBuf>,
        /// Skip post-generation scripts even if the profile declares them
        #[arg(long)]
        no_post_gen: bool,
        /// Paths to IFML DSL (.ifml) files
        #[arg(long)]
        ifml_files: Vec<PathBuf>,
    },
    /// Start the IFML Language Server Protocol server
    Lsp {
        /// Paths to JSON schema directories
        #[arg(long)]
        schemas: Vec<PathBuf>,

        /// Path to classifier.toml
        #[arg(long)]
        classifier: Option<PathBuf>,

        /// Path to domains.toml
        #[arg(long)]
        config: Option<PathBuf>,
    },
}
