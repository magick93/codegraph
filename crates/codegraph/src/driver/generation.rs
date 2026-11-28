//! Generation preparation: the template engine (override dirs +
//! design-system pack), the extension-points + integration-manifest
//! configs, the resolved ux-rules, and the IFML component mappings — the
//! inputs every generating workflow resolves the same way.

use std::path::{Path, PathBuf};

use super::config_files::{load_extension_points, load_integration_manifests};
use crate::error::Result;
use crate::generate::ProjectConfig;

pub(super) fn load_ifml_component_mappings(
    path: Option<&Path>,
    design_system: Option<&str>,
) -> Result<Option<codegraph_config::IfmlComponentMappings>> {
    let project = match path {
        Some(path) => codegraph_config::IfmlComponentMappings::load(path)
            .map(Some)
            .map_err(crate::error::Error::Config)?,
        None => None,
    };
    let pack = match design_system.filter(|name| !name.is_empty()) {
        Some(name) => {
            Some(codegraph_config::built_in_pack(name).map_err(crate::error::Error::Config)?)
        }
        None => None,
    };
    Ok(match (project, pack) {
        (Some(project), Some(pack)) => Some(
            codegraph_config::IfmlComponentMappings::merge_with_pack(project, &pack),
        ),
        (Some(project), None) => Some(project),
        (None, Some(pack)) => Some(pack),
        (None, None) => None,
    })
}

/// Effective design-system pack: the explicit argument wins over the
/// profile's `ifml_design_system` feature; empty values mean "no pack".
pub(super) fn effective_design_system<'a>(
    arg: Option<&'a str>,
    build_plan: Option<&'a crate::profile::BuildPlan>,
) -> Option<&'a str> {
    arg.filter(|name| !name.is_empty()).or_else(|| {
        build_plan
            .and_then(|bp| bp.features.get("ifml_design_system"))
            .and_then(|v| v.as_str())
            .filter(|name| !name.is_empty())
    })
}

/// Effective ux-rules resolution (issue #293), mirroring
/// `effective_design_system`'s precedence: an explicit `--ux-rules` file
/// (merged over the built-in pack — project rules shadow pack rules per
/// selector tier) wins over the profiles.toml `ux_rules` feature (pack
/// only). Neither ⇒ `None`, keeping generation byte-identical.
///
/// An invalid CLI path is a hard error. A parse failure of the embedded
/// pack is an internal error that tests pin impossible.
pub(super) fn effective_ux_rules(
    cli_flag: Option<&Path>,
    build_plan: Option<&crate::profile::BuildPlan>,
) -> Result<Option<codegraph_config::UxRules>> {
    let pack = codegraph_config::builtin_ux_rules().map_err(|e| {
        crate::error::Error::Config(format!("built-in ux-default pack failed to parse: {e}"))
    })?;
    if let Some(path) = cli_flag {
        let parsed = codegraph_config::load_ux_rules(path)
            .map_err(|e| crate::error::Error::Config(e.to_string()))?;
        for warning in &parsed.warnings {
            eprintln!("  WARN ux-rules: {warning}");
        }
        return Ok(Some(codegraph_config::merge(&parsed.rules, &pack.rules)));
    }
    let enabled = build_plan.is_some_and(|bp| bp.ux_rules);
    Ok(enabled.then_some(pack.rules))
}

