//! Wizard auto-detection heuristics for HR entity UI generation.
//!
//! Analyses entity relationships to determine which entities should get
//! multi-step wizards and what steps those wizards should have.

use codegraph_config::config::UiEntityEntry;
use serde::Serialize;

/// Info about a child entity, gathered from the graph.
#[derive(Debug, Clone)]
pub struct ChildInfo {
    pub name: String,
    pub relationship: String,     // "one-to-one" | "one-to-many"
    pub is_tightly_coupled: bool, // only referenced by one parent
    pub has_own_children: bool,   // has children of its own (depth 2+)
}

/// A wizard step definition for the descriptor.
#[derive(Debug, Clone, Serialize)]
pub struct WizardStepDef {
    pub key: String,
    pub label: String,
    pub source: String, // "self" | "child" | "summary"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cardinality: Option<String>, // "one" | "many"
}

/// Result of wizard detection for an entity.
#[derive(Debug, Clone, Serialize)]
pub struct WizardCandidate {
    pub enabled: bool,
    pub steps: Vec<WizardStepDef>,
}

/// Determine if an entity should have a wizard.
///
/// Heuristics:
/// - Entities with 2+ tightly-coupled children are candidates
/// - Entities with children that have their own children (depth 2+) are candidates
/// - ui-domains.toml can force enable or disable
///
/// Returns `None` if the entity should NOT have a wizard.
pub fn detect_wizard_candidate(
    entity_name: &str,
    children: &[ChildInfo],
    field_groups: &[String],
    ui_override: Option<&UiEntityEntry>,
) -> Option<WizardCandidate> {
    // 1. Check explicit UI override first.
    if let Some(entry) = ui_override {
        match entry.wizard {
            Some(false) => return None,
            Some(true) => {
                let steps = match &entry.wizard_config {
                    Some(cfg) if !cfg.steps.is_empty() => {
                        build_explicit_steps(&cfg.steps, children, field_groups)
                    }
                    _ => auto_derive_steps(entity_name, children, field_groups),
                };
                return Some(WizardCandidate {
                    enabled: true,
                    steps,
                });
            }
            None => {} // fall through to heuristics
        }
    }

    // 2. Heuristic: count tightly-coupled children.
    let tightly_coupled: Vec<&ChildInfo> =
        children.iter().filter(|c| c.is_tightly_coupled).collect();

    let qualifies =
        tightly_coupled.len() >= 2 || tightly_coupled.iter().any(|c| c.has_own_children);

    if !qualifies {
        return None;
    }

    // 3. Auto-derive steps.
    let steps = auto_derive_steps(entity_name, children, field_groups);
    Some(WizardCandidate {
        enabled: true,
        steps,
    })
}

/// Build wizard steps automatically from entity structure.
///
/// Produces: one "self" step with all field groups, one step per tightly-coupled
/// child, then one "summary" step.
pub fn auto_derive_steps(
    entity_name: &str,
    children: &[ChildInfo],
    field_groups: &[String],
) -> Vec<WizardStepDef> {
    let mut steps = Vec::new();

    // "self" step — covers the entity's own fields.
    steps.push(WizardStepDef {
        key: "basics".into(),
        label: format!("{} Details", humanize(&snake_case(entity_name))),
        source: "self".into(),
        groups: if field_groups.is_empty() {
            None
        } else {
            Some(field_groups.to_vec())
        },
        child: None,
        cardinality: None,
    });

    // One step per tightly-coupled child.
    for child in children.iter().filter(|c| c.is_tightly_coupled) {
        let child_key = snake_case(&child.name);
        let cardinality = if child.relationship == "one-to-many" {
            "many"
        } else {
            "one"
        };
        steps.push(WizardStepDef {
            key: child_key.clone(),
            label: humanize(&child_key),
            source: "child".into(),
            groups: None,
            child: Some(child.name.clone()),
            cardinality: Some(cardinality.into()),
        });
    }

    // "summary" step.
    steps.push(WizardStepDef {
        key: "summary".into(),
        label: "Review & Submit".into(),
        source: "summary".into(),
        groups: None,
        child: None,
        cardinality: None,
    });

    steps
}

/// Map explicit step key names to step definitions.
///
/// Keys that look like a child entity's snake_case name are resolved to "child"
/// source steps. Keys named "summary" or "review" become "summary" steps.
/// Everything else becomes a "self" step.
pub fn build_explicit_steps(
    step_keys: &[String],
    children: &[ChildInfo],
    field_groups: &[String],
) -> Vec<WizardStepDef> {
    step_keys
        .iter()
        .map(|key| {
            // Check if key matches a child name (by snake_case comparison).
            if let Some(child) = children.iter().find(|c| {
                snake_case(&c.name) == *key || c.name.to_lowercase() == key.to_lowercase()
            }) {
                let cardinality = if child.relationship == "one-to-many" {
                    "many"
                } else {
                    "one"
                };
                WizardStepDef {
                    key: key.clone(),
                    label: humanize(key),
                    source: "child".into(),
                    groups: None,
                    child: Some(child.name.clone()),
                    cardinality: Some(cardinality.into()),
                }
            } else if key == "summary" || key == "review" {
                WizardStepDef {
                    key: key.clone(),
                    label: humanize(key),
                    source: "summary".into(),
                    groups: None,
                    child: None,
                    cardinality: None,
                }
            } else {
                WizardStepDef {
                    key: key.clone(),
                    label: humanize(key),
                    source: "self".into(),
                    groups: if field_groups.is_empty() {
                        None
                    } else {
                        Some(field_groups.to_vec())
                    },
                    child: None,
                    cardinality: None,
                }
            }
        })
        .collect()
}

