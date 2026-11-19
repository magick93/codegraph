//! Fixture compatibility sweep against the pinned pherolang compiler.
//!
//! Every model fixture in `tests/fixtures/**` is compiled the way the
//! pipeline compiles it:
//!
//! - `.mox` — all files of one fixture group as **one union** (the `run
//!   --mox-files a.mox b.mox …` semantics: cross-package references resolve
//!   against the union namespace, which a per-file compile cannot see);
//! - `.ddd` / `.evt` — through the production `read_and_compile_*` front
//!   doors, which gather the file's transitively imported domains;
//! - `.ifml` — through `compile_and_check_ifml`, the run/doctor front door.
//!
//! This is the pin's migration gate: a pherolang rev that adds a compile
//! rule (typed lifecycles, strict value semantics, DTO-boundary rules, …)
//! shows up here as a fixture failure with the diagnostic that caused it,
//! instead of surfacing later as a `codegraph run` hard error in a
//! consumer. One fixture is *expected* to fail — `ddd/invalid.ddd` is the
//! aggregate-boundary hard-error case — and its wording is pinned below.
//!
//! Node-free and DB-free: it only compiles sources in-process.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use phero_driver::{DomainImports, compile_files};

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Recursively collect files with one of `extensions`.
fn collect(dir: &Path, extensions: &[&str], out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read fixture dir {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("fixture dir entry").path();
        if path.is_dir() {
            collect(&path, extensions, out);
        } else if let Some(extension) = path.extension().and_then(|e| e.to_str())
            && extensions.contains(&extension)
        {
            out.push(path);
        }
    }
}

/// The union of `.mox` sources reachable from one group: the group's own
/// files plus every `import "x.mox"` target, transitively (paths keyed as
/// written — the compiler matches on that spelling).
fn gather_mox(root: &Path, sources: &mut Vec<(String, String)>, seen: &mut Vec<PathBuf>) {
    let Some(dir) = root.parent() else {
        return;
    };
    for import in scan_mox_imports(&std::fs::read_to_string(root).unwrap_or_default()) {
        let resolved = dir.join(&import);
        if seen.iter().any(|p| p == &resolved) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&resolved) else {
            // A missing import is the compiler's diagnostic to report, not
            // the sweep's.
            continue;
        };
        seen.push(resolved.clone());
        sources.push((import, text));
        gather_mox(&resolved, sources, seen);
    }
}

/// `import "x.mox"` targets of a `.mox` source (line scan; `import schema` /
/// `import sigil` start with a keyword, not a quote, so they never match).
fn scan_mox_imports(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("import ")?;
            let rest = rest.trim_start();
            let rest = rest.strip_prefix('"')?;
            let end = rest.find('"')?;
            Some(rest[..end].to_string())
        })
        .filter(|import| import.ends_with(".mox"))
        .collect()
}

/// Compile each fixture group's `.mox` set as one union.
fn sweep_mox() {
    let root = fixtures_root();
    let mut files = Vec::new();
    collect(&root, &["mox"], &mut files);
    files.sort();

    let mut groups: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for file in &files {
        let group = file
            .strip_prefix(&root)
            .ok()
            .and_then(|rel| rel.components().next())
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_default();
        groups.entry(group).or_default().push(file.clone());
    }
    assert!(!groups.is_empty(), "the fixture tree found no .mox files");

    for (group, group_files) in &groups {
        let mut sources: Vec<(String, String)> = Vec::new();
        let mut seen: Vec<PathBuf> = Vec::new();
        for file in group_files {
            let text = std::fs::read_to_string(file)
                .unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
            if !seen.iter().any(|p| p == file) {
                seen.push(file.clone());
                sources.push((file.to_string_lossy().into_owned(), text));
            }
            gather_mox(file, &mut sources, &mut seen);
        }
        let compilation = compile_files(&sources, &DomainImports::default());
        let errors: Vec<String> = compilation
            .diagnostics
            .iter()
            .filter(|(_, diagnostic)| diagnostic.is_error())
            .map(|(path, diagnostic)| format!("{path}: {}", diagnostic.message))
            .collect();
        assert!(
            errors.is_empty(),
            "pherolang .mox group `{group}` must compile clean under the pinned compiler:\n  {}",
            errors.join("\n  ")
        );
        assert!(
            compilation.model.is_some(),
            "pherolang .mox group `{group}` produced no model"
        );
    }
}

