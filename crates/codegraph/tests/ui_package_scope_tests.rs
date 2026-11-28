//! Issue #18: the generated UI's component packages come from the PROJECT,
//! not a hardcoded private `@crewbase/*` scope.
//!
//! - The generator reads the scope from profile `meta.ui_package_scope`
//!   (default: kebab-case of `app_name`), and every import uses it.
//! - `codegraph init` vendors `@{scope}/{ui,entities,common}` under
//!   `packages/`, with a `pnpm-workspace.yaml` covering `generated/ui`.
//! - Every named import the generated UI makes from `@{scope}/*` is
//!   exported by the vendored package (parity — a template that imports a
//!   name the package lacks fails here, node-free).
//!
//! Node-free, DB-free: `cmd_init` + `driver::run` over a tempdir.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use codegraph::init::commands::{DoctorArgs, InitArgs, cmd_doctor, cmd_init};
use tempfile::TempDir;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("codegraph crate lives at <repo>/crates/codegraph")
        .to_path_buf()
}

fn scaffold(dir: &Path, name: &str) -> PathBuf {
    cmd_init(&InitArgs {
        name: Some(name.to_string()),
        output_dir: dir.to_path_buf(),
        domains: vec!["common".to_string()],
        database_target: "postgres".to_string(),
        persistence_provider: "sea_orm".to_string(),
        deployment_topology: "monolith".to_string(),
        grpc: false,
        ifml: false,
        ops: true,
        rosetta: false,
        rev: Some("abc123".to_string()),
        codegraph_path: Some(repo_root()),
        force: false,
        template_dirs: vec![],
    })
    .unwrap();
    dir.join(name)
}

