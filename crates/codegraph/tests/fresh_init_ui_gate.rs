//! The stub-free fresh-init UI lane (issue #858).
//!
//! `entity_codegen_gate` typechecks the generated UI against hand-written
//! stubs of the shadcn primitives and the consumer tsconfig, so it cannot see
//! a fresh project that fails to build because of a primitive nobody installs
//! or a tsconfig nobody emits. This lane does what a new user does:
//!
//! 1. `codegraph init` (path deps on this checkout),
//! 2. `driver::run` with the default profile,
//! 3. `pnpm install` at the project root (the workspace carries
//!    `packages/*` + `generated/ui`),
//! 4. the `PRIMITIVES.md` install command — the shadcn-svelte CLI against the
//!    emitted `components.json`, with exactly [`SHADCN_PRIMITIVES`],
//! 5. `vite build`.
//!
//! The lane gates on the BUILD, not on `svelte-check`: a fresh tree still
//! carries type-only findings tracked elsewhere (the kit-3 `$env` move, G1 in
//! `docs/tdd/entity-kit-sweep.tdd.md`), which `entity_codegen_gate`'s
//! shrink-only ledger owns.
//!
//! No stub is written anywhere. Ignored (network + node): the nightly
//! `entity-gate.yml` workflow runs it.
//!
//! ```bash
//! cargo test -p codegraph --test fresh_init_ui_gate -- --ignored --nocapture
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use codegraph::init::commands::{InitArgs, cmd_init};
use codegraph_generate::ui::scaffold::SHADCN_PRIMITIVES;

/// The shadcn-svelte CLI the lane installs with — the same `@next` spelling
/// `PRIMITIVES.md` documents, so the lane runs the command users run.
const SHADCN_CLI: &str = "shadcn-svelte@next";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("codegraph crate lives at <repo>/crates/codegraph")
        .to_path_buf()
}

fn gate_dir() -> PathBuf {
    repo_root().join("target").join("fresh-init-ui-gate")
}

fn run(dir: &Path, program: &str, args: &[&str]) {
    eprintln!(
        "[fresh-init-ui] {} $ {program} {}",
        dir.display(),
        args.join(" ")
    );
    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
        .env("CI", "1")
        .status()
        .unwrap_or_else(|e| panic!("spawn {program}: {e}"));
    assert!(
        status.success(),
        "`{program} {}` failed in {} ({status})",
        args.join(" "),
        dir.display()
    );
}

#[tokio::test]
#[ignore = "network + node: the nightly entity-gate lane (issue #858)"]
async fn fresh_init_ui_installs_and_builds_without_stubs() {
    let dir = gate_dir();
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("wipe the previous run");
    }
    fs::create_dir_all(&dir).expect("gate dir");

    cmd_init(&InitArgs {
        name: Some("fresh-app".to_string()),
        output_dir: dir.clone(),
        domains: vec!["common".to_string()],
        database_target: "postgres".to_string(),
        persistence_provider: "sea_orm".to_string(),
        deployment_topology: "monolith".to_string(),
        grpc: false,
        ifml: false,
        ops: true,
        rosetta: false,
        rev: None,
        codegraph_path: Some(repo_root().canonicalize().expect("repo root")),
        force: false,
        template_dirs: vec![],
    })
    .expect("codegraph init");
    let project = dir.join("fresh-app");

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
    .await
    .expect("generate");

    let ui = output.join("ui");
    for required in [
        "package.json",
        "tsconfig.json",
        "components.json",
        "PRIMITIVES.md",
    ] {
        assert!(
            ui.join(required).is_file(),
            "a fresh generate must emit ui/{required}"
        );
    }
    // The documented install command names exactly the pinned set.
    let docs = fs::read_to_string(ui.join("PRIMITIVES.md")).expect("PRIMITIVES.md");
    let documented = format!("add {}", SHADCN_PRIMITIVES.join(" "));
    assert!(
        docs.contains(&documented),
        "PRIMITIVES.md must document `{documented}`"
    );

    run(&project, "pnpm", &["install", "--no-frozen-lockfile"]);
    let mut add = vec!["--yes", SHADCN_CLI, "add", "--yes", "--overwrite"];
    add.extend(SHADCN_PRIMITIVES.iter().copied());
    run(&ui, "npx", &add);
    // The CLI may add runtime deps (bits-ui, …) to package.json.
    run(&project, "pnpm", &["install", "--no-frozen-lockfile"]);
    run(&ui, "pnpm", &["exec", "svelte-kit", "sync"]);
    run(&ui, "pnpm", &["exec", "vite", "build"]);
}
