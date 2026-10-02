//! UX plan diagnostics (ux-rules epic, phase 2 — issue #296).
//!
//! Advisory findings about a built [`UxPlan`]: heuristic guesses worth
//! pinning with an explicit rule (money), collection shapes the author
//! might want to opt into (timeline), and action-budget accounting. All
//! findings are WARNINGS — nothing here ever changes a plan, and nothing
//! prints: the caller renders [`report`] lines through its existing
//! warning channel.

use codegraph_config::ux::{Dimension, UxRules, glob_match};

use super::dimension::infer_dimension_with_hints;
use super::plan::{CollectionPlan, UxPlan, UxPlanInput, resolve_column};

/// Advisory findings for one entity's UX plan.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct UxDiagnostics {
    /// Row actions that fell into the overflow menu
    /// (`[actions] inline_max` budget).
    pub moved_to_menu: usize,
    /// Entities where the data looks timeline-shaped but renders as a
    /// table — an opt-in suggestion, never an auto-switch. Each entry is a
    /// full human-readable sentence naming the candidate field.
    pub timeline_suggestions: Vec<String>,
    /// Money-keyword inference hints (from #295), skipping columns whose
    /// dimension a matching rule pinned (the author already made the call).
    pub money_hints: Vec<String>,
}

/// Collect diagnostics for a built plan.
///
/// Re-runs Pass-1 inference over the same inputs the plan was built from,
/// so findings and plan always agree by construction.
pub fn collect_diagnostics(
    rules: &UxRules,
    input: &UxPlanInput<'_>,
    plan: &UxPlan,
) -> UxDiagnostics {
    let mut diagnostics = UxDiagnostics {
        moved_to_menu: plan.actions.menu.len(),
        ..UxDiagnostics::default()
    };

    for field in input.fields {
        let prop = input.prop_by_name.get(field.name.as_str()).copied();
        let resolution = resolve_column(rules, prop, field, input.workflow_status_field);
        if resolution.dimension_pinned {
            // A matching rule pinned the dimension: the hint is stale —
            // the author already declared intent.
            continue;
        }
        diagnostics.money_hints.extend(resolution.hints.into_vec());
    }

    if let Some(field_name) = timeline_suggestion_field(rules, input, plan) {
        diagnostics
            .timeline_suggestions
            .push(timeline_suggestion_text(input.entity_title, &field_name));
    }

    diagnostics
}

/// The timeline candidate: when the plan renders a table, the entity has
/// at least one inferred TimePoint field, a created_at/updated_at/due_*/*_at
/// field exists, and NO collection rule matched the entity (an explicit
/// table rule is a deliberate opt-out — no suggestion noise). Returns the
/// first field that is BOTH timeline-named and TimePoint-inferred, falling
/// back to the first timeline-named field.
fn timeline_suggestion_field(
    rules: &UxRules,
    input: &UxPlanInput<'_>,
    plan: &UxPlan,
) -> Option<String> {
    if !matches!(plan.collection, CollectionPlan::Table) {
        return None;
    }
    let rule_matched = rules.collections.iter().any(|rule| {
        rule.entity_pattern
            .as_deref()
            .is_none_or(|pattern| glob_match(pattern, input.entity_title))
    });
    if rule_matched {
        return None;
    }

    let mut named = None;
    let mut has_time_point = false;
    for field in input.fields {
        let prop = input.prop_by_name.get(field.name.as_str()).copied();
        let (inferred, _) = infer_dimension_with_hints(prop, field, input.workflow_status_field);
        if inferred == Dimension::TimePoint {
            has_time_point = true;
        }
        let lower = field.name.to_lowercase();
        let name_shape = lower == "created_at"
            || lower == "updated_at"
            || lower.starts_with("due_")
            || lower.ends_with("_at");
        if name_shape && named.is_none() {
            named = Some(field.name.clone());
            if inferred == Dimension::TimePoint {
                // A TimePoint-shaped candidate beats a later one.
                break;
            }
        }
    }

    if has_time_point { named } else { None }
}

