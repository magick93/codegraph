//! CI toolchain parity contract (issue #627): the repo pins its Rust
//! toolchain in a root `rust-toolchain.toml` so local and CI compilers
//! cannot drift apart. `dtolnay/rust-toolchain@stable` respects the file
//! (it reads the channel from it), and local `rustup` follows it — the
//! lint class that fired on PR #626 (`clippy::duplicated_attributes` on
//! CI's newer stable while local lagged) becomes structurally impossible.
//!
//! Bump discipline (AGENTS.md dependency policy): move the pin DELIBERATELY
//! (monthly, alongside the Tier-0 lockfile refresh), never silently — bump
//! the channel AND the `PINNED_CHANNEL` literal here in the same commit.
//!
//! ```text
//! cargo test -p codegraph --test ci_toolchain_contract
//! ```

use std::path::Path;

/// The pinned channel. When CI's stable moves and the team opts into the
/// monthly bump, update `rust-toolchain.toml` and this literal together.
const PINNED_CHANNEL: &str = "1.100.0";

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn toolchain_file() -> toml::Table {
    let path = workspace_root().join("rust-toolchain.toml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} must exist (issue #627): {e}", path.display()));
    text.parse()
        .unwrap_or_else(|e| panic!("{} must parse as TOML: {e}", path.display()))
}

fn ci_yml() -> String {
    std::fs::read_to_string(workspace_root().join(".github/workflows/ci.yml"))
        .expect(".github/workflows/ci.yml must exist")
}

#[test]
fn toolchain_file_pins_the_ci_channel() {
    let table = toolchain_file();
    let channel = table
        .get("toolchain")
        .and_then(|t| t.get("channel"))
        .and_then(|c| c.as_str())
        .unwrap_or_else(|| {
            panic!(
                "rust-toolchain.toml must pin [toolchain] channel — expected \
                 channel = \"{PINNED_CHANNEL}\" (issue #627). To bump: move \
                 BOTH the channel and PINNED_CHANNEL in the same commit, \
                 monthly alongside the Tier-0 lockfile refresh."
            )
        });
    assert_eq!(
        channel, PINNED_CHANNEL,
        "the channel must match PINNED_CHANNEL — bump both together (monthly discipline)"
    );
}

#[test]
fn toolchain_file_declares_the_lint_components() {
    let table = toolchain_file();
    let components: Vec<&str> = table
        .get("toolchain")
        .and_then(|t| t.get("components"))
        .and_then(|c| c.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    for component in ["rustfmt", "clippy"] {
        assert!(
            components.contains(&component),
            "`{component}` must be declared in rust-toolchain.toml components — \
             the lint job runs it and local rustup only installs declared \
             components for the pinned toolchain"
        );
    }
}

#[test]
fn toolchain_file_declares_the_wasm_target() {
    let table = toolchain_file();
    let targets: Vec<&str> = table
        .get("toolchain")
        .and_then(|t| t.get("targets"))
        .and_then(|c| c.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    assert!(
        targets.contains(&"wasm32-unknown-unknown"),
        "wasm32-unknown-unknown must be declared in rust-toolchain.toml targets — \
         the mox-plane compile pin (#517) and the worker wasm gate (#550) \
         cargo-check that target; local parity needs rustup to install it"
    );
}

#[test]
fn ci_jobs_stay_on_the_stable_action_so_the_pin_is_authoritative() {
    let ci = ci_yml();
    let uses = ci
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- uses: dtolnay/rust-toolchain@"))
        .collect::<Vec<_>>();
    assert!(
        !uses.is_empty(),
        "the CI workflow must use dtolnay/rust-toolchain — the contract is \
         action@stable + rust-toolchain.toml, not a hand-pinned action ref"
    );
    for variant in uses {
        assert_eq!(
            variant, "stable",
            "every dtolnay/rust-toolchain use must stay @stable so \
             rust-toolchain.toml is the single toolchain authority — \
             a hand-pinned ref here would bypass the pin file (issue #627)"
        );
    }
}
