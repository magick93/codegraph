use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::SchemaNode;
use serde::Serialize;

use crate::api::api_model::{
    normalized_resource_name_with, resolve_entity_operations, resolve_path_segment,
};

/// Resolved API surface for the entity an IFML component binds to.
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedApi {
    /// Collection path including version + domain, e.g. `/api/v1/sales/customer`.
    pub base_path: String,
    pub has_list: bool,
    pub has_create: bool,
    pub has_read: bool,
    pub has_update: bool,
    pub has_delete: bool,
}

/// Resolve the API base path + CRUD operations for an IFML component entity.
///
/// Resolution order:
/// 1. Schema node in the graph (entity name or `{entity}Type`) — path segment
///    via `resolve_path_segment` (EntityConfig override > `api_path_segment` >
///    lowercase title), matching the routes the API generators emit.
/// 2. ApiResource nodes from the ingested API model (available even when no
///    schemas were loaded).
/// 3. Legacy guess `/api/{version}/{entity-lowercase}` when neither exists.
pub async fn resolve_entity_api(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    entity: &str,
    api_version: &str,
) -> Option<ResolvedApi> {
    if let Some((domain, schema)) = resolve_schema_with_domain(db, config, entity).await {
        let ec = config
            .domains
            .get(&domain)
            .and_then(|de| de.get_entity_config(&schema.title));
        let segment = resolve_path_segment(ec, &schema);
        let ops = resolve_entity_operations(db, config, &domain, &schema.title).await;
        return Some(from_ops(
            &format!("/api/{api_version}/{domain}/{segment}"),
            &ops,
        ));
    }

    if let Ok(resources) = db.get_api_resources().await {
        if !resources.is_empty() {
            let target = normalized_resource_name_with(entity, &config.defaults.type_suffix);
            let suffixed = format!("{entity}Type");
            let resource = resources.iter().find(|r| {
                r.schema_title == entity || r.schema_title == suffixed || r.name == target
            });
            if let Some(resource) = resource {
                // api_ingest defaults path_segment to the raw entity name
                // (e.g. "CustomerType"); the actual route uses the normalized
                // resource name, so only honor an explicit override.
                let segment = if resource.path_segment == resource.schema_title {
                    resource.name.to_lowercase()
                } else {
                    resource.path_segment.clone()
                };
                let ops: Vec<String> = db
                    .get_api_operations(&resource.name)
                    .await
                    .map(|ops| ops.iter().map(|op| op.kind.clone()).collect())
                    .unwrap_or_else(|_| config.defaults.operations.clone());
                return Some(from_ops(
                    &format!("/api/{api_version}/{}/{segment}", resource.domain),
                    &ops,
                ));
            }
        }
    }

    // Legacy guess: assume full CRUD like the pre-API-model templates did.
    Some(ResolvedApi {
        base_path: format!("/api/{api_version}/{}", entity.to_lowercase()),
        has_list: true,
        has_create: true,
        has_read: true,
        has_update: true,
        has_delete: true,
    })
}

fn from_ops(base_path: &str, ops: &[String]) -> ResolvedApi {
    ResolvedApi {
        base_path: base_path.to_string(),
        has_list: ops.iter().any(|op| op == "list"),
        has_create: ops.iter().any(|op| op == "create"),
        has_read: ops.iter().any(|op| op == "read"),
        has_update: ops.iter().any(|op| op == "update"),
        has_delete: ops.iter().any(|op| op == "delete"),
    }
}

async fn resolve_schema_with_domain(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    entity: &str,
) -> Option<(String, SchemaNode)> {
    let schema = match db.get_schema(entity).await {
        Ok(Some(schema)) => schema,
        _ => match db.get_schema(&format!("{entity}Type")).await {
            Ok(Some(schema)) => schema,
            _ => return None,
        },
    };
    let domain = schema
        .domain
        .clone()
        .or_else(|| find_domain_for_entity(config, &schema.title))?;
    Some((domain, schema))
}

fn find_domain_for_entity(config: &DomainConfig, title: &str) -> Option<String> {
    config
        .domains
        .iter()
        .find(|(_, entry)| entry.entities.iter().any(|e| e == title))
        .map(|(name, _)| name.clone())
}

/// The view parameter that carries the entity id (edit mode), if any.
pub fn id_param_from(params: &[super::context::ParameterDef]) -> Option<String> {
    params
        .iter()
        .find(|p| p.name.to_ascii_lowercase().ends_with("id"))
        .map(|p| p.name.clone())
        .or_else(|| params.first().map(|p| p.name.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::api_model::normalized_resource_name;

    #[test]
    fn id_param_prefers_id_suffixed_names() {
        use crate::ifml::context::ParameterDef;
        let params = vec![
            ParameterDef {
                name: "region".to_string(),
                type_ref: "String".to_string(),
                default: None,
            },
            ParameterDef {
                name: "customerId".to_string(),
                type_ref: "Uuid".to_string(),
                default: None,
            },
        ];
        assert_eq!(id_param_from(&params).as_deref(), Some("customerId"));
        assert_eq!(id_param_from(&[]), None);
    }

    #[test]
    fn normalized_names_match_api_resources() {
        assert_eq!(normalized_resource_name("Customer"), "Customer");
        assert_eq!(normalized_resource_name("CustomerType"), "Customer");
    }
}