async fn generate(project: &Path) -> codegraph::error::Result<PathBuf> {
    let mox_files = vec![project.join("model/common.mox")];
    let output = project.join("generated");
    codegraph::driver::run(codegraph::driver::RunArgs {
        integration_manifests: &[],
        schemas: None,
        classifier: None,
        config_path: &project.join("domains.toml"),
        output: &output,
        extension_points_path: None,
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(project.join("profiles.toml")),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &mox_files,
        rosetta_files: &[],
        ddd_files: &[],
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await?;
    Ok(output)
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect()
}

fn set_scope(project: &Path, scope: &str) {
    let path = project.join("profiles.toml");
    let text = fs::read_to_string(&path).unwrap();
    let rewritten: String = text
        .lines()
        .map(|l| {
            if l.starts_with("ui_package_scope = ") {
                format!("ui_package_scope = \"{scope}\"")
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(text, rewritten, "profiles.toml must carry ui_package_scope");
    fs::write(&path, rewritten).unwrap();
}

/// `import { A, B as C, type D } from '@scope/pkg'` → (pkg, {A, B, D}).
/// Also covers `import type { … }`. Only named-brace imports exist in the
/// generated tree; anything else fails the parity pin loudly.
fn scoped_imports(source: &str, scope: &str) -> Vec<(String, BTreeSet<String>)> {
    let needle = format!("'@{scope}/");
    let mut out = Vec::new();
    for stmt in source.split(';') {
        let Some(at) = stmt.find(&needle) else {
            continue;
        };
        let Some(import_at) = stmt.rfind("import") else {
            continue;
        };
        let pkg = stmt[at + needle.len()..]
            .split('\'')
            .next()
            .unwrap_or_default()
            .to_string();
        let clause = &stmt[import_at..at];
        let (Some(open), Some(close)) = (clause.find('{'), clause.rfind('}')) else {
            panic!("non-named import from @{scope}/{pkg}: {stmt}");
        };
        let names = clause[open + 1..close]
            .split(',')
            .map(|n| n.trim().trim_start_matches("type ").trim())
            .filter(|n| !n.is_empty())
            .map(|n| n.split(" as ").next().unwrap_or(n).trim().to_string())
            .collect();
        out.push((pkg, names));
    }
    out
}

/// Names a vendored `src/index.ts` exports (`export { a, default as B }`,
/// `export type { … }`, `export interface X`, `export function f`, …).
fn index_exports(index: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for stmt in index.split(';') {
        let stmt = stmt.trim();
        let Some(open) = stmt.find("export ") else {
            continue;
        };
        let rest = &stmt[open..];
        // Only re-export lists (`export { … } from` / `export type { … }`),
        // never an `export interface X { … }` body.
        let list = rest
            .strip_prefix("export {")
            .or_else(|| rest.strip_prefix("export type {"));
        if let Some(list) = list
            && let Some(close) = list.find('}')
        {
            for n in list[..close].split(',') {
                let n = n.trim();
                if n.is_empty() {
                    continue;
                }
                let exported = n.rsplit(" as ").next().unwrap_or(n).trim();
                names.insert(exported.trim_start_matches("type ").to_string());
            }
        }
    }
    for line in index.lines() {
        for kw in [
            "export interface ",
            "export type ",
            "export function ",
            "export class ",
        ] {
            if let Some(rest) = line.trim().strip_prefix(kw) {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    names.insert(name);
                }
            }
        }
    }
    names
}

#[test]
fn init_vendors_the_scoped_ui_packages_into_a_pnpm_workspace() {
    let dir = TempDir::new().unwrap();
    let project = scaffold(dir.path(), "demo-app");

    let workspace = fs::read_to_string(project.join("pnpm-workspace.yaml")).unwrap();
    assert!(workspace.contains("\"packages/*\""), "{workspace}");
    assert!(workspace.contains("\"generated/ui\""), "{workspace}");

    let profiles = fs::read_to_string(project.join("profiles.toml")).unwrap();
    assert!(
        profiles.contains("ui_package_scope = \"demo-app\""),
        "profiles.toml meta must name the vendored scope:\n{profiles}"
    );

    for pkg in ["ui", "entities", "common"] {
        let manifest = project.join("packages").join(pkg).join("package.json");
        let json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&manifest).unwrap()).unwrap();
        assert_eq!(
            json["name"],
            format!("@demo-app/{pkg}"),
            "{}",
            manifest.display()
        );
        assert!(
            project
                .join("packages")
                .join(pkg)
                .join("src/index.ts")
                .exists()
        );
    }
    for file in files_under(&project.join("packages")) {
        let text = fs::read_to_string(&file).unwrap();
        assert!(
            !text.contains("crewbase"),
            "{} names crewbase",
            file.display()
        );
        assert!(
            !text.contains("{%") && !text.contains("{{ "),
            "{} leaked Tera syntax",
            file.display()
        );
    }
}

#[tokio::test]
async fn generated_ui_imports_only_what_the_vendored_packages_export() {
    let dir = TempDir::new().unwrap();
    let project = scaffold(dir.path(), "demo-app");
    let output = generate(&project).await.unwrap();
    let ui = output.join("ui");

    let pkg: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(ui.join("package.json")).unwrap()).unwrap();
    for dep in ["ui", "entities", "common"] {
        assert_eq!(
            pkg["dependencies"][format!("@demo-app/{dep}")],
            "workspace:*",
            "generated package.json must depend on @demo-app/{dep}"
        );
    }

    let mut imported: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for file in files_under(&ui) {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        assert!(
            !text.contains("@crewbase"),
            "{} still hardcodes @crewbase (issue #18)",
            file.display()
        );
        assert!(
            !text.contains("/brand/crewbase"),
            "{} references an unemitted crewbase brand asset",
            file.display()
        );
        for (pkg, names) in scoped_imports(&text, "demo-app") {
            imported.entry(pkg).or_default().extend(names);
        }
    }
    assert!(
        imported.get("ui").is_some_and(|n| n.contains("LoginPage")),
        "the parity pin must see the auth pages' imports: {imported:?}"
    );
    assert!(
        imported
            .get("entities")
            .is_some_and(|n| n.contains("EntityDescriptor")),
        "the parity pin must see the descriptor imports: {imported:?}"
    );

    for (pkg, names) in &imported {
        let index = project.join("packages").join(pkg).join("src/index.ts");
        let exports = index_exports(&fs::read_to_string(&index).unwrap_or_else(|e| {
            panic!(
                "generated UI imports @demo-app/{pkg} but {}: {e}",
                index.display()
            )
        }));
        let missing: Vec<&String> = names.iter().filter(|n| !exports.contains(*n)).collect();
        assert!(
            missing.is_empty(),
            "@demo-app/{pkg} does not export {missing:?} (exports: {exports:?})"
        );
    }
}

#[tokio::test]
async fn explicit_scope_overrides_the_app_name_default() {
    let dir = TempDir::new().unwrap();
    let project = scaffold(dir.path(), "demo-app");
    set_scope(&project, "crewbase");
    let output = generate(&project).await.unwrap();

    let pkg = fs::read_to_string(output.join("ui/package.json")).unwrap();
    assert!(pkg.contains("\"@crewbase/ui\": \"workspace:*\""), "{pkg}");
    assert!(!pkg.contains("@demo-app/"), "{pkg}");
}

#[tokio::test]
async fn invalid_scope_is_a_hard_config_error() {
    let dir = TempDir::new().unwrap();
    let project = scaffold(dir.path(), "demo-app");
    set_scope(&project, "@Bad Scope");
    let err = generate(&project)
        .await
        .expect_err("an invalid scope must refuse");
    let msg = err.to_string();
    assert!(
        msg.contains("ui_package_scope") && msg.contains("@Bad Scope"),
        "{msg}"
    );
    assert!(
        !project.join("generated/ui/package.json").exists(),
        "the refusal must precede any written tree"
    );
}

#[test]
fn doctor_warns_when_the_scope_names_packages_the_workspace_lacks() {
    let dir = TempDir::new().unwrap();
    let project = scaffold(dir.path(), "demo-app");
    let args = |project: &Path| DoctorArgs {
        config: project.join("domains.toml"),
        schemas: None,
        classifier: None,
        profiles_config: Some(project.join("profiles.toml")),
        mox_files: vec![project.join("model/common.mox")],
        rosetta_files: vec![],
        ddd_files: vec![],
        evt_files: vec![],
        deploy_files: vec![],
        ifml_files: vec![],
        extension_points: None,
        fail_on_findings: false,
        graph_artifact: None,
        emit_graph: None,
        graph_reachability: vec![],
    };
    let doctor = |project: &Path| cmd_doctor(&args(project)).unwrap();
    let baseline = doctor(&project);
    assert_eq!(baseline.hard_failures, 0);

    set_scope(&project, "other");
    let drifted = doctor(&project);
    assert_eq!(drifted.hard_failures, 0, "a missing package is advisory");
    assert_eq!(
        drifted.soft_warnings,
        baseline.soft_warnings + 1,
        "one WARN names every missing/misnamed package"
    );

    set_scope(&project, "Bad@");
    let err = cmd_doctor(&args(&project)).expect_err("an invalid scope fails doctor");
    assert!(err.to_string().contains("1 hard check(s) failed"), "{err}");
}

#[test]
fn import_and_export_scanners_read_the_shapes_the_tree_uses() {
    let imports = scoped_imports(
        "import { A, type B, C as D } from '@s/ui';\nimport type { E } from '@s/entities';",
        "s",
    );
    assert_eq!(imports[0].0, "ui");
    assert_eq!(
        imports[0].1,
        ["A", "B", "C"].iter().map(|s| s.to_string()).collect()
    );
    assert_eq!(imports[1].0, "entities");
    let exports = index_exports(
        "export { default as X } from './X.svelte';\nexport type { Y, Z } from './t';\nexport interface W {}",
    );
    assert_eq!(
        exports,
        ["W", "X", "Y", "Z"].iter().map(|s| s.to_string()).collect()
    );
}

/// Issue #860: no UI template hardcodes a PRIVATE package scope — every
/// generated import routes through `@{{ project.ui_package_scope }}`. The
/// emdash templates are excluded on purpose: they build a plugin FOR the
/// community-os host (`@community-os/sdk`, `@community-os/emdash-common` are
/// that host's published contract, not codegraph's vendored packages).
#[test]
fn no_ui_template_hardcodes_a_private_package_scope() {
    let templates = repo_root().join("crates/codegraph-generate/templates");
    let mut offenders = Vec::new();
    for file in files_under(&templates) {
        let rel = file
            .strip_prefix(&templates)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        if rel.starts_with("emdash/") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        for scope in ["@crewbase/", "@community-os/"] {
            if text.contains(scope) {
                offenders.push(format!("{rel}: {scope}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "private scopes in templates: {offenders:?}"
    );
}

/// Issue #860: the `ui-form` generator imports only what the vendored
/// `@{scope}/ui` index exports — the #18 parity rule, held at the template
/// level because no shipped profile runs `ui-form` (the init+generate pin
/// above cannot see it).
#[test]
fn ui_form_imports_only_what_the_vendored_ui_package_exports() {
    let templates = repo_root().join("crates/codegraph-generate/templates");
    let form = fs::read_to_string(templates.join("ui/cosmos_entity_form.tera")).unwrap();
    let imports = scoped_imports(&form, "{{ project.ui_package_scope }}");
    assert_eq!(imports.len(), 1, "one scoped import: {imports:?}");
    let (pkg, names) = &imports[0];
    assert_eq!(pkg, "ui");
    let exports = index_exports(
        &fs::read_to_string(templates.join("project/packages/ui/index_ts.tera")).unwrap(),
    );
    let missing: Vec<&String> = names.iter().filter(|n| !exports.contains(*n)).collect();
    assert!(
        missing.is_empty(),
        "@{{scope}}/ui does not export {missing:?}"
    );
    for name in ["Button", "Input", "Select"] {
        assert!(names.contains(name), "{names:?}");
    }
}
