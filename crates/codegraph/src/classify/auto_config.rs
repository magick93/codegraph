use std::collections::{HashMap, HashSet};

use super::DomainClassificationResult;
use codegraph_core::types::ParentCandidate;
use codegraph_naming::to_kebab_case;

/// Validate parent candidates for ambiguous parents and circular chains.
///
/// Returns an error if any child has multiple distinct parents (ambiguous) or if
/// the parent relationships form a cycle (circular chain).
pub fn validate_parent_candidates(candidates: &[ParentCandidate]) -> crate::error::Result<()> {
    // Check ambiguous parents
    let mut child_parents: HashMap<&str, Vec<&str>> = HashMap::new();
    for c in candidates {
        child_parents
            .entry(&c.child_title)
            .or_default()
            .push(&c.parent_title);
    }
    for (child, parents) in &child_parents {
        let unique: HashSet<&&str> = parents.iter().collect();
        if unique.len() > 1 {
            return Err(crate::error::Error::Validation(format!(
                "ambiguous parent for '{}': detected parents {:?}. Resolve manually in domains.toml entity_config.",
                child,
                unique
            )));
        }
    }

    // Check circular chains
    let parent_map: HashMap<&str, &str> = candidates
        .iter()
        .map(|c| (c.child_title.as_str(), c.parent_title.as_str()))
        .collect();
    for start in parent_map.keys() {
        let mut visited = HashSet::new();
        let mut current = *start;
        while let Some(&parent) = parent_map.get(current) {
            if !visited.insert(current) {
                return Err(crate::error::Error::Validation(format!(
                    "circular parent chain detected involving '{}'",
                    current
                )));
            }
            current = parent;
        }
    }
    Ok(())
}

/// Emit `domains.auto.toml` content from classification results.
pub fn emit_auto_config(
    results: &[DomainClassificationResult],
    parent_candidates: &[ParentCandidate],
) -> String {
    if let Err(e) = validate_parent_candidates(parent_candidates) {
        tracing::warn!("Parent candidate validation: {e}");
    }
    let mut output = String::new();
    output.push_str("# =============================================================\n");
    output.push_str("# AUTO-GENERATED — do not edit manually\n");
    output.push_str("#\n");
    output.push_str("# This file is computed by `hr-graph classify` from graph analysis.\n");
    output.push_str("# To override any classification or config:\n");
    output
        .push_str("#   1. Add the type to force_entities / force_value_objects in domains.toml\n");
    output.push_str("#   2. Add entity_config blocks in domains.toml for custom path_segment,\n");
    output.push_str("#      tag, workflow, or parent-child overrides\n");
    output.push_str("#\n");
    output.push_str("# Manual entity_config in domains.toml takes precedence over this file.\n");
    output.push_str("# Re-run `hr-graph classify` to regenerate after schema changes.\n");
    output.push_str("# =============================================================\n\n");

    for result in results {
        let entity_names: Vec<&str> = result.entities.iter().map(|e| e.title.as_str()).collect();
        let vo_names: Vec<&str> = result
            .value_objects
            .iter()
            .map(|v| v.title.as_str())
            .collect();

        output.push_str(&format!("[domains.{}.auto_entities]\n", result.domain));
        output.push_str(&format!("entities = {:?}\n", entity_names));
        output.push_str(&format!("value_objects = {:?}\n\n", vo_names));

        for score in &result.entities {
            let segment = derive_path_segment(&score.title);
            output.push_str(&format!(
                "[domains.{}.auto_entity_config.{}]\n",
                result.domain, score.title
            ));

            // Check if this entity is a child of another
            let parent = parent_candidates
                .iter()
                .find(|pc| pc.child_title == score.title);
            if let Some(pc) = parent {
                output.push_str("role = \"child\"\n");
                output.push_str(&format!("parent = \"{}\"\n", pc.parent_title));
                output.push_str(&format!("parent_ref = \"{}\"\n", pc.field_name));
            } else {
                output.push_str("role = \"root\"\n");
            }

            output.push_str(&format!("path_segment = \"{}\"\n", segment));
            output.push_str(&format!("tag = \"{}\"\n", result.domain));
            output.push_str(&format!(
                "# score = {}, reason = \"{}\"\n\n",
                score.net_score,
                score.reasons.join(", ")
            ));
        }
    }

    output
}

