//! The driver's public argument structures. `RunArgs` is consumed by the
//! generated `{name}-graph` wrapper binaries, so its shape is a public API
//! (issue #348 drift detection keys on [`run_args_field_names`]).

use std::path::{Path, PathBuf};

/// Arguments for the full pipeline (`run`): ingest the model sources
/// (mox-first, then JSON schemas), classify, then run all generators.
#[derive(Debug)]
pub struct RunArgs<'a> {
    /// JSON schema directory. Optional when `mox_files` is provided
    /// (mox-first, issue #231); schemas-only runs are deprecated as the
    /// primary model source but keep identical behavior.
    pub schemas: Option<&'a Path>,
    /// classifier.toml path. Required when `schemas` is provided; ignored
    /// (an empty config is used) for mox-only runs without one.
    pub classifier: Option<&'a Path>,
    pub config_path: &'a Path,
    pub output: &'a Path,
    pub extension_points_path: Option<&'a Path>,
    /// Integration catalog manifest files (issue #552). When non-empty the
    /// integration tables generator additionally emits
    /// `0010_integration_catalog_seed.sql` (catalog rows + optional demo
    /// installations). Requires `extension_points_path`.
    pub integration_manifests: &'a [PathBuf],
    pub profile_name: &'a str,
    pub variant: Option<&'a str>,
    pub profiles_config_path: Option<PathBuf>,
    pub no_post_gen: bool,
    pub template_dir: &'a [PathBuf],
    pub ifml_files: &'a [PathBuf],
    pub openapi_files: &'a [PathBuf],
    /// pherolang `.mox` domain model files to compile in-process and ingest
    /// into the graph (vocabularies with facets, class operations, derived
    /// features). Absent/empty = no mox ingest, generators unchanged.
    pub mox_files: &'a [PathBuf],
    /// Rosetta (Rune DSL) `.rosetta` model files, bridged into the graph
    /// data plane (issue #256). Ingested after mox and BEFORE the JSON
    /// schema pass; bridged titles join the schema pass's skip-set.
    pub rosetta_files: &'a [PathBuf],
    /// pherolang `.ddd` design files (issue #449), compiled against their
    /// imported `.mox` domains and ingested as the DDD design plane
    /// (designs/repositories/services/searches). Absent/empty = no design
    /// ingest, generators unchanged.
    pub ddd_files: &'a [PathBuf],
    /// pherolang `.evt` event-contract files (issue #454), compiled against
    /// their imported `.mox` domains and ingested as the events plane
    /// (events/channels/subscriptions). Absent/empty = no event ingest,
    /// generators unchanged.
    pub evt_files: &'a [PathBuf],
    /// pherolang `.deploy` deployment files (pherolang #49), compiled against
    /// their imported `.ddd` designs and `.ifml` flows and ingested as the
    /// deployment plane (applications/profiles/deployments). Absent/empty =
    /// no deployment ingest, generators unchanged.
    pub deploy_files: &'a [PathBuf],
    pub ifml_framework: &'a [String],
    /// Optional `ifml-components.toml` mapping IFML components to
    /// handcrafted framework components. Absent = all built-in templates.
    pub ifml_components: Option<&'a Path>,
    /// Built-in IFML design-system pack name (e.g. "shadcn-svelte").
    /// Overrides the profiles.toml `ifml_design_system` feature. Project
    /// `ifml_components` entries take precedence over pack entries.
    pub ifml_design_system: Option<&'a str>,
    /// Optional `ux-rules.toml` (issue #293). Rules shadow the built-in
    /// `ux-default` pack. Overrides the profiles.toml `ux_rules` feature.
    pub ux_rules: Option<&'a Path>,
    /// Git rev to pin in generated Cargo.toml codegraph deps. When None the
    /// driver falls back to `git rev-parse HEAD` of the current directory.
    pub codegraph_rev: Option<String>,
    /// Post-generate compile gate (issue #336): when true, run `cargo
    /// check` over `<output>/Cargo.toml` after generation and fail the run
    /// when it fails. Defaults to false — no cargo invocation.
    pub check: bool,
}

