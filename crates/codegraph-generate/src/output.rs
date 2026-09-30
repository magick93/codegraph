use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use tera::Tera;

use crate::db::dialect::{db_template_for, SqlDialect};
use crate::error::{Error, Result};
use crate::project_config::{GenerationEntry, ProjectConfig};
use crate::traits::{DomainGeneratorKind, EntityGeneratorKind, GeneratedFile};

/// Returns true if the generator name is an API-layer entity generator
/// (handler, workflow, media, test, UI, CLI, gRPC, playwright).
/// DDD generators (ddl, entity, repo, command, query, event, dto, lifecycle_trait,
/// domain_types) are NOT considered API generators.
pub fn is_api_entity_generator(name: &str) -> bool {
    EntityGeneratorKind::from_name(name).is_some_and(EntityGeneratorKind::is_api)
}

/// True for entity generators whose output is backend Rust source scoped to a
/// single domain (`src/domain/{domain}/`, `src/api/{domain}/`,
/// `src/entity/{module}.rs`, `queries/`).  In workers topology these are
/// constructed with the per-domain crate directory (`workers/{domain}/`) so
/// their output lands inside the worker crate.
///
/// Everything else — DDL migrations, API tests, UI, CLI, gRPC, playwright,
/// domain-types, hooks — stays anchored at the output root (single shared
/// database, unchanged frontend).
///
/// `cornucopia_queries` is deliberately root-anchored in both topologies:
/// the annotated `.sql` files feed the single shared `cornucopia-queries`
/// codegen crate at the output root (`queries/{domain}/{entity}.sql`), which
/// every worker crate depends on by path.
pub fn is_worker_routed_entity_generator(name: &str) -> bool {
    EntityGeneratorKind::from_name(name).is_some_and(EntityGeneratorKind::is_worker_routed)
}

/// True for domain generators whose output belongs to a single domain's
/// backend crate (`src/domain/{domain}/`, `src/api/{domain}/`).  UI, CLI and
/// gRPC domain generators stay anchored at the output root.
pub fn is_worker_routed_domain_generator(name: &str) -> bool {
    DomainGeneratorKind::from_name(name).is_some_and(DomainGeneratorKind::is_worker_routed)
}

/// Resolve the construction-time base directory for a generator.
///
/// In workers topology, routed generators are constructed with the
/// per-domain crate directory (`workers/{domain}/`) so their
/// `output_dir.join(...)` paths land inside the worker crate.  Everything
/// else keeps the monolith output root.  When `worker_base` is `None`
/// (monolith topology) the root is always used, keeping monolith output
/// byte-identical to previous behaviour.
pub(crate) fn generator_base<'a>(
    root: &'a Path,
    worker_base: Option<&'a Path>,
    routed: bool,
) -> &'a Path {
    match worker_base {
        Some(base) if routed => base,
        _ => root,
    }
}

