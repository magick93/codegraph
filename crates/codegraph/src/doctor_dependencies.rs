//! Doctor checks for cross-project domain dependencies (issue #276).
//!
//! Pure config + artifact inspection — no graph required. For every
//! `[[domains.X.dependencies]]` entry it reports one [`DependencyCheck`]:
//! the face resolves (with the version the artifact carries, when any), the
//! artifact file is missing, the pinned version conflicts with the
//! artifact's version, or two faces export the same schema title
//! (cross-face conflict).

use std::collections::HashMap;
use std::path::Path;

use codegraph_config::config::DomainConfig;

use crate::artifact::{parse_document, NodeRecord};
use crate::ingest::dependencies::{read_dependency_meta, resolve_dependency_path};

/// Outcome of one dependency check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyCheckStatus {
    /// The artifact exists and parses. `artifact_version` is the version
    /// the face publishes in its `meta` block, `None` when unversioned
    /// (the pin cannot be verified — doctor prints a warning).
    Resolved {
        artifact_version: Option<String>,
        schema_count: usize,
    },
    /// The artifact file is absent or unreadable.
    MissingArtifact { reason: String },
    /// The pin and the artifact's published version disagree.
    VersionMismatch { declared: String, found: String },
    /// Two faces export the same schema title; the consumer's dedup
    /// precedence (local wins, then declaration order) hides the drift.
    TitleConflict { titles: Vec<String> },
}

/// One dependency's doctor verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyCheck {
    /// The consumer domain declaring the dependency.
    pub consumer_domain: String,
    /// The foreign (publisher-side) domain name.
    pub dependency_domain: String,
    /// The version pin from domains.toml.
    pub declared_version: String,
    pub status: DependencyCheckStatus,
}

impl DependencyCheck {
    /// True when the outcome must fail `doctor` (non-zero exit).
    pub fn is_hard_failure(&self) -> bool {
        matches!(
            self.status,
            DependencyCheckStatus::MissingArtifact { .. }
                | DependencyCheckStatus::VersionMismatch { .. }
        )
    }

    /// True when the outcome is worth a warning line.
    pub fn is_warning(&self) -> bool {
        matches!(self.status, DependencyCheckStatus::TitleConflict { .. })
            || matches!(
                self.status,
                DependencyCheckStatus::Resolved {
                    artifact_version: None,
                    ..
                }
            )
    }
}

/// Check every declared dependency face: file presence, parse validity,
/// version agreement, and cross-face title conflicts. Faces pinned by more
/// than one consumer domain are checked once (deduplicated by domain).
pub fn check_domain_dependencies(
    config: &DomainConfig,
    config_dir: Option<&Path>,
) -> Vec<DependencyCheck> {
    let mut checks: Vec<DependencyCheck> = Vec::new();
    let mut seen_domains: Vec<String> = Vec::new();
    let mut face_titles: HashMap<String, Vec<String>> = HashMap::new();

    let mut consumer_domains: Vec<&String> = config.domains.keys().collect();
    consumer_domains.sort();
    for consumer in consumer_domains {
        let entry = &config.domains[consumer.as_str()];
        for dep in &entry.dependencies {
            if seen_domains.contains(&dep.domain) {
                continue;
            }
            seen_domains.push(dep.domain.clone());
            let path = resolve_dependency_path(config_dir, &dep.source);
            let status = match inspect_artifact(&path) {
                Err(reason) => DependencyCheckStatus::MissingArtifact { reason },
                Ok((meta, titles)) => {
                    face_titles
                        .entry(dep.domain.clone())
                        .or_default()
                        .extend(titles);
                    match meta.version.as_deref() {
                        Some(found) if found != dep.version => {
                            DependencyCheckStatus::VersionMismatch {
                                declared: dep.version.clone(),
                                found: found.to_string(),
                            }
                        }
                        _ => DependencyCheckStatus::Resolved {
                            artifact_version: meta.version,
                            schema_count: face_titles
                                .get(&dep.domain)
                                .map(|t| t.len())
                                .unwrap_or(0),
                        },
                    }
                }
            };
            checks.push(DependencyCheck {
                consumer_domain: consumer.clone(),
                dependency_domain: dep.domain.clone(),
                declared_version: dep.version.clone(),
                status,
            });
        }
    }

    // Cross-face title conflicts: a title exported by two different faces
    // is reported on every participating check.
    let conflicted = cross_face_conflicts(&face_titles);
    if !conflicted.is_empty() {
        for check in &mut checks {
            let titles = conflicted
                .iter()
                .filter(|(_, domains)| domains.contains(&check.dependency_domain))
                .map(|(title, _)| title.clone())
                .collect::<Vec<_>>();
            if titles.is_empty() {
                continue;
            }
            check.status = DependencyCheckStatus::TitleConflict { titles };
        }
    }
    checks
}

