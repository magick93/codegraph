//! UX rules configuration plane (ux-rules epic, phase 1).
//!
//! A ux-rules TOML document (project file or the built-in pack) declares
//! presentation rules keyed by SELECTORS. Rules resolve **first-match-wins**
//! over their selector tier, and merging prepends project rules ahead of
//! pack rules (`merge`), so a project rule shadows the pack wholesale for
//! the selectors it covers.
//!
//! Parsing is strict: unknown keys and unknown enum variants are errors,
//! every validation error names the offending TOML location (`[[column]] #N`,
//! 1-based counting of the array-of-tables blocks in the file) and carries a
//! trailing `hint:` line where a fix exists. Non-fatal findings (e.g.
//! duplicate selectors) come back as warnings on [`ParsedUxRules::warnings`].

mod dimension;
mod presentation;
mod rule;

use std::path::Path;

use thiserror::Error;

pub use dimension::Dimension;
pub use presentation::{
    Align, Display, FormatConfig, ToneMap, VALID_TONE_VALUES, WORKFLOW_TONE_FALLBACK,
};
pub use rule::{ActionRules, CollectionRule, ColumnRule, UxRules, glob_match};

/// The built-in `ux-default` pack: the default behavior contract shipped
/// with codegraph (precedent: `BUILT_IN_PACKS` in `ifml_components.rs`).
/// The file's header comment doubles as the key reference / user example.
pub const BUILT_IN_UX_PACK: &str = include_str!("packs/ux_default.toml");

/// Strict parse failure for a ux-rules TOML document.
///
/// Every variant's `Display` names the offending location and, where a fix
/// exists, carries a trailing `hint: ...` line (message shape mirrored from
/// `codegraph-ops` error hints — no dependency).
#[derive(Debug, Error)]
pub enum UxParseError {
    /// TOML syntax or shape error, prefixed with the array-of-tables
    /// section the span resolves to (`[[column]] #2: ...`), or `ux-rules:`
    /// when the error sits outside any rule array.
    #[error("{location}: {message}")]
    Toml {
        /// Human-facing location prefix (`[[column]] #2` or `ux-rules`).
        location: String,
        /// Underlying serde/toml message (names the field and valid values).
        message: String,
        /// The wrapped toml error (boxed — `toml::de::Error` is large and
        /// would trip `clippy::result_large_err` on `Result<_, UxParseError>`).
        #[source]
        source: Box<toml::de::Error>,
    },
    /// The `--ux-rules` file could not be read.
    #[error(
        "failed to read ux-rules file {path}: {source}\n\
         hint: check the path — copy the built-in ux-default pack \
         (crates/codegraph-config/src/ux/packs/ux_default.toml) as a starting point"
    )]
    Io {
        /// The unreadable path, as given.
        path: String,
        /// The IO error.
        #[source]
        source: std::io::Error,
    },
    /// A `[[collection]]` rule opted into timeline rendering without a sort key.
    #[error(
        "[[collection]] #{index}: `display = \"timeline\"` requires `order_by`\n\
         hint: set order_by = \"<datetime property>\"; the timeline sorts by it"
    )]
    TimelineRequiresOrderBy {
        /// 1-based index of the offending `[[collection]]` block.
        index: usize,
    },
    /// A `[[collection]]` rule named a display outside the closed set.
    #[error(
        "[[collection]] #{index}: unknown display \"{display}\"; expected \"table\" or \"timeline\"\n\
         hint: \"table\" is the default; \"timeline\" opts into chronological rendering and requires `order_by`"
    )]
    UnknownCollectionDisplay {
        /// 1-based index of the offending `[[collection]]` block.
        index: usize,
        /// The unknown display value as authored.
        display: String,
    },
    /// `[actions] inline_max = 0` would hide every row action.
    #[error(
        "[actions]: `inline_max` must be at least 1; 0 would hide every action\n\
         hint: inline_max = 1 keeps one action inline and collapses the rest into a menu"
    )]
    InlineMaxZero,
    /// An empty glob matches nothing and is always an authoring mistake.
    #[error(
        "[[{kind}]] #{index}: `{field}` glob must not be empty\n\
         hint: {hint}"
    )]
    EmptyGlob {
        /// Which rule array (`column` or `collection`).
        kind: &'static str,
        /// 1-based index of the offending block.
        index: usize,
        /// The selector key carrying the empty glob.
        field: &'static str,
        /// A concrete-pattern suggestion for this field.
        hint: &'static str,
    },
    /// A match-everything glob on a column selector would silently shadow
    /// every other rule (collections keep bare `*` legal — coarse opt-in
    /// by design).
    #[error(
        "[[column]] #{index}: `name_pattern = \"*\"` matches every column and would silently shadow every other rule\n\
         hint: select on `dimension` instead (one of {})",
        Dimension::all_kebab_names().join(", ")
    )]
    BareStarNamePattern {
        /// 1-based index of the offending `[[column]]` block.
        index: usize,
    },
    /// A `tone` value outside the badge-variant set (chips render variants,
    /// not free CSS).
    #[error(
        "[[column]] #{index}: unknown tone value \"{value}\" for \"{keyword}\"; \
         expected one of {}\n\
         hint: tones render as badge variants, not free CSS",
        VALID_TONE_VALUES
            .iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(", ")
    )]
    UnknownToneValue {
        /// 1-based index of the offending `[[column]]` block.
        index: usize,
        /// The status keyword whose tone is wrong.
        keyword: String,
        /// The invalid tone value as authored.
        value: String,
    },
}

