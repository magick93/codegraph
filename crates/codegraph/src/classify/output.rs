use super::scoring::ClassificationScore;
use super::DomainClassificationResult;

pub fn format_table(results: &[DomainClassificationResult], domain_filter: Option<&str>) {
    println!(
        "{:<14} {:<30} {:>5}  {:<16} Reason",
        "Domain", "Schema", "Score", "Classification"
    );
    println!("{}", "─".repeat(90));

    for result in results {
        if let Some(filter) = domain_filter {
            if result.domain != filter {
                continue;
            }
        }

        let mut all_scores: Vec<&ClassificationScore> = result
            .entities
            .iter()
            .chain(result.value_objects.iter())
            .collect();
        all_scores.sort_by_key(|b| std::cmp::Reverse(b.net_score));

        for score in all_scores {
            let class_str = match score.classification {
                super::scoring::AutoClassification::Entity => "entity (root)",
                super::scoring::AutoClassification::ValueObject => "value_object",
                super::scoring::AutoClassification::Excluded => "excluded",
            };
            println!(
                "{:<14} {:<30} {:>5}  {:<16} {}",
                result.domain,
                score.title,
                score.net_score,
                class_str,
                score.reasons.join(", ")
            );
        }

        for title in &result.excluded {
            println!(
                "{:<14} {:<30} {:>5}  {:<16} path/config rule",
                result.domain, title, "-", "excluded"
            );
        }
    }
}

pub fn format_json(results: &[DomainClassificationResult]) {
    println!(
        "{}",
        serde_json::to_string_pretty(results).unwrap_or_default()
    );
}