/// Compile the `.ddd` fixtures through the production front door.
fn sweep_ddd() {
    let root = fixtures_root();
    let mut files = Vec::new();
    collect(&root, &["ddd"], &mut files);
    files.sort();
    assert!(!files.is_empty(), "the fixture tree found no .ddd files");

    for file in &files {
        let compiled = codegraph::ingest::ddd_ingest::read_and_compile_ddd(file)
            .unwrap_or_else(|e| panic!("{} must read and compile: {e}", file.display()));
        let errors: Vec<String> = compiled
            .compilation
            .diagnostics
            .iter()
            .filter(|(_, diagnostic)| diagnostic.is_error())
            .map(|(path, diagnostic)| format!("{path}: {}", diagnostic.message))
            .collect();

        let is_hard_error_fixture =
            file.file_name().and_then(|n| n.to_str()) == Some("invalid.ddd");
        if is_hard_error_fixture {
            // The aggregate-boundary rule: a contained entity may not declare
            // a repository. This fixture exists to pin the hard-error path, so
            // it MUST fail — and fail with that rule, not a parse error.
            assert!(
                errors
                    .iter()
                    .any(|e| e.contains("only aggregate roots may declare a repository")),
                "invalid.ddd must fail the aggregate-boundary rule; got {errors:?}"
            );
            continue;
        }
        assert!(
            errors.is_empty(),
            "pherolang .ddd fixture {} must compile clean:\n  {}",
            file.display(),
            errors.join("\n  ")
        );
        assert!(
            compiled.compilation.model.is_some(),
            "{} produced no design artifact",
            file.display()
        );
        // The authored-aggregate fixture (issue #666 slice 4b): the DD4
        // artifact carries the effective verdict on every entity design,
        // and an authored `aggregate <Root>` membership lands with
        // `origin = "authored"` naming the authored root class.
        if file.file_name().and_then(|n| n.to_str()) == Some("aggregate.ddd") {
            let model = compiled.compilation.model.as_ref().unwrap();
            let designs: Vec<_> = model
                .modules
                .iter()
                .flat_map(|m| m.designs.iter())
                .collect();
            let shelf = designs
                .iter()
                .find(|d| d.class.as_deref() == Some("ShelfType"))
                .expect("the ShelfType design");
            assert_eq!(shelf.is_aggregate_root, Some(true), "authored root");
            let book = designs
                .iter()
                .find(|d| d.class.as_deref() == Some("BookType"))
                .expect("the BookType design");
            assert_eq!(book.is_aggregate_root, Some(false), "authored member");
            let membership = book.aggregate.as_ref().expect("the membership");
            assert_eq!(
                membership.origin,
                phero_ir::ddd::AggregateOrigin::Authored,
                "an authored clause outranks the containment derivation"
            );
            assert_eq!(
                membership.root,
                phero_ir::TypeRef::Class {
                    package: "nz.example.shelf".to_string(),
                    name: "ShelfType".to_string(),
                },
                "the membership names the authored root class"
            );
        }
    }
}

/// Compile the `.evt` fixtures through the production front door.
fn sweep_evt() {
    let root = fixtures_root();
    let mut files = Vec::new();
    collect(&root, &["evt"], &mut files);
    files.sort();

    for file in &files {
        let compiled = codegraph::ingest::evt_ingest::read_and_compile_evt(file)
            .unwrap_or_else(|e| panic!("{} must read and compile: {e}", file.display()));
        let errors: Vec<String> = compiled
            .compilation
            .diagnostics
            .iter()
            .filter(|(_, diagnostic)| diagnostic.is_error())
            .map(|(path, diagnostic)| format!("{path}: {}", diagnostic.message))
            .collect();
        assert!(
            errors.is_empty(),
            "pherolang .evt fixture {} must compile clean:\n  {}",
            file.display(),
            errors.join("\n  ")
        );
    }
}

/// Compile + check the `.ifml` fixtures through the production front door.
fn sweep_ifml() {
    let root = fixtures_root();
    let mut files = Vec::new();
    collect(&root, &["ifml"], &mut files);
    files.sort();

    for file in &files {
        codegraph::ifml_compile::compile_and_check_ifml(file)
            .unwrap_or_else(|e| panic!("pherolang .ifml fixture must pass the front door: {e}"));
    }
}

#[test]
fn every_fixture_compiles_under_the_pinned_pherolang() {
    sweep_mox();
    sweep_ddd();
    sweep_evt();
    sweep_ifml();
}
