//! Wrapper drift detection (issue #348): the init-rendered wrapper
//! (`{name}-graph/src/main.rs`) must never lag the driver's `RunArgs`
//! surface — a scaffold that omits a field the driver gained does not
//! compile against the codegraph revision that produced it.
//!
//! Layers pinned here:
//! 1. contract test — every `RunArgs` field must appear in the rendered
//!    wrapper's `RunArgs` construction (template ↔ driver binding);
//! 2. doctor drift — existing wrappers warn per missing field, naming its
//!    CLI flag, and per hardcoded `&[]`/`None` value (no CLI flag);
//! 3. template rev — the scaffold stamps the template content hash and
//!    doctor flags a mismatch with a re-render hint.

use std::fs;
use std::path::{Path, PathBuf};

use codegraph::driver::run_args_field_names;
use codegraph::init::commands::{DoctorArgs, InitArgs, cmd_doctor, cmd_init};
use codegraph::init::wrapper_drift::{check_wrapper_drift, wrapper_runargs_drift};
use tempfile::TempDir;

fn init_args(dir: &Path, name: &str, codegraph_path: Option<PathBuf>, force: bool) -> InitArgs {
    InitArgs {
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
        codegraph_path,
        force,
        template_dirs: vec![],
    }
}

/// Scaffold a demo-app wrapper and return its rendered `main.rs`.
fn scaffold_wrapper_main(dir: &TempDir) -> String {
    cmd_init(&init_args(dir.path(), "demo-app", None, false)).unwrap();
    fs::read_to_string(dir.path().join("demo-app/demo-app-graph/src/main.rs")).unwrap()
}

