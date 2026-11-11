//! gate-graph codegen wrapper: a thin clap CLI over the `codegraph` driver
//! library (the init-scaffold `wrapper_main.tera` shape, trimmed to the
//! flags the workers suite drives).
//!
//! Fixture behavior: `--ddd-files` defaults to the checked-in
//! `design/leave.ddd` — the ops harness's workers-suite regeneration passes
//! only `--mox-files`/`--config`/`--profile`/`--output`, and the design
//! plane (the `notify` 501 seam the gate asserts) must ride every run.
//! Consumer wrappers hardcode their driver args the same way (the hr-specs
//! precedent).

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use codegraph::error::Result;

#[derive(Parser)]
#[command(name = "gate-graph", about = "Codegen driver for the workers gate fixture")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Ingest the .mox model (+ the design plane), then run the generators.
    Run {
        /// rexlang .mox domain model files (primary model source).
        #[arg(long = "mox-files")]
        mox_files: Vec<PathBuf>,
        /// rexlang .ddd design files; defaults to `design/leave.ddd`.
        #[arg(long = "ddd-files")]
        ddd_files: Option<Vec<PathBuf>>,
        /// Domain config path.
        #[arg(long, default_value = "domains.toml")]
        config: PathBuf,
        /// Output directory for generated code.
        #[arg(long, default_value = "generated-workers")]
        output: PathBuf,
        /// Profile name.
        #[arg(long, default_value = "gate-workers")]
        profile: String,
        /// profiles.toml path (defaults to ./profiles.toml).
        #[arg(long)]
        profiles_config: Option<PathBuf>,
    },
}

fn empty_paths() -> &'static [PathBuf] {
    &[]
}

fn empty_strings() -> &'static [String] {
    &[]
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            mox_files,
            ddd_files,
            config,
            output,
            profile,
            profiles_config,
        } => {
            let ddd = ddd_files.unwrap_or_else(|| vec![PathBuf::from("design/leave.ddd")]);
            let args = codegraph::driver::RunArgs {
                schemas: None,
                classifier: None,
                config_path: &config,
                output: &output,
                extension_points_path: None,
                integration_manifests: empty_paths(),
                profile_name: &profile,
                variant: None,
                profiles_config_path: profiles_config,
                no_post_gen: false,
                template_dir: empty_paths(),
                ifml_files: empty_paths(),
                openapi_files: empty_paths(),
                mox_files: &mox_files,
                rosetta_files: empty_paths(),
                ddd_files: &ddd,
                evt_files: empty_paths(),
                deploy_files: empty_paths(),
                ifml_framework: empty_strings(),
                ifml_components: None,
                ifml_design_system: None,
                ux_rules: None,
                codegraph_rev: None,
                check: false,
            };
            codegraph::driver::run(args).await
        }
    }
}
