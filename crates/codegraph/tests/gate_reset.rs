//! The #694 reset shared by the live gates that keep a persistent output
//! directory.
//!
//! The rule (AGENTS.md → Testing): *a gate that keeps a persistent output
//! directory must, at run start, sweep the files the previous generation
//! produced (keep the build cache) or key discovery on content — never
//! `--test-threads=1`.*
//!
//! Every gate here keeps its generated app under `target/<gate>/app` and its
//! cargo cache BESIDE it (`target/<gate>/target`) or entirely outside it
//! (`/tmp/opencode/ws695-target`). That split is the whole point: the SOURCE is
//! regenerated from clean while the cache survives, and wiping only the source
//! tree gives both by construction — no stash-rename, no allow-list of files to
//! keep.
//!
//! Why it matters (#687, #688): discovery in these gates is suffix/prefix
//! based — a service-op contract is found by `*_actions.hurl`, a Playwright
//! page class by `*-page.ts`, a spec by the `testDir` glob. An artifact an
//! OLDER generation emitted therefore sits in exactly the place this run
//! looks, and is read as this run's signal (the #688 false red: a leftover
//! `11_…_actions.hurl` hurl'd against the `12_…` app). Sweeping the source
//! before regenerating is what makes those content-blind discoveries sound; it
//! is not optional hygiene.
//!
//! Included by path from `service_ops_hurl_gate`, `ddd_invariants_live_gate`
//! and `media_runtime_tests` (the `review_fixture_gate` precedent), so the pins
//! below run in every binary that includes it — in PR CI, with no Postgres,
//! node or hurl required.

use std::fs;
use std::io;
use std::path::Path;

/// Delete the generated SOURCE tree a previous run left at `dir`, then
/// recreate it empty.
///
/// Call this immediately before regenerating, with the gate's cargo
/// `CARGO_TARGET_DIR`, logs and any install cache living OUTSIDE `dir`. An
/// absent `dir` (first run, or a cache pruned by the harness `clean` command)
/// is not an error — it is simply created.
pub fn reset_generated_source(dir: &Path) -> io::Result<()> {
    match fs::remove_dir_all(dir) {
        Ok(()) => {}
        // First run / pruned cache: nothing to sweep.
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    fs::create_dir_all(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Plant a file the way a generation writes it (parents created).
    fn plant(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    /// The gate-shaped layout: the generated source in `app/`, everything the
    /// rule says must survive beside it.
    fn gate_layout() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        fs::create_dir_all(&app).unwrap();
        (tmp, app)
    }

    /// The reset is the from-clean half of the rule: every file a previous
    /// generation produced is gone, including the artifact classes the gates
    /// discover content-blindly.
    ///
    /// TEETH: making [`reset_generated_source`] a no-op (an
    /// `fs::create_dir_all` only) fails this test on the first stale file.
    #[test]
    fn a_stale_generation_does_not_survive_the_reset() {
        let (_tmp, app) = gate_layout();
        plant(&app, "Cargo.toml", "[workspace]\n");
        plant(&app, "src/old_module.rs", "// an entity deleted since\n");
        plant(&app, "src/domain/payroll/mod.rs", "// current\n");
        plant(
            &app,
            "hurl/11_payroll_pay_run_actions.hurl",
            "# an older generation's numbering\n",
        );
        plant(
            &app,
            "tests/pages/removed-view-page.ts",
            "// a removed view\n",
        );

        reset_generated_source(&app).expect("reset");

        assert!(app.is_dir(), "the source tree is recreated empty");
        let left: Vec<String> = fs::read_dir(&app)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            left.is_empty(),
            "a stale generated file survived the reset: {left:?}"
        );
        for stale in [
            "Cargo.toml",
            "src/old_module.rs",
            "hurl/11_payroll_pay_run_actions.hurl",
            "tests/pages/removed-view-page.ts",
        ] {
            assert!(
                !app.join(stale).exists(),
                "stale artifact {stale} is still discoverable by this run"
            );
        }
    }

    /// The cache half of the rule: the cargo target dir, the logs and any
    /// install cache live BESIDE the source tree and must survive the reset —
    /// a gate that also deleted them would recompile from cold on every run.
    #[test]
    fn the_build_cache_beside_the_source_tree_survives_the_reset() {
        let (tmp, app) = gate_layout();
        let root = tmp.path();
        plant(&app, "src/stale.rs", "// stale\n");
        plant(root, "target/debug/marker", "compiled");
        plant(root, "logs/run.log", "last run");
        plant(root, "svelte/node_modules/marker", "installed");

        reset_generated_source(&app).expect("reset");

        assert!(
            root.join("target/debug/marker").is_file(),
            "the cargo cache must survive the source reset"
        );
        assert!(root.join("logs/run.log").is_file(), "logs survive");
        assert!(
            root.join("svelte/node_modules/marker").is_file(),
            "an install cache outside the source tree survives"
        );
        assert!(
            !app.join("src").exists(),
            "the stale generated source is gone"
        );
    }

    /// First run (or a pruned cache): the gate must not fail on an absent
    /// tree, and must leave a directory for the generator to write into.
    #[test]
    fn the_reset_creates_an_absent_source_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        assert!(!app.exists());

        reset_generated_source(&app).expect("reset of an absent tree");

        assert!(app.is_dir());
        assert_eq!(fs::read_dir(&app).unwrap().count(), 0);
    }

    /// The #688 shape, end to end at the discovery seam: the gates find a
    /// service-op contract by suffix, so a stale numbered contract is found
    /// until the reset removes it, and only the current generation's contract
    /// is found afterwards.
    #[test]
    fn suffix_discovery_after_the_reset_sees_only_the_current_generation() {
        /// `service_ops_hurl_gate::actions_path`'s rule.
        fn discover_actions(dir: &Path) -> Option<PathBuf> {
            fs::read_dir(dir)
                .ok()?
                .flatten()
                .map(|e| e.path())
                .find(|p| {
                    p.extension().is_some_and(|x| x == "hurl")
                        && p.file_name()
                            .is_some_and(|n| n.to_string_lossy().ends_with("_pay_run_actions.hurl"))
                })
        }

        let (_tmp, app) = gate_layout();
        let hurl = app.join("hurl");
        fs::create_dir_all(&hurl).unwrap();
        // The previous generation numbered the contract 11; this one emits 12.
        fs::write(hurl.join("11_payroll_pay_run_actions.hurl"), "# stale\n").unwrap();
        assert_eq!(
            discover_actions(&hurl),
            Some(hurl.join("11_payroll_pay_run_actions.hurl")),
            "a stale contract is discovered by its suffix — the hazard the \
             reset exists to remove"
        );

        reset_generated_source(&app).expect("reset");
        fs::create_dir_all(&hurl).unwrap();
        fs::write(hurl.join("12_payroll_pay_run_actions.hurl"), "# current\n").unwrap();

        assert_eq!(
            discover_actions(&hurl),
            Some(hurl.join("12_payroll_pay_run_actions.hurl")),
            "after the reset, suffix discovery can only see this run's contract"
        );
    }
}