/// Parsed ux-rules plus non-fatal advisories (warnings never fail the parse).
#[derive(Debug)]
pub struct ParsedUxRules {
    /// The parsed rule set.
    pub rules: UxRules,
    /// Authoring-mistake advisories (e.g. duplicate selectors) — printed,
    /// never fatal.
    pub warnings: Vec<String>,
}

/// Parse a ux-rules TOML document.
///
/// Validation matrix (strict — errors, in check order):
/// - unknown keys / unknown enum variants (serde, `deny_unknown_fields`),
///   located to their `[[column]] #N` / `[[collection]] #N` section
/// - empty `name_pattern` / `entity_pattern` globs
/// - match-everything (`*`-only) `name_pattern` globs on `[[column]]`
/// - `tone` values outside [`VALID_TONE_VALUES`]
/// - `display = "timeline"` without `order_by`
/// - unknown `[[collection]] display` values
/// - `[actions] inline_max = 0`
///
/// Warnings (non-fatal, on [`ParsedUxRules::warnings`]):
/// - duplicate identical selectors (first wins — likely an authoring mistake)
/// - `order_by` on a non-timeline collection (no effect)
pub fn parse_ux_rules_str(input: &str) -> Result<ParsedUxRules, UxParseError> {
    let rules: UxRules = toml::from_str(input).map_err(|source| {
        let message = {
            let msg = source.message();
            if msg.is_empty() {
                source.to_string()
            } else {
                msg.to_string()
            }
        };
        let location = source
            .span()
            .and_then(|span| line_of_byte_offset(input, span.start))
            .and_then(|line| array_section_at(input, line))
            .unwrap_or_else(|| "ux-rules".to_string());
        UxParseError::Toml {
            location,
            message,
            source: Box::new(source),
        }
    })?;

    let mut warnings = Vec::new();

    for (idx, column) in rules.columns.iter().enumerate() {
        let index = idx + 1;
        if let Some(pattern) = column.name_pattern.as_deref() {
            if pattern.trim().is_empty() {
                return Err(UxParseError::EmptyGlob {
                    kind: "column",
                    index,
                    field: "name_pattern",
                    hint: "name a concrete pattern, e.g. \"*_amount\" or \"status\"",
                });
            }
            if pattern.chars().all(|c| c == '*') {
                return Err(UxParseError::BareStarNamePattern { index });
            }
        }
        if let Some(tone) = &column.tone {
            for (keyword, value) in &tone.0 {
                if !VALID_TONE_VALUES.contains(&value.as_str()) {
                    return Err(UxParseError::UnknownToneValue {
                        index,
                        keyword: keyword.clone(),
                        value: value.clone(),
                    });
                }
            }
        }
    }

    for (idx, collection) in rules.collections.iter().enumerate() {
        let index = idx + 1;
        if let Some(pattern) = collection.entity_pattern.as_deref()
            && pattern.trim().is_empty()
        {
            return Err(UxParseError::EmptyGlob {
                kind: "collection",
                index,
                field: "entity_pattern",
                hint: "name a concrete pattern, e.g. \"refund*\" (or \"*\" for every entity)",
            });
        }
        match collection.display.as_deref() {
            Some("timeline") => {
                if collection.order_by.is_none() {
                    return Err(UxParseError::TimelineRequiresOrderBy { index });
                }
            }
            Some("table") | None => {}
            Some(other) => {
                return Err(UxParseError::UnknownCollectionDisplay {
                    index,
                    display: other.to_string(),
                });
            }
        }
        if collection.display.as_deref() != Some("timeline") && collection.order_by.is_some() {
            warnings.push(format!(
                "[[collection]] #{index}: `order_by` has no effect without `display = \"timeline\"`"
            ));
        }
    }

    if rules.actions.inline_max == 0 {
        return Err(UxParseError::InlineMaxZero);
    }

    warn_duplicate_selectors(&rules, &mut warnings);

    Ok(ParsedUxRules { rules, warnings })
}

