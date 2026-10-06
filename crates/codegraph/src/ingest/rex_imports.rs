//! Shared rexlang source-import plumbing: `import "x.mox"` domain
//! collection, `import schema` / `import sigil` line scans, and the
//! rosetta candidate-pool directory walk. Used by both the `.actor`
//! policy imports ([`crate::ifml_actor_import`]) and the `.ddd` design
//! ingest ([`crate::ingest::ddd_ingest`]) so their import resolution
//! cannot drift.

use std::path::{Path, PathBuf};

/// One collected `.mox` domain: the import-path-as-written key the rex
/// compiler matches on, the source text, and the directory its own imports
/// (`import "x.mox"`, `import schema "y.json"`, `import sigil "z.rosetta"`)
/// resolve against.
pub(crate) struct DomainSource {
    pub path: String,
    pub source: String,
    pub dir: PathBuf,
}

/// Collect the `.mox` domain sources a rexlang surface file imports,
/// following `import "x.mox"` lines transitively. Paths are keyed exactly
/// as written (that is what the rex compile matches on) and resolved
/// relative to the importing file's directory; missing files warn and are
/// skipped (the compile reports the unsatisfied import).
pub(crate) fn collect_domains(dir: &Path, source: &str) -> Vec<DomainSource> {
    let mut domains: Vec<DomainSource> = Vec::new();
    collect_domains_inner(dir, source, &mut domains);
    domains
}

fn collect_domains_inner(dir: &Path, source: &str, domains: &mut Vec<DomainSource>) {
    for import in scan_imports(source) {
        if domains.iter().any(|d| d.path == import) {
            continue;
        }
        let resolved = dir.join(&import);
        match std::fs::read_to_string(&resolved) {
            Ok(domain_source) => {
                if let Some(domain_dir) = resolved.parent().map(Path::to_path_buf) {
                    domains.push(DomainSource {
                        path: import.clone(),
                        source: domain_source.clone(),
                        dir: domain_dir.clone(),
                    });
                    collect_domains_inner(&domain_dir, &domain_source, domains);
                }
            }
            Err(e) => {
                eprintln!(
                    "Warning: domain file '{import}' imported by a rexlang source was not found ({e}) — skipped"
                );
            }
        }
    }
}

/// Scan source text for rexlang domain import lines (`import "path.mox"`).
/// A simple line-oriented scan is sufficient for codegraph's import
/// resolution; `import schema`/`import sigil` lines do not match (their
/// next token is a keyword, not a quote).
pub(crate) fn scan_imports(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("import ")?.trim();
            let rest = rest.trim_end_matches(';').trim();
            let path = rest.strip_prefix('"')?.strip_suffix('"')?;
            (!path.is_empty()).then(|| path.to_string())
        })
        .collect()
}

/// Scan `.mox` source text for `import sigil "<path>"` declarations (same
/// line-scan contract as the `import schema` scanner in
/// [`crate::ingest::mox_ingest`], which does not capture sigil imports).
pub(crate) fn scan_sigil_imports(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("import sigil ")?.trim();
            let rest = rest.trim_end_matches(';').trim();
            let path = rest.strip_prefix('"')?.strip_suffix('"')?;
            (!path.is_empty()).then(|| path.to_string())
        })
        .collect()
}

/// Recursively collects `*.rosetta` files under `file`'s directory,
/// mirroring sigil's project discovery (the rex-cli
/// `discover_rosetta_files` port): entries whose name starts with `.` or is
/// exactly `target` are skipped with their subtree, and the walk is
/// depth-capped (bounding symlink cycles). The named file itself is not
/// included. Entries are sorted per directory, so the result order is
/// deterministic.
pub(crate) fn discover_rosetta_files(file: &Path) -> Vec<PathBuf> {
    const MAX_DEPTH: usize = 16;
    fn collect(dir: &Path, depth: usize, named: &Path, out: &mut Vec<PathBuf>) {
        if depth > MAX_DEPTH {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<std::fs::DirEntry> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name.starts_with('.') || name == "target" {
                continue;
            }
            if path.is_dir() {
                collect(&path, depth + 1, named, out);
            } else if name.ends_with(".rosetta") && path != named {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    let base = file
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    collect(base, 0, file, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_imports_finds_bare_and_semicolon_lines() {
        let source = "import \"support.mox\"\n\nactors S {\n    actor A\n}";
        assert_eq!(scan_imports(source), vec!["support.mox".to_string()]);
        assert_eq!(
            scan_imports("import \"a.mox\";\nimport \"b.mox\""),
            vec!["a.mox".to_string(), "b.mox".to_string()]
        );
        assert!(scan_imports("actors S { actor A }").is_empty());
    }

    #[test]
    fn scan_imports_ignores_schema_and_sigil_declarations() {
        let source = concat!(
            "import schema \"schemas/todo_item.json\" as TodoItem\n",
            "import sigil \"oracle/trade.rosetta\"\n",
            "import \"lib.mox\"\n",
        );
        assert_eq!(scan_imports(source), vec!["lib.mox".to_string()]);
    }

    #[test]
    fn scan_sigil_imports_finds_only_sigil_declarations() {
        let source = concat!(
            "import schema \"schemas/todo_item.json\" as TodoItem\n",
            "import sigil \"oracle/trade.rosetta\"\n",
            "import sigil \"other.rosetta\";\n",
            "import \"lib.mox\"\n",
        );
        assert_eq!(
            scan_sigil_imports(source),
            vec![
                "oracle/trade.rosetta".to_string(),
                "other.rosetta".to_string()
            ]
        );
    }

    #[test]
    fn discover_rosetta_files_skips_dot_and_target_entries() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("named.rosetta"), "namespace a\nversion \"1.0.0\"").unwrap();
        std::fs::write(
            root.join("sibling.rosetta"),
            "namespace b\nversion \"1.0.0\"",
        )
        .unwrap();
        std::fs::create_dir(root.join(".hidden")).unwrap();
        std::fs::write(root.join(".hidden/deep.rosetta"), "namespace c").unwrap();
        std::fs::create_dir(root.join("target")).unwrap();
        std::fs::write(root.join("target/built.rosetta"), "namespace d").unwrap();
        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/nested.rosetta"), "namespace e").unwrap();
        std::fs::write(root.join("not-rosetta.txt"), "nope").unwrap();

        let named = root.join("named.rosetta");
        let found = discover_rosetta_files(&named);
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            names,
            vec!["sibling.rosetta".to_string(), "nested.rosetta".to_string()]
        );
    }
}
