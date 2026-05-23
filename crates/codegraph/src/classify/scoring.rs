use codegraph_core::types::SchemaClassificationData;

/// Classification result from the auto-classifier.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ClassificationScore {
    pub title: String,
    pub domain: Option<String>,
    pub entity_score: i32,
    pub vo_score: i32,
    pub net_score: i32,
    pub classification: AutoClassification,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum AutoClassification {
    Entity,
    ValueObject,
    Excluded,
}

/// Score a single schema based on structural graph signals.
pub fn score_structural(data: &SchemaClassificationData) -> ClassificationScore {
    let mut entity_score: i32 = 0;
    let mut vo_score: i32 = 0;
    let mut reasons = Vec::new();

    // Hard VO signals — bypass scoring
    if data.is_primitive_wrapper {
        return ClassificationScore {
            title: data.title.clone(),
            domain: data.domain.clone(),
            entity_score: 0,
            vo_score: 5,
            net_score: -5,
            classification: AutoClassification::ValueObject,
            reasons: vec!["hard:primitive_wrapper".to_string()],
        };
    }
    if data.is_codelist {
        return ClassificationScore {
            title: data.title.clone(),
            domain: data.domain.clone(),
            entity_score: 0,
            vo_score: 5,
            net_score: -5,
            classification: AutoClassification::ValueObject,
            reasons: vec!["hard:codelist".to_string()],
        };
    }
    if data.is_enum {
        return ClassificationScore {
            title: data.title.clone(),
            domain: data.domain.clone(),
            entity_score: 0,
            vo_score: 5,
            net_score: -5,
            classification: AutoClassification::ValueObject,
            reasons: vec!["hard:enum".to_string()],
        };
    }

    // Structural signals
    match data.in_degree {
        0 => {
            vo_score += 2;
            reasons.push("in_degree=0 (+2 VO)".to_string());
        }
        1..=2 => {
            entity_score += 1;
            reasons.push(format!("in_degree={} (+1 entity)", data.in_degree));
        }
        _ => {
            entity_score += 3;
            reasons.push(format!("in_degree={} (+3 entity)", data.in_degree));
        }
    }

    if data.field_count >= 8 {
        entity_score += 2;
        reasons.push(format!("fields={} (+2 entity)", data.field_count));
    } else if data.field_count <= 3 {
        vo_score += 2;
        reasons.push(format!("fields={} (+2 VO)", data.field_count));
    }

    if data.composes_noun_type {
        entity_score += 2;
        reasons.push("composes_noun (+2 entity)".to_string());
    }

    if data.has_all_of {
        entity_score += 2;
        reasons.push("has_allOf (+2 entity)".to_string());
    }

    let net_score = entity_score - vo_score;
    let classification = if net_score >= 4 {
        AutoClassification::Entity
    } else {
        AutoClassification::ValueObject
    };

    ClassificationScore {
        title: data.title.clone(),
        domain: data.domain.clone(),
        entity_score,
        vo_score,
        net_score,
        classification,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_data(
        title: &str,
        in_degree: usize,
        field_count: usize,
        composes_noun: bool,
        is_primitive: bool,
        is_codelist: bool,
        is_enum: bool,
    ) -> SchemaClassificationData {
        SchemaClassificationData {
            title: title.to_string(),
            domain: Some("test".to_string()),
            rel_path: format!("test/json/{}.json", title),
            schema_type: "object".to_string(),
            is_codelist,
            is_primitive_wrapper: is_primitive,
            has_all_of: false,
            composes_noun_type: composes_noun,
            field_count,
            required_field_count: field_count / 2,
            ref_count: 0,
            in_degree,
            is_enum,
            is_string_type: false,
        }
    }

    #[test]
    fn high_in_degree_high_fields_scores_entity() {
        let data = make_data("PersonType", 5, 15, true, false, false, false);
        let score = score_structural(&data);
        assert_eq!(score.classification, AutoClassification::Entity);
        assert!(
            score.net_score >= 4,
            "net_score={} should be >= 4",
            score.net_score
        );
    }

    #[test]
    fn low_in_degree_low_fields_scores_vo() {
        let data = make_data("NameType", 0, 3, false, false, false, false);
        let score = score_structural(&data);
        assert_eq!(score.classification, AutoClassification::ValueObject);
        assert!(
            score.net_score < 4,
            "net_score={} should be < 4",
            score.net_score
        );
    }

    #[test]
    fn primitive_wrapper_is_hard_vo() {
        let data = make_data("CodeType", 10, 20, true, true, false, false);
        let score = score_structural(&data);
        assert_eq!(score.classification, AutoClassification::ValueObject);
        assert!(score.reasons.iter().any(|r| r.contains("hard")));
    }

    #[test]
    fn codelist_is_hard_vo() {
        let data = make_data("GenderCodeList", 5, 0, false, false, true, false);
        let score = score_structural(&data);
        assert_eq!(score.classification, AutoClassification::ValueObject);
    }

    #[test]
    fn enum_is_hard_vo() {
        let data = make_data("SomeEnum", 3, 0, false, false, false, true);
        let score = score_structural(&data);
        assert_eq!(score.classification, AutoClassification::ValueObject);
    }

    #[test]
    fn boundary_score_3_is_vo() {
        // in_degree=2 (+1 entity), field_count=5 (no signal), composes_noun (+2) = net 3
        let data = make_data("AmbiguousType", 2, 5, true, false, false, false);
        let score = score_structural(&data);
        assert_eq!(score.classification, AutoClassification::ValueObject);
        assert!(score.net_score < 4);
    }

    #[test]
    fn boundary_score_4_is_entity() {
        // in_degree=3 (+3 entity), field_count=5 (no signal), composes_noun (+2) = net 5
        let data = make_data("BorderlineEntity", 3, 5, true, false, false, false);
        let score = score_structural(&data);
        assert_eq!(score.classification, AutoClassification::Entity);
        assert!(score.net_score >= 4);
    }
}