/// Everything the generation call consumes that isn't the graph or the
/// domain config: the template engine (override dirs + design-system pack),
/// the extension-points + integration-manifest configs, the resolved
/// ux-rules, and the IFML component mappings. Also stamps the two
/// project-config fields the pipeline resolves here (`ux`, and the
/// extension-points presence flag, issue #503).
pub(super) struct GenerationPrep {
    pub(super) tera: tera::Tera,
    pub(super) ext_config: Option<codegraph_ext_points::ExtensionPointsConfig>,
    pub(super) integration_manifests: Option<codegraph_ext_points::IntegrationManifestSet>,
    pub(super) ux_resolved: Option<codegraph_config::UxRules>,
    pub(super) ifml_component_mappings: Option<codegraph_config::IfmlComponentMappings>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_generation(
    template_dir: &[PathBuf],
    ifml_design_system: Option<&str>,
    build_plan: Option<&crate::profile::BuildPlan>,
    ifml_components: Option<&Path>,
    extension_points_path: Option<&Path>,
    integration_manifests: &[PathBuf],
    ux_rules: Option<&Path>,
    project_config: &mut ProjectConfig,
) -> Result<GenerationPrep> {
    // Precedence: project template dirs > design-system pack templates >
    // embedded built-ins.
    let design_system = effective_design_system(ifml_design_system, build_plan);
    let tera = create_tera(template_dir, design_system)?;
    let ext_config = load_extension_points(extension_points_path)?;
    let integration_manifests = load_integration_manifests(integration_manifests)?;

    // Pass 4-ish: resolve ux-rules (issue #293) once — CLI file merged over
    // the pack, or pack-only when the profile enables the feature. Threaded
    // into GeneratorOpts for generators and into ProjectConfig.ux so every
    // template can read `project.ux.*`. Profiled runs only: plan-less runs
    // keep the template context ux-blind (the historical contract) even
    // though issue #447 threads a minimal config for the dep pins —
    // generators still receive the rules via GeneratorOpts.
    let ux_resolved = effective_ux_rules(ux_rules, build_plan)?;
    if build_plan.is_some() {
        project_config.ux.ux = ux_resolved.clone();
    }
    // Extension-points flag (issue #503): templates gate the integration
    // plane (AppState field, dispatcher wiring) on this project-level flag;
    // per-entity call sites gate on the covered set instead.
    project_config.integration.has_extension_points = ext_config.is_some();

    let ifml_component_mappings = load_ifml_component_mappings(ifml_components, design_system)?;

    Ok(GenerationPrep {
        tera,
        ext_config,
        integration_manifests,
        ux_resolved,
        ifml_component_mappings,
    })
}

/// The template engine for a run: project template dirs > design-system
/// pack templates > embedded built-ins.
pub(super) fn create_tera(
    template_dir: &[PathBuf],
    design_system: Option<&str>,
) -> Result<tera::Tera> {
    let override_dirs: Vec<&Path> = template_dir.iter().map(|p| p.as_path()).collect();
    crate::generate::template_engine::create_tera_for_run(design_system, &override_dirs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_config::SemanticRole;

    #[test]
    fn design_system_arg_beats_profile_feature() {
        let dir = tempfile::tempdir().unwrap();
        let profiles = dir.path().join("profiles.toml");
        std::fs::write(
            &profiles,
            "[profiles.default.meta]\nname = \"default\"\nversion = \"1.0.0\"\n\
             description = \"test\"\n\n\
             [profiles.default.features]\n\
             ifml_design_system = \"shadcn-svelte\"\n",
        )
        .unwrap();
        let resolved =
            crate::profile::load_and_resolve_profile(&profiles, "default", None).unwrap();
        let registry = crate::profile::CapabilityRegistry::new();
        let plan = crate::profile::BuildPlan::from_profile(&resolved, &registry).unwrap();

        assert_eq!(
            effective_design_system(Some("other"), Some(&plan)),
            Some("other")
        );
        assert_eq!(
            effective_design_system(None, Some(&plan)),
            Some("shadcn-svelte")
        );
    }

    #[test]
    fn design_system_defaults_to_none_without_flag_or_feature() {
        assert_eq!(effective_design_system(None, None), None);
        assert_eq!(effective_design_system(Some(""), None), None);
    }

    fn plan_with_ux_rules(enabled: bool) -> crate::profile::BuildPlan {
        let toml = format!(
            r#"
[profiles.default.meta]
name = "default"
version = "1.0.0"
description = ""

[profiles.default.features]
ux_rules = {enabled}

[profiles.default.api]
generators = ["ddl"]
"#
        );
        let dir = tempfile::tempdir().unwrap();
        let profiles = dir.path().join("profiles.toml");
        std::fs::write(&profiles, toml).unwrap();
        crate::profile::load_and_resolve_profile(&profiles, "default", None)
            .map(|resolved| {
                crate::profile::BuildPlan::from_profile(
                    &resolved,
                    &crate::profile::CapabilityRegistry::new(),
                )
                .unwrap()
            })
            .unwrap()
    }

    #[test]
    fn ux_rules_cli_file_merges_over_pack() {
        use codegraph_config::Dimension;

        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("ux-rules.toml");
        std::fs::write(
            &project,
            "[format]\nlocale = \"de-DE\"\n\n[[column]]\ndimension = \"money\"\ndisplay = \"raw\"\n",
        )
        .unwrap();

        let resolved = effective_ux_rules(Some(&project), None).unwrap().unwrap();
        // Project locale wins; the pack's currency default is replaced.
        assert_eq!(resolved.format.locale, "de-DE");
        assert_eq!(resolved.format.currency, None);
        // Project rule prepended: shadows the pack's money rule.
        assert_eq!(resolved.columns[0].dimension, Some(Dimension::Money));
        assert_eq!(
            resolved.columns[0].display,
            Some(codegraph_config::Display::Raw)
        );
        // Pack rules still reachable behind the project's.
        assert!(
            resolved
                .columns
                .iter()
                .any(|c| c.dimension == Some(Dimension::Identifier))
        );
    }

    #[test]
    fn ux_rules_profile_flag_resolves_pack_only() {
        let plan = plan_with_ux_rules(true);
        let resolved = effective_ux_rules(None, Some(&plan)).unwrap().unwrap();
        assert_eq!(resolved.format.locale, "en-NZ");
        assert_eq!(resolved.format.currency.as_deref(), Some("NZD"));
        assert_eq!(resolved.actions.confirm, vec!["delete".to_string()]);
    }

    #[test]
    fn ux_rules_defaults_to_none_without_flag_or_feature() {
        assert!(effective_ux_rules(None, None).unwrap().is_none());
        let plan = plan_with_ux_rules(false);
        assert!(effective_ux_rules(None, Some(&plan)).unwrap().is_none());
    }

    #[test]
    fn ux_rules_cli_flag_wins_over_disabled_feature() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("ux-rules.toml");
        std::fs::write(&project, "").unwrap();
        let plan = plan_with_ux_rules(false);
        assert!(
            effective_ux_rules(Some(&project), Some(&plan))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn ux_rules_missing_cli_file_is_a_hard_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.toml");
        let plan = plan_with_ux_rules(true);
        let err = effective_ux_rules(Some(&missing), Some(&plan)).unwrap_err();
        assert!(
            err.to_string().contains("missing.toml"),
            "bad path must error naming the file: {err}"
        );
    }

    #[test]
    fn design_system_pack_loads_and_project_entries_win() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("ifml-components.toml");
        std::fs::write(
            &project,
            "[[component]]\nrole = \"collection\"\npath = \"$lib/components/MyTable.svelte\"\n",
        )
        .unwrap();

        let merged = load_ifml_component_mappings(Some(&project), Some("shadcn-svelte"))
            .unwrap()
            .unwrap();
        assert_eq!(
            merged
                .resolve_slot("V", "grid", "list", "table", Some(SemanticRole::Collection))
                .unwrap()
                .path,
            "$lib/components/MyTable.svelte"
        );
        assert_eq!(
            merged
                .resolve_slot(
                    "V",
                    "save",
                    "button",
                    "button",
                    Some(SemanticRole::ActionControl)
                )
                .unwrap()
                .export_name(),
            "Button"
        );
    }

    #[test]
    fn design_system_pack_loads_without_project_file() {
        let pack_only = load_ifml_component_mappings(None, Some("shadcn-svelte"))
            .unwrap()
            .unwrap();
        assert_eq!(
            pack_only
                .resolve_by_role("V", SemanticRole::ModalView)
                .unwrap()
                .export_name(),
            "Dialog"
        );
    }

    #[test]
    fn unknown_design_system_pack_is_an_error() {
        let err = load_ifml_component_mappings(None, Some("material"))
            .expect_err("unknown pack must error");
        assert!(err.to_string().contains("unknown IFML design system pack"));
    }
}
