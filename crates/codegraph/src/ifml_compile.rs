//! The rexlang IFML front door (issue #591): gather a `.ifml` file's
//! import closure, compile it (module composition resolved), run the
//! interaction-reference + authorization checker, and derive the
//! declaration-backed control/route manifest.
//!
//! The old ingest path parsed the entry file and trusted every reference:
//! `action("X")` invocations without a declaration synthesized
//! indistinguishable `action:X` nodes (the #559 defect class), and
//! `requires:`/`roles:` gates were never validated against the policy
//! layer. rexlang #117/#118 moved the proof upstream — E0104–E0109 are
//! checker errors, and [`rex_ifml::manifest_ifml`] exports the positive
//! artifact for what the model lowers to. This module is the host-side
//! seam: everything downstream (graph ingest, generators, doctor) works
//! from a **checked** compilation, so a dangling reference is a hard
//! error before anything can land in the graph.
//!
//! Scope contract (matching `rex_driver::project`'s gather discipline):
//!
//! - `.ifml` imports feed the resolver bundle (module composition); a
//!   missing one fails the compile (hard).
//! - `.actor`/`.json` imports compile to [`rex_ir::ActorModel`]s through
//!   [`crate::ifml_actor_import::resolve_actor_models`] — the run path's
//!   warn-and-continue contract. Policies that resolve feed
//!   E0107/E0108; an empty slice skips the authorization pass.
//! - `.mox` imports (and the domain union they could provide) are
//!   gathered but NOT compiled here — expression checking (E0100–E0103)
//!   stays off (`domains: None`); the `.ifml`↔domain-typechecking slice
//!   is a recorded follow-up.
//!
//! The manifest rides along on [`IfmlCompiled`] so consumers (route
//! generation, the #559 service-op plane) can trust every edge without
//! re-deriving resolution.

use std::path::{Path, PathBuf};

use rex_driver::project::{IfmlProject, gather_ifml_project};
use rex_ir::ActorModel;
use rex_ir::ifml_manifest::IfmlManifest;

use crate::error::{Error, Result};

/// One gathered `.ifml` compilation: the checked model, the manifest, and
/// the actor models the authorization pass ran against.
#[derive(Debug)]
pub struct IfmlCompiled {
    /// The resolved compilation — `model` is artifact-identical to
    /// `rex_ifml::parse_ifml` for the entry file, with module composition
    /// proven.
    pub compilation: rex_ifml::IfmlCompilation,
    /// The declaration-backed control/route manifest (rexlang #118):
    /// routes, expanded controls, proven edges, actions, commands.
    pub manifest: IfmlManifest,
    /// The actor models gathered from the file's policy imports (empty
    /// when none resolve — the authorization pass was skipped).
    pub actor_models: Vec<ActorModel>,
    /// The policy import paths as written (the graph policy ingest's
    /// input, unchanged).
    pub policy_imports: Vec<String>,
    /// Warning-severity checker findings (the rexlang lint channel,
    /// W0150–W0154 — dead actions, unreachable views, shadowed modules,
    /// components without data, views without domain binding), rendered
    /// one `file:code message` line per finding. Warnings never fail the
    /// compilation; hosts render them and decide (rexlang
    /// `check --deny-warnings` is the strict knob). Codegraph renders
    /// them on the run path and counts them in doctor's soft warnings.
    pub lints: Vec<String>,
}

