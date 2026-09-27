//! Doctor-side structured incompleteness reporting (issue #279).
//!
//! Scans the `--mox-files` model sources for partial authoring and reports
//! each finding in the shared incompleteness vocabulary
//! ([`codegraph_core::types::Incompleteness`]):
//!
//! - `UnresolvedReference { target }` — an `import schema "<path>"`
//!   declaration whose target cannot be read (doctor's `check_mox_files`
//!   already hard-fails on these; this adds the structured reason line).
//! - `Draft` — a `derived` feature with no `expr` body: syntactically
//!   valid, semantically a placeholder.
//!
//! Deliberately advisory: findings are WARN-level lines and soft warnings;
//! hard failures stay owned by `check_mox_files`.

use std::path::{Path, PathBuf};

use codegraph_core::types::Incompleteness;

/// One incompleteness finding: the declaring file plus the structured
/// reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncompleteFinding {
    pub file: String,
    pub incompleteness: Incompleteness,
}

impl IncompleteFinding {
    fn unresolved(file: &str, target: &str) -> Self {
        Self {
            file: file.to_string(),
            incompleteness: Incompleteness::unresolved_reference(target),
        }
    }

    fn draft(file: &str) -> Self {
        Self {
            file: file.to_string(),
            incompleteness: Incompleteness::draft(),
        }
    }
}

/// The doctor WARN line for one finding — carries the structured reason
/// (`UnresolvedReference(target: '...')` / `Draft`), not just prose.
pub fn report_line(finding: &IncompleteFinding) -> String {
    format!(
        "WARN mox incomplete — {}: {}",
        finding.file, finding.incompleteness
    )
}

/// Scan the given `.mox` sources for incompleteness findings (issue #279).
/// Line-scans the `import schema` declarations (fs check — the same
/// contract as `check_mox_files` and the LSP), then compiles each file
/// with its readable imports to find derived features with no `expr`
/// body. Files that fail to compile are skipped here: `check_mox_files`
/// owns that hard failure.
pub fn scan_mox_incompleteness(mox_files: &[PathBuf]) -> Vec<IncompleteFinding> {
    let mut findings = Vec::new();
    for path in mox_files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let file = path.display().to_string();
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        let mut schema_imports = rex_driver::SchemaImports::new();
        let mut import_broken = false;
        for decl in crate::ingest::mox_ingest::scan_schema_imports(&text) {
            let abs_path = base_dir.join(&decl.path);
            match std::fs::read_to_string(&abs_path) {
                Ok(json) => {
                    schema_imports.insert(&file, &decl.path, json);
                }
                Err(_) => {
                    findings.push(IncompleteFinding::unresolved(&file, &decl.path));
                    import_broken = true;
                }
            }
        }
        if import_broken {
            continue;
        }
        let compilation =
            rex_driver::compile_files_with_imports(&[(file.clone(), text)], &schema_imports);
        let Some(model) = compilation.model else {
            continue;
        };
        for package in &model.packages {
            for class in &package.classes {
                for feature in &class.features {
                    // A derived feature with no (or a whitespace-only)
                    // `expr` body is a placeholder definition — a Draft.
                    if feature.is_derived
                        && feature
                            .bodies
                            .get("expr")
                            .is_none_or(|expr| expr.trim().is_empty())
                    {
                        findings.push(IncompleteFinding::draft(&file));
                    }
                }
            }
        }
    }
    findings
}
