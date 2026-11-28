//! Loading the configuration files every workflow reads: `domains.toml`,
//! `classifier.toml`, and the optional sibling files next to the domain
//! config (`ui-overrides.toml`, `ui-domains.toml`, `seed.toml`), plus the
//! extension-points and integration-manifest inputs.
//!
//! One loader per file, shared by every workflow, so `run`, `classify`,
//! `generate` and `ifml_generate` cannot read the same file two ways.

use std::path::{Path, PathBuf};

use crate::error::Result;

/// Parse `domains.toml`.
pub(super) fn load_domain_config(
    config_path: &Path,
) -> Result<codegraph_config::config::DomainConfig> {
    codegraph_config::config::parse_domain_config(config_path)
        .map_err(|e| crate::error::Error::Config(e.to_string()))
}

/// Parse `classifier.toml`, or the all-defaults config when none is given
/// (an empty TOML document deserializes to the defaults — the mox-first
/// runs that carry no classifier).
pub(super) fn load_classifier_config(
    classifier: Option<&Path>,
) -> Result<codegraph_classifier::config::ClassifierConfig> {
    match classifier {
        Some(classifier) => codegraph_classifier::config::parse_classifier_config(classifier)
            .map_err(|e| crate::error::Error::Config(e.to_string())),
        None => codegraph_classifier::config::parse_classifier_config_str("")
            .map_err(|e| crate::error::Error::Config(e.to_string())),
    }
}

/// The optional `extension-points.toml` (`--extension-points`).
pub(super) fn load_extension_points(
    path: Option<&Path>,
) -> Result<Option<codegraph_ext_points::ExtensionPointsConfig>> {
    match path {
        Some(path) => Ok(Some(
            codegraph_ext_points::parse_extension_points(path)
                .map_err(|e| crate::error::Error::Config(e.to_string()))?,
        )),
        None => Ok(None),
    }
}

/// Integration catalog manifests (issue #552): parse + merge the
/// repeatable `--integration-manifests` files. Validation against the
/// extension points happens in the generation pipeline (a broken manifest
/// is a config error, never half-generation).
pub(super) fn load_integration_manifests(
    paths: &[PathBuf],
) -> Result<Option<codegraph_ext_points::IntegrationManifestSet>> {
    if paths.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        codegraph_ext_points::parse_integration_manifests(paths)
            .map_err(crate::error::Error::Config)?,
    ))
}

pub(super) fn load_ui_overrides(config_path: &Path) -> Result<codegraph_config::UiOverrideConfig> {
    let parent = config_parent(config_path)?;
    let path = parent.join("ui-overrides.toml");
    if path.exists() {
        codegraph_config::parse_ui_overrides_config(&path)
            .map_err(|e| crate::error::Error::Config(e.to_string()))
    } else {
        Ok(codegraph_config::UiOverrideConfig::default())
    }
}

pub(super) fn load_seed_config(config_path: &Path) -> Option<PathBuf> {
    let path = config_path.parent()?.join("seed.toml");
    if path.exists() { Some(path) } else { None }
}

pub(super) fn load_ui_domains(config_path: &Path) -> Result<codegraph_config::UiDomainConfig> {
    let parent = config_parent(config_path)?;
    let path = parent.join("ui-domains.toml");
    if path.exists() {
        codegraph_config::parse_ui_domains_config(&path)
            .map_err(|e| crate::error::Error::Config(e.to_string()))
    } else {
        Ok(codegraph_config::UiDomainConfig::default())
    }
}

fn config_parent(config_path: &Path) -> Result<&Path> {
    config_path.parent().ok_or_else(|| {
        crate::error::Error::Config(format!(
            "Config path '{}' has no parent directory",
            config_path.display()
        ))
    })
}