/// Read one `.ifml` file, gather its import closure
/// ([`rex_driver::project::gather_ifml_project`]), compile it, run the
/// E0104–E0109 checker, and derive the manifest.
///
/// Hard errors (`Error::IfmlModel`): unreadable file, compile/resolution
/// diagnostics, ERROR-severity checker diagnostics. WARNING-severity
/// findings (the lint channel, W0150–W0154) ride [`IfmlCompiled::lints`]
/// and never fail. Unresolvable `.actor`/`.mox`/`.json` imports warn and
/// continue (the run-path contract — the compile hard-errors only for
/// `.ifml` imports, which module composition cannot proceed without).
pub fn compile_and_check_ifml(path: &Path) -> Result<IfmlCompiled> {
    let source = std::fs::read_to_string(path).map_err(|e| Error::IfmlModel {
        file: path.display().to_string(),
        reason: format!("could not be read: {e}"),
    })?;
    let key = path.to_string_lossy().to_string();

    // The fetch resolves every import relative to the IMPORTING file's
    // directory (the entry file first, then each imported file's own
    // closure — the walker calls back with the resolved key it handed
    // out).
    let mut fetch = |importer: &str, import: &str| -> Option<(String, String)> {
        let resolved = resolve_import(importer, import);
        let text = std::fs::read_to_string(&resolved).ok()?;
        Some((resolved.to_string_lossy().to_string(), text))
    };
    let project: IfmlProject = gather_ifml_project(&key, &source, &mut fetch);
    for (importer, import) in &project.unresolved {
        eprintln!(
            "Warning: ifml — '{import}' imported by '{importer}' could not be read; \
             any references through it stay unexpanded"
        );
    }

    let compilation =
        rex_ifml::compile_ifml_str(&key, &source, &project.imports).map_err(|diags| {
            Error::IfmlModel {
                file: key.clone(),
                reason: render_diags(&diags),
            }
        })?;

    // Policy imports come from the entry file's own declarations (the
    // same list the graph policy ingest walks).
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let policy_imports: Vec<String> = compilation
        .model
        .imports
        .iter()
        .filter(|raw| matches!(raw.rsplit('.').next(), Some("actor") | Some("json")))
        .cloned()
        .collect();
    let actor_models = crate::ifml_actor_import::resolve_actor_models(&policy_imports, base_dir);
    let actor_models = with_snake_role_aliases(actor_models);

    // `domains: None`: the interaction-reference pass (E0104–E0106), the
    // action-target check (E0109), and the authorization pass (E0107/E0108
    // when actors resolve) run; the expression checks stay off (recorded
    // follow-up). Error-severity diagnostics hard-fail; the lint channel
    // (W0150–W0154) rides the compiled artifact as warnings — a linty
    // model still ingests.
    let diags = rex_ifml::check_ifml_with_actors(&compilation, None, &actor_models);
    let (errors, lints): (Vec<_>, Vec<_>) =
        diags.into_iter().partition(|diag| diag.severity.is_error());
    if !errors.is_empty() {
        return Err(Error::IfmlModel {
            file: key.clone(),
            reason: render_diags(&errors),
        });
    }

    let manifest = rex_ifml::manifest_ifml(&compilation, &project.imports, None);
    Ok(IfmlCompiled {
        compilation,
        manifest,
        actor_models,
        policy_imports,
        lints: render_diags(&lints).lines().map(str::to_string).collect(),
    })
}

/// Resolve one import path relative to the importing file's directory.
fn resolve_import(importer: &str, import: &str) -> PathBuf {
    let importer_path = Path::new(importer);
    let base = importer_path.parent().unwrap_or_else(|| Path::new("."));
    base.join(import)
}

/// The checker is strict-basename, the guard plane is not: a view may
/// author its `roles:` in the actor's snake_case form (the crewbase
/// convention — `roles: [member]` against actor `Member`); personas seed
/// both tokens and `route_generator::roles::role_token_matches` compares
/// both forms, so the runtime contract accepts the alias. The checker's
/// E0108 is verbatim-name, so the models fed to it carry a snake_case
/// twin per actor — CHECKING ONLY: the twins are never ingested into the
/// graph or the emitted policy surface. Capabilities get no alias: the
/// generated `can()` guard compares capability names verbatim, so a
/// snake-cased `requires:` could never pass at runtime and E0107's
/// refusal is correct.
fn with_snake_role_aliases(mut models: Vec<ActorModel>) -> Vec<ActorModel> {
    use codegraph_naming::to_snake_case;

    for model in &mut models {
        let existing: std::collections::HashSet<String> = model
            .blocks
            .iter()
            .flat_map(|block| block.actors.iter().map(|actor| actor.name.clone()))
            .collect();
        for block in &mut model.blocks {
            let originals: Vec<_> = block.actors.clone();
            for actor in originals {
                let snake = to_snake_case(&actor.name);
                if snake != actor.name && !existing.contains(&snake) {
                    block.actors.push(rex_ir::ActorDef {
                        name: snake,
                        ..actor
                    });
                }
            }
        }
    }
    models
}