/// The human-readable opt-in suggestion (naming the candidate field and
/// the exact rule to add). The pattern keeps the entity title's case —
/// `glob_match` is case-sensitive.
fn timeline_suggestion_text(entity_title: &str, field_name: &str) -> String {
    let pattern = format!("{entity_title}*");
    format!(
        "entity {entity_title:?} renders as a table but has time-ordered data \
         (field {field_name:?}); to opt into timeline rendering add:\n\
         [[collection]]\nentity_pattern = \"{pattern}\"\ndisplay = \"timeline\"\n\
         order_by = \"{field_name}\"\
         \n(tables never auto-switch)"
    )
}

/// Render diagnostics as human-readable warning lines (the caller prints
/// them; this module never writes anywhere).
pub fn report(diag: &UxDiagnostics) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(diag.money_hints.iter().cloned());
    lines.extend(diag.timeline_suggestions.iter().cloned());
    if diag.moved_to_menu > 0 {
        lines.push(format!(
            "{} row action(s) collapsed into the overflow menu ([actions] inline_max)",
            diag.moved_to_menu
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use codegraph_config::{builtin_ux_rules, parse_ux_rules_str};

    use super::super::plan::{CollectionPlan, UxPlanInput, build_ux_plan};
    use super::*;
    use crate::ui::page::UiField;

    fn field(name: &str, pg_type: &str, ts_type: &str, input_type: &str) -> UiField {
        UiField {
            name: name.to_string(),
            label: String::new(),
            ts_type: ts_type.to_string(),
            input_type: input_type.to_string(),
            is_required: false,
            is_array: false,
            is_entity_ref: false,
            is_immutable: false,
            is_codelist: false,
            is_range: false,
            codelist_values: vec![],
            description: String::new(),
            pg_type: pg_type.to_string(),
            open_end: false,
            ref_api_path: None,
            structured_sub_fields: vec![],
            nested_type_name: None,
        }
    }

    fn input<'a>(entity_title: &'a str, fields: &'a [UiField]) -> UxPlanInput<'a> {
        UxPlanInput {
            entity_title,
            fields,
            prop_by_name: BTreeMap::new(),
            workflow_status_field: None,
            workflow_terminal_states: &[],
            has_soft_delete: false,
            user_pinned_list_order: false,
        }
    }

    #[test]
    fn money_hints_fire_and_rule_pin_skips_them() {
        let fields = vec![field("total_amount", "NUMERIC(10,2)", "string", "number")];
        let pack = builtin_ux_rules().unwrap().rules;
        let plan = build_ux_plan(Some(&pack), &input("Refund", &fields))
            .unwrap()
            .unwrap();
        let diag = collect_diagnostics(&pack, &input("Refund", &fields), &plan);
        assert_eq!(diag.money_hints.len(), 1, "{:?}", diag.money_hints);
        assert!(diag.money_hints[0].contains("total_amount"));

        // Pinning the dimension via a matching name-pattern rule skips it.
        let pinned = parse_ux_rules_str(
            "[[column]]\nname_pattern = \"*_amount\"\ndimension = \"quantity\"\n",
        )
        .unwrap()
        .rules;
        let plan = build_ux_plan(Some(&pinned), &input("Refund", &fields))
            .unwrap()
            .unwrap();
        let diag = collect_diagnostics(&pinned, &input("Refund", &fields), &plan);
        assert!(diag.money_hints.is_empty(), "{:?}", diag.money_hints);
    }

    #[test]
    fn timeline_suggestion_fires_for_default_tables_with_time_fields() {
        let pack = builtin_ux_rules().unwrap().rules;
        let fields = vec![
            field("reference", "TEXT", "string", "text"),
            field("created_at", "TIMESTAMPTZ", "string", "datetime-local"),
        ];
        let plan = build_ux_plan(Some(&pack), &input("Refund", &fields))
            .unwrap()
            .unwrap();
        assert_eq!(plan.collection, CollectionPlan::Table);

        let diag = collect_diagnostics(&pack, &input("Refund", &fields), &plan);
        assert_eq!(
            diag.timeline_suggestions.len(),
            1,
            "{:?}",
            diag.timeline_suggestions
        );
        let suggestion = &diag.timeline_suggestions[0];
        assert!(suggestion.contains("Refund"), "{suggestion}");
        assert!(suggestion.contains("created_at"), "{suggestion}");
        assert!(suggestion.contains("timeline"), "{suggestion}");
        assert!(
            suggestion.contains("entity_pattern = \"Refund*\""),
            "{suggestion}"
        );
        assert_eq!(plan.collection, CollectionPlan::Table);
    }

    #[test]
    fn timeline_suggestion_quiet_without_time_fields_or_on_opt_out() {
        let pack = builtin_ux_rules().unwrap().rules;

        // No TimePoint field: quiet.
        let fields = vec![field("reference", "TEXT", "string", "text")];
        let plan = build_ux_plan(Some(&pack), &input("Refund", &fields))
            .unwrap()
            .unwrap();
        let diag = collect_diagnostics(&pack, &input("Refund", &fields), &plan);
        assert!(diag.timeline_suggestions.is_empty());

        // TimePoint present but no *_at-shaped name: quiet.
        let fields = vec![field("window", "DATERANGE", "string", "date-range")];
        let plan = build_ux_plan(Some(&pack), &input("Refund", &fields))
            .unwrap()
            .unwrap();
        let diag = collect_diagnostics(&pack, &input("Refund", &fields), &plan);
        assert!(diag.timeline_suggestions.is_empty());

        // Timeline already opted in: quiet.
        let rules = parse_ux_rules_str(
            "[[collection]]\nentity_pattern = \"Refund*\"\ndisplay = \"timeline\"\norder_by = \"created_at\"\n",
        )
        .unwrap()
        .rules;
        let fields = vec![field(
            "created_at",
            "TIMESTAMPTZ",
            "string",
            "datetime-local",
        )];
        let plan = build_ux_plan(Some(&rules), &input("Refund", &fields))
            .unwrap()
            .unwrap();
        let diag = collect_diagnostics(&rules, &input("Refund", &fields), &plan);
        assert!(diag.timeline_suggestions.is_empty());

        // Explicit table rule (deliberate opt-out): quiet.
        let rules = parse_ux_rules_str(
            "[[collection]]\nentity_pattern = \"Refund*\"\ndisplay = \"table\"\n",
        )
        .unwrap()
        .rules;
        let fields = vec![field(
            "created_at",
            "TIMESTAMPTZ",
            "string",
            "datetime-local",
        )];
        let plan = build_ux_plan(Some(&rules), &input("Refund", &fields))
            .unwrap()
            .unwrap();
        let diag = collect_diagnostics(&rules, &input("Refund", &fields), &plan);
        assert!(
            diag.timeline_suggestions.is_empty(),
            "{:?}",
            diag.timeline_suggestions
        );
    }

    #[test]
    fn moved_to_menu_counts_overflow_actions() {
        let pack = builtin_ux_rules().unwrap().rules;
        let plan = build_ux_plan(Some(&pack), &input("Refund", &[]))
            .unwrap()
            .unwrap();
        let diag = collect_diagnostics(&pack, &input("Refund", &[]), &plan);
        assert_eq!(diag.moved_to_menu, 2);
    }

    #[test]
    fn report_renders_lines_and_stays_quiet_when_clean() {
        let diag = UxDiagnostics::default();
        assert!(report(&diag).is_empty(), "a clean plan reports nothing");

        let diag = UxDiagnostics {
            moved_to_menu: 2,
            timeline_suggestions: vec!["timeline suggestion line".to_string()],
            money_hints: vec!["money hint line".to_string()],
        };
        let lines = report(&diag);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "money hint line");
        assert_eq!(lines[1], "timeline suggestion line");
        assert!(lines[2].contains("2 row action(s)"), "{}", lines[2]);
        assert!(lines[2].contains("inline_max"), "{}", lines[2]);
    }
}
