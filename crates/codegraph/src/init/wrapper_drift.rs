//! Wrapper drift detection (issue #348): the scaffolded `{name}-graph`
//! wrapper is rendered once by `codegraph init` and then evolves only when
//! its consumer project re-renders it. When the driver's `RunArgs` surface
//! moves, existing wrappers lag — a missing field means the wrapper no
//! longer COMPILES against the linked codegraph, and a hardcoded value
//! (`&[]` / `None`) means a CLI flag the binary exposes never reached the
//! wrapper. Doctor diffs every `*-graph/src/main.rs` under the project
//! root against the driver's live field list ([`crate::driver::
//! run_args_field_names`]) and warns per drifted wrapper, naming the CLI
//! flag for each field so re-rendering (`codegraph init --force`) is
//! mechanical.

use std::path::Path;

/// One drifted `RunArgs` field: the struct field and the CLI flag that
/// would wire it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriftedField {
    /// `RunArgs` field name ("ddd_files").
    pub field: String,
    /// Wrapper CLI flag that sets it ("--ddd-files").
    pub flag: String,
}

/// The stamped-vs-live template rev of one wrapper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateRevDrift {
    /// Rev stamped in the wrapper (empty when the const is absent —
    /// pre-#348 wrappers).
    pub stamped: String,
    /// The linked binary's live template hash.
    pub current: String,
}

/// The drift of one wrapper binary against the linked driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapperDrift {
    /// Wrapper crate directory name ("demo-app-graph").
    pub wrapper: String,
    /// Fields the driver has that the wrapper never constructs — the
    /// scaffold would not compile against this codegraph.
    pub missing: Vec<DriftedField>,
    /// Fields constructed as a bare literal (`&[]` / `None`) —
    /// compiles, but no CLI flag reaches them.
    pub hardcoded: Vec<DriftedField>,
    /// Set when the wrapper's stamped template rev differs from this
    /// binary's (or is absent) — the scaffold came from other templates.
    pub template_rev: Option<TemplateRevDrift>,
}

impl WrapperDrift {
    /// True when the wrapper lags the driver in any way.
    pub fn is_drifted(&self) -> bool {
        !self.missing.is_empty() || !self.hardcoded.is_empty() || self.template_rev.is_some()
    }
}

fn flag_for(field: &str) -> String {
    format!("--{}", field.replace('_', "-"))
}

/// Diff one wrapper's `main.rs` source against the driver's live
/// `RunArgs` field list. `wrapper` is the crate directory name used in
/// diagnostics. A wrapper with no `RunArgs` construction at all (pre-driver
/// layout) counts every field as missing — maximally stale.
pub fn wrapper_runargs_drift(wrapper: &str, main_rs: &str) -> WrapperDrift {
    let current_rev = crate::generate::template_engine::project_template_rev();
    let template_rev = match stamped_template_rev(main_rs) {
        Some(stamped) if stamped == current_rev => None,
        Some(stamped) => Some(TemplateRevDrift {
            stamped,
            current: current_rev.to_string(),
        }),
        None => Some(TemplateRevDrift {
            stamped: String::new(),
            current: current_rev.to_string(),
        }),
    };
    let Some(block) = extract_run_args_block(main_rs) else {
        return WrapperDrift {
            wrapper: wrapper.to_string(),
            missing: crate::driver::run_args_field_names()
                .into_iter()
                .map(|field| DriftedField {
                    flag: flag_for(&field),
                    field,
                })
                .collect(),
            hardcoded: Vec::new(),
            template_rev,
        };
    };
    let mut missing = Vec::new();
    let mut hardcoded = Vec::new();
    for field in crate::driver::run_args_field_names() {
        match line_value(&block, &field) {
            None => missing.push(DriftedField {
                flag: flag_for(&field),
                field,
            }),
            Some(value) => {
                if is_hardcoded_literal(value) {
                    hardcoded.push(DriftedField {
                        flag: flag_for(&field),
                        field,
                    });
                }
            }
        }
    }
    WrapperDrift {
        wrapper: wrapper.to_string(),
        missing,
        hardcoded,
        template_rev,
    }
}

