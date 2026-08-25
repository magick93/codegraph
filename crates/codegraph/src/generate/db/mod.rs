pub mod basejump_setup;
pub mod codelist;
pub mod cornucopia_config;
pub mod cornucopia_queries;
pub mod ddl;
pub mod dialect;
pub mod entity;
pub mod event_trigger;
pub mod label_setup;
pub mod platform_grants;
pub mod platform_schema;
pub mod report_view;
pub mod seed;
pub mod workflow_seed;

use std::path::{Path, PathBuf};

/// Resolve the repo-level `migrations` root for SQL migrations.
///
/// Migrations are hand-extended at the repository root (`0000`–`0009` bootstrap
/// and platform files), so they must live OUTSIDE the generated tree. The
/// codegen output dir is `<repo>/generated/cosmos-app`, so the migrations root
/// is two parents up, joined with `migrations`.
///
/// Falls back to `output_dir.join("migrations")` when the output dir isn't
/// shaped like `<root>/generated/cosmos-app` (e.g. codegraph unit tests or a
/// project with a different layout), preserving the pre-relocation behavior
/// for those callers.
pub fn migrations_root(output_dir: &Path) -> PathBuf {
    let in_repo_layout = output_dir
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n == "cosmos-app")
        .unwrap_or(false)
        && output_dir
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|n| n == "generated")
            .unwrap_or(false);
    if in_repo_layout {
        if let Some(repo_root) = output_dir.parent().and_then(Path::parent) {
            return repo_root.join("migrations");
        }
    }
    output_dir.join("migrations")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_root_derives_repo_level_path_for_repo_layout() {
        let out = Path::new("/repo/community-os/generated/cosmos-app");
        assert_eq!(
            migrations_root(out),
            PathBuf::from("/repo/community-os/migrations")
        );
    }

    #[test]
    fn migrations_root_resolves_relative_repo_layout_against_cwd() {
        let out = Path::new("generated/cosmos-app");
        assert_eq!(migrations_root(out), PathBuf::from("migrations"));
    }

    #[test]
    fn migrations_root_falls_back_to_output_dir_subdir() {
        let out = Path::new("/tmp/db-fixture-required-ref");
        assert_eq!(
            migrations_root(out),
            PathBuf::from("/tmp/db-fixture-required-ref/migrations")
        );
    }
}
