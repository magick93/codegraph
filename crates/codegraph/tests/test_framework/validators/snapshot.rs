use std::collections::HashMap;
use std::path::Path;

use codegraph::generate::traits::GeneratedFile;

use super::OutputValidator;

/// Validator that collects generated files into a map keyed by relative path.
/// Useful when you want to inspect specific files after generation.
pub struct SnapshotCollector {
    pub label: String,
    pub files: std::sync::Mutex<HashMap<String, GeneratedFile>>,
}

impl SnapshotCollector {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            files: std::sync::Mutex::new(HashMap::new()),
        }
    }
}

impl OutputValidator for SnapshotCollector {
    fn name(&self) -> &str {
        &self.label
    }

    fn validate(
        &self,
        files: &[GeneratedFile],
        _work_dir: &Path,
    ) -> Result<(), Vec<String>> {
        let mut map = self.files.lock().unwrap();
        for file in files {
            let path_str = file.path.to_string_lossy().to_string();
            map.insert(path_str, GeneratedFile {
                path: file.path.clone(),
                content: file.content.clone(),
            });
        }
        Ok(())
    }
}