/// One `file: code message` line per diagnostic (checker codes E0104–E0109
/// name the exact token; spans are byte offsets into the keyed file).
/// Warning-severity findings (the lint channel) render identically — the
/// severity lives in the code's W-band, and the caller decides fatal vs
/// advisory.
pub fn render_diags(diags: &[rex_ifml::IfmlDiagnostic]) -> String {
    diags
        .iter()
        .map(|d| format!("  {}:{} {} at byte {}", d.file, d.code, d.message, d.span.0))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    const CLEAN_APP: &str = r#"
view "List" {
    component "grid" {
        type: list;
        data: Item;
        on select(row) -> navigate("Detail", { id: row.id });
    }
}

view "Detail" {
    params { id: Uuid };
    component "info" {
        type: details;
        data: Item;
        on save -> action("RefreshList");
    }
}

action "RefreshList" {
    target: Item;
}
"#;

    #[test]
    fn clean_model_compiles_checks_and_yields_manifest() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = write(dir.path(), "app.ifml", CLEAN_APP);
        let compiled = compile_and_check_ifml(&path).expect("compiles and checks");

        // The manifest is declaration-backed: one route per view, one
        // action, and every edge proven.
        assert_eq!(compiled.manifest.views.len(), 2);
        assert_eq!(compiled.manifest.actions.len(), 1);
        assert_eq!(compiled.manifest.actions[0].name, "RefreshList");
        let nav_edges = compiled
            .manifest
            .edges
            .iter()
            .filter(|e| e.target == "Detail")
            .count();
        assert_eq!(nav_edges, 1);
        // No policy imports — the authorization pass was skipped.
        assert!(compiled.actor_models.is_empty());
        assert!(compiled.policy_imports.is_empty());
        // The fixture invokes every action it declares — the lint channel
        // (W0150 dead action et al) stays empty on a clean model.
        assert!(compiled.lints.is_empty(), "{:?}", compiled.lints);
    }

    #[test]
    fn lint_severity_never_fails_the_compilation() {
        // W0150 (dead action): the declaration is never invoked. The lint
        // channel is warning-severity — the model still compiles, checks,
        // and derives its manifest; the finding rides `lints`.
        let dir = tempfile::TempDir::new().unwrap();
        let path = write(
            dir.path(),
            "app.ifml",
            r#"
view "List" {
    component "grid" {
        type: list;
        data: Item;
        on select(row) -> navigate("Detail", { id: row.id });
    }
}

view "Detail" {
    params { id: Uuid };
    component "info" { type: details; data: Item; }
}

action "Orphaned" {
    target: Item;
}
"#,
        );
        let compiled = compile_and_check_ifml(&path).expect("warnings never fail");
        assert_eq!(compiled.lints.len(), 1, "{:?}", compiled.lints);
        assert!(compiled.lints[0].contains("W0150"), "{:?}", compiled.lints);
        assert!(
            compiled.lints[0].contains("Orphaned"),
            "{:?}",
            compiled.lints
        );
        // The dead action still lands in the declaration-backed manifest.
        assert_eq!(compiled.manifest.actions.len(), 1);
    }

    #[test]
    fn undeclared_action_invocation_is_a_hard_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = write(
            dir.path(),
            "app.ifml",
            r#"
view "List" {
    component "grid" {
        type: list;
        on save -> action("UpdateItem", { on success -> stay; });
    }
}
"#,
        );
        let err = compile_and_check_ifml(&path).expect_err("E0104 must hard-error");
        let rendered = format!("{err}");
        assert!(rendered.contains("E0104"), "{rendered}");
        assert!(rendered.contains("UpdateItem"), "{rendered}");
    }

    #[test]
    fn undeclared_navigate_target_is_a_hard_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = write(
            dir.path(),
            "app.ifml",
            r#"
view "List" {
    component "grid" {
        type: list;
        on select(row) -> navigate("Nowhere");
    }
}
"#,
        );
        let err = compile_and_check_ifml(&path).expect_err("E0105 must hard-error");
        assert!(format!("{err}").contains("E0105"));
    }

    #[test]
    fn requires_and_roles_resolve_against_the_policy_layer() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "catalog.mox",
            "package catalog\n\nclass Item {\n    String title\n}\n",
        );
        write(
            dir.path(),
            "auth.actor",
            "import \"catalog.mox\"\n\nactors Shop {\n    actor Admin\n\n    capability ManageItems on Item\n\n    grant Admin {\n        permit ManageItems\n    }\n}\n",
        );
        let path = write(
            dir.path(),
            "app.ifml",
            r#"
import "auth.actor";

view "Admin" {
    roles: [Admin];
    requires: [ManageItems];
    component "grid" { type: list; }
}

view "Broken" {
    roles: [Ghost];
    requires: [MissingCapability];
    component "grid" { type: list; }
}
"#,
        );
        let err = compile_and_check_ifml(&path).expect_err("E0107/E0108 must hard-error");
        let rendered = format!("{err}");
        assert!(rendered.contains("E0107"), "{rendered}");
        assert!(rendered.contains("MissingCapability"), "{rendered}");
        assert!(rendered.contains("E0108"), "{rendered}");
        assert!(rendered.contains("Ghost"), "{rendered}");
        // The declared names never diagnose.
        assert!(!rendered.contains("ManageItems:"), "{rendered}");
        assert!(!rendered.contains("'Admin'"), "{rendered}");
    }

    #[test]
    fn module_composition_resolves_through_imported_files() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "pager.ifml",
            r#"
