use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ClassifyFormat {
    Table,
    Json,
}

#[derive(Parser)]
#[command(
    name = "codegraph",
    about = "Graph-driven code generation from JSON schemas"
)]
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
        /// Paths to additional template directories. Templates in these directories
        /// shadow codegraph's built-in templates by name. May be specified multiple
        /// times; later directories take precedence.
        #[arg(long)]
        template_dir: Vec<PathBuf>,
        /// IFML framework targets for code generation (e.g. svelte, react)
        #[arg(long)]
        ifml_framework: Vec<String>,
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
        /// Paths to additional template directories. Templates in these directories
        /// shadow codegraph's built-in templates by name. May be specified multiple
        /// times; later directories take precedence.
        #[arg(long)]
        template_dir: Vec<PathBuf>,
        /// Paths to IFML DSL (.ifml) files
        #[arg(long)]
        ifml_files: Vec<PathBuf>,
        /// Paths to OpenAPI 3.0/3.1 spec files (JSON) to import into the graph
        #[arg(long)]
        openapi_files: Vec<PathBuf>,
        /// IFML framework targets for code generation (e.g. svelte, react)
        #[arg(long)]
        ifml_framework: Vec<String>,
    },
    /// IFML-only UI generation: ingest .ifml DSL files and emit framework routes
    IfmlGenerate {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
        /// Paths to IFML DSL (.ifml) files
        #[arg(long, required = true)]
        ifml_files: Vec<PathBuf>,
        /// Path to JSON schema directory (enables entity resolution enrichment)
        #[arg(long)]
        schemas: Option<PathBuf>,
        /// Path to classifier.toml (required only with --schemas)
        #[arg(long)]
        classifier: Option<PathBuf>,
        /// IFML framework targets for code generation (e.g. svelte, react)
        #[arg(long, default_values = ["svelte"])]
        framework: Vec<String>,
        /// Path to profiles.toml (optional)
        #[arg(long)]
        profiles_config: Option<PathBuf>,
        /// Paths to additional template directories. Templates in these directories
        /// shadow codegraph's built-in templates by name. May be specified multiple
        /// times; later directories take precedence.
        #[arg(long)]
        template_dir: Vec<PathBuf>,
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
    /// Migrate domain configuration to the graph-based API model.
    /// Reads domains.toml and creates ApiResource/Operation/Endpoint
    /// nodes in an existing graph database.
    Migrate(MigrateArgs),
    /// Scaffold a new consumer project (domains.toml, schemas, workspace, ops harness)
    Init {
        /// Project name (kebab-case); prompts interactively when omitted.
        name: Option<String>,

        /// Parent directory to create the project in (default: ./<name>)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Domain names, comma-separated (default: common)
        #[arg(long, value_delimiter = ',')]
        domains: Option<Vec<String>>,

        #[arg(long, default_value = "postgres")]
        database_target: String,

        #[arg(long, default_value = "sea_orm")]
        persistence_provider: String,

        #[arg(long, default_value = "monolith")]
        deployment_topology: String,

        #[arg(long)]
        grpc: bool,

        #[arg(long)]
        ifml: bool,

        #[arg(long = "no-ops")]
        no_ops: bool,

        /// Codegraph git rev to pin (default: this binary's rev)
        #[arg(long)]
        rev: Option<String>,

        /// Use local path deps to this codegraph checkout instead of git+rev
        #[arg(long)]
        codegraph_path: Option<PathBuf>,

        #[arg(long)]
        force: bool,

        /// Paths to additional template directories (later dirs take precedence)
        #[arg(long)]
        template_dir: Vec<PathBuf>,
    },
    /// Validate an existing consumer project's configuration and toolchain
    Doctor {
        #[arg(long, default_value = "domains.toml")]
        config: PathBuf,

        #[arg(long, default_value = "schemas")]
        schemas: PathBuf,

        #[arg(long, default_value = "classifier.toml")]
        classifier: PathBuf,

        #[arg(long)]
        profiles_config: Option<PathBuf>,
    },
    /// Add to an existing consumer project
    Add {
        #[command(subcommand)]
        target: AddTarget,
    },
}

#[derive(Subcommand)]
pub enum AddTarget {
    /// Add a domain (schemas dir + domains.toml entry).
    Domain { name: String },
}

#[derive(Parser, Debug)]
pub struct MigrateArgs {
    /// Path to the domain configuration file
    #[arg(long, default_value = "domains.toml")]
    pub config: PathBuf,

    /// Path to the schema directory (for loading existing schemas)
    #[arg(long, default_value = "schemas")]
    pub schemas: PathBuf,

    /// Path to the classifier configuration
    #[arg(long, default_value = "classifier.toml")]
    pub classifier: PathBuf,
}
