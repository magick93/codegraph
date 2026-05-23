use codegraph_classifier::config::NamingRule;
use std::collections::HashMap;

/// Apply domain naming rules to adjust a classification score.
/// Returns additional VO score and reason, or None if no rule matches.
pub fn apply_naming_rules(
    title: &str,
    rules: &HashMap<String, NamingRule>,
) -> Option<NamingRuleResult> {
    for (pattern, rule) in rules {
        if title.contains(pattern.as_str()) {
            let is_hard = matches!(rule.rule_type, codegraph_classifier::config::NamingRuleType::Hard);
            let type_label = if is_hard { "hard" } else { "soft" };
            return Some(NamingRuleResult {
                vo_score: rule.score,
                is_hard,
                reason: format!("naming:{type_label}:{pattern}"),
            });
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamingRuleResult {
    pub vo_score: i32,
    pub is_hard: bool,
    pub reason: String,
}

/// Check if a schema should be excluded entirely based on its path.
pub fn should_exclude_by_path(rel_path: &str) -> bool {
    let normalized = rel_path.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').collect();
    parts
        .iter()
        .any(|p| matches!(*p, "meta" | "search" | "samples"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_classifier::config::{NamingRule, NamingRuleType};

    fn test_rules() -> HashMap<String, NamingRule> {
        let mut rules = HashMap::new();
        rules.insert(
            "Inclusion".to_string(),
            NamingRule {
                score: 5,
                rule_type: NamingRuleType::Hard,
            },
        );
        rules.insert(
            "Report".to_string(),
            NamingRule {
                score: 3,
                rule_type: NamingRuleType::Soft,
            },
        );
        rules.insert(
            "Notification".to_string(),
            NamingRule {
                score: 3,
                rule_type: NamingRuleType::Soft,
            },
        );
        rules.insert(
            "Vendor".to_string(),
            NamingRule {
                score: 3,
                rule_type: NamingRuleType::Soft,
            },
        );
        rules.insert(
            "Message".to_string(),
            NamingRule {
                score: 3,
                rule_type: NamingRuleType::Soft,
            },
        );
        rules
    }

    #[test]
    fn inclusion_types_are_hard_vo() {
        let rules = test_rules();
        let result = apply_naming_rules("DataProtectionPolicyInclusion", &rules);
        let r = result.unwrap();
        assert!(r.is_hard);
        assert_eq!(r.vo_score, 5);
    }

    #[test]
    fn report_types_are_soft_vo() {
        let rules = test_rules();
        let result = apply_naming_rules("WorkerCompensationReportType", &rules);
        let r = result.unwrap();
        assert!(!r.is_hard);
        assert_eq!(r.vo_score, 3);
    }

    #[test]
    fn notification_types_are_soft_vo() {
        let rules = test_rules();
        let result = apply_naming_rules("StatusNotificationType", &rules);
        let r = result.unwrap();
        assert!(!r.is_hard);
        assert_eq!(r.vo_score, 3);
    }

    #[test]
    fn message_types_are_soft_vo() {
        let rules = test_rules();
        let result = apply_naming_rules("ScreeningVendorMessageType", &rules);
        let r = result.unwrap();
        assert!(!r.is_hard);
        assert_eq!(r.vo_score, 3);
    }

    #[test]
    fn vendor_types_are_soft_vo() {
        let rules = test_rules();
        let result = apply_naming_rules("ScreeningVendorOrderType", &rules);
        let r = result.unwrap();
        assert!(!r.is_hard);
        assert_eq!(r.vo_score, 3);
    }

    #[test]
    fn normal_entity_no_rule() {
        let rules = test_rules();
        let result = apply_naming_rules("PersonType", &rules);
        assert!(result.is_none());
    }

    #[test]
    fn empty_rules_returns_none() {
        let rules = HashMap::new();
        let result = apply_naming_rules("DataProtectionPolicyInclusion", &rules);
        assert!(result.is_none());
    }

    #[test]
    fn meta_path_excluded() {
        assert!(should_exclude_by_path("common/json/meta/hros.json"));
    }

    #[test]
    fn search_path_excluded() {
        assert!(should_exclude_by_path(
            "common/json/search/SearchQueryType.json"
        ));
    }

    #[test]
    fn samples_path_excluded() {
        assert!(should_exclude_by_path("common/json/samples/person_01.json"));
    }

    #[test]
    fn normal_path_not_excluded() {
        assert!(!should_exclude_by_path("benefits/json/EnrollmentType.json"));
    }
}
