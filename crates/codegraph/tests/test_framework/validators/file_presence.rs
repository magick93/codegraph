use std::path::Path;

use codegraph::generate::traits::GeneratedFile;

use super::OutputValidator;

/// Validator that checks required files exist in the generated output.
pub struct FilePresenceValidator {
    pub label: String,
    /// Relative file paths that must exist.
    pub required_paths: Vec<String>,
}

impl OutputValidator for FilePresenceValidator {
    fn name(&self) -> &str {
        &self.label
    }

    fn validate(
        &self,
        files: &[GeneratedFile],
        work_dir: &Path,
    ) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        let existing_paths: Vec<String> = files
            .iter()
            .map(|f| f.path.to_string_lossy().to_string())
            .collect();

        // Also check files on disk that might not be in the list
        for required in &self.required_paths {
            let on_disk = work_dir.join(required).exists();
            let in_list = existing_paths.iter().any(|p| p == required || p.ends_with(required));

            if !on_disk && !in_list {
                errors.push(format!("Required file not found: {}", required));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}
