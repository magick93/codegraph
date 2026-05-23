use std::path::{Path, PathBuf};

use heck::ToUpperCamelCase;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template;
use crate::generate::traits::GeneratedFile;
use codegraph_naming::to_snake_case;

/// Rust keywords that cannot be used as enum variant identifiers.
const RUST_KEYWORDS: &[&str] = &[
    "Self", "self", "super", "crate", "as", "break", "const", "continue", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "static", "struct", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
];

/// Template context for a codelist Rust enum.
#[derive(Debug, Serialize)]
pub struct RustEnumContext {
    pub enum_name: String,
    pub description: String,
    pub variants: Vec<RustEnumVariant>,
}

/// A single variant in a codelist Rust enum.
#[derive(Debug, Serialize)]
pub struct RustEnumVariant {
    pub name: String,
    pub code: String,
    pub serde_rename: Option<String>,
}

/// Sanitize a codelist value into a valid Rust enum variant name.
///
/// Rules (matching legacy `codelist_enum_emitter.rs`):
/// 1. Replace `+` with `Plus`, Unicode minus (`−`) with `Minus`
/// 2. Replace hyphens, spaces, dots, slashes with underscores
/// 3. Remove colons
/// 4. Convert to PascalCase
/// 5. If the result starts with a digit, prefix with `_`
/// 6. If the result is a Rust keyword, prefix with `R`
pub fn sanitize_variant_name(value: &str) -> String {
    let s = value
        .replace('+', "Plus")
        .replace('\u{2212}', "Minus") // Unicode minus sign
        .replace('-', "_")
        .replace(':', "")
        .replace([' ', '.', '/'], "_");
    let s = s.to_upper_camel_case();
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{}", s)
    } else if s.is_empty() {
        "_Empty".to_string()
    } else if RUST_KEYWORDS.contains(&s.as_str()) {
        format!("R{}", s)
    } else {
        s
    }
}

/// Generator that emits Rust enum files for all codelists in the graph,
/// plus a `mod.rs` re-exporting them.
pub struct RustCodelistGenerator {
    output_dir: PathBuf,
}

impl RustCodelistGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }

    /// Generate Rust enum files for all codelists + a `mod.rs`.
    pub async fn generate_all(
        &self,
        db: &dyn GraphQuerier,
        tera: &tera::Tera,
    ) -> Result<Vec<GeneratedFile>> {
        let codelists = db.list_codelists().await?;
        if codelists.is_empty() {
            return Ok(Vec::new());
        }

        let codelist_dir = self.output_dir.join("src").join("codelist");

        // Remove stale .rs files from previous runs so leftover files
        // don't accumulate when the codelist set changes between generations.
        if codelist_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&codelist_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "rs")
                        && path.file_name().is_some_and(|n| n != "mod.rs")
                    {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }

        let mut files = Vec::new();
        let mut mod_entries: Vec<(String, String)> = Vec::new(); // (snake_name, EnumName)

        for cl in &codelists {
            let enum_values = db.get_enum_values(&cl.name).await.unwrap_or_default();
            if enum_values.is_empty() {
                continue;
            }

            let enum_name = cl.name.clone();
            let snake_name = to_snake_case(&enum_name);
            let default_desc = format!("{} codelist values.", enum_name);
            let description = cl.description.as_deref().unwrap_or(&default_desc);

            let variants: Vec<RustEnumVariant> = enum_values
                .iter()
                .map(|v| {
                    let sanitized = sanitize_variant_name(&v.value);
                    let serde_rename = if sanitized != v.value {
                        Some(v.value.clone())
                    } else {
                        None
                    };
                    RustEnumVariant {
                        name: sanitized,
                        code: v.value.clone(),
                        serde_rename,
                    }
                })
                .collect();

            let ctx = RustEnumContext {
                enum_name: enum_name.clone(),
                description: description.to_string(),
                variants,
            };

            let content = render_template(tera, "codelist/enum.tera", &ctx)?;
            files.push(GeneratedFile {
                path: codelist_dir.join(format!("{}.rs", snake_name)),
                content,
            });

            mod_entries.push((snake_name, enum_name));
        }

        // Generate mod.rs with pub mod + pub use for each codelist enum
        if !mod_entries.is_empty() {
            mod_entries.sort_by(|a, b| a.0.cmp(&b.0));

            let mut mod_content = String::from(
                "//! Codelist enum re-exports.\n//! Generated by hr-graph. DO NOT EDIT.\n\n",
            );
            for (snake, enum_name) in &mod_entries {
                mod_content.push_str(&format!("pub mod {};\n", snake));
                mod_content.push_str(&format!("pub use {}::{};\n\n", snake, enum_name));
            }

            files.push(GeneratedFile {
                path: codelist_dir.join("mod.rs"),
                content: mod_content,
            });
        }

        Ok(files)
    }

    /// Generate a `mod.rs` that re-exports only the codelist enums actually
    /// referenced by the generated app code (scanned from `src/domain/` and
    /// `src/api/`).  Also removes any leftover local codelist `.rs` files
    /// from previous runs — the generated app uses re-exports from
    /// `hr_domain_types`, not local copies.
    pub async fn generate_reexport_mod(&self, db: &dyn GraphQuerier) -> Result<Vec<GeneratedFile>> {
        let codelists = db.list_codelists().await?;
        if codelists.is_empty() {
            return Ok(Vec::new());
        }

        let codelist_dir = self.output_dir.join("src").join("codelist");

        // Remove leftover local codelist .rs files (not mod.rs) so
        // generate_mod_files doesn't pick them up later.
        if codelist_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&codelist_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "rs")
                        && path.file_name().is_some_and(|n| n != "mod.rs")
                    {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }

        // Build a set of all available codelist enum names.
        let mut available: std::collections::HashSet<String> = std::collections::HashSet::new();
        for cl in &codelists {
            let enum_values = db.get_enum_values(&cl.name).await.unwrap_or_default();
            if !enum_values.is_empty() {
                available.insert(cl.name.clone());
            }
        }

        // Scan src/domain/ and src/api/ for `crate::codelist::<Name>` references.
        let mut used: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let prefix = "crate::codelist::";
        for subdir in &["domain", "api"] {
            let dir = self.output_dir.join("src").join(subdir);
            if dir.is_dir() {
                scan_rs_files_for_prefix(&dir, prefix, &mut used);
            }
        }

        // Only re-export codelists that are both available and referenced.
        let mut content = String::from(
            "//! Codelist enum re-exports from hr_domain_types.\n//! Generated by hr-graph. DO NOT EDIT.\n\n",
        );
        for name in &used {
            if available.contains(name.as_str()) {
                content.push_str(&format!("pub use hr_domain_types::codelist::{};\n", name));
            }
        }

        Ok(vec![GeneratedFile {
            path: codelist_dir.join("mod.rs"),
            content,
        }])
    }
}

/// Recursively scan `.rs` files under `dir` for occurrences of `prefix`
/// and collect the identifier that immediately follows each match.
fn scan_rs_files_for_prefix(
    dir: &std::path::Path,
    prefix: &str,
    out: &mut std::collections::BTreeSet<String>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_rs_files_for_prefix(&path, prefix, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                for (idx, _) in content.match_indices(prefix) {
                    let rest = &content[idx + prefix.len()..];
                    let ident: String = rest
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    if !ident.is_empty() {
                        out.insert(ident);
                    }
                }
            }
        }
    }
}
