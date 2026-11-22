//! The review fixture's HAND-MAINTAINED ops-manifest section (issue #764).
//!
//! The generators still render `crates/review/generated-candidate/codegraph-ops.toml`,
//! but the fixture owns one section the generator does not emit: `[supabase]`
//! (the MANUAL full-stack fallback for `review_api_suite`; the gate itself
//! runs shape B and needs no stack). Every in-place refresh of the fixture
//! must carry that section forward — otherwise a full test run or the
//! documented maintenance helper silently deletes it, and the next manual
//! `testkit api` run falls back to the stack-startup path with no project.
//!
//! The drift pin compares the generated half of the manifest with comments
//! and the maintained section normalized out, so this module and that rule
//! are two halves of one contract.

/// The maintained `[supabase]` table block verbatim (with its trailing
/// newline), or `None` when the manifest carries none.
pub fn maintained_supabase_section(manifest: &str) -> Option<String> {
    let mut lines = Vec::new();
    let mut in_section = false;
    for line in manifest.lines() {
        if line.trim_start().starts_with('[') {
            in_section = line.trim() == "[supabase]";
        }
        if in_section {
            lines.push(line);
        }
    }
    (!lines.is_empty()).then(|| format!("{}\n", lines.join("\n")))
}

/// Re-apply `previous`'s maintained `[supabase]` section to the freshly
/// generated manifest at `path` (no-op when the previous manifest had none or
/// the generated one already carries the section).
pub fn reapply_maintained_manifest_section(
    path: &std::path::Path,
    previous: &str,
) -> Result<(), String> {
    let Some(section) = maintained_supabase_section(previous) else {
        return Ok(());
    };
    let generated = std::fs::read_to_string(path)
        .map_err(|e| format!("read generated manifest {}: {e}", path.display()))?;
    if generated.lines().any(|line| line.trim() == "[supabase]") {
        return Ok(());
    }
    let merged = format!("{}\n{}", generated.trim_end(), section);
    std::fs::write(path, merged)
        .map_err(|e| format!("write maintained manifest {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_only_the_supabase_table() {
        let manifest = "app_name = \"app\"\n\n[capabilities]\nhas_cli = true\n\n\
                        [supabase]\ndir = \"supabase\"\nhealth_url = \"http://localhost:54351/auth/v1/health\"\n\n\
                        [hurl]\ndir = \"hurl\"\n";
        let section = maintained_supabase_section(manifest).expect("section");
        assert!(section.starts_with("[supabase]\n"), "{section:?}");
        assert!(section.contains("health_url"), "{section:?}");
        assert!(!section.contains("[hurl]"), "{section:?}");
        assert!(!section.contains("app_name"), "{section:?}");
    }

    #[test]
    fn absent_section_is_none() {
        assert!(maintained_supabase_section("app_name = \"app\"\n").is_none());
    }

    #[test]
    fn reapply_appends_the_section_once() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("codegraph-ops.toml");
        std::fs::write(&path, "app_name = \"app\"\n\n[capabilities]\nhas_cli = true\n")
            .expect("write generated");
        let previous = "app_name = \"app\"\n\n[supabase]\ndir = \"supabase\"\n\n[capabilities]\nhas_cli = true\n";
        reapply_maintained_manifest_section(&path, previous).expect("reapply");
        let merged = std::fs::read_to_string(&path).expect("read");
        assert_eq!(merged.matches("[supabase]").count(), 1, "{merged}");
        assert!(merged.contains("dir = \"supabase\""), "{merged}");

        // Idempotent: a second pass is a no-op.
        reapply_maintained_manifest_section(&path, previous).expect("reapply again");
        let again = std::fs::read_to_string(&path).expect("read");
        assert_eq!(again, merged);
    }
}
