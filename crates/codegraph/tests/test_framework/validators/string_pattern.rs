use std::path::Path;

use codegraph::generate::traits::GeneratedFile;

use super::OutputValidator;

/// Validator that checks generated file content for required or forbidden patterns.
pub struct StringPatternValidator {
    pub label: String,
    /// Patterns that must appear in at least one generated file.
    pub required_patterns: Vec<String>,
    /// Patterns that must NOT appear in any generated file.
    pub forbidden_patterns: Vec<String>,
}

impl OutputValidator for StringPatternValidator {
    fn name(&self) -> &str {
        &self.label
    }

    fn validate(
        &self,
        files: &[GeneratedFile],
        _work_dir: &Path,
    ) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if files.is_empty() && (!self.required_patterns.is_empty() || !self.forbidden_patterns.is_empty()) {
            errors.push("No files generated to validate patterns against".to_string());
            return Err(errors);
        }

        // Check required patterns
        for pattern in &self.required_patterns {
            let found = files.iter().any(|f| f.content.contains(pattern.as_str()));
            if !found {
                errors.push(format!(
                    "Required pattern not found in any file: '{}'",
                    pattern
                ));
            }
        }

        // Check forbidden patterns
        for pattern in &self.forbidden_patterns {
            for file in files {
                if file.content.contains(pattern.as_str()) {
                    errors.push(format!(
                        "Forbidden pattern found in '{}': '{}'",
                        file.path.display(),
                        pattern
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}