/// Scan `root` one level deep for `{name}-graph/src/main.rs` wrapper
/// crates and diff each against the driver. Projects without a wrapper
/// yield nothing (doctor stays quiet for non-scaffold layouts).
pub fn check_wrapper_drift(root: &Path) -> Vec<WrapperDrift> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut wrapper_dirs: Vec<_> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("-graph"))
        })
        .collect();
    wrapper_dirs.sort();
    wrapper_dirs
        .into_iter()
        .filter_map(|dir| {
            let main_rs = std::fs::read_to_string(dir.join("src/main.rs")).ok()?;
            let name = dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            Some(wrapper_runargs_drift(&name, &main_rs))
        })
        .collect()
}

/// Extract the `codegraph::driver::RunArgs { ... }` construction block
/// (brace-balanced) from a wrapper main.rs.
fn extract_run_args_block(main_rs: &str) -> Option<String> {
    let start = main_rs.find("codegraph::driver::RunArgs {")?;
    let brace = main_rs[start..].find('{')? + start;
    let mut depth = 0usize;
    for (i, ch) in main_rs[brace..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(main_rs[brace..=brace + i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// The template rev stamped in a wrapper main.rs, when the const is
/// present (`CODEGRAPH_TEMPLATE_REV: &str = "<hex>"`). Absent on pre-#348
/// wrappers — its own drift signal.
fn stamped_template_rev(main_rs: &str) -> Option<String> {
    let marker = "CODEGRAPH_TEMPLATE_REV: &str = \"";
    let start = main_rs.find(marker)? + marker.len();
    let rest = &main_rs[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// The value expression a field is constructed with, when the wrapper sets
/// it. Accepts the full form (`field: expr`) and Rust field-init shorthand
/// (`field,` — shorthand means the value is a same-named binding, i.e.
/// CLI-wired). Multi-line values (the `codegraph_rev: if …` block) surface
/// as their first line, which is never a bare literal.
fn line_value<'a>(block: &'a str, field: &str) -> Option<&'a str> {
    let full = format!("{field}:");
    let shorthand = format!("{field},");
    block.lines().find_map(|line| {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(&full) {
            Some(rest.trim().trim_end_matches(',').trim())
        } else if line == shorthand {
            Some(line)
        } else {
            None
        }
    })
}

/// A bare literal value means the field is hardcoded rather than wired to
/// a CLI flag: empty slices and `None` are the no-flag defaults the issue
/// calls out (`ifml_framework: &[]`, `ifml_components: None`, …), and a
/// string literal pins a value a flag should own. Boolean literals are
/// deliberately NOT flagged — `check: false` can be a project's conscious
/// opt-out. Shorthand (`field,`) never reaches here: its value is the
/// same-named binding, i.e. CLI-wired.
fn is_hardcoded_literal(value: &str) -> bool {
    matches!(value, "&[]" | "None")
        || (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorthand_fields_are_cli_wired() {
        let block = "RunArgs {\n    no_post_gen,\n    check: false,\n}";
        assert_eq!(line_value(block, "no_post_gen"), Some("no_post_gen,"));
        assert!(!is_hardcoded_literal("no_post_gen,"));
        // Boolean literals are a conscious opt-out, never flagged.
        assert_eq!(line_value(block, "check"), Some("false"));
        assert!(!is_hardcoded_literal("false"));
    }

    #[test]
    fn flag_naming_is_kebab() {
        assert_eq!(flag_for("ddd_files"), "--ddd-files");
        assert_eq!(flag_for("ifml_design_system"), "--ifml-design-system");
    }

    #[test]
    fn hardcoded_values_are_detected() {
        let block = "RunArgs {\n    ifml_framework: &[],\n    ux_rules: None,\n    variant: variant.as_deref(),\n}";
        assert!(is_hardcoded_literal(
            line_value(block, "ifml_framework").unwrap()
        ));
        assert!(is_hardcoded_literal(line_value(block, "ux_rules").unwrap()));
        assert!(!is_hardcoded_literal(line_value(block, "variant").unwrap()));
    }

    #[test]
    fn stamped_template_rev_extraction() {
        let source = "const CODEGRAPH_TEMPLATE_REV: &str = \"abc123\";\nfn main() {}";
        assert_eq!(stamped_template_rev(source).as_deref(), Some("abc123"));
        assert_eq!(stamped_template_rev("fn main() {}"), None);
    }

    #[test]
    fn missing_template_rev_const_is_drift() {
        let drift = wrapper_runargs_drift("demo-app-graph", "fn main() {}");
        let rev = drift.template_rev.expect("absent const must drift");
        assert!(rev.stamped.is_empty());
        assert_eq!(
            rev.current,
            crate::generate::template_engine::project_template_rev()
        );
    }
}
