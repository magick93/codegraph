//! Wrapper compile gate (issue #348): the mention-contract test
//! (`wrapper_drift_tests`) pins the `RunArgs` field set, but the wrapper's
//! `Generate`/`Classify` subcommands call `driver::generate`/`classify`
//! POSITIONALLY — only a real compile catches signature drift there (the
//! original 8-vs-7-arg incident that motivated #348).
//!
//! Ignored like the other slow compile gates (`ops_generator_tests`);
//! runs in CI's `test-ops-integration` lane:
//!
//! ```bash
//! cargo test -p codegraph --test wrapper_compile_tests -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

use codegraph::init::commands::{InitArgs, cmd_init};
use tempfile::TempDir;

/// Absolute repo root (`<repo>/crates/codegraph` → two parents up).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("codegraph crate lives at <repo>/crates/codegraph")
        .to_path_buf()
}

#[test]
#[ignore = "slow: cargo-checks the scaffolded wrapper crate"]
fn scaffolded_wrapper_compiles_against_the_local_checkout() {
    let repo = repo_root();
    let dir = TempDir::new().unwrap();
    cmd_init(&InitArgs {
        name: Some("demo-app".to_string()),
        output_dir: dir.path().to_path_buf(),
        domains: vec!["common".to_string()],
        database_target: "postgres".to_string(),
        persistence_provider: "sea_orm".to_string(),
        deployment_topology: "monolith".to_string(),
        grpc: false,
        ifml: false,
        ops: true,
        rosetta: false,
        rev: None,
        codegraph_path: Some(repo.clone()),
        force: false,
        template_dirs: vec![],
    })
    .unwrap();

    let manifest = dir.path().join("demo-app/demo-app-graph/Cargo.toml");
    assert!(manifest.is_file(), "scaffold must emit the wrapper crate");

    // A stable scratch target dir keeps local reruns warm (the scaffold
    // itself lives in an auto-deleted tempdir); CI pays the cold build
    // once per lane.
    let target = std::env::temp_dir().join("codegraph-wrapper-gate-target");
    let output = Command::new("cargo")
        .args(["check", "--manifest-path", manifest.to_str().unwrap()])
        .env("CARGO_TARGET_DIR", &target)
        .env("CARGO_INCREMENTAL", "0")
        .current_dir(&repo)
        .output()
        .expect("spawn cargo check");
    assert!(
        output.status.success(),
        "the scaffolded wrapper must compile against the linked checkout — \
         this is the acceptance criterion the mention-contract cannot pin \
         (positional generate()/classify() signatures):\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Issue #602: a fresh scaffold's first build must be warning-free —
    // every consumer sees the wrapper's diagnostics. Cargo replays cached
    // diagnostics on a warm target dir, so a warm rerun still sees them.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let wrapper_warnings: Vec<&str> = stderr
        .lines()
        .filter(|l| l.contains("warning: `demo-app-graph`") || l.contains("demo-app-graph/src/"))
        .collect();
    assert!(
        wrapper_warnings.is_empty(),
        "the scaffolded wrapper must compile with zero warnings (issue #602):\n\
         {wrapper_warnings:#?}\n--- stderr ---\n{stderr}"
    );
}
