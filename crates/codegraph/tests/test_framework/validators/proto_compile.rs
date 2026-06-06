use std::path::Path;

use codegraph::generate::traits::GeneratedFile;

use super::OutputValidator;

/// Validator that runs protoc on all generated .proto files.
/// Skipped silently if `protoc` is not in PATH.
pub struct ProtoCompileValidator;

impl OutputValidator for ProtoCompileValidator {
    fn name(&self) -> &str {
        "proto_compile"
    }

    fn validate(
        &self,
        files: &[GeneratedFile],
        work_dir: &Path,
    ) -> Result<(), Vec<String>> {
        // Check if protoc is available
        let has_protoc = std::process::Command::new("protoc")
            .arg("--version")
            .output()
            .is_ok();

        if !has_protoc {
            // Silently skip if protoc is not available
            return Ok(());
        }

        // Collect proto files
        let proto_files: Vec<&GeneratedFile> = files
            .iter()
            .filter(|f| f.path.extension().map_or(false, |e| e == "proto"))
            .collect();

        if proto_files.is_empty() {
            return Ok(());
        }

        // Write proto files to temp dir for compilation
        let proto_dir = work_dir.join("proto");

        let mut errors = Vec::new();
        for pf in &proto_files {
            let proto_path = proto_dir.join(&pf.path);
            let parent = proto_path.parent().unwrap();
            if !parent.exists() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    errors.push(format!(
                        "Failed to create proto dir '{}': {}",
                        parent.display(),
                        e
                    ));
                    continue;
                }
            }
            if let Err(e) = std::fs::write(&proto_path, &pf.content) {
                errors.push(format!(
                    "Failed to write proto file '{}': {}",
                    proto_path.display(),
                    e
                ));
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        // Run protoc compilation check
        let output = std::process::Command::new("protoc")
            .arg("-I")
            .arg(&proto_dir)
            .arg("-o")
            .arg("/dev/null")
            .args(proto_files.iter().map(|pf| proto_dir.join(&pf.path)))
            .output();

        match output {
            Ok(out) => {
                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    errors.push(format!("protoc compilation failed:\n{}", stderr));
                }
            }
            Err(e) => {
                errors.push(format!("Failed to run protoc: {}", e));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}
