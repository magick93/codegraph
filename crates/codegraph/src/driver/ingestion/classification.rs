//! Entity classification — the ONE auto-classification algorithm the
//! pipeline (`run`, `ifml_generate`) and the `classify` report share.
//!
//! [`classify_domains`] is the algorithm: build the classifier's wrapper
//! type set, read the graph's classification data, and score each domain in
//! sorted order. What happens next is the caller's business —
//! [`apply_entity_classification`] stamps the entity flags into the graph,
//! the `classify` workflow formats a report.

use std::collections::HashSet;
use std::path::Path;

use codegraph_backend::Backend;

use crate::classify::DomainClassificationResult;
use crate::error::Result;
use crate::generate::ProjectConfig;

/// Classify every configured domain, in sorted domain order.
pub(in crate::driver) async fn classify_domains(
    be: &Backend,
    domain_config: &codegraph_config::config::DomainConfig,
    classifier_config: &codegraph_classifier::config::ClassifierConfig,
) -> Result<Vec<DomainClassificationResult>> {
    let all_data = be
        .querier()
        .get_classification_data()
        .await
        .map_err(crate::error::Error::Graph)?;

    let auto_classifier = crate::classify::AutoClassifier::new(
        classifier_types(classifier_config),
        classifier_config.naming_rules.clone(),
    );

    let mut sorted_domain_names: Vec<&String> = domain_config.domains.keys().collect();
    sorted_domain_names.sort();
    Ok(sorted_domain_names
        .into_iter()
        .map(|domain_name| {
            let domain_entry = &domain_config.domains[domain_name.as_str()];
            let domain_schemas: Vec<_> = all_data
                .iter()
                .filter(|d| d.domain.as_deref() == Some(domain_name.as_str()))
                .cloned()
                .collect();
            auto_classifier.classify_domain(domain_name, domain_entry, &domain_schemas)
        })
        .collect())
}

/// Every wrapper type the classifier config names (primitive, array, range
/// and composite wrappers) — the types the classifier never scores.
fn classifier_types(
    classifier_config: &codegraph_classifier::config::ClassifierConfig,
) -> HashSet<String> {
    classifier_config
        .primitive_wrappers
        .keys()
        .cloned()
        .chain(classifier_config.array_wrappers.keys().cloned())
        .chain(classifier_config.range_wrappers.keys().cloned())
        .chain(
            classifier_config
                .composite_wrappers
                .iter()
                .map(|cw| cw.schema.clone()),
        )
        .collect()
}

/// The entity title set a classification yields: every auto-classified
/// entity plus the legacy `entities[]` lists (backward compat during the
/// migration).
fn entity_names(
    results: &[DomainClassificationResult],
    domain_config: &codegraph_config::config::DomainConfig,
) -> HashSet<String> {
    let mut names: HashSet<String> = results
        .iter()
        .flat_map(|result| result.entities.iter().map(|score| score.title.clone()))
        .collect();
    for domain_entry in domain_config.domains.values() {
        names.extend(domain_entry.entities.iter().cloned());
    }
    names
}

/// Auto-classify, then Pass 2: update the graph with the entity flags.
pub(in crate::driver) async fn apply_entity_classification(
    be: &Backend,
    domain_config: &codegraph_config::config::DomainConfig,
    classifier_config: &codegraph_classifier::config::ClassifierConfig,
) -> Result<()> {
    let results = classify_domains(be, domain_config, classifier_config).await?;
    let entity_names = entity_names(&results, domain_config);
    println!("Auto-classified {} entities", entity_names.len());
    crate::ingest::async_ingest::reclassify_with_entities(
        be.ingestor(),
        be.querier(),
        &entity_names,
    )
    .await
}

/// The full pipeline's classification group (driver passes 2/2b): entity
/// classification, then the pinned dependency faces, then the AT Protocol
/// projection. Skipped entirely by the caller when the graph cache reused
/// the persisted graph.
pub(in crate::driver) async fn run_classification_passes(
    be: &Backend,
    domain_config: &codegraph_config::config::DomainConfig,
    classifier_config: &codegraph_classifier::config::ClassifierConfig,
    config_dir: Option<&Path>,
    project_config: &ProjectConfig,
) -> Result<()> {
    apply_entity_classification(be, domain_config, classifier_config).await?;

    // Pass 2b: pinned dependency faces (issue #276). Foreign schemas
    // load AFTER classification so their artifact classification is
    // authoritative (never re-classified); local titles shadow foreign
    // titles (local wins dedup) and every shadow is reported.
    let dep_stats = crate::ingest::dependencies::load_dependency_artifacts(
        be.ingestor(),
        be.querier(),
        domain_config,
        config_dir,
    )
    .await?;
    if dep_stats.artifacts > 0 {
        println!("Pass 2b: dependency faces — {dep_stats}");
        for title in &dep_stats.shadowed_titles {
            eprintln!(
                "WARN dependency title {title} is also defined locally — local definition wins dedup"
            );
        }
    }

    // AT Protocol projection pass — populates Lexicon/Collection/Namespace nodes
    if project_config.integration.has_atproto {
        crate::ingest::atproto_projection::project_atproto_lexicons(
            be.ingestor(),
            be.querier(),
            domain_config,
            project_config,
        )
        .await
        .map_err(|e| crate::error::Error::Config(format!("AT Protocol projection failed: {e}")))?;
    }
    Ok(())
}
