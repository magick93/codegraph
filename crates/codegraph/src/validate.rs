use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

#[derive(Debug)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub entity: String,
    pub check: &'static str,
    pub message: String,
}

pub struct ValidationPass;

impl ValidationPass {
    pub async fn run(db: &dyn GraphQuerier, config: &DomainConfig) -> Vec<ValidationIssue> {
        // Run all independent validation checks concurrently.
        let (r1, r2, r3, r4, r5, r6) = tokio::join!(
            Self::check_codelists(db),
            Self::check_ref_targets(db, config),
            Self::check_fk_targets(db, config),
            Self::check_composition_depth(db, config),
            Self::check_circular_entity_refs(db, config),
            Self::check_phantom_fk_columns(db, config),
        );
        let mut issues = Vec::new();
        issues.extend(r1);
        issues.extend(r2);
        issues.extend(r3);
        issues.extend(r4);
        issues.extend(r5);
        issues.extend(r6);
        issues
    }

    async fn check_codelists(db: &dyn GraphQuerier) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let codelists = match db.list_codelists().await {
            Ok(v) => v,
            Err(e) => {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    entity: "(graph)".into(),
                    check: "graph_query_failed",
                    message: format!("list_codelists failed: {e}"),
                });
                return issues;
            }
        };
        for cl in codelists {
            let values = match db.get_enum_values(&cl.name).await {
                Ok(v) => v,
                Err(e) => {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        entity: cl.name.clone(),
                        check: "graph_query_failed",
                        message: format!("get_enum_values failed: {e}"),
                    });
                    continue;
                }
            };
            if values.is_empty() {
                issues.push(ValidationIssue {
                    severity: Severity::Warning,
                    entity: cl.name,
                    check: "empty_codelist",
                    message: "Codelist has no enum values".into(),
                });
            }
        }
        issues
    }

    async fn check_ref_targets(
        db: &dyn GraphQuerier,
        config: &DomainConfig,
    ) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let entity_names = match db.get_entity_names().await {
            Ok(v) => v,
            Err(e) => {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    entity: "(graph)".into(),
                    check: "graph_query_failed",
                    message: format!("get_entity_names failed: {e}"),
                });
                return issues;
            }
        };

        for title in &entity_names {
            let props = match db.get_properties(title).await {
                Ok(v) => v,
                Err(e) => {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        entity: title.clone(),
                        check: "graph_query_failed",
                        message: format!("get_properties failed: {e}"),
                    });
                    continue;
                }
            };
            for prop in &props {
                if let Some(ref ref_target) = prop.ref_target {
                    // Check that the ref target exists as a schema in the graph
                    if let Ok(None) = db.get_schema(ref_target).await {
                        // Only flag as error if it looks like an entity reference
                        // that should be in the graph
                        if prop.classification.as_deref() == Some("entity_reference") {
                            let domain = config
                                .domains
                                .values()
                                .find(|d| d.entities.contains(ref_target));
                            if domain.is_none() {
                                issues.push(ValidationIssue {
                                    severity: Severity::Error,
                                    entity: title.clone(),
                                    check: "ref_target_missing",
                                    message: format!(
                                        "Property '{}' references '{}' which is not in any domain",
                                        prop.name, ref_target
                                    ),
                                });
                            }
                        }
                    }
                }
            }
        }
        issues
    }

    /// Checks that entity references classified as `entity_reference` point to
    /// entities that belong to the same domain or a declared dependency domain.
    async fn check_fk_targets(
        db: &dyn GraphQuerier,
        config: &DomainConfig,
    ) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let entity_names = match db.get_entity_names().await {
            Ok(v) => v,
            Err(e) => {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    entity: "(graph)".into(),
                    check: "graph_query_failed",
                    message: format!("get_entity_names failed: {e}"),
                });
                return issues;
            }
        };

        // Build entity→domain lookup
        let mut entity_domain: std::collections::HashMap<&str, &str> =
            std::collections::HashMap::new();
        for (domain_name, domain_entry) in &config.domains {
            for e in &domain_entry.entities {
                entity_domain.insert(e.as_str(), domain_name.as_str());
            }
        }

        for title in &entity_names {
            let source_domain = match entity_domain.get(title.as_str()) {
                Some(d) => *d,
                None => continue,
            };
            let props = match db.get_properties(title).await {
                Ok(v) => v,
                Err(e) => {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        entity: title.clone(),
                        check: "graph_query_failed",
                        message: format!("get_properties failed: {e}"),
                    });
                    continue;
                }
            };
            for prop in &props {
                if prop.classification.as_deref() != Some("entity_reference") {
                    continue;
                }
                if let Some(ref ref_target) = prop.ref_target {
                    if let Some(&target_domain) = entity_domain.get(ref_target.as_str()) {
                        if target_domain != source_domain {
                            // Cross-domain FK — check dependency is declared
                            let source_entry = &config.domains[source_domain];
                            if !source_entry.depends_on.contains(&target_domain.to_string()) {
                                issues.push(ValidationIssue {
                                    severity: Severity::Error,
                                    entity: title.clone(),
                                    check: "fk_target_undeclared_dependency",
                                    message: format!(
                                        "Property '{}' references '{}' in domain '{}', but '{}' does not declare depends_on '{}'",
                                        prop.name, ref_target, target_domain, source_domain, target_domain
                                    ),
                                });
                            }
                        }
                    }
                    // If target isn't in any domain, check_ref_targets already catches it
                }
            }
        }
        issues
    }

    async fn check_composition_depth(
        db: &dyn GraphQuerier,
        config: &DomainConfig,
    ) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let entity_names = match db.get_entity_names().await {
            Ok(v) => v,
            Err(e) => {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    entity: "(graph)".into(),
                    check: "graph_query_failed",
                    message: format!("get_entity_names failed: {e}"),
                });
                return issues;
            }
        };

        for title in &entity_names {
            let in_config = config.domains.values().any(|d| d.entities.contains(title));
            if !in_config {
                continue;
            }
            if let Ok(tree) = db.get_composition_tree(title).await {
                let depth = max_depth(&tree.root);
                if depth > 3 {
                    issues.push(ValidationIssue {
                        severity: Severity::Warning,
                        entity: title.clone(),
                        check: "composition_depth",
                        message: format!("Composition tree depth is {} (max supported: 3)", depth),
                    });
                }
            }
        }
        issues
    }

    /// Detects circular entity references: A→B→A. These can cause issues
    /// with DDL ordering and repository generation.
    async fn check_circular_entity_refs(
        db: &dyn GraphQuerier,
        config: &DomainConfig,
    ) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let entity_names = match db.get_entity_names().await {
            Ok(v) => v,
            Err(e) => {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    entity: "(graph)".into(),
                    check: "graph_query_failed",
                    message: format!("get_entity_names failed: {e}"),
                });
                return issues;
            }
        };

        // Only check entities that are in config
        let config_entities: std::collections::HashSet<&str> = config
            .domains
            .values()
            .flat_map(|d| d.entities.iter().map(|e| e.as_str()))
            .collect();

        for title in &entity_names {
            if !config_entities.contains(title.as_str()) {
                continue;
            }
            let props = match db.get_properties(title).await {
                Ok(v) => v,
                Err(e) => {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        entity: title.clone(),
                        check: "graph_query_failed",
                        message: format!("get_properties failed: {e}"),
                    });
                    continue;
                }
            };
            for prop in &props {
                if prop.classification.as_deref() != Some("entity_reference") {
                    continue;
                }
                if let Some(ref ref_target) = prop.ref_target {
                    if !config_entities.contains(ref_target.as_str()) {
                        continue;
                    }
                    // Check if the target references back to us
                    let target_props = match db.get_properties(ref_target).await {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    for target_prop in &target_props {
                        if target_prop.classification.as_deref() == Some("entity_reference")
                            && target_prop.ref_target.as_deref() == Some(title.as_str())
                        {
                            issues.push(ValidationIssue {
                                severity: Severity::Warning,
                                entity: title.clone(),
                                check: "circular_entity_ref",
                                message: format!(
                                    "Circular reference: {} -> {} -> {} (via properties '{}' and '{}')",
                                    title, ref_target, title, prop.name, target_prop.name
                                ),
                            });
                        }
                    }
                }
            }
        }
        issues
    }

    /// Detects phantom FK columns: array properties classified as EntityReference
    /// that would produce an invalid single UUID FK column on the parent table.
    /// A one-to-many relationship requires the FK on the child table, not the parent.
    async fn check_phantom_fk_columns(
        db: &dyn GraphQuerier,
        config: &DomainConfig,
    ) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let entity_names = match db.get_entity_names().await {
            Ok(v) => v,
            Err(_) => return issues,
        };

        for title in &entity_names {
            let in_config = config.domains.values().any(|d| d.entities.contains(title));
            if !in_config {
                continue;
            }
            if let Ok(tree) = db.get_composition_tree(title).await {
                check_node_for_phantom_fks(&tree.root, &mut issues);
            }
        }
        issues
    }
}

/// Recursively check a composition node and its children for phantom FK columns.
fn check_node_for_phantom_fks(
    node: &codegraph_core::types::CompositionNode,
    issues: &mut Vec<ValidationIssue>,
) {
    use codegraph_type_contracts::RefClassificationKind;

    for col in &node.columns {
        if col.is_array
            && col.classification == Some(RefClassificationKind::EntityReference)
            && col.fk_target.is_some()
        {
            issues.push(ValidationIssue {
                severity: Severity::Warning,
                entity: node.schema_title.clone(),
                check: "phantom_fk_column",
                message: format!(
                    "Array property '{}' classified as EntityReference would produce \
                     phantom FK column '{}_id' — one-to-many relationships need the FK \
                     on the child table, not a single UUID on the parent",
                    col.name, col.name,
                ),
            });
        }
    }

    for child in &node.children {
        check_node_for_phantom_fks(child, issues);
    }
}

fn max_depth(node: &codegraph_core::types::CompositionNode) -> usize {
    if node.children.is_empty() {
        1
    } else {
        1 + node.children.iter().map(max_depth).max().unwrap_or(0)
    }
}