/// Resolves the directory in which the optional `reports.toml` config is
/// looked up: the domain config directory when supplied, else the process
/// current directory (legacy behavior).
pub(crate) fn reports_config_dir(domain_config_dir: Option<&Path>) -> PathBuf {
    domain_config_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

/// Render a serializable context through a Tera template.
/// Injects `project` config into the template context when provided.
pub fn render_template<C: serde::Serialize>(
    tera: &Tera,
    template_name: &str,
    ctx: &C,
) -> Result<String> {
    let context = tera::Context::from_serialize(ctx)
        .map_err(|e| Error::Template(format!("Serialize context: {}", e)))?;
    tera.render(template_name, &context)
        .map_err(|e| Error::Template(format!("'{}': {}", template_name, e)))
}

/// Like [`render_template`] but injects `project` into the template context.
/// Templates can use `{{ project.app_name }}`, `{{ project.domain_types_crate }}`, etc.
pub fn render_template_with_project<C: serde::Serialize>(
    tera: &Tera,
    template_name: &str,
    ctx: &C,
    project: &ProjectConfig,
) -> Result<String> {
    let mut context = tera::Context::from_serialize(ctx)
        .map_err(|e| Error::Template(format!("Serialize context: {}", e)))?;
    context.insert("project", project);
    tera.render(template_name, &context)
        .map_err(|e| Error::Template(format!("'{}': {}", template_name, e)))
}

/// Render a template with project context, resolving the template name
/// through the dialect's template path mapping.
pub fn render_template_with_project_and_dialect<C: serde::Serialize>(
    tera: &Tera,
    template_name: &str,
    ctx: &C,
    project: &ProjectConfig,
    dialect: &dyn SqlDialect,
) -> Result<String> {
    let resolved = db_template_for(dialect, template_name);
    render_template_with_project(tera, &resolved, ctx, project)
}

/// Partitioned migration sequence bands guaranteeing lexicographic == numeric
/// order: platform bootstrap files occupy 0..9 (four-digit prefixes), codelists
/// start at [`MigrationSeq::CODELIST_START`], entities at
/// [`MigrationSeq::ENTITY_START`]; emitted prefixes are zero-padded to six
/// digits so ordering stays correct across the five-digit boundary (9999 →
/// 010000).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct MigrationSeq(u32);

impl MigrationSeq {
    /// First sequence of the codelist band (`CODELIST_START + codelist idx`).
    pub(crate) const CODELIST_START: MigrationSeq = MigrationSeq(10);
    /// First sequence of the entity band, clear of the codelist range.
    pub(crate) const ENTITY_START: MigrationSeq = MigrationSeq(500);

    pub(crate) const fn get(self) -> u32 {
        self.0
    }

    pub(crate) const fn plus(self, n: u32) -> Self {
        MigrationSeq(self.0 + n)
    }
}

/// Add a numeric prefix to migration file paths so alphabetical order matches
/// dependency order. Non-migration files are returned unchanged.
///
/// The prefix is zero-padded to SIX digits: consumers (supabase db reset,
/// glob-based migration loops) order files lexicographically, so an
/// unpadded width would break once the sequence crosses 9999 — e.g. the
/// entity DDL `9996_benefits_dependent.sql` sorting *after* its own child
/// RLS at `10000_..._rls.sql` (SQLSTATE 42P01 during db reset).
pub(crate) fn prefix_migration_path(mut file: GeneratedFile, seq: usize) -> GeneratedFile {
    let is_migration = file
        .path
        .parent()
        .and_then(|p| p.file_name())
        .is_some_and(|d| d == "migrations");
    if is_migration {
        if let Some(name) = file.path.file_name().and_then(|n| n.to_str()) {
            // Skip files that already have a numeric prefix (e.g. 000005_pgmq_setup.sql)
            if !name.starts_with(|c: char| c.is_ascii_digit()) {
                let prefixed = format!("{:06}_{}", seq, name);
                file.path = file.path.with_file_name(prefixed);
            }
        }
    }
    file
}

/// Clean stale generated files from the output directory.
///
/// Removes entity-specific subdirectories under `src/domain/{domain}/` that
/// are NOT in the current generation order.  Stale modules from a previous run
/// (e.g. entities reclassified as value objects, or sample schemas removed from
/// the schema loader) must not linger and cause broken `pub mod` declarations
/// in mod.rs.
///
/// Directories that ARE in the generation order are left intact so that, if any
/// generator fails or is skipped partway through, the previous working state is
/// preserved.  Generators always overwrite files they produce, so leaving the
/// directory in place is safe.
///
/// The `mod.rs` file in each domain directory is always preserved; it is
/// regenerated by `generate_mod_files` after the generators run.
///
/// In workers topology the backend source scans (`src/domain/{domain}/`,
/// `src/api/{domain}/`) target the per-domain worker crate at
/// `workers/{domain}/`; UI route and UI test scans stay at the output root
/// (the frontend is shared).
pub(crate) fn clean_generated_output(
    output_dir: &Path,
    generation_order: &[GenerationEntry],
    suffix: &str,
    workers_topology: bool,
) {
    // Build the set of (domain, module_name) pairs that SHOULD exist after
    // this run.
    let mut expected: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for entry in generation_order {
        let stripped = codegraph_naming::strip_suffix(&entry.schema_title, suffix);
        let module_name = codegraph_naming::to_snake_case(&stripped);
        expected.insert((entry.domain.clone(), module_name));
    }

    // Build the set of (domain, path_segment) pairs for UI route/test directories.
    // path_segment is kebab-case of the stripped title.
    let mut expected_paths: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for entry in generation_order {
        let stripped = codegraph_naming::strip_suffix(&entry.schema_title, suffix);
        let path_segment = codegraph_naming::to_kebab_case(&stripped);
        expected_paths.insert((entry.domain.clone(), path_segment));
    }

    // Collect the set of domain names that appear in the generation order so
    // we only scan domains we actually care about.
    let domains: std::collections::HashSet<&str> =
        generation_order.iter().map(|e| e.domain.as_str()).collect();

    for domain in &domains {
        // Backend source base: the per-domain worker crate in workers
        // topology, the output root in monolith topology.
        let backend_base = if workers_topology {
            output_dir.join("workers").join(domain)
        } else {
            output_dir.to_path_buf()
        };
        let domain_dir = backend_base.join("src").join("domain").join(domain);
        if !domain_dir.is_dir() {
            continue;
        }

        // Walk the immediate children of src/domain/{domain}/.
        // Each subdirectory is an entity module; mod.rs is preserved.
        let entries = match fs::read_dir(&domain_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for child in entries.flatten() {
            let path = child.path();
            if !path.is_dir() {
                continue; // skip mod.rs and other files
            }
            let module_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            // Only remove directories that are NOT in the expected set.
            // Directories in the expected set are left for generators to overwrite —
            // if a generator fails partway through, the previous working state is preserved.
            let key = (domain.to_string(), module_name.clone());
            if expected.contains(&key) {
                // NOTE: dto_included.rs is deliberately NOT deleted here. The
                // dto generator emits it only when include paths resolve, so a
                // profile whose plan skips dto hydration (e.g. an e2e-only
                // regen after a fullstack run) would otherwise destroy the
                // fullstack-emitted file while handlers still import it. The
                // per-run filesystem scan in generate_mod_files re-declares
                // `pub mod dto_included;` only while the file exists, which is
                // exactly the desired behaviour.
                continue;
            }
            tracing::debug!(
                domain = %domain,
                entity = %module_name,
                path = %path.display(),
                "removing stale entity directory"
            );
            let _ = fs::remove_dir_all(&path);
        }

        // Clean stale API handler files: src/api/{domain}/*_handler.rs
        let api_domain_dir = backend_base.join("src").join("api").join(domain);
        if api_domain_dir.is_dir() {
            if let Ok(api_entries) = fs::read_dir(&api_domain_dir) {
                for child in api_entries.flatten() {
                    let path = child.path();
                    let name = match path.file_name().and_then(|n| n.to_str()) {
                        Some(n) => n.to_string(),
                        None => continue,
                    };
                    // Only clean *_handler.rs files (not mod.rs, router.rs, etc.)
                    if let Some(module) = name.strip_suffix("_handler.rs") {
                        let key = (domain.to_string(), module.to_string());
                        if !expected.contains(&key) {
                            tracing::debug!(
                                domain = %domain,
                                handler = %name,
                                path = %path.display(),
                                "removing stale API handler file"
                            );
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }

        // Also clean stale UI route directories: ui/src/routes/(app)/{domain}/{path_segment}/
        let ui_route_dir = output_dir
            .join("ui")
            .join("src")
            .join("routes")
            .join("(app)")
            .join(domain);
        if ui_route_dir.is_dir() {
            if let Ok(route_entries) = fs::read_dir(&ui_route_dir) {
                for child in route_entries.flatten() {
                    let path = child.path();
                    if !path.is_dir() {
                        continue;
                    }
                    let seg = match path.file_name().and_then(|n| n.to_str()) {
                        Some(n) => n.to_string(),
                        None => continue,
                    };
                    // Keep special SvelteKit files like +layout.svelte's directory
                    if seg.starts_with('+') {
                        continue;
                    }
                    let key = (domain.to_string(), seg.clone());
                    if !expected_paths.contains(&key) {
                        tracing::debug!(
                            domain = %domain,
                            path_segment = %seg,
                            path = %path.display(),
                            "removing stale UI route directory"
                        );
                        let _ = fs::remove_dir_all(&path);
                    }
                }
            }
        }

        // Clean stale UI e2e test files: ui/tests/generated/{domain}/{path_segment}.*.test.ts
        let ui_tests_dir = output_dir
            .join("ui")
            .join("tests")
            .join("generated")
            .join(domain);
        if ui_tests_dir.is_dir() {
            if let Ok(test_entries) = fs::read_dir(&ui_tests_dir) {
                for child in test_entries.flatten() {
                    let path = child.path();
                    if path.is_dir() {
                        continue;
                    }
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        // Test file names are like "{path_segment}.api.crud.test.ts"
                        let path_seg = name.split('.').next().unwrap_or("").to_string();
                        let key = (domain.to_string(), path_seg);
                        if !expected_paths.contains(&key) {
                            tracing::debug!(
                                domain = %domain,
                                file = %name,
                                "removing stale UI test file"
                            );
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }
    }

    // Clean stale entity test files: tests/{domain}/{module}_test.rs and
    // {module}_dto_test.rs. The integration-test glue (tests/<domain>/mod.rs,
    // tests/tests.rs) is regenerated from the filesystem after generation, so
    // removed files simply disappear from the declared modules.
    let tests_dir = output_dir.join("tests");
    if tests_dir.is_dir() {
        if let Ok(test_domain_entries) = fs::read_dir(&tests_dir) {
            for child in test_domain_entries.flatten() {
                let path = child.path();
                if !path.is_dir() {
                    continue; // top-level test crates are not per-domain
                }
                let domain = match path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };
                if !domains.contains(domain.as_str()) {
                    tracing::debug!(
                        domain = %domain,
                        path = %path.display(),
                        "removing stale test domain directory"
                    );
                    let _ = fs::remove_dir_all(&path);
                    continue;
                }
                if let Ok(file_entries) = fs::read_dir(&path) {
                    for child in file_entries.flatten() {
                        let file_path = child.path();
                        if file_path.is_dir() {
                            continue;
                        }
                        let name = match child.file_name().to_str() {
                            Some(n) => n.to_string(),
                            None => continue,
                        };
                        // Only clean the generated per-entity test file
                        // patterns ({module}_test.rs, {module}_dto_test.rs);
                        // mod.rs and anything else is left alone.
                        let module = if let Some(m) = name.strip_suffix("_dto_test.rs") {
                            m
                        } else if let Some(m) = name.strip_suffix("_test.rs") {
                            m
                        } else {
                            continue;
                        };
                        let key = (domain.clone(), module.to_string());
                        if !expected.contains(&key) {
                            tracing::debug!(
                                domain = %domain,
                                file = %name,
                                path = %file_path.display(),
                                "removing stale entity test file"
                            );
                            let _ = fs::remove_file(&file_path);
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn write_output(file: &GeneratedFile) -> Result<()> {
    if let Some(parent) = file.path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Write-if-changed: deterministic generators re-emit identical bytes on
    // every run; skipping identical writes preserves consumer mtimes so
    // downstream staleness checks (ops e2e binary-vs-src freshness) and
    // build caches stay accurate across regenerations.
    if fs::metadata(&file.path).is_ok_and(|m| m.is_file())
        && fs::read(&file.path).is_ok_and(|existing| existing == file.content.as_bytes())
    {
        return Ok(());
    }
    fs::write(&file.path, &file.content)?;
    Ok(())
}

/// Clean stale IFML-generated route files that are no longer in the IFML model.
///
/// `fw_output` is the per-framework output root (e.g. `out/svelte`); the scan
/// follows each framework's layout conventions from
/// `ifml::output_paths::OutputPaths::for_framework`. Only files/directories
/// matching the framework's page naming pattern are considered IFML-owned;
/// everything else is left untouched.
pub(crate) fn clean_stale_ifml_routes(fw_output: &Path, framework: &str, active_views: &[String]) {
    match framework {
        "svelte" => clean_svelte_routes(fw_output, active_views),
        "react" => clean_react_routes(fw_output, active_views),
        "vue" => clean_file_routes(&fw_output.join("pages"), active_views, |name| {
            format!("{}.vue", codegraph_naming::to_kebab_case(name))
        }),
        "flutter" => clean_file_routes(
            &fw_output.join("lib").join("screens"),
            active_views,
            |name| format!("{}_screen.dart", codegraph_naming::to_snake_case(name)),
        ),
        "swiftui" => clean_file_routes(&fw_output.join("Views"), active_views, |name| {
            format!("{name}View.swift")
        }),
        _ => {}
    }
}

/// SvelteKit: routes live in `src/routes/{view}/+page.svelte` directories.
/// Directories starting with `_`, `(`, or `.` are SvelteKit conventions
/// (private modules, route groups, hidden files) and are never touched.
fn clean_svelte_routes(fw_output: &Path, active_views: &[String]) {
    let routes_dir = fw_output.join("src").join("routes");
    if !routes_dir.is_dir() {
        return;
    }
    let entries = match fs::read_dir(&routes_dir) {
        Ok(e) => e,
        _ => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if dir_name.starts_with('_') || dir_name.starts_with('(') || dir_name.starts_with('.') {
            continue;
        }
        if path.join("+page.svelte").exists() {
            let is_active = active_views.iter().any(|v| v.to_lowercase() == dir_name);
            if !is_active {
                tracing::debug!("Removing stale IFML route: {}", path.display());
                let _ = fs::remove_dir_all(&path);
            }
        }
    }
}

/// Next.js: routes live in `app/{kebab-view}/page.tsx` directories.
fn clean_react_routes(fw_output: &Path, active_views: &[String]) {
    let app_dir = fw_output.join("app");
    if !app_dir.is_dir() {
        return;
    }
    let entries = match fs::read_dir(&app_dir) {
        Ok(e) => e,
        _ => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if path.join("page.tsx").exists() {
            let is_active = active_views
                .iter()
                .any(|v| codegraph_naming::to_kebab_case(v) == dir_name);
            if !is_active {
                tracing::debug!("Removing stale IFML route: {}", path.display());
                let _ = fs::remove_dir_all(&path);
            }
        }
    }
}

/// Flat-file frameworks (vue/flutter/swiftui): each view is one file in a
/// single directory, named via `file_name`. Only files matching the naming
/// pattern are removed.
fn clean_file_routes(dir: &Path, active_views: &[String], file_name: impl Fn(&str) -> String) {
    if !dir.is_dir() {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        _ => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let is_active = active_views.iter().any(|v| file_name(v) == name);
        if !is_active {
            tracing::debug!("Removing stale IFML route file: {}", path.display());
            let _ = fs::remove_file(&path);
        }
    }
}

/// Remove stale `mod.rs` files from a previous generation run.
/// This ensures `generate_mod_files` always creates fresh module declarations
/// that include all `.rs` files in the directory.
#[allow(dead_code)]
fn clean_stale_mod_files(src_dir: &Path) {
    if !src_dir.exists() {
        return;
    }
    for entry in walkdir::WalkDir::new(src_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_name() == "mod.rs" {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Generate `mod.rs` files for all directories under `src_dir` that contain `.rs` files.
///
/// Scans the directory tree and creates a `mod.rs` in each directory with
/// `pub mod <name>;` declarations for every `.rs` file and subdirectory.
/// Skips `mod.rs`, `main.rs`, and `lib.rs` (these are not submodules).
pub(crate) fn generate_mod_files(src_dir: &Path) -> Result<Vec<GeneratedFile>> {
    let mut files = Vec::new();
    generate_mod_files_recursive(src_dir, &mut files)?;
    Ok(files)
}

/// Returns `true` if the directory has any content (`.rs` files or subdirectories
/// with content), avoiding a redundant second `read_dir` call.
fn generate_mod_files_recursive(dir: &Path, out: &mut Vec<GeneratedFile>) -> Result<bool> {
    if !dir.is_dir() {
        return Ok(false);
    }

    // Cargo treats `src/bin/mod.rs` as a binary target — never generate mod.rs
    // inside a `bin` directory to avoid phantom binary targets with no `main()`.
    if dir.file_name().is_some_and(|n| n == "bin") {
        return Ok(false);
    }

    let mut modules = BTreeSet::new();
    let mut has_content = false;

    let entries = fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            let subdir_has_content = generate_mod_files_recursive(&path, out)?;
            if subdir_has_content {
                modules.insert(name);
            }
        } else if let Some(ext) = path.extension() {
            if ext == "rs" {
                has_content = true;
                // Skip special files that aren't submodules
                if matches!(name.as_str(), "mod.rs" | "main.rs" | "lib.rs") {
                    continue;
                }
                let module_name = name.strip_suffix(".rs").unwrap_or(&name);
                modules.insert(module_name.to_string());
            }
        }
    }

    if !modules.is_empty() {
        has_content = true;
        let mod_path = dir.join("mod.rs");
        // Skip if a mod.rs with pub-use re-exports exists (written by a specialised
        // generator like codelist). Always overwrite plain `pub mod` declarations.
        // Skip if mod.rs was explicitly generated (contains real code beyond
        // auto-generated pub mod declarations). Check for pub use (codelist)
        // or use statements (scaffold middleware, etc.).
        let skip = mod_path.exists()
            && fs::read_to_string(&mod_path)
                .map(|c| c.contains("pub use ") || c.contains("use "))
                .unwrap_or(false);
        if !skip {
            let content = modules
                .iter()
                .map(|m| format!("pub mod {};", m))
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            out.push(GeneratedFile {
                path: mod_path,
                content,
            });
        }
    }

    Ok(has_content)
}

/// Emit cargo integration-test glue for the generated entity tests.
///
/// Entity test generators write into `tests/<domain>/<entity>_test.rs`
/// subdirectories, but cargo only auto-discovers top-level `tests/*.rs` files
/// as integration test crates. This pass gives the tree the missing module
/// structure: a `mod.rs` in every test subdirectory declaring its `*_test.rs`
/// files, plus a top-level `tests/tests.rs` declaring every domain module, so
/// `cargo test` compiles and runs every generated test.
///
/// Like [`generate_mod_files`], this scans the filesystem after generation, so
/// files removed by the stale-cleanup pass in [`clean_generated_output`]
/// simply disappear from the declared modules on the next run.
pub(crate) fn generate_test_mod_files(output_dir: &Path) -> Result<Vec<GeneratedFile>> {
    let mut out = Vec::new();
    let tests_dir = output_dir.join("tests");
    if !tests_dir.is_dir() {
        return Ok(out);
    }

    // Only subdirectories are glued into the crate: bare `tests/*.rs` files
    // are standalone integration test crates that cargo discovers itself.
    let mut domains = BTreeSet::new();
    for entry in fs::read_dir(&tests_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let disk_name = entry.file_name().to_string_lossy().to_string();
        let mut modules = BTreeSet::new();
        collect_test_mods(&path, &mut modules, &mut out)?;
        if modules.is_empty() {
            // Nothing to declare (e.g. every test file was stale) — drop any
            // leftover mod.rs from a previous run so it cannot dangle.
            let _ = fs::remove_file(path.join("mod.rs"));
            continue;
        }
        out.push(GeneratedFile {
            path: path.join("mod.rs"),
            content: test_mod_content(&modules),
        });
        domains.insert(test_module_declaration(&disk_name, true));
    }

    if !domains.is_empty() {
        out.push(GeneratedFile {
            path: tests_dir.join("tests.rs"),
            content: test_mod_content(&domains),
        });
    }
    Ok(out)
}

/// Collect `mod` declarations for every `.rs` file under `dir`, recursing into
/// subdirectories (each of which gets its own `mod.rs`, appended to `out`).
fn collect_test_mods(
    dir: &Path,
    modules: &mut BTreeSet<String>,
    out: &mut Vec<GeneratedFile>,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let disk_name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            let mut submodules = BTreeSet::new();
            collect_test_mods(&path, &mut submodules, out)?;
            if submodules.is_empty() {
                let _ = fs::remove_file(path.join("mod.rs"));
                continue;
            }
            out.push(GeneratedFile {
                path: path.join("mod.rs"),
                content: test_mod_content(&submodules),
            });
            modules.insert(test_module_declaration(&disk_name, true));
        } else if path.extension().is_some_and(|e| e == "rs") && disk_name != "mod.rs" {
            let stem = disk_name
                .strip_suffix(".rs")
                .unwrap_or(&disk_name)
                .to_string();
            modules.insert(test_module_declaration(&stem, false));
        }
    }
    Ok(())
}

/// Render `mod` declarations as a mod file body.
pub(crate) fn test_mod_content(modules: &BTreeSet<String>) -> String {
    modules.iter().cloned().collect::<Vec<_>>().join("\n") + "\n"
}

/// Build a `mod` declaration for a test-tree entry. Returns a plain
/// `mod <name>;` when the name is already a valid Rust identifier, otherwise
/// a `#[path]`-attributed declaration using the sanitized module name so
/// names with spaces, dashes, keyword or digit collisions still compile.
pub(crate) fn test_module_declaration(disk_name: &str, is_dir: bool) -> String {
    let module = rust_module_name(disk_name);
    if module == disk_name {
        return format!("mod {};", module);
    }
    let target = if is_dir {
        disk_name.to_string()
    } else {
        format!("{}.rs", disk_name)
    };
    format!("#[path = {:?}]\nmod {};", target, module)
}

/// Sanitize an on-disk name into a valid Rust module identifier: characters
/// outside `[A-Za-z0-9_]` become underscores, digit-leading names get an
/// `n_` prefix, and reserved keywords get a `_mod` suffix.
pub(crate) fn rust_module_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    codegraph_naming::escape_module_keyword(&sanitized)
}

/// Scan `src/domain/` for `crate::entity::<module>::` references and rewrite
/// `src/entity/mod.rs` to only declare the modules that are actually used.
/// This eliminates thousands of dead-code warnings from unused SeaORM entities.
pub(crate) fn prune_entity_mod(src_dir: &Path) -> Result<Option<GeneratedFile>> {
    let entity_mod_path = src_dir.join("entity").join("mod.rs");
    if !entity_mod_path.exists() {
        return Ok(None);
    }

    let domain_dir = src_dir.join("domain");
    if !domain_dir.is_dir() {
        return Ok(None);
    }

    // Collect all entity module names referenced via `crate::entity::<name>::`
    let prefix = "crate::entity::";
    let mut used = BTreeSet::new();

    fn scan_dir(dir: &Path, prefix: &str, used: &mut BTreeSet<String>) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                scan_dir(&path, prefix, used)?;
            } else if path.extension().is_some_and(|e| e == "rs") {
                let content = fs::read_to_string(&path)?;
                for (idx, _) in content.match_indices(prefix) {
                    let rest = &content[idx + prefix.len()..];
                    let module: String = rest
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    if !module.is_empty() {
                        used.insert(module);
                    }
                }
            }
        }
        Ok(())
    }

    scan_dir(&domain_dir, prefix, &mut used)?;
    // Also scan api/ for entity references (e.g., media upload handlers).
    let api_dir = src_dir.join("api");
    if api_dir.is_dir() {
        scan_dir(&api_dir, prefix, &mut used)?;
    }

    if used.is_empty() {
        return Ok(None);
    }

    let content = used
        .iter()
        .map(|m| format!("pub mod {};", m))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    Ok(Some(GeneratedFile {
        path: entity_mod_path,
        content,
    }))
}
