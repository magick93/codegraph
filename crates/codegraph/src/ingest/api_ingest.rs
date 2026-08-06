use std::fmt;

use codegraph_config::config::DomainConfig;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{
    ApiOperationNode, ApiResourceNode, EdgeType, HttpEndpointNode, InteractionNode,
};

use crate::error::{Error, Result};

#[derive(Debug, Default)]
pub struct ApiModelIngestStats {
    pub resources: usize,
    pub operations: usize,
    pub interactions: usize,
    pub endpoints: usize,
    pub errors: usize,
}

impl fmt::Display for ApiModelIngestStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "API model: {} resources, {} operations, {} interactions, {} endpoints",
            self.resources, self.operations, self.interactions, self.endpoints
        )
    }
}

/// Ingest the API model into the graph from the domain configuration.
///
/// For every entity in every domain:
/// 1. Creates an ApiResource node (with path_segment from EntityConfig or auto-derived)
/// 2. Creates an ApiOperation node for each operation (create/read/update/delete/list)
/// 3. Creates an Interaction node linking the operation to HTTP transport
/// 4. Creates an HttpEndpoint node with method + path template
/// 5. Creates edges: BindsToSchema, HasOperation, OutputBoundTo, HasInteraction, BindsHttpEndpoint
pub async fn ingest_api_model(
    db: &dyn GraphIngestor,
    config: &DomainConfig,
) -> Result<ApiModelIngestStats> {
    let mut stats = ApiModelIngestStats::default();

    for (domain_name, domain_entry) in &config.domains {
        for entity_name in &domain_entry.entities {
            let ec = domain_entry.get_entity_config(entity_name);

            let path_segment = ec
                .and_then(|c| c.path_segment.as_deref())
                .unwrap_or(entity_name)
                .to_string();

            let operations = ec
                .and_then(|c| c.operations.as_ref())
                .unwrap_or(&config.defaults.operations);

            let schema_title = entity_name.clone();
            let resource_name = entity_name.trim_end_matches("Type");

            let resource_id = db
                .ingest_api_resource(&ApiResourceNode {
                    name: resource_name.to_string(),
                    schema_title: schema_title.clone(),
                    domain: domain_name.to_string(),
                    label: ec
                        .and_then(|c| c.tag.clone())
                        .or_else(|| Some(entity_name.to_string())),
                    path_segment: path_segment.clone(),
                })
                .await
                .map_err(|e| Error::Graph(e))?;
            stats.resources += 1;

            db.ingest_edge(
                &resource_id,
                &schema_title,
                EdgeType::BindsToSchema,
                None,
            )
            .await
            .map_err(|e| Error::Graph(e))?;

            let op_mappings: Vec<(&str, &str, &str)> = operations
                .iter()
                .filter_map(|op| match op.as_str() {
                    "list" => Some(("list", "GET", "")),
                    "create" => Some(("create", "POST", "")),
                    "read" => Some(("read", "GET", "/{id}")),
                    "update" => Some(("update", "PUT", "/{id}")),
                    "delete" => Some(("delete", "DELETE", "/{id}")),
                    _ => None,
                })
                .collect();

            let base_path = format!("/api/{}/{}", domain_name, path_segment);

            for (op_kind, method, path_suffix) in &op_mappings {
                let op_name = format!("{}_{}", op_kind, resource_name);
                let op_id = db
                    .ingest_api_operation(&ApiOperationNode {
                        name: op_name.clone(),
                        kind: op_kind.to_string(),
                        input_schema: None,
                        output_schema: schema_title.clone(),
                        paging: *op_kind == "list",
                        sorting: *op_kind == "list",
                        filtering: *op_kind == "list",
                        domain: Some(domain_name.to_string()),
                    })
                    .await
                    .map_err(|e| Error::Graph(e))?;
                stats.operations += 1;

                db.ingest_edge(&resource_id, &op_id, EdgeType::HasOperation, None)
                    .await
                    .map_err(|e| Error::Graph(e))?;

                db.ingest_edge(&op_id, &schema_title, EdgeType::OutputBoundTo, None)
                    .await
                    .map_err(|e| Error::Graph(e))?;

                let interaction_id = db
                    .ingest_interaction(&InteractionNode {
                        transport: "http".to_string(),
                        domain: Some(domain_name.to_string()),
                    })
                    .await
                    .map_err(|e| Error::Graph(e))?;
                stats.interactions += 1;

                db.ingest_edge(&op_id, &interaction_id, EdgeType::HasInteraction, None)
                    .await
                    .map_err(|e| Error::Graph(e))?;

                let path_template = format!("{}{}", base_path, path_suffix);
                let endpoint_id = db
                    .ingest_http_endpoint(&HttpEndpointNode {
                        method: method.to_string(),
                        path_template,
                        domain: Some(domain_name.to_string()),
                    })
                    .await
                    .map_err(|e| Error::Graph(e))?;
                stats.endpoints += 1;

                db.ingest_edge(
                    &interaction_id,
                    &endpoint_id,
                    EdgeType::BindsHttpEndpoint,
                    None,
                )
                .await
                .map_err(|e| Error::Graph(e))?;
            }
        }
    }

    Ok(stats)
}
