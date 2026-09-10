//! EmDash plugin generator family.
//!
//! Emits per-domain EmDash plugin TypeScript packages from the same resolved
//! schema model that emits routers/contracts, plus the matching public site
//! pages and a generated Playwright CRUD journey. Configuration comes from
//! `plugins.toml` (see [`config::EmdashPluginsConfig`]); the reference
//! behavior is the hand-written `packages/emdash-community-events` package.
//!
//! Generators:
//! - [`plugin_gen::EmdashPluginGenerator`] (`emdash_plugin`, per-domain) —
//!   one plugin package + site pages + e2e spec per configured domain.
//! - [`scaffold_gen::EmdashPluginScaffoldGenerator`]
//!   (`emdash_plugin_scaffold`, global) — shared README at the packages root.
//!
//! Both are gated behind the `emdash_plugins` profile feature and the
//! `GeneratorOpts::emdash_plugins` config; plan-less runs never emit them.

pub mod config;
pub mod context;
pub mod plugin_gen;
pub mod scaffold_gen;

use std::path::{Path, PathBuf};

pub use config::{load_plugins_config, EmdashPluginConfig, EmdashPluginsConfig};

/// Marker file name of the generated app output dir (shape detection).
const APP_OUTPUT_DIR: &str = "cosmos-app";
/// Marker parent of the generated app output dir (shape detection).
const GENERATED_DIR: &str = "generated";
/// Repo-level directory holding the generated plugin packages.
const PACKAGES_DIR: &str = "packages";
/// Default repo-relative base for the community site's public pages.
pub const DEFAULT_SITE_PAGES_BASE: &str = "apps/community-site/src/pages";
/// Default repo-relative base for the community site's e2e suite.
pub const DEFAULT_SITE_E2E_BASE: &str = "apps/community-site/e2e";

/// Kebab-case a domain key, keeping only URL/package-safe characters.
pub(crate) fn kebab_domain(domain: &str) -> String {
    codegraph_naming::to_kebab_case(domain)
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect()
}

/// Detect the repo root from the generated-app output dir layout
/// (`<repo>/generated/cosmos-app`). Returns `None` for any other shape.
///
/// A relative `generated/cosmos-app` yields `Some("")` so downstream joins
/// stay relative — mirroring `playwright::e2e_tests_root`.
fn repo_root_from_output(output_dir: &Path) -> Option<PathBuf> {
    if output_dir.file_name()?.to_str()? != APP_OUTPUT_DIR {
        return None;
    }
    let generated = output_dir.parent()?;
    if generated.file_name()?.to_str()? != GENERATED_DIR {
        return None;
    }
    generated.parent().map(Path::to_path_buf)
}

/// Root of the EmDash plugin package for one domain:
/// `<repo>/packages/emdash-community-{domain_key}`.
///
/// Falls back to `<output_dir>/../packages/emdash-community-{domain_key}`
/// when the output dir isn't shaped like `<repo>/generated/cosmos-app`
/// (unit tests, scratch generations, different layouts).
pub fn emdash_package_root(output_dir: &Path, domain_key: &str) -> PathBuf {
    let kebab = kebab_domain(domain_key);
    match repo_root_from_output(output_dir) {
        Some(repo) => repo
            .join(PACKAGES_DIR)
            .join(format!("emdash-community-{kebab}")),
        None => output_dir
            .parent()
            .unwrap_or(output_dir)
            .join(PACKAGES_DIR)
            .join(format!("emdash-community-{kebab}")),
    }
}

/// Root directory holding all generated plugin packages.
pub fn emdash_packages_root(output_dir: &Path) -> PathBuf {
    match repo_root_from_output(output_dir) {
        Some(repo) => repo.join(PACKAGES_DIR),
        None => output_dir.parent().unwrap_or(output_dir).join(PACKAGES_DIR),
    }
}

/// Root of the community site's public pages, honoring a profile-configured
/// base (`ProjectConfig::emdash_site_pages_base`); an empty base falls back
/// to the default.
pub fn emdash_site_pages_root_with_base(output_dir: &Path, base: &str) -> PathBuf {
    let base = if base.is_empty() {
        DEFAULT_SITE_PAGES_BASE
    } else {
        base
    };
    match repo_root_from_output(output_dir) {
        Some(repo) => repo.join(base),
        None => output_dir.join(base),
    }
}

/// Root of the community site's public pages (default base):
/// `<repo>/apps/community-site/src/pages`.
pub fn emdash_site_pages_root(output_dir: &Path) -> PathBuf {
    emdash_site_pages_root_with_base(output_dir, DEFAULT_SITE_PAGES_BASE)
}

/// Root of the community site's e2e suite, honoring a profile-configured
/// base (`ProjectConfig::emdash_site_e2e_base`).
pub fn emdash_site_e2e_root_with_base(output_dir: &Path, base: &str) -> PathBuf {
    let base = if base.is_empty() {
        DEFAULT_SITE_E2E_BASE
    } else {
        base
    };
    match repo_root_from_output(output_dir) {
        Some(repo) => repo.join(base),
        None => output_dir.join(base),
    }
}

/// Root of the community site's e2e suite (default base):
/// `<repo>/apps/community-site/e2e`.
pub fn emdash_site_e2e_root(output_dir: &Path) -> PathBuf {
    emdash_site_e2e_root_with_base(output_dir, DEFAULT_SITE_E2E_BASE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_root_derives_repo_level_path_for_repo_layout() {
        let out = Path::new("/repo/community-os/generated/cosmos-app");
        assert_eq!(
            emdash_package_root(out, "events"),
            PathBuf::from("/repo/community-os/packages/emdash-community-events")
        );
    }

    #[test]
    fn package_root_falls_back_next_to_output_dir() {
        let out = Path::new("/tmp/scratch-out");
        assert_eq!(
            emdash_package_root(out, "crm"),
            PathBuf::from("/tmp/packages/emdash-community-crm")
        );
    }

    #[test]
    fn site_roots_derive_repo_level_paths() {
        let out = Path::new("/repo/community-os/generated/cosmos-app");
        assert_eq!(
            emdash_site_pages_root(out),
            PathBuf::from("/repo/community-os/apps/community-site/src/pages")
        );
        assert_eq!(
            emdash_site_e2e_root(out),
            PathBuf::from("/repo/community-os/apps/community-site/e2e")
        );
    }

    #[test]
    fn site_roots_fall_back_to_output_subdir() {
        let out = Path::new("/tmp/scratch-out");
        assert_eq!(
            emdash_site_pages_root(out),
            PathBuf::from("/tmp/scratch-out/apps/community-site/src/pages")
        );
    }

    #[test]
    fn site_roots_honor_custom_base() {
        let out = Path::new("/repo/generated/cosmos-app");
        assert_eq!(
            emdash_site_pages_root_with_base(out, "apps/site/pages"),
            PathBuf::from("/repo/apps/site/pages")
        );
        // Empty base falls back to the default.
        assert_eq!(
            emdash_site_e2e_root_with_base(out, ""),
            PathBuf::from("/repo/apps/community-site/e2e")
        );
    }

    #[test]
    fn load_plugins_config_reads_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plugins.toml");
        std::fs::write(
            &path,
            "[plugins.events]\nlabel = \"Events\"\n[plugins.events.entities.RsvpType]\n",
        )
        .unwrap();
        let cfg = load_plugins_config(&path).unwrap();
        assert_eq!(cfg.plugins["events"].label, "Events");
    }

    #[test]
    fn load_plugins_config_errors_are_strings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.toml");
        let err = load_plugins_config(&path).unwrap_err();
        assert!(err.contains("failed to read"));
    }
}