/// Map a byte offset to its 1-based line number.
fn line_of_byte_offset(input: &str, offset: usize) -> Option<usize> {
    if offset > input.len() {
        return None;
    }
    Some(input[..offset].bytes().filter(|b| *b == b'\n').count() + 1)
}

/// The array-of-tables section covering `line` (1-based), as a location
/// label like `[[column]] #2` (1-based block counting). Table arrays are
/// attributed by their most recent header at or before `line`; any other
/// table header (`[format]`, `[actions]`, `[column.tone]`) stays attached
/// to its parent rule array or resets the attribution.
fn array_section_at(input: &str, line: usize) -> Option<String> {
    let mut column_count = 0usize;
    let mut collection_count = 0usize;
    let mut current: Option<String> = None;
    for (idx, raw) in input.lines().enumerate() {
        if idx + 1 > line {
            break;
        }
        let trimmed = raw.trim().replace(' ', "");
        if trimmed == "[[column]]" {
            column_count += 1;
            current = Some(format!("[[column]] #{column_count}"));
        } else if trimmed == "[[collection]]" {
            collection_count += 1;
            current = Some(format!("[[collection]] #{collection_count}"));
        } else if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let nested = trimmed.starts_with("[column.")
                || trimmed.starts_with("[[column.")
                || trimmed.starts_with("[collection.")
                || trimmed.starts_with("[[collection.");
            if !nested {
                current = None;
            }
        }
    }
    current
}

/// Emit a warning for every rule whose selector set is IDENTICAL to an
/// earlier rule's (first wins; likely an authoring mistake). Only
/// intra-document duplicates warn — project-over-pack shadowing is the
/// merge design, not a mistake.
fn warn_duplicate_selectors(rules: &UxRules, warnings: &mut Vec<String>) {
    let mut seen_columns = std::collections::HashSet::new();
    for (idx, column) in rules.columns.iter().enumerate() {
        if !seen_columns.insert(column.selector_key()) {
            warnings.push(format!(
                "[[column]] #{}: duplicate selector {{{}}}; first wins",
                idx + 1,
                column_selector_summary(column)
            ));
        }
    }
    let mut seen_collections = std::collections::HashSet::new();
    for (idx, collection) in rules.collections.iter().enumerate() {
        if !seen_collections.insert(collection.selector_key()) {
            warnings.push(format!(
                "[[collection]] #{}: duplicate selector {{{}}}; first wins",
                idx + 1,
                collection_selector_summary(collection)
            ));
        }
    }
}

