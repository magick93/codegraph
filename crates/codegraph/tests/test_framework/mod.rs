pub mod validators;

use std::path::Path;

use codegraph_core::caching_querier::CachingQuerier;
use codegraph_core::traits::GraphQuerier;
use codegraph_config::DomainConfig;

use codegraph::generate::{run_generators_with_opts, GeneratorOpts, ProjectConfig};
use codegraph::generate::traits::GeneratedFile;

use validators::OutputValidator;

/// A composable test harness for running generators and validating output.
pub struct GeneratorTest<'a> {
    pub db: &'a dyn GraphQuerier,
    pub config: &'a DomainConfig,
    pub tera: &'a tera::Tera,
    pub output_dir: &'a Path,
    pub validators: Vec<Box<dyn OutputValidator>>,
}

impl<'a> GeneratorTest<'a> {
    /// Run all generators and validate the output.
    /// Returns the list of generated files on success.
    pub fn run(&self) -> Result<Vec<GeneratedFile>, Vec<String>> {
        std::fs::create_dir_all(self.output_dir).map_err(|e| vec![e.to_string()])?;

        let project = ProjectConfig::default();
        let cached = CachingQuerier::new(self.db);

        let report = tokio::runtime::Runtime::new()
            .map_err(|e| vec![format!("Runtime error: {e}")])?
            .block_on(run_generators_with_opts(GeneratorOpts {
                db: &cached,
                config: self.config,
                output_dir: self.output_dir,
                tera: self.tera,
                ui_overrides: &Default::default(),
                ui_domains: &Default::default(),
                schema_base_dir: Path::new(""),
                seed_config: None,
                domain_types_base: None,
                hooks_base: None,
                ext_points: None,
                build_plan: None,
                ifml_frameworks: Vec::new(),
                project_config: Some(&project),
            }))
            .map_err(|e| vec![e.to_string()])?;

        if report.has_errors() {
            let errs: Vec<String> = report
                .errors
                .iter()
                .map(|e| format!("{}/{}: {}", e.entity, e.generator, e.source))
                .collect();
            return Err(errs);
        }

        let mut files = Vec::new();
        collect_files(self.output_dir, self.output_dir, &mut files);
        Ok(files)
    }
}

fn collect_files(base: &Path, dir: &Path, files: &mut Vec<GeneratedFile>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_files(base, &path, files);
            } else if let Ok(content) = std::fs::read_to_string(&path) {
                let relative = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
                files.push(GeneratedFile {
                    path: relative,
                    content,
                });
            }
        }
    }
}

/// Helper to verify that all validators pass on a set of generated files.
pub fn validate_all(
    files: &[GeneratedFile],
    work_dir: &Path,
    validators: &[Box<dyn OutputValidator>],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for v in validators {
        if let Err(ve) = v.validate(files, work_dir) {
            errors.extend(ve);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
