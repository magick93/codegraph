//! Pinned Supabase CLI spec (issue #743): every `npx supabase …` invocation
//! in the ops harness routes through ONE version constant — bare `npx
//! supabase` resolves npm `latest`, and an unpinned CLI can reject consumer
//! config keys the day npm ships a breaking release (the #729 failure
//! class, one lane over).
//!
//! Teeth (node-free): these tests read the harness's own source; a call
//! site that regresses to a bare `&["supabase", …]` argument sequence — or
//! a gate that redeclares the version literal instead of importing the
//! shared constant — fails here, naming the file.
//!
//! ```text
//! cargo test -p codegraph-ops --test supabase_pin_tests
//! ```

use std::path::{Path, PathBuf};

fn crate_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn suites_sources() -> Vec<PathBuf> {
    let dir = crate_root().join("src").join("suites");
    let mut files = Vec::new();
    let mut stack = vec![dir];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("suites dir must exist") {
            let entry = entry.expect("readable");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// No bare `&["supabase", …]` argument sequences under `src/suites/`:
/// every Supabase CLI spawn must route through
/// `supabase::npx_supabase_args` so the pinned spec cannot re-drift
/// (issue #743).
#[test]
fn no_bare_supabase_argument_sequences_remain_in_the_suites() {
    let mut offenders = Vec::new();
    for file in suites_sources() {
        let text = std::fs::read_to_string(&file).expect("suites source readable");
        if text.contains("&[\"supabase\"") {
            offenders.push(file.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "bare `&[\"supabase\", …]` argument sequences found (issue #743: every \
         Supabase CLI spawn must route through supabase::npx_supabase_args so \
         the pinned spec cannot re-drift): {offenders:?}"
    );
}

/// The known Supabase spawn sites in `suites/e2e.rs` reference the pinned
/// args helper — a new spawn site added without the helper fails here.
#[test]
fn supabase_spawn_sites_route_through_the_pinned_args_helper() {
    let e2e = std::fs::read_to_string(crate_root().join("src/suites/e2e.rs"))
        .expect("suites/e2e.rs must exist");
    let uses = e2e.matches("npx_supabase_args").count();
    assert!(
        uses >= 2,
        "suites/e2e.rs must route its Supabase CLI spawns through \
         supabase::npx_supabase_args (issue #743); found {uses} references — \
         `supabase start` and `supabase db reset` are the two known sites"
    );
}

/// The workers live gate's CLI version is the SHARED constant — no local
/// redeclaration in `crates/codegraph/tests/workers_live_gate.rs` (issue
/// #743: one constant, or the harness/gate pairing drifts the way #729
/// saw on the workflow side).
#[test]
fn the_workers_live_gate_imports_the_shared_cli_constant() {
    let gate = crate_root()
        .parent()
        .map(|crates| crates.join("codegraph/tests/workers_live_gate.rs"))
        .expect("workspace layout");
    let text = std::fs::read_to_string(&gate)
        .unwrap_or_else(|e| panic!("{} must exist: {e}", gate.display()));
    assert!(
        !text.contains("pub const SUPABASE_CLI_VERSION"),
        "workers_live_gate.rs must import codegraph_ops::supabase::\
         SUPABASE_CLI_VERSION instead of redeclaring the literal (issue \
         #743: one constant — a second copy is the drift #729 killed)"
    );
    assert!(
        text.contains("codegraph_ops::supabase::SUPABASE_CLI_VERSION"),
        "workers_live_gate.rs must reference the shared \
         codegraph_ops::supabase::SUPABASE_CLI_VERSION constant"
    );
}
