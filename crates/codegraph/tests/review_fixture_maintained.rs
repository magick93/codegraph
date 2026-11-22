//! The review fixture's HAND-MAINTAINED ops-manifest sections (issue #764).
//!
//! The generators still render `crates/review/generated-candidate/codegraph-ops.toml`,
//! but the fixture owns the manifest's environment-specific tables:
//!
//! * `[database.api]` / `[database.e2e]` — the fixture stack's port (54350),
//!   so a MANUAL `npx supabase start` + `testkit api` run is coherent (the
//!   gate retargets the whole `[database.api]` block at its own base target,
//!   so shape B never depends on these values);
//! * `[supabase]` — the stack project spec (the MANUAL full-stack fallback;
//!   the gate itself runs shape B and needs no stack).
//!
//! Every in-place refresh of the fixture must carry those sections forward —
//! otherwise a full test run or the documented maintenance helper silently
//! resets the ports and deletes the stack spec.
//!
//! The drift pin compares the GENERATED half of the manifest
//! ([`normalized_generated_half`]) with comments, blank lines and these
//! sections normalized out, so this module and that rule are two halves of
//! one contract.

/// The manifest tables the fixture maintains by hand, in declaration order.
pub const MAINTAINED_SECTIONS: [&str; 3] = ["[database.api]", "[database.e2e]", "[supabase]"];

fn is_maintained_header(trimmed: &str) -> bool {
    MAINTAINED_SECTIONS.contains(&trimmed)
}

/// Every maintained section block (`[header]` + following lines, without the
/// trailing blank line), in the order they appear in `manifest`.
pub fn maintained_sections(manifest: &str) -> Vec<(String, Vec<String>)> {
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    let mut current: Option<(String, Vec<String>)> = None;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if let Some(section) = current.take() {
                sections.push(section);
            }
            if is_maintained_header(trimmed) {
                current = Some((trimmed.to_string(), vec![line.to_string()]));
            }
            continue;
        }
        if let Some((_, body)) = current.as_mut() {
            body.push(line.to_string());
        }
    }
    if let Some(section) = current.take() {
        sections.push(section);
    }
    sections
        .into_iter()
        .map(|(header, mut body)| {
            while body.last().is_some_and(|line| line.trim().is_empty()) {
                body.pop();
            }
            (header, body)
        })
        .collect()
}

/// The generated half of a maintained manifest: comment lines, blank lines and
/// the maintained sections dropped, everything else verbatim. Pure so the
/// drift-pin comparison rule is unit-testable.
pub fn normalized_generated_half(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_maintained_section = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_maintained_section = is_maintained_header(trimmed);
        }
        if in_maintained_section || trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        out.push(line.to_string());
    }
    out
}

/// Re-apply `previous`'s maintained sections to the freshly generated manifest
/// at `path`: each generated section is replaced by the maintained block, and
/// a section the generator does not emit (`[supabase]`) is appended. No-op
/// when the previous manifest carried none.
pub fn reapply_maintained_sections(path: &std::path::Path, previous: &str) -> Result<(), String> {
    let sections = maintained_sections(previous);
    if sections.is_empty() {
        return Ok(());
    }
    let generated = std::fs::read_to_string(path)
        .map_err(|e| format!("read generated manifest {}: {e}", path.display()))?;

    // Drop the generated copy of every maintained section.
    let mut kept: Vec<String> = Vec::new();
    let mut skip = false;
    for line in generated.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            skip = is_maintained_header(trimmed);
        }
        if !skip {
            kept.push(line.to_string());
        }
    }
    let mut merged = kept.join("\n").trim_end().to_string();
    for (_header, body) in sections {
        merged.push_str("\n\n");
        merged.push_str(body.join("\n").trim_end());
    }
    merged.push('\n');
    std::fs::write(path, merged)
        .map_err(|e| format!("write maintained manifest {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PREVIOUS: &str = "app_name = \"app\"\n\n\
                            [database.api]\nhost = \"localhost\"\nport = 54350\n\n\
                            [database.e2e]\nhost = \"localhost\"\nport = 54350\n\n\
                            [capabilities]\nhas_cli = true\n\n\
                            [supabase]\ndir = \"supabase\"\nhealth_url = \"http://localhost:54351/auth/v1/health\"\n\n\
                            [hurl]\ndir = \"hurl\"\n";

    #[test]
    fn extracts_every_maintained_section() {
        let sections = maintained_sections(PREVIOUS);
        assert_eq!(sections.len(), 3, "{sections:?}");
        assert_eq!(sections[0].0, "[database.api]");
        assert!(sections[0].1.iter().any(|l| l.contains("port = 54350")));
        assert_eq!(sections[1].0, "[database.e2e]");
        assert_eq!(sections[2].0, "[supabase]");
        assert!(sections[2].1.iter().any(|l| l.contains("health_url")));
        assert!(
            !sections
                .iter()
                .flat_map(|(_, body)| body)
                .any(|l| l.contains("[hurl]")),
            "a non-maintained header must end the block"
        );
    }

    #[test]
    fn normalization_drops_comments_and_maintained_sections_only() {
        let generated = normalized_generated_half(PREVIOUS);
        // Everything outside the maintained sections/comments remains.
        assert!(generated.iter().any(|l| l == "app_name = \"app\""));
        assert!(generated.iter().any(|l| l == "has_cli = true"));
        assert!(generated.iter().any(|l| l == "dir = \"hurl\""));
        for line in &generated {
            assert!(!line.contains("port = 54350"), "{line}");
            assert!(!line.contains("health_url"), "{line}");
        }
    }

    #[test]
    fn reapply_replaces_generated_sections_and_appends_supabase() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("codegraph-ops.toml");
        std::fs::write(
            &path,
            "app_name = \"app\"\n\n[database.api]\nhost = \"localhost\"\nport = 5432\n\n\
             [capabilities]\nhas_cli = true\n\n[hurl]\ndir = \"hurl\"\n",
        )
        .expect("write generated");
        reapply_maintained_sections(&path, PREVIOUS).expect("reapply");
        let merged = std::fs::read_to_string(&path).expect("read");
        assert_eq!(merged.matches("[database.api]").count(), 1, "{merged}");
        assert_eq!(merged.matches("[database.e2e]").count(), 1, "{merged}");
        assert_eq!(merged.matches("[supabase]").count(), 1, "{merged}");
        assert!(merged.contains("port = 54350"), "{merged}");
        assert!(!merged.contains("port = 5432\n"), "{merged}");
        assert_eq!(
            normalized_generated_half(&merged),
            normalized_generated_half(PREVIOUS),
            "the generated half must survive unchanged"
        );

        // Idempotent.
        reapply_maintained_sections(&path, PREVIOUS).expect("reapply again");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), merged);
    }
}
