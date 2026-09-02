use codegraph_config::config::{DomainConfig, EntityConfig};
use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::SchemaNode;

/// Returns the versioned API prefix path (e.g. "/api/v1").
pub fn api_prefix(api_version: &str) -> String {
    format!("/api/{}", api_version)
}

#[derive(Debug, Clone)]
pub struct ResolvedOperation {
    pub kind: String,
    pub name: String,
    pub paging: bool,
    pub sorting: bool,
    pub filtering: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedApiResource {
    pub name: String,
    pub path_segment: String,
    pub operations: Vec<ResolvedOperation>,
    pub http_endpoints: Vec<ResolvedHttpEndpoint>,
}

#[derive(Debug, Clone)]
pub struct ResolvedHttpEndpoint {
    pub method: String,
    pub path_template: String,
    pub operation_kind: String,
}

pub async fn resolve_domain_api_resources(
    querier: &dyn GraphQuerier,
    domain_config: &DomainConfig,
    domain_name: &str,
) -> Result<Vec<ResolvedApiResource>, GraphError> {
    let api_resources = querier.get_api_resources().await?;

    let domain_resources: Vec<_> = api_resources
        .into_iter()
        .filter(|r| r.domain == domain_name)
        .collect();

    if !domain_resources.is_empty() {
        let mut results = Vec::new();
        for resource in domain_resources {
            let ops = querier.get_api_operations(&resource.name).await?;

            let mut resolved_ops = Vec::new();
            let mut resolved_endpoints = Vec::new();

            for op in &ops {
                resolved_ops.push(ResolvedOperation {
                    kind: op.kind.clone(),
                    name: op.name.clone(),
                    paging: op.paging,
                    sorting: op.sorting,
                    filtering: op.filtering,
                });

                let (method, suffix) = op_kind_to_http(&op.kind);
                resolved_endpoints.push(ResolvedHttpEndpoint {
                    method: method.to_string(),
                    path_template: format!(
                        "{}/{}/{}{}",
                        api_prefix("v1"),
                        domain_name,
                        resource.path_segment,
                        suffix
                    ),
                    operation_kind: op.kind.clone(),
                });
            }

            results.push(ResolvedApiResource {
                name: resource.name.clone(),
                path_segment: resource.path_segment.clone(),
                operations: resolved_ops,
                http_endpoints: resolved_endpoints,
            });
        }
        Ok(results)
    } else {
        resolve_from_entity_config(domain_config, domain_name)
    }
}

fn resolve_from_entity_config(
    domain_config: &DomainConfig,
    domain_name: &str,
) -> Result<Vec<ResolvedApiResource>, GraphError> {
    let domain_entry = match domain_config.domains.get(domain_name) {
        Some(de) => de,
        None => return Ok(Vec::new()),
    };

    let mut results = Vec::new();
    for entity_name in &domain_entry.entities {
        let ec = domain_entry.get_entity_config(entity_name);
        let operations = ec
            .and_then(|c| c.operations.clone())
            .unwrap_or_else(|| domain_config.defaults.operations.clone());

        let path_segment = ec
            .and_then(|c| c.path_segment.clone())
            .unwrap_or_else(|| entity_name.clone());

        let resource_name = entity_name
            .strip_suffix("Type")
            .unwrap_or(entity_name)
            .to_string();

        let mut resolved_ops = Vec::new();
        let mut resolved_endpoints = Vec::new();

        let base_path = format!("/api/v1/{}/{}", domain_name, path_segment);

        for op_kind in &operations {
            resolved_ops.push(ResolvedOperation {
                kind: op_kind.clone(),
                name: format!("{}_{}", op_kind, resource_name),
                paging: op_kind == "list",
                sorting: op_kind == "list",
                filtering: op_kind == "list",
            });

            let (method, suffix) = op_kind_to_http(op_kind);
            resolved_endpoints.push(ResolvedHttpEndpoint {
                method: method.to_string(),
                path_template: format!("{}{}", base_path, suffix),
                operation_kind: op_kind.clone(),
            });
        }

        results.push(ResolvedApiResource {
            name: resource_name,
            path_segment,
            operations: resolved_ops,
            http_endpoints: resolved_endpoints,
        });
    }
    Ok(results)
}

/// Resolve the operations list for a single entity.
///
/// Priority (highest to lowest):
/// 1. `EntityConfig.operations` explicitly set in domains.toml — always wins,
///    even when the graph contains a different (e.g. stale or auto-seeded)
///    set of ApiOperation nodes.
/// 2. Graph-derived operations from the ApiResource → HasOperation → ApiOperation.
/// 3. `config.defaults.operations`.
///
/// Normalize an entity/resource name for API-model lookups: strip the "Type"
/// suffix and PascalCase whatever remains (titles may contain spaces, e.g.
/// "Review Decision" → "ReviewDecision").
pub fn normalized_resource_name(name: &str) -> String {
    codegraph_naming::to_pascal_case(name.trim_end_matches("Type"))
}

pub async fn resolve_entity_operations(
    querier: &dyn GraphQuerier,
    config: &DomainConfig,
    domain_name: &str,
    entity_name: &str,
) -> Vec<String> {
    let domain_entry = config.domains.get(domain_name);
    // An explicitly configured operations list takes precedence over the graph.
    if let Some(explicit) = domain_entry
        .and_then(|de| de.get_entity_config(entity_name))
        .and_then(|c| c.operations.clone())
    {
        return explicit;
    }

    if let Ok(resources) = querier.get_api_resources().await {
        let resource_name = normalized_resource_name(entity_name);
        if let Some(resource) = resources
            .iter()
            .find(|r| r.domain == domain_name && r.name == resource_name)
        {
            if let Ok(ops) = querier.get_api_operations(&resource.name).await {
                if !ops.is_empty() {
                    return ops.iter().map(|op| op.kind.clone()).collect();
                }
            }
        }
    }

    // No explicit config and no usable graph ops — fall back to defaults.
    config.defaults.operations.clone()
}

fn op_kind_to_http(kind: &str) -> (&'static str, &'static str) {
    match kind {
        "list" => ("GET", ""),
        "search" => ("GET", "/search"),
        "create" => ("POST", ""),
        "read" => ("GET", "/{id}"),
        "update" => ("PUT", "/{id}"),
        "delete" => ("DELETE", "/{id}"),
        _ => ("POST", ""),
    }
}

/// Resolve the URL path segment for an entity.
/// Priority: EntityConfig.path_segment > SchemaNode.api_path_segment > entity_name (lowercase)
pub fn resolve_path_segment(ec: Option<&EntityConfig>, schema_node: &SchemaNode) -> String {
    ec.and_then(|c| c.path_segment.clone()).unwrap_or_else(|| {
        if schema_node.api_path_segment.is_empty() {
            schema_node.title.to_lowercase()
        } else {
            schema_node.api_path_segment.clone()
        }
    })
}

/// Like resolve_path_segment but falls back to a domain-config lookup when
/// `ec` is `None`. Use this when the entity config may exist in the domain
/// config but isn't readily available at the call site.
pub fn resolve_path_segment_with_config(
    ec: Option<&EntityConfig>,
    schema_node: &SchemaNode,
    config: &DomainConfig,
) -> String {
    let effective_ec = ec.or_else(|| {
        let domain = schema_node.domain.as_deref()?;
        config
            .domains
            .get(domain)?
            .get_entity_config(&schema_node.title)
    });
    resolve_path_segment(effective_ec, schema_node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_core::mock::MockEngine;

    fn test_config() -> DomainConfig {
        toml::from_str(
            r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.compliance]
label = "Compliance"
schema_dir = "compliance"
postgres_schema = "compliance"
entities = ["Screening Result", "Document"]

[domains.compliance.entity_config."Screening Result"]
operations = ["create", "read", "list"]

[domains.compliance.entity_config.Document]
"#,
        )
        .unwrap()
    }

    /// Explicit EntityConfig.operations must win over whatever the graph
    /// contains (here: empty). Regression test for the router emitting
    /// PUT/DELETE on append-only entities (issue #79). The graph-backed
    /// counterpart lives in tests/api_model_graph_tests.rs.
    #[tokio::test]
    async fn explicit_config_operations_beat_graph() {
        let querier = MockEngine::new();
        let config = test_config();
        let ops =
            resolve_entity_operations(&querier, &config, "compliance", "ScreeningResult").await;
        assert_eq!(ops, vec!["create", "read", "list"]);
    }

    /// The explicit-config lookup must also work when passed the raw schema
    /// title containing spaces.
    #[tokio::test]
    async fn explicit_config_operations_matched_by_raw_title() {
        let querier = MockEngine::new();
        let config = test_config();
        let ops =
            resolve_entity_operations(&querier, &config, "compliance", "Screening Result").await;
        assert_eq!(ops, vec!["create", "read", "list"]);
    }

    /// No explicit config and no graph nodes — fall back to defaults.
    #[tokio::test]
    async fn defaults_used_when_no_config_and_no_graph() {
        let querier = MockEngine::new();
        let config = test_config();
        let ops = resolve_entity_operations(&querier, &config, "compliance", "Unknown").await;
        assert_eq!(ops, vec!["create", "read", "update", "delete", "list"]);
    }

    /// Defaults apply to entities with a config block but no explicit
    /// operations when the graph has nothing either.
    #[tokio::test]
    async fn defaults_used_for_unconfigured_entity_with_empty_graph() {
        let querier = MockEngine::new();
        let config = test_config();
        let ops = resolve_entity_operations(&querier, &config, "compliance", "Document").await;
        assert_eq!(ops, vec!["create", "read", "update", "delete", "list"]);
    }

    #[test]
    fn normalized_resource_name_strips_and_pascals() {
        assert_eq!(
            normalized_resource_name("ScreeningResult"),
            "ScreeningResult"
        );
        assert_eq!(
            normalized_resource_name("Screening Result"),
            "ScreeningResult"
        );
        assert_eq!(
            normalized_resource_name("review decisionType"),
            "ReviewDecision"
        );
    }
}