/// Extract the `codegraph::driver::RunArgs { ... }` construction block
/// (brace-balanced) from a wrapper main.rs.
fn extract_run_args_block(main_rs: &str) -> Option<String> {
    let start = main_rs.find("codegraph::driver::RunArgs {")?;
    let brace = main_rs[start..].find('{')? + start;
    let mut depth = 0usize;
    for (i, ch) in main_rs[brace..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(main_rs[brace..=brace + i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// A field counts as constructed when its line is the full form
/// (`field: expr`) or Rust field-init shorthand (`field,`) — both compile
/// against the struct. The colon terminates the name, so `check` never
/// matches `check_something:`.
fn wrapper_sets_field(block: &str, field: &str) -> bool {
    let full = format!("{field}:");
    let shorthand = format!("{field},");
    block.lines().any(|line| {
        let line = line.trim();
        line.starts_with(&full) || line == shorthand
    })
}

/// The contract (issue #348): every `RunArgs` field of the linked driver
/// must be constructed by the rendered wrapper — a scaffolded wrapper that
/// omits a field cannot compile against the binary that produced it.
#[test]
fn rendered_wrapper_runargs_covers_every_driver_field() {
    let dir = TempDir::new().unwrap();
    let main = scaffold_wrapper_main(&dir);
    let block = extract_run_args_block(&main)
        .expect("wrapper main.rs must construct codegraph::driver::RunArgs");
    let missing: Vec<String> = run_args_field_names()
        .into_iter()
        .filter(|field| !wrapper_sets_field(&block, field))
        .collect();
    assert!(
        missing.is_empty(),
        "wrapper_main.tera's RunArgs construction is missing fields — \
         the scaffold would NOT compile against this driver: {missing:?}. \
         Add each field with its CLI flag to project/wrapper_main.tera \
         (re-rendering is the mechanical fix for consumers)."
    );
}

fn doctor_args(project: &Path) -> DoctorArgs {
    DoctorArgs {
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
    }
}

/// The doctor hint must name the missing flag per drifted field so
/// re-rendering is mechanical (issue #348 comment). The synthetic source
/// here is the exact shape of a pre-#348 wrapper: the ddd/evt/deploy
/// fields absent, the ifml trio hardcoded.
#[test]
fn wrapper_runargs_drift_names_missing_and_hardcoded_fields_with_flags() {
    let dir = TempDir::new().unwrap();
    let main = scaffold_wrapper_main(&dir);
    let stale = main
        .replace("ddd_files: &ddd_files,\n", "")
        .replace("evt_files: &evt_files,\n", "")
        .replace("deploy_files: &deploy_files,\n", "")
        .replace("ifml_framework: &ifml_framework,", "ifml_framework: &[],")
        .replace(
            "ifml_components: ifml_components.as_deref(),",
            "ifml_components: None,",
        )
        .replace(
            "ifml_design_system: ifml_design_system.as_deref(),",
            "ifml_design_system: None,",
        );
    assert_ne!(stale, main, "mangling must change the wrapper source");

    let drift = wrapper_runargs_drift("demo-app-graph", &stale);
    let missing: Vec<String> = drift
        .missing
        .iter()
        .map(|f| format!("{} ({})", f.field, f.flag))
        .collect();
    assert_eq!(
        missing,
        vec![
            "ddd_files (--ddd-files)",
            "evt_files (--evt-files)",
            "deploy_files (--deploy-files)"
        ]
    );
    let hardcoded: Vec<String> = drift
        .hardcoded
        .iter()
        .map(|f| format!("{} ({})", f.field, f.flag))
        .collect();
    assert_eq!(
        hardcoded,
        vec![
            "ifml_framework (--ifml-framework)",
            "ifml_components (--ifml-components)",
            "ifml_design_system (--ifml-design-system)"
        ]
    );

    // The fresh wrapper is clean on both axes.
    let clean = wrapper_runargs_drift("demo-app-graph", &main);
    assert!(clean.missing.is_empty(), "{:?}", clean.missing);
    assert!(clean.hardcoded.is_empty(), "{:?}", clean.hardcoded);
}

/// A wrapper with no RunArgs construction at all (pre-driver layout) is
/// maximally stale: every field counts as missing.
#[test]
fn wrapper_without_runargs_construction_is_fully_stale() {
    let drift = wrapper_runargs_drift("demo-app-graph", "fn main() {}");
    assert_eq!(drift.missing.len(), run_args_field_names().len());
}

/// The scan finds `*-graph/src/main.rs` one level under the project root
/// and stays quiet when there is no wrapper (non-scaffold projects).
#[test]
fn check_wrapper_drift_scans_project_root_for_graph_crates() {
    let dir = TempDir::new().unwrap();
    let project = dir.path().join("demo-app");
    cmd_init(&init_args(dir.path(), "demo-app", None, false)).unwrap();

    let drifts = check_wrapper_drift(&project);
    assert_eq!(drifts.len(), 1, "{drifts:?}");
    assert_eq!(drifts[0].wrapper, "demo-app-graph");
    assert!(
        drifts[0].missing.is_empty() && drifts[0].hardcoded.is_empty(),
        "fresh scaffold must be drift-free: {drifts:?}"
    );

    // A directory without any wrapper yields nothing to warn about.
    let empty = TempDir::new().unwrap();
    assert!(check_wrapper_drift(empty.path()).is_empty());
}

/// The scaffold stamps the template content hash so doctor can flag a
/// wrapper rendered by different templates (issue #348 recommended
/// approach: single source of truth for template drift).
#[test]
fn wrapper_stamps_current_template_rev() {
    let dir = TempDir::new().unwrap();
    let main = scaffold_wrapper_main(&dir);
    let current = codegraph::generate::template_engine::project_template_rev();
    assert!(
        !current.is_empty(),
        "template rev must be embedded in the binary"
    );
    assert!(
        main.contains(&format!("CODEGRAPH_TEMPLATE_REV: &str = \"{current}\"")),
        "wrapper must stamp the live template rev {current}:\n{}",
        main
    );
}

/// A wrapper rendered by older templates (stamped rev ≠ this binary's, or
/// no stamp at all — pre-#348 wrappers) warns with a re-render hint.
#[test]
fn doctor_warns_on_template_rev_mismatch() {
    let dir = TempDir::new().unwrap();
    let project = dir.path().join("demo-app");
    cmd_init(&init_args(dir.path(), "demo-app", None, false)).unwrap();
    let fresh = cmd_doctor(&doctor_args(&project)).unwrap();

    let main_path = project.join("demo-app-graph/src/main.rs");
    let main = fs::read_to_string(&main_path).unwrap();
    let stale = main.replace(
        &format!(
            "CODEGRAPH_TEMPLATE_REV: &str = \"{}\"",
            codegraph::generate::template_engine::project_template_rev()
        ),
        "CODEGRAPH_TEMPLATE_REV: &str = \"0000000000000000\"",
    );
    assert_ne!(stale, main, "mangling must change the stamped rev");
    fs::write(&main_path, stale).unwrap();

    let stale_summary = cmd_doctor(&doctor_args(&project)).unwrap();
    assert_eq!(
        stale_summary.soft_warnings,
        fresh.soft_warnings + 1,
        "a stale template rev must add exactly one soft warning \
         (fresh {}, stale {})",
        fresh.soft_warnings,
        stale_summary.soft_warnings
    );
}

/// Doctor flags a stale wrapper: mangling the scaffold into the pre-#348
/// shape adds exactly one soft warning over the fresh-scaffold baseline.
#[test]
fn doctor_warns_on_stale_wrapper() {
    let dir = TempDir::new().unwrap();
    let project = dir.path().join("demo-app");
    cmd_init(&init_args(dir.path(), "demo-app", None, false)).unwrap();

    let fresh = cmd_doctor(&doctor_args(&project)).unwrap();
    assert_eq!(
        fresh.hard_failures, 0,
        "fresh scaffold must have no hard failures"
    );

    let main_path = project.join("demo-app-graph/src/main.rs");
    let main = fs::read_to_string(&main_path).unwrap();
    let stale = main
        .replace("ddd_files: &ddd_files,\n", "")
        .replace("evt_files: &evt_files,\n", "")
        .replace("deploy_files: &deploy_files,\n", "")
        .replace("ifml_framework: &ifml_framework,", "ifml_framework: &[],")
        .replace(
            "ifml_components: ifml_components.as_deref(),",
            "ifml_components: None,",
        )
        .replace(
            "ifml_design_system: ifml_design_system.as_deref(),",
            "ifml_design_system: None,",
        );
    fs::write(&main_path, stale).unwrap();

    let stale_summary = cmd_doctor(&doctor_args(&project)).unwrap();
    assert_eq!(
        stale_summary.soft_warnings,
        fresh.soft_warnings + 1,
        "one drifted wrapper must add exactly one soft warning \
         (fresh {}, stale {})",
        fresh.soft_warnings,
        stale_summary.soft_warnings
    );
}
