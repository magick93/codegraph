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
                        domain_name, resource.path_segment, suffix
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
/// Tries the graph-based API model first, falls back to EntityConfig.
/// When no ApiResource nodes exist in the graph, this produces the same
/// result as the previous `entity_cfg.operations.unwrap_or(defaults)` pattern.
pub async fn resolve_entity_operations(
    querier: &dyn GraphQuerier,
    config: &DomainConfig,
    domain_name: &str,
    entity_name: &str,
) -> Vec<String> {
    let resource_name = entity_name.trim_end_matches("Type");
    if let Ok(resources) = querier.get_api_resources().await {
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

    let domain_entry = config.domains.get(domain_name);
    domain_entry
        .and_then(|de| de.get_entity_config(entity_name))
        .and_then(|c| c.operations.clone())
        .unwrap_or_else(|| config.defaults.operations.clone())
}

fn op_kind_to_http(kind: &str) -> (&'static str, &'static str) {
    match kind {
        "list" => ("GET", ""),
        "create" => ("POST", ""),
        "read" => ("GET", "/{id}"),
        "update" => ("PUT", "/{id}"),
        "delete" => ("DELETE", "/{id}"),
        _ => ("POST", ""),
    }
}

/// Resolve the URL path segment for an entity.
/// Priority: EntityConfig.path_segment > SchemaNode.api_path_segment > entity_name (lowercase)
pub fn resolve_path_segment(
    ec: Option<&EntityConfig>,
    schema_node: &SchemaNode,
) -> String {
    ec.and_then(|c| c.path_segment.clone())
        .unwrap_or_else(|| {
            if schema_node.api_path_segment.is_empty() {
                schema_node.title.to_lowercase()
            } else {
                schema_node.api_path_segment.clone()
            }
        })
}
