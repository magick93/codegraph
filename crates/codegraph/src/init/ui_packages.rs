//! Doctor check for the vendored UI component packages (issue #18).
//!
//! The generated SvelteKit app imports `@{scope}/ui`, `@{scope}/entities`
//! and `@{scope}/common` (`workspace:*`). `codegraph init` vendors those
//! packages under `packages/`; this check flags a project whose profile
//! scope names packages the workspace does not provide — the class that
//! made every non-crewbase `pnpm install` fail before #18.

use std::path::Path;

/// The packages the generated UI depends on, by directory under `packages/`.
pub const UI_PACKAGES: &[&str] = &["ui", "entities", "common"];

/// One finding per missing or misnamed package, as a printable sentence.
/// Empty when every `packages/{ui,entities,common}/package.json` exists
/// and is named `@{scope}/{dir}`.
pub fn check_ui_packages(root: &Path, scope: &str) -> Vec<String> {
    let mut findings = Vec::new();
    for dir in UI_PACKAGES {
        let expected = format!("@{scope}/{dir}");
        let manifest = root.join("packages").join(dir).join("package.json");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            findings.push(format!(
                "the generated UI depends on `{expected}` but {} does not exist",
                manifest.display()
            ));
            continue;
        };
        let name = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_string));
        match name {
            Some(name) if name == expected => {}
            Some(name) => findings.push(format!(
                "the generated UI depends on `{expected}` but {} is named `{name}`",
                manifest.display()
            )),
            None => findings.push(format!(
                "{} has no readable `name` (the generated UI depends on `{expected}`)",
                manifest.display()
            )),
        }
    }
    findings
}

/// The scope doctor checks: `None` when the plan builds no `ui_scaffold`
/// (no generated UI, nothing to depend on), else the resolved scope —
/// explicit `meta.ui_package_scope` or kebab(`app_name`). An invalid
/// explicit scope is the same hard error the run path raises.
pub fn resolve_doctor_scope(
    plan: &crate::profile::BuildPlan,
    meta: &crate::profile::ProfileMeta,
) -> Result<Option<String>, String> {
    if !plan.has_global_gen("ui_scaffold") {
        return Ok(None);
    }
    let identity = crate::generate::IdentityConfig {
        app_name: meta.app_name.clone().unwrap_or_else(|| "app".into()),
        ui_package_scope: meta.ui_package_scope.clone(),
        ..Default::default()
    };
    let scope = identity.ui_package_scope();
    crate::generate::validate_ui_package_scope(&scope)?;
    Ok(Some(scope))
}

/// Print the PASS line or ONE WARN naming every missing/misnamed package
/// (a consumer whose workspace hosts them elsewhere can ignore it).
/// Returns the soft warnings contributed (0 or 1).
pub fn report(root: &Path, scope: &str) -> usize {
    let findings = check_ui_packages(root, scope);
    if findings.is_empty() {
        println!("PASS ui packages — @{scope}/{{ui,entities,common}} vendored under packages/");
        return 0;
    }
    println!("WARN ui packages — {}", findings.join("; "));
    println!(
        "     hint: `codegraph init --force` re-vendors the starter packages, or set \
         profiles.toml `meta.ui_package_scope` to the scope your workspace provides"
    );
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, dir: &str, name: &str) {
        let pkg = root.join("packages").join(dir);
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(
            pkg.join("package.json"),
            format!("{{\"name\": \"{name}\"}}"),
        )
        .unwrap();
    }

    #[test]
    fn matching_packages_are_quiet() {
        let tmp = tempfile::tempdir().unwrap();
        for dir in UI_PACKAGES {
            write(tmp.path(), dir, &format!("@demo/{dir}"));
        }
        assert!(check_ui_packages(tmp.path(), "demo").is_empty());
    }

    #[test]
    fn missing_and_misnamed_packages_are_named() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "ui", "@crewbase/ui");
        write(tmp.path(), "entities", "@demo/entities");
        let findings = check_ui_packages(tmp.path(), "demo");
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert!(findings[0].contains("`@demo/ui`") && findings[0].contains("`@crewbase/ui`"));
        assert!(findings[1].contains("`@demo/common`") && findings[1].contains("does not exist"));
    }
}