module "Pagination" {
    input { page: Int = 1 }
    output { total: Int }

    component "pager" { type: list; }
}
"#,
        );
        let path = write(
            dir.path(),
            "app.ifml",
            r#"
import "pager.ifml";

view "List" {
    use "Pagination" as pager;

    component "grid" { type: list; }
}
"#,
        );
        let compiled = compile_and_check_ifml(&path).expect("module use resolves");
        // The manifest's controls are EXPANDED: the pattern-contributed
        // pager is indistinguishable from app-authored UI.
        assert_eq!(compiled.manifest.controls.len(), 2);
        assert!(
            compiled.manifest.controls.iter().any(|c| c.name == "pager"),
            "expanded control set carries the spliced pager: {:?}",
            compiled.manifest.controls
        );
    }

    #[test]
    fn missing_imported_module_file_is_a_hard_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = write(
            dir.path(),
            "app.ifml",
            "import \"missing.ifml\";\n\nview \"List\" {}\n",
        );
        let err = compile_and_check_ifml(&path).expect_err("unresolvable import must fail");
        assert!(format!("{err}").contains("missing.ifml"), "{err}");
    }

    #[test]
    fn snake_case_view_roles_match_pascal_case_actors() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "catalog.mox",
            "package catalog\n\nclass Item {\n    String title\n}\n",
        );
        write(
            dir.path(),
            "auth.actor",
            "import \"catalog.mox\"\n\nactors Shop {\n    actor Admin\n    actor Member\n\n    capability ManageItems on Item\n\n    grant Admin {\n        permit ManageItems\n    }\n}\n",
        );
        // The crewbase convention (issue #591): the view authors the
        // snake_case form; personas seed both tokens, so the checker must
        // accept it too.
        let path = write(
            dir.path(),
            "app.ifml",
            r#"
import "auth.actor";

view "Board" {
    roles: [member];
    component "grid" { type: list; }
}
"#,
        );
        let compiled = compile_and_check_ifml(&path).expect("snake role alias resolves");
        // The alias twins ride the CHECKING models only — the graph
        // policy ingest walks the policy files themselves
        // (`ingest_actor_imports`), so nothing here reaches the graph.
        let actor_names: std::collections::HashSet<String> = compiled
            .actor_models
            .iter()
            .flat_map(|m| m.blocks.iter())
            .flat_map(|b| b.actors.iter().map(|a| a.name.clone()))
            .collect();
        assert!(
            actor_names.contains("member") && actor_names.contains("Admin"),
            "both the alias and the policy token resolve: {actor_names:?}"
        );
    }

    #[test]
    fn unreadable_file_is_a_hard_error() {
        let err = compile_and_check_ifml(Path::new("/nonexistent/app.ifml"))
            .expect_err("missing file must fail");
        assert!(format!("{err}").contains("could not be read"));
    }
}
