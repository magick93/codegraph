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
        let (r1, r2, r3, r4, r5, r6, r7, r8, r9) = tokio::join!(
            Self::check_codelists(db),
            Self::check_ref_targets(db, config),
            Self::check_fk_targets(db, config),
            Self::check_composition_depth(db, config),
            Self::check_circular_entity_refs(db, config),
            Self::check_phantom_fk_columns(db, config),
            Self::check_namespace_imports_declared(db, config),
            Self::check_namespace_import_dependencies(db, config),
            Self::check_namespace_domain_conflicts(db, config),
        );
        let mut issues = Vec::new();
        issues.extend(r1);
        issues.extend(r2);
        issues.extend(r3);
        issues.extend(r4);
        issues.extend(r5);
        issues.extend(r6);
        issues.extend(r7);
        issues.extend(r8);
        issues.extend(r9);
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

    // ── Namespace plane checks (issue #267) ─────────────────────────────

    /// `namespace_import_undeclared` (Error): every NamespaceImports target
    /// must exist as a namespace in the graph OR be declared in
    /// domains.toml (`[namespaces.*]` — the declared set is the validation
    /// baseline; source-discovered namespaces are allowed without entries).
    /// Mirrors `fk_target_undeclared_dependency` in shape.
    async fn check_namespace_imports_declared(
        db: &dyn GraphQuerier,
        config: &DomainConfig,
    ) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let namespaces = match db.list_namespaces().await {
            Ok(v) => v,
            Err(e) => {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    entity: "(graph)".into(),
                    check: "graph_query_failed",
                    message: format!("list_namespaces failed: {e}"),
                });
                return issues;
            }
        };
        if namespaces.is_empty() {
            return issues;
        }
        for ns in &namespaces {
            let imports = match db.get_namespace_imports(&ns.fqn).await {
                Ok(v) => v,
                Err(e) => {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        entity: ns.fqn.clone(),
                        check: "graph_query_failed",
                        message: format!("get_namespace_imports failed: {e}"),
                    });
                    continue;
                }
            };
            for import in imports {
                let in_graph = namespaces.iter().any(|n| n.fqn == import.to_ns);
                let in_allowlist = config.namespaces.contains_key(&import.to_ns);
                if !in_graph && !in_allowlist {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        entity: ns.fqn.clone(),
                        check: "namespace_import_undeclared",
                        message: format!(
                            "Namespace '{}' imports '{}' which is neither in the graph nor declared in domains.toml",
                            import.from_ns, import.to_ns
                        ),
                    });
                }
            }
        }
        issues
    }

    /// Cross-domain namespace imports require the importing namespace's
    /// assigned domain to declare `depends_on` on the imported namespace's
    /// assigned domain (reuses the domain `depends_on` gate behind
    /// `fk_target_undeclared_dependency`). Namespaces without a resolvable
    /// domain assignment are skipped.
    async fn check_namespace_import_dependencies(
        db: &dyn GraphQuerier,
        config: &DomainConfig,
    ) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let namespaces = match db.list_namespaces().await {
            Ok(v) => v,
            Err(e) => {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    entity: "(graph)".into(),
                    check: "graph_query_failed",
                    message: format!("list_namespaces failed: {e}"),
                });
                return issues;
            }
        };
        if namespaces.is_empty() {
            return issues;
        }
        let assignments = namespace_domain_assignments(db, config, &namespaces).await;
        for ns in &namespaces {
            let imports = match db.get_namespace_imports(&ns.fqn).await {
                Ok(v) => v,
                Err(e) => {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        entity: ns.fqn.clone(),
                        check: "graph_query_failed",
                        message: format!("get_namespace_imports failed: {e}"),
                    });
                    continue;
                }
            };
            for import in imports {
                let (Some(source_domain), Some(target_domain)) = (
                    assignments.get(&import.from_ns),
                    assignments.get(&import.to_ns),
                ) else {
                    continue;
                };
                if source_domain == target_domain {
                    continue;
                }
                let declared = config
                    .domains
                    .get(source_domain.as_str())
                    .map(|d| d.depends_on.contains(target_domain))
                    .unwrap_or(false);
                if !declared {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        entity: import.from_ns.clone(),
                        check: "namespace_import_undeclared_dependency",
                        message: format!(
                            "Namespace '{}' (domain '{}') imports namespace '{}' (domain '{}'), but domain '{}' does not declare depends_on '{}'",
                            import.from_ns,
                            source_domain,
                            import.to_ns,
                            target_domain,
                            source_domain,
                            target_domain
                        ),
                    });
                }
            }
        }
        issues
    }

    /// `namespace_domain_conflict` (Warning): a namespace is assigned to
    /// more than one domain — either via conflicting `[namespaces.*]`
    /// declarations... (config allows one `domain` per entry, so conflicts
    /// surface when the namespace's member schemas carry dir-derived
    /// domains that disagree with the declared assignment or each other).
    async fn check_namespace_domain_conflicts(
        db: &dyn GraphQuerier,
        config: &DomainConfig,
    ) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let graph_namespaces = match db.list_namespaces().await {
            Ok(v) => v,
            Err(e) => {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    entity: "(graph)".into(),
                    check: "graph_query_failed",
                    message: format!("list_namespaces failed: {e}"),
                });
                return issues;
            }
        };
        // Union of graph namespaces and config-declared fqns, sorted.
        let mut fqns: Vec<String> = graph_namespaces.iter().map(|n| n.fqn.clone()).collect();
        for fqn in config.namespaces.keys() {
            if !fqns.contains(fqn) {
                fqns.push(fqn.clone());
            }
        }
        fqns.sort();
        if fqns.is_empty() {
            return issues;
        }
        for fqn in &fqns {
            // Effective assignment set: declared config domain + observed
            // domains of member schemas.
            let mut domains: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            if let Some(declared) = config.namespaces.get(fqn).and_then(|e| e.domain.clone()) {
                domains.insert(declared);
            }
            if let Ok(schemas) = db.list_schemas_by_namespace(fqn, true).await {
                for schema in schemas {
                    if let Some(d) = schema.domain {
                        domains.insert(d);
                    }
                }
            }
            if domains.len() > 1 {
                issues.push(ValidationIssue {
                    severity: Severity::Warning,
                    entity: fqn.clone(),
                    check: "namespace_domain_conflict",
                    message: format!(
                        "Namespace '{}' is assigned to multiple domains: {} (declared in config: {}; observed on member schemas: {})",
                        fqn,
                        {
                            let all: Vec<String> = domains.iter().cloned().collect();
                            all.join(", ")
                        },
                        config
                            .namespaces
                            .get(fqn)
                            .and_then(|e| e.domain.clone())
                            .unwrap_or_else(|| "(none)".into()),
                        {
                            let observed: Vec<String> = domains
                                .iter()
                                .filter(|d| {
                                    config.namespaces.get(fqn).and_then(|e| e.domain.as_deref())
                                        != Some(d.as_str())
                                })
                                .cloned()
                                .collect();
                            if observed.is_empty() {
                                "(none)".to_string()
                            } else {
                                observed.join(", ")
                            }
                        }
                    ),
                });
            }
        }
        issues
    }
}

/// Namespace → domain assignment resolution: the config-declared `domain`
/// wins; namespaces without a config entry (source-discovered) fall back to
/// the unique domain among their member schemas (ambiguous → unresolved).
async fn namespace_domain_assignments(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    graph_namespaces: &[codegraph_core::types::NamespaceNode],
) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let mut fqns: Vec<&str> = graph_namespaces.iter().map(|n| n.fqn.as_str()).collect();
    for fqn in config.namespaces.keys() {
        if !fqns.contains(&fqn.as_str()) {
            fqns.push(fqn.as_str());
        }
    }
    for fqn in fqns {
        if let Some(domain) = config.namespaces.get(fqn).and_then(|e| e.domain.clone()) {
            out.insert(fqn.to_string(), domain);
            continue;
        }
        // Fallback: unique domain among the namespace's member schemas.
        if let Ok(schemas) = db.list_schemas_by_namespace(fqn, false).await {
            let mut observed: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
            for schema in &schemas {
                if let Some(d) = schema.domain.as_deref() {
                    observed.insert(d);
                }
            }
            if observed.len() == 1 {
                if let Some(d) = observed.iter().next() {
                    out.insert(fqn.to_string(), d.to_string());
                }
            }
        }
    }
    out
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