fn cross_face_conflicts(face_titles: &HashMap<String, Vec<String>>) -> Vec<(String, Vec<String>)> {
    let mut by_title: HashMap<&str, Vec<&str>> = HashMap::new();
    for (domain, titles) in face_titles {
        for title in titles {
            by_title
                .entry(title.as_str())
                .or_default()
                .push(domain.as_str());
        }
    }
    let mut conflicts: Vec<(String, Vec<String>)> = by_title
        .into_iter()
        .filter(|(_, domains)| domains.len() > 1)
        .map(|(title, domains)| {
            (
                title.to_string(),
                domains.into_iter().map(str::to_string).collect(),
            )
        })
        .collect();
    conflicts.sort();
    conflicts
}

/// Read + parse one face artifact: returns its `meta` block and the sorted
/// schema titles it exports.
fn inspect_artifact(
    path: &Path,
) -> std::result::Result<
    (
        crate::ingest::dependencies::DependencyArtifactMeta,
        Vec<String>,
    ),
    String,
> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let meta = read_dependency_meta(path).map_err(|e| e.to_string())?;
    let doc = parse_document(&bytes).map_err(|e| e.to_string())?;
    let titles: Vec<String> = doc
        .nodes
        .iter()
        .filter(|n| n.label == "Schema")
        .filter_map(schema_title)
        .collect();
    Ok((meta, titles))
}

fn schema_title(node: &NodeRecord) -> Option<String> {
    match node.properties.get("title")? {
        crate::artifact::PropValue::Str(title) => Some(title.clone()),
        _ => None,
    }
}

/// Print one doctor line per check in the established PASS/WARN/FAIL shape.
/// Returns `(hard_failures, warnings)` for the doctor summary counters.
pub fn print_dependency_checks(checks: &[DependencyCheck]) -> (usize, usize) {
    let mut hard = 0;
    let mut warnings = 0;
    for check in checks {
        let label = format!(
            "dependency {}.{} (pin {})",
            check.consumer_domain, check.dependency_domain, check.declared_version
        );
        match &check.status {
            DependencyCheckStatus::Resolved {
                artifact_version, ..
            } => {
                if let Some(version) = artifact_version {
                    println!("PASS {label} — face at version {version}");
                } else {
                    warnings += 1;
                    println!(
                        "WARN {label} — face carries no version metadata; the pin is unverified"
                    );
                }
            }
            DependencyCheckStatus::MissingArtifact { reason } => {
                hard += 1;
                println!("FAIL {label} — artifact missing: {reason}");
                println!(
                    "     hint: build the publisher face first (export the graph with the #275 artifact export)"
                );
            }
            DependencyCheckStatus::VersionMismatch { declared, found } => {
                hard += 1;
                println!("FAIL {label} — pinned {declared} but the face publishes {found}");
                println!(
                    "     hint: bump the pin in domains.toml or republish the face at the pinned version"
                );
            }
            DependencyCheckStatus::TitleConflict { titles } => {
                warnings += 1;
                println!(
                    "WARN {label} — title conflict with another face: {}",
                    titles.join(", ")
                );
            }
        }
    }
    (hard, warnings)
}
