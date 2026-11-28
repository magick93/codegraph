//! CI apt resilience contract (the IFML nightly flake, run 45993292506).
//!
//! `apt-get update` is all-or-nothing: ONE third-party source answering 403
//! (`packages.microsoft.com`, preinstalled on the GitHub runner image) fails
//! the whole update, so a gate died in its `Install psql` step on a SHA that
//! had passed earlier. Nothing the gates install comes from a third-party
//! source (`postgresql-client`, the hurl `.deb`'s dependencies, Playwright's
//! system deps are all in the Ubuntu archive), so every apt use goes through
//! `scripts/ci/apt-install.sh`, which disables the third-party sources before
//! updating, retries the distro archive, and skips the install when the tool
//! is already on the image. Because the sources stay disabled for the rest of
//! the job, a later `playwright install --with-deps` (which runs its own
//! `apt-get update`) is covered too — provided it runs AFTER the script.
//!
//! ```text
//! cargo test -p codegraph --test ci_apt_contract
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

const SCRIPT: &str = "scripts/ci/apt-install.sh";

fn workflows() -> Vec<(PathBuf, String)> {
    let dir = workspace_root().join(".github/workflows");
    let mut out: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "yml" || x == "yaml"))
        .map(|p| {
            let text = std::fs::read_to_string(&p).unwrap();
            (p, text)
        })
        .collect();
    out.sort();
    out
}

/// Non-comment lines (a `#` comment may name apt-get to explain itself).
fn code_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim_start().starts_with('#'))
}

#[test]
fn no_workflow_calls_apt_get_directly() {
    let mut offenders = Vec::new();
    for (path, text) in workflows() {
        for (n, line) in code_lines(&text) {
            if line.contains("apt-get") {
                offenders.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "apt must go through {SCRIPT} (a third-party source 403 fails a bare \
         `apt-get update`):\n{}",
        offenders.join("\n")
    );
}

#[test]
fn playwright_with_deps_runs_after_the_apt_script() {
    let mut offenders = Vec::new();
    for (path, text) in workflows() {
        let mut script_seen = false;
        for (n, line) in code_lines(&text) {
            if line.contains(SCRIPT) {
                script_seen = true;
            }
            if line.contains("--with-deps") && !script_seen {
                offenders.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "`playwright install --with-deps` runs its own apt-get update; it must \
         follow {SCRIPT} so the third-party sources are already disabled:\n{}",
        offenders.join("\n")
    );
}

/// The script's source handling, run for real against a scratch sources dir
/// (`CI_APT_SOURCES_DIR`) with apt itself stubbed (`CI_APT_DRY_RUN=1`).
#[test]
fn the_script_disables_third_party_sources_and_keeps_the_distro_archive() {
    let script = workspace_root().join(SCRIPT);
    let tmp = tempfile::tempdir().unwrap();
    let sources = tmp.path();
    for f in [
        "ubuntu.sources",
        "microsoft-prod.list",
        "github_git-lfs.list",
        "azure-cli.sources",
    ] {
        std::fs::write(sources.join(f), "deb http://example.invalid stable main\n").unwrap();
    }
    let out = Command::new("bash")
        .arg(&script)
        .arg("postgresql-client")
        .env("CI_APT_SOURCES_DIR", sources)
        .env("CI_APT_DRY_RUN", "1")
        .output()
        .expect("run the script");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        sources.join("ubuntu.sources").exists(),
        "the distro archive stays"
    );
    for f in [
        "microsoft-prod.list",
        "github_git-lfs.list",
        "azure-cli.sources",
    ] {
        assert!(!sources.join(f).exists(), "{f} must be disabled");
        assert!(
            sources.join(format!("{f}.disabled-by-ci")).exists(),
            "{f} is renamed, not deleted"
        );
    }
    assert!(stdout.contains("apt-get update"), "{stdout}");
    assert!(
        stdout.contains("apt-get install -y postgresql-client"),
        "{stdout}"
    );

    // Idempotent: a second run (a later step in the same job) is a no-op on
    // the already-disabled sources.
    let again = Command::new("bash")
        .arg(&script)
        .arg("postgresql-client")
        .env("CI_APT_SOURCES_DIR", sources)
        .env("CI_APT_DRY_RUN", "1")
        .output()
        .unwrap();
    assert!(again.status.success());
    assert!(
        !sources
            .join("microsoft-prod.list.disabled-by-ci.disabled-by-ci")
            .exists()
    );
}

/// `--if-missing <cmd>` skips the apt fetch when the tool is already on the
/// image (the runner images ship a Postgres client) — but still disables the
/// third-party sources, so a later `--with-deps` step stays covered.
#[test]
fn if_missing_skips_apt_when_the_command_exists() {
    let script = workspace_root().join(SCRIPT);
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("microsoft-prod.list"), "deb x y z\n").unwrap();
    let out = Command::new("bash")
        .arg(&script)
        .args(["--if-missing", "bash", "postgresql-client"])
        .env("CI_APT_SOURCES_DIR", tmp.path())
        .env("CI_APT_DRY_RUN", "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert!(stdout.contains("already installed"), "{stdout}");
    assert!(!stdout.contains("apt-get update"), "{stdout}");
    assert!(
        !tmp.path().join("microsoft-prod.list").exists(),
        "the sources are disabled even when the install is skipped"
    );
}