/// Convert PascalCase to snake_case.
pub fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(ch.to_lowercase().next().unwrap());
    }
    out
}

/// Convert snake_case to Title Case.
pub fn humanize(name: &str) -> String {
    name.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_config::config::{UiEntityEntry, UiWizardConfig};

    #[test]
    fn test_entity_with_required_children_is_candidate() {
        let children = vec![
            ChildInfo {
                name: "PositionProfile".into(),
                relationship: "one-to-many".into(),
                is_tightly_coupled: true,
                has_own_children: false,
            },
            ChildInfo {
                name: "PositionCompetency".into(),
                relationship: "one-to-many".into(),
                is_tightly_coupled: true,
                has_own_children: false,
            },
        ];
        let groups = vec!["default".into(), "dates".into()];
        let result = detect_wizard_candidate("PositionOpening", &children, &groups, None);
        assert!(result.is_some());
        let candidate = result.unwrap();
        assert!(candidate.enabled);
        assert!(candidate.steps.len() >= 3); // self + children + summary
    }

    #[test]
    fn test_leaf_entity_is_not_candidate() {
        let result = detect_wizard_candidate("SimpleEntity", &[], &["default".into()], None);
        assert!(result.is_none());
    }

    #[test]
    fn test_single_loosely_coupled_child_not_candidate() {
        let children = vec![ChildInfo {
            name: "SharedType".into(),
            relationship: "one-to-many".into(),
            is_tightly_coupled: false,
            has_own_children: false,
        }];
        let result = detect_wizard_candidate("ParentEntity", &children, &["default".into()], None);
        assert!(result.is_none());
    }

    #[test]
    fn test_ui_domains_force_enable() {
        let ui_override = UiEntityEntry {
            wizard: Some(true),
            wizard_config: None,
        };
        let result =
            detect_wizard_candidate("ForcedEntity", &[], &["default".into()], Some(&ui_override));
        assert!(result.is_some());
        assert!(result.unwrap().enabled);
    }

    #[test]
    fn test_ui_domains_force_disable() {
        let children = vec![
            ChildInfo {
                name: "Child".into(),
                relationship: "one-to-many".into(),
                is_tightly_coupled: true,
                has_own_children: false,
            },
            ChildInfo {
                name: "Child2".into(),
                relationship: "one-to-many".into(),
                is_tightly_coupled: true,
                has_own_children: false,
            },
        ];
        let ui_override = UiEntityEntry {
            wizard: Some(false),
            wizard_config: None,
        };
        let result = detect_wizard_candidate(
            "DisabledEntity",
            &children,
            &["default".into()],
            Some(&ui_override),
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_ui_domains_explicit_steps() {
        let children = vec![ChildInfo {
            name: "Profile".into(),
            relationship: "one-to-many".into(),
            is_tightly_coupled: true,
            has_own_children: false,
        }];
        let ui_override = UiEntityEntry {
            wizard: Some(true),
            wizard_config: Some(UiWizardConfig {
                steps: vec!["basics".into(), "profile".into(), "review".into()],
            }),
        };
        let result = detect_wizard_candidate(
            "CustomSteps",
            &children,
            &["default".into(), "dates".into()],
            Some(&ui_override),
        );
        assert!(result.is_some());
        let candidate = result.unwrap();
        assert_eq!(candidate.steps.len(), 3);
        assert_eq!(candidate.steps[0].key, "basics");
        assert_eq!(candidate.steps[2].key, "review");
    }

    #[test]
    fn test_deep_nesting_is_candidate() {
        // One tightly coupled child with its own children = candidate.
        let children = vec![ChildInfo {
            name: "DeepChild".into(),
            relationship: "one-to-many".into(),
            is_tightly_coupled: true,
            has_own_children: true,
        }];
        let result = detect_wizard_candidate("DeepParent", &children, &["default".into()], None);
        assert!(result.is_some());
    }

    #[test]
    fn test_snake_case() {
        assert_eq!(snake_case("PositionProfile"), "position_profile");
        assert_eq!(snake_case("SimpleEntity"), "simple_entity");
        assert_eq!(snake_case("A"), "a");
    }

    #[test]
    fn test_humanize() {
        assert_eq!(humanize("position_profile"), "Position Profile");
        assert_eq!(humanize("default"), "Default");
    }
}