fn column_selector_summary(rule: &ColumnRule) -> String {
    let mut parts = Vec::new();
    if let Some(dimension) = rule.dimension {
        parts.push(format!("dimension = \"{}\"", dimension.as_str()));
    }
    if let Some(classification) = &rule.classification {
        parts.push(format!("classification = \"{classification}\""));
    }
    if let Some(pg_type) = &rule.pg_type {
        parts.push(format!("pg_type = \"{pg_type}\""));
    }
    if let Some(name_pattern) = &rule.name_pattern {
        parts.push(format!("name_pattern = \"{name_pattern}\""));
    }
    parts.join(", ")
}

fn collection_selector_summary(rule: &CollectionRule) -> String {
    let mut parts = Vec::new();
    if let Some(entity_pattern) = &rule.entity_pattern {
        parts.push(format!("entity_pattern = \"{entity_pattern}\""));
    }
    if let Some(display) = &rule.display {
        parts.push(format!("display = \"{display}\""));
    }
    parts.join(", ")
}

/// Parse the embedded `ux-default` pack.
///
/// The pack is compiled into the binary; a parse failure is an internal
/// error that cannot surface in production (pinned by tests). The pack
/// flows through the same parse-path validation as project files.
pub fn builtin_ux_rules() -> Result<ParsedUxRules, UxParseError> {
    parse_ux_rules_str(BUILT_IN_UX_PACK)
}

/// Read and parse a project ux-rules TOML file.
pub fn load_ux_rules(path: &Path) -> Result<ParsedUxRules, UxParseError> {
    let raw = std::fs::read_to_string(path).map_err(|source| UxParseError::Io {
        path: path.display().to_string(),
        source,
    })?;
    parse_ux_rules_str(&raw)
}

/// Merge project ux-rules over the built-in pack (precedent:
/// `merge_with_pack` in `ifml_components.rs`).
///
/// - `[format]`: project wins field-wise — a non-empty project `locale`
///   wins, and the project `currency` Option replaces the pack's outright.
/// - `[[column]]` / `[[collection]]` rule arrays: project rules PREPEND, so
///   first-match-wins resolution shadows the pack per selector tier while
///   pack rules stay reachable for selectors the project does not cover.
/// - `[actions]`: the project `inline_max` wins; project `confirm` entries
///   win when non-empty, otherwise the pack's defaults stay in effect.
pub fn merge(project: &UxRules, pack: &UxRules) -> UxRules {
    UxRules {
        format: merge_format(&project.format, &pack.format),
        columns: project
            .columns
            .iter()
            .chain(pack.columns.iter())
            .cloned()
            .collect(),
        collections: project
            .collections
            .iter()
            .chain(pack.collections.iter())
            .cloned()
            .collect(),
        actions: ActionRules {
            inline_max: project.actions.inline_max,
            confirm: if project.actions.confirm.is_empty() {
                pack.actions.confirm.clone()
            } else {
                project.actions.confirm.clone()
            },
        },
    }
}