/// Derive a pluralized kebab-case path segment from a type name.
/// `PlanSetupType` → `plan-setups`, `CandidateType` → `candidates`
pub fn derive_path_segment(type_name: &str) -> String {
    // Special cases for irregular pluralization
    let specials: &[(&str, &str)] = &[
        ("WellnessType", "wellness-records"),
        ("StatusType", "statuses"),
    ];
    for (name, segment) in specials {
        if type_name == *name {
            return segment.to_string();
        }
    }

    // Strip "Type" suffix
    let base = type_name.strip_suffix("Type").unwrap_or(type_name);
    let kebab = to_kebab_case(base);

    // Simple pluralization
    if kebab.ends_with('s') || kebab.ends_with("ss") {
        format!("{}es", kebab)
    } else if kebab.ends_with('y')
        && !kebab.ends_with("ey")
        && !kebab.ends_with("ay")
        && !kebab.ends_with("oy")
    {
        format!("{}ies", &kebab[..kebab.len() - 1])
    } else {
        format!("{}s", kebab)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::scoring::{AutoClassification, ClassificationScore};
    use codegraph_core::types::DetectionSource;

    #[test]
    fn test_rejects_circular_parent_chain() {
        let candidates = vec![
            ParentCandidate {
                child_title: "A".into(),
                parent_title: "B".into(),
                field_name: "b_id".into(),
                source: DetectionSource::ScalarRef,
            },
            ParentCandidate {
                child_title: "B".into(),
                parent_title: "A".into(),
                field_name: "a_id".into(),
                source: DetectionSource::ScalarRef,
            },
        ];
        let result = validate_parent_candidates(&candidates);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("circular"));
    }

    #[test]
    fn test_warns_ambiguous_parents() {
        let candidates = vec![
            ParentCandidate {
                child_title: "C".into(),
                parent_title: "A".into(),
                field_name: "a_id".into(),
                source: DetectionSource::ArrayItems,
            },
            ParentCandidate {
                child_title: "C".into(),
                parent_title: "B".into(),
                field_name: "b_id".into(),
                source: DetectionSource::ArrayItems,
            },
        ];
        let result = validate_parent_candidates(&candidates);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("ambiguous"));
    }

    #[test]
    fn path_segment_from_type_name() {
        assert_eq!(derive_path_segment("PlanSetupType"), "plan-setups");
        assert_eq!(derive_path_segment("CandidateType"), "candidates");
        assert_eq!(derive_path_segment("PersonType"), "persons");
        assert_eq!(derive_path_segment("WellnessType"), "wellness-records");
        assert_eq!(derive_path_segment("TimecardType"), "timecards");
    }

    #[test]
    fn auto_config_contains_entities() {
        let results = vec![DomainClassificationResult {
            domain: "benefits".to_string(),
            entities: vec![ClassificationScore {
                title: "PlanSetupType".to_string(),
                domain: Some("benefits".to_string()),
                entity_score: 6,
                vo_score: 0,
                net_score: 6,
                classification: AutoClassification::Entity,
                reasons: vec!["in_degree=4 (+3 entity)".to_string()],
            }],
            value_objects: vec![],
            excluded: vec![],
        }];
        let output = emit_auto_config(&results, &[]);
        assert!(output.contains("[domains.benefits.auto_entities]"));
        assert!(output.contains("PlanSetupType"));
        assert!(output.contains("plan-setups"));
    }

    #[test]
    fn child_entity_emits_parent_config() {
        let results = vec![DomainClassificationResult {
            domain: "benefits".to_string(),
            entities: vec![
                ClassificationScore {
                    title: "EnrollmentType".to_string(),
                    domain: Some("benefits".to_string()),
                    entity_score: 5,
                    vo_score: 0,
                    net_score: 5,
                    classification: AutoClassification::Entity,
                    reasons: vec![],
                },
                ClassificationScore {
                    title: "ElectionType".to_string(),
                    domain: Some("benefits".to_string()),
                    entity_score: 4,
                    vo_score: 0,
                    net_score: 4,
                    classification: AutoClassification::Entity,
                    reasons: vec![],
                },
            ],
            value_objects: vec![],
            excluded: vec![],
        }];
        let parents = vec![ParentCandidate {
            child_title: "ElectionType".to_string(),
            parent_title: "EnrollmentType".to_string(),
            field_name: "enrollment_type_id".to_string(),
            source: codegraph_core::types::DetectionSource::ScalarRef,
        }];
        let output = emit_auto_config(&results, &parents);
        assert!(output.contains("role = \"child\""));
        assert!(output.contains("parent = \"EnrollmentType\""));
    }

    #[test]
    fn test_valid_parent_candidates_pass() {
        let candidates = vec![
            ParentCandidate {
                child_title: "B".into(),
                parent_title: "A".into(),
                field_name: "a_id".into(),
                source: DetectionSource::ScalarRef,
            },
            ParentCandidate {
                child_title: "C".into(),
                parent_title: "A".into(),
                field_name: "a_id".into(),
                source: DetectionSource::ArrayItems,
            },
        ];
        // Multiple children of same parent is fine
        assert!(validate_parent_candidates(&candidates).is_ok());
    }

    #[test]
    fn test_empty_candidates_pass() {
        assert!(validate_parent_candidates(&[]).is_ok());
    }

    #[test]
    fn test_three_level_chain_valid() {
        let candidates = vec![
            ParentCandidate {
                child_title: "B".into(),
                parent_title: "A".into(),
                field_name: "a_id".into(),
                source: DetectionSource::ScalarRef,
            },
            ParentCandidate {
                child_title: "C".into(),
                parent_title: "B".into(),
                field_name: "b_id".into(),
                source: DetectionSource::ScalarRef,
            },
        ];
        // Linear chain is valid (not circular)
        assert!(validate_parent_candidates(&candidates).is_ok());
    }
}