/// The [`RunArgs`] field names, in declaration order — the single source of
/// truth for wrapper drift detection (issue #348). Derived from the struct
/// itself: the exhaustive construction below makes the compiler force an
/// update here whenever the driver gains a field, so no second list can
/// silently rot. The init contract test (`wrapper_drift_tests`) asserts
/// the rendered wrapper mentions every name, and doctor diffs existing
/// wrappers against this list.
pub fn run_args_field_names() -> Vec<String> {
    let empty_paths: &[PathBuf] = &[];
    let empty_strings: &[String] = &[];
    let args = RunArgs {
        schemas: None,
        classifier: None,
        config_path: Path::new(""),
        output: Path::new(""),
        extension_points_path: None,
        integration_manifests: empty_paths,
        profile_name: "",
        variant: None,
        profiles_config_path: None,
        no_post_gen: false,
        template_dir: empty_paths,
        ifml_files: empty_paths,
        openapi_files: empty_paths,
        mox_files: empty_paths,
        rosetta_files: empty_paths,
        ddd_files: empty_paths,
        evt_files: empty_paths,
        deploy_files: empty_paths,
        ifml_framework: empty_strings,
        ifml_components: None,
        ifml_design_system: None,
        ux_rules: None,
        codegraph_rev: None,
        check: false,
    };
    parse_struct_field_names(&format!("{args:?}"))
}

/// Extract field names from a derived-`Debug` struct dump
/// (`RunArgs { a: v, b: v }`). Trivial values (None / [] / false / "")
/// keep the parse exact: no value contains commas or braces.
fn parse_struct_field_names(debug: &str) -> Vec<String> {
    let Some((_, inner)) = debug.split_once('{') else {
        return Vec::new();
    };
    let inner = inner.strip_suffix('}').unwrap_or(inner);
    inner
        .split(", ")
        .filter_map(|pair| pair.split(':').next())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Arguments for the IFML-only UI generation path (`ifml_generate`).
pub struct IfmlGenerateArgs<'a> {
    pub config_path: &'a Path,
    pub output: &'a Path,
    pub ifml_files: &'a [PathBuf],
    /// Optional JSON schema directory — enables entity resolution enrichment.
    pub schemas: Option<&'a Path>,
    /// Path to classifier.toml (required only with `schemas`).
    pub classifier: Option<&'a Path>,
    /// IFML framework targets (e.g. "svelte", "react"). Empty defaults to svelte.
    pub frameworks: &'a [String],
    /// Optional path to profiles.toml.
    pub profiles_config_path: Option<PathBuf>,
    pub template_dir: &'a [PathBuf],
    /// Optional `ifml-components.toml` mapping IFML components to
    /// handcrafted framework components. Absent = all built-in templates.
    pub ifml_components: Option<&'a Path>,
    /// Built-in IFML design-system pack name (e.g. "shadcn-svelte").
    /// Overrides the profiles.toml `ifml_design_system` feature. Project
    /// `ifml_components` entries take precedence over pack entries.
    pub ifml_design_system: Option<&'a str>,
    /// Optional `ux-rules.toml` (issue #293). Rules shadow the built-in
    /// `ux-default` pack. Overrides the profiles.toml `ux_rules` feature.
    pub ux_rules: Option<&'a Path>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_args_field_names_lists_every_field_in_declaration_order() {
        // Issue #348: the wrapper contract test and the doctor drift check
        // both consume this list, so the pin here protects the Debug-parse
        // derivation. The exhaustive construction inside
        // `run_args_field_names` makes the compiler force an update when
        // `RunArgs` gains a field; this pin catches a parser regression.
        assert_eq!(
            run_args_field_names(),
            vec![
                "schemas",
                "classifier",
                "config_path",
                "output",
                "extension_points_path",
                "integration_manifests",
                "profile_name",
                "variant",
                "profiles_config_path",
                "no_post_gen",
                "template_dir",
                "ifml_files",
                "openapi_files",
                "mox_files",
                "rosetta_files",
                "ddd_files",
                "evt_files",
                "deploy_files",
                "ifml_framework",
                "ifml_components",
                "ifml_design_system",
                "ux_rules",
                "codegraph_rev",
                "check",
            ]
        );
    }
}