fn merge_format(project: &FormatConfig, pack: &FormatConfig) -> FormatConfig {
    FormatConfig {
        locale: if project.locale.is_empty() {
            pack.locale.clone()
        } else {
            project.locale.clone()
        },
        currency: project.currency.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> ParsedUxRules {
        parse_ux_rules_str(input).unwrap()
    }

    const FULL_EXAMPLE: &str = r#"
[format]
locale = "en-NZ"
currency = "NZD"

[[column]]
dimension = "money"
display = "chip"
align = "right"
sortable = true

[column.tone]
active = "default"
approved = "default"
pending = "secondary"
draft = "secondary"

[[column]]
name_pattern = "*_amount"
display = "copy-chip"
align = "right"

[[column]]
classification = "codelist"
display = "chip"

[[column]]
pg_type = "bool"
dimension = "flag"

[[collection]]
entity_pattern = "refund*"
display = "timeline"
order_by = "created_at"
title_field = "reference"
preview = ["status", "total_amount"]

[[collection]]
display = "table"

[actions]
inline_max = 2
confirm = ["refund", "void"]
"#;

    #[test]
    fn full_example_parses_with_expected_values() {
        let parsed = parse(FULL_EXAMPLE);
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        let rules = &parsed.rules;
        assert_eq!(rules.format.locale, "en-NZ");
        assert_eq!(rules.format.currency.as_deref(), Some("NZD"));
        assert_eq!(rules.columns.len(), 4);
        assert_eq!(rules.columns[0].dimension, Some(Dimension::Money));
        assert_eq!(rules.columns[0].display, Some(Display::Chip));
        assert_eq!(rules.columns[0].align, Some(Align::Right));
        assert_eq!(rules.columns[0].sortable, Some(true));
        let tone = rules.columns[0].tone.as_ref().unwrap();
        assert_eq!(tone.lookup("active"), "default");
        assert_eq!(tone.lookup("draft"), "secondary");
        assert_eq!(rules.columns[1].name_pattern.as_deref(), Some("*_amount"));
        assert_eq!(rules.columns[1].display, Some(Display::CopyChip));
        assert_eq!(rules.columns[2].classification.as_deref(), Some("codelist"));
        assert_eq!(rules.columns[3].pg_type.as_deref(), Some("bool"));
        assert_eq!(rules.columns[3].dimension, Some(Dimension::Flag));
        assert_eq!(rules.collections.len(), 2);
        assert_eq!(
            rules.collections[0].entity_pattern.as_deref(),
            Some("refund*")
        );
        assert_eq!(rules.collections[0].display.as_deref(), Some("timeline"));
        assert_eq!(rules.collections[0].order_by.as_deref(), Some("created_at"));
        assert_eq!(
            rules.collections[0].title_field.as_deref(),
            Some("reference")
        );
        assert_eq!(
            rules.collections[0].preview,
            vec!["status".to_string(), "total_amount".to_string()]
        );
        assert_eq!(rules.collections[1].display.as_deref(), Some("table"));
        assert_eq!(rules.actions.inline_max, 2);
        assert_eq!(
            rules.actions.confirm,
            vec!["refund".to_string(), "void".to_string()]
        );
    }

    #[test]
    fn empty_document_takes_defaults() {
        let parsed = parse("");
        assert!(parsed.warnings.is_empty());
        let rules = parsed.rules;
        assert_eq!(rules.format, FormatConfig::default());
        assert_eq!(rules.format.locale, "en-NZ");
        assert!(rules.format.currency.is_none());
        assert!(rules.columns.is_empty());
        assert!(rules.collections.is_empty());
        assert_eq!(rules.actions.inline_max, 1);
        assert!(rules.actions.confirm.is_empty());
    }

    /// The consolidated strictness matrix: every hard-error case asserts
    /// error MESSAGE CONTENT (location, offending value, valid values),
    /// not just `is_err`.
    #[test]
    fn strictness_matrix_asserts_error_message_content() {
        struct Case {
            name: &'static str,
            input: &'static str,
            expect: &'static [&'static str],
        }
        let cases = vec![
            Case {
                name: "unknown dimension variant",
                input: "[[column]]\ndimension = \"amunt\"\n",
                expect: &["[[column]] #1", "unknown variant", "money"],
            },
            Case {
                name: "unknown column key",
                input: "[[column]]\ndisply = \"chip\"\n",
                expect: &["[[column]] #1", "unknown field", "disply"],
            },
            Case {
                name: "unknown root key",
                input: "coluns = []\n",
                expect: &["ux-rules", "unknown field", "coluns"],
            },
            Case {
                name: "timeline without order_by",
                input: "[[collection]]\ndisplay = \"timeline\"\n",
                expect: &["[[collection]] #1", "order_by", "hint:"],
            },
            Case {
                name: "unknown collection display",
                input: "[[collection]]\ndisplay = \"kanban\"\n",
                expect: &["[[collection]] #1", "kanban", "table", "timeline"],
            },
            Case {
                name: "inline_max = 0",
                input: "[actions]\ninline_max = 0\n",
                expect: &["[actions]", "inline_max", "at least 1"],
            },
            Case {
                name: "empty name_pattern glob",
                input: "[[column]]\nname_pattern = \"\"\n",
                expect: &["[[column]] #1", "name_pattern", "must not be empty"],
            },
            Case {
                name: "empty entity_pattern glob",
                input: "[[collection]]\nentity_pattern = \"  \"\n",
                expect: &["[[collection]] #1", "entity_pattern", "must not be empty"],
            },
            Case {
                name: "bare-star name_pattern glob",
                input: "[[column]]\nname_pattern = \"*\"\n",
                expect: &[
                    "[[column]] #1",
                    "name_pattern",
                    "every column",
                    "dimension",
                    "hint:",
                ],
            },
            Case {
                name: "unknown tone value",
                input: "[[column]]\ndimension = \"status-category\"\n\n[column.tone]\nfailed = \"busted\"\n",
                expect: &[
                    "[[column]] #1",
                    "busted",
                    "failed",
                    "\"default\"",
                    "\"secondary\"",
                    "\"destructive\"",
                    "\"outline\"",
                    "hint:",
                ],
            },
        ];
        for case in cases {
            let err = parse_ux_rules_str(case.input)
                .expect_err(format!("case '{}' must fail the parse", case.name).as_str());
            let message = err.to_string();
            for needle in case.expect {
                assert!(
                    message.contains(needle),
                    "case '{}': message {message:?} must contain {needle:?}",
                    case.name
                );
            }
        }
    }

    #[test]
    fn toml_errors_carry_the_offending_section_index() {
        // Typo in the SECOND [[column]] block → "#2", not "#1".
        let err = parse_ux_rules_str(
            "[[column]]\ndimension = \"money\"\n\n[[column]]\ndisply = \"chip\"\n",
        )
        .expect_err("second column typo must fail");
        assert!(err.to_string().contains("[[column]] #2"), "{err}");

        // A [actions]-section error is NOT attributed to a rule array.
        let err =
            parse_ux_rules_str("[actions]\ninline_maax = 2\n").expect_err("actions typo must fail");
        let message = err.to_string();
        assert!(message.contains("inline_maax"), "{message}");
        assert!(!message.contains("[[column]]"), "{message}");
        assert!(!message.contains("[[collection]]"), "{message}");

        // A column error after a collection block stays column-attributed.
        let err = parse_ux_rules_str(
            "[[collection]]\nentity_pattern = \"refund*\"\n\n[[column]]\nnmae_pattern = \"x\"\n",
        )
        .expect_err("column typo must fail");
        assert!(err.to_string().contains("[[column]] #1"), "{err}");
    }

    #[test]
    fn bare_star_entity_pattern_stays_legal() {
        // Collection opt-in is coarse by design: "*" is allowed there.
        let parsed = parse("[[collection]]\nentity_pattern = \"*\"\n");
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        assert_eq!(
            parsed.rules.collections[0].entity_pattern.as_deref(),
            Some("*")
        );
    }

    #[test]
    fn all_valid_tone_values_parse() {
        let parsed = parse(
            "[[column]]\ndimension = \"status-category\"\n\n[column.tone]\na = \"default\"\nb = \"secondary\"\nc = \"destructive\"\nd = \"outline\"\n",
        );
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        assert_eq!(VALID_TONE_VALUES.len(), 4);
    }

    #[test]
    fn duplicate_column_selector_warns_first_wins() {
        let parsed = parse(
            "[[column]]\ndimension = \"money\"\nalign = \"right\"\n\n[[column]]\ndimension = \"money\"\ndisplay = \"chip\"\n",
        );
        assert_eq!(parsed.warnings.len(), 1, "{:?}", parsed.warnings);
        let warning = &parsed.warnings[0];
        assert!(warning.contains("[[column]] #2"), "{warning}");
        assert!(warning.contains("duplicate selector"), "{warning}");
        assert!(warning.contains("first wins"), "{warning}");
        assert!(warning.contains("dimension = \"money\""), "{warning}");
    }

    #[test]
    fn duplicate_collection_selector_warns_first_wins() {
        let parsed = parse(
            "[[collection]]\nentity_pattern = \"refund*\"\ndisplay = \"timeline\"\norder_by = \"created_at\"\n\n[[collection]]\nentity_pattern = \"refund*\"\ndisplay = \"timeline\"\norder_by = \"title\"\n",
        );
        assert_eq!(parsed.warnings.len(), 1, "{:?}", parsed.warnings);
        let warning = &parsed.warnings[0];
        assert!(warning.contains("[[collection]] #2"), "{warning}");
        assert!(warning.contains("first wins"), "{warning}");
    }

    #[test]
    fn distinct_selectors_do_not_warn() {
        // Same dimension but an extra name_pattern selector → different tier.
        let parsed = parse(
            "[[column]]\ndimension = \"money\"\n\n[[column]]\ndimension = \"money\"\nname_pattern = \"*_amount\"\n",
        );
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        // Same entity_pattern but a different display → different pair.
        let parsed = parse(
            "[[collection]]\nentity_pattern = \"refund*\"\ndisplay = \"table\"\n\n[[collection]]\nentity_pattern = \"refund*\"\ndisplay = \"timeline\"\norder_by = \"created_at\"\n",
        );
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    }

    #[test]
    fn unknown_preview_field_parses_deferred_to_generation() {
        // Config can't see the graph: `preview` names resolve at generation
        // time, so an unknown field is a generation-time diagnostic, NOT a
        // parse error.
        let parsed = parse(
            "[[collection]]\nentity_pattern = \"refund*\"\npreview = [\"not_a_real_field\"]\n",
        );
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        assert_eq!(
            parsed.rules.collections[0].preview,
            vec!["not_a_real_field".to_string()]
        );
    }

    #[test]
    fn order_by_without_timeline_warns() {
        let parsed = parse("[[collection]]\ndisplay = \"table\"\norder_by = \"created_at\"\n");
        assert_eq!(parsed.warnings.len(), 1, "{:?}", parsed.warnings);
        assert!(parsed.warnings[0].contains("[[collection]] #1"));
        assert!(parsed.warnings[0].contains("order_by"));
    }

    #[test]
    fn order_by_without_display_warns() {
        let parsed = parse("[[collection]]\norder_by = \"created_at\"\n");
        assert_eq!(parsed.warnings.len(), 1);
    }

    #[test]
    fn builtin_pack_parses_with_expected_defaults() {
        let parsed = builtin_ux_rules().expect("embedded ux-default pack must parse");
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        let rules = &parsed.rules;
        assert_eq!(rules.format.locale, "en-NZ");
        assert_eq!(rules.format.currency.as_deref(), Some("NZD"));
        // One default rule per dimension; time-point pins align only.
        assert_eq!(rules.columns.len(), 7, "{:?}", rules.columns);
        let align_of = |dim: Dimension| {
            rules
                .columns
                .iter()
                .find(|c| c.dimension == Some(dim))
                .unwrap_or_else(|| panic!("pack must cover {dim:?}"))
        };
        assert_eq!(align_of(Dimension::Quantity).align, Some(Align::Right));
        assert_eq!(align_of(Dimension::Money).align, Some(Align::Right));
        assert_eq!(align_of(Dimension::TimePoint).align, Some(Align::Left));
        assert_eq!(
            align_of(Dimension::StatusCategory).display,
            Some(Display::Chip)
        );
        assert_eq!(
            align_of(Dimension::Identifier).display,
            Some(Display::CopyChip)
        );
        assert_eq!(align_of(Dimension::Reference).display, Some(Display::Link));
        assert_eq!(align_of(Dimension::Flag).display, Some(Display::Chip));
        // Timeline is strictly opt-in: no [[collection]] rules in the pack.
        assert!(rules.collections.is_empty());
        assert_eq!(rules.actions.inline_max, 1);
        assert_eq!(rules.actions.confirm, vec!["delete".to_string()]);
    }

    #[test]
    fn merge_prepends_project_rules_so_they_shadow_the_pack() {
        let pack = builtin_ux_rules().unwrap().rules;
        let project = parse(
            r#"
[[column]]
dimension = "money"
display = "raw"
align = "left"

[[column]]
name_pattern = "*_code"
display = "copy-chip"
"#,
        )
        .rules;
        let merged = merge(&project, &pack);

        // Project rules come first: the money rule shadows the pack's.
        assert_eq!(merged.columns[0].dimension, Some(Dimension::Money));
        assert_eq!(merged.columns[0].display, Some(Display::Raw));
        assert_eq!(merged.columns[0].align, Some(Align::Left));
        // Pack rules stay reachable behind the project's.
        assert!(merged.columns.len() > project.columns.len());
        assert!(
            merged
                .columns
                .iter()
                .any(|c| c.dimension == Some(Dimension::Identifier)
                    && c.display == Some(Display::CopyChip))
        );
        // Collections pass through likewise.
        let with_collection = parse("[[collection]]\nentity_pattern = \"refund*\"\n").rules;
        let merged2 = merge(&with_collection, &pack);
        assert_eq!(merged2.collections.len(), pack.collections.len() + 1);
        assert_eq!(
            merged2.collections[0].entity_pattern.as_deref(),
            Some("refund*")
        );
    }

    #[test]
    fn merge_format_project_wins_field_wise() {
        let pack = builtin_ux_rules().unwrap().rules;
        let project = parse(
            "[format]\nlocale = \"de-DE\"\ncurrency = \"EUR\"\n\n[actions]\ninline_max = 3\nconfirm = [\"void\"]\n",
        )
        .rules;
        let merged = merge(&project, &pack);
        assert_eq!(merged.format.locale, "de-DE");
        assert_eq!(merged.format.currency.as_deref(), Some("EUR"));
        assert_eq!(merged.actions.inline_max, 3);
        assert_eq!(merged.actions.confirm, vec!["void".to_string()]);
    }

    #[test]
    fn merge_falls_back_to_pack_for_untouched_fields() {
        let pack = builtin_ux_rules().unwrap().rules;
        // Project sets locale only: currency Option replaces (stays None =
        // no currency formatting), confirm is untouched so the pack's
        // delete-confirmation default survives.
        let project = parse("[format]\nlocale = \"de-DE\"\n").rules;
        let merged = merge(&project, &pack);
        assert_eq!(merged.format.locale, "de-DE");
        assert_eq!(merged.format.currency, None);
        assert_eq!(merged.actions.confirm, vec!["delete".to_string()]);
    }

    #[test]
    fn load_ux_rules_reads_file_and_wraps_io_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ux-rules.toml");
        std::fs::write(&path, "[actions]\ninline_max = 2\n").unwrap();
        let parsed = load_ux_rules(&path).unwrap();
        assert_eq!(parsed.rules.actions.inline_max, 2);

        let missing = dir.path().join("missing.toml");
        let err = load_ux_rules(&missing).expect_err("missing file must fail");
        let message = err.to_string();
        assert!(message.contains("missing.toml"), "{message}");
        // Io errors carry a remediation hint (ops-style).
        assert!(message.contains("hint:"), "{message}");
    }
}
