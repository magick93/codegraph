use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::SchemaNode;
use serde::Serialize;

use crate::api::api_model::{
    normalized_resource_name, resolve_entity_operations, resolve_path_segment,
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
        let mut api = from_ops(&format!("/api/{api_version}/{domain}/{segment}"), &ops);
        // Non-root read models (contained entities, value objects) have no
        // top-level list/read route — the API plane only nests child CRUD
        // under the owning aggregate. Leaving `has_list`/`has_read` set made
        // generated list views fetch a 404 before the persona guard could
        // act; zero the surface so the view renders an EMPTY shape instead.
        if entity_is_non_root(db, config, &domain, &schema.title).await {
            api.has_list = false;
            api.has_read = false;
            api.has_create = false;
            api.has_update = false;
            api.has_delete = false;
        }
        return Some(api);
    }

    if let Ok(resources) = db.get_api_resources().await
        && !resources.is_empty()
    {
        let target = normalized_resource_name(entity);
        let suffixed = format!("{entity}Type");
        let resource = resources
            .iter()
            .find(|r| r.schema_title == entity || r.schema_title == suffixed || r.name == target);
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

/// Whether the entity is a non-root read model: a contained entity/value
/// object with no top-level API surface. Resolution mirrors the router's
/// role rules (config `role = "child"`, non-entity schemas, and graph parent
/// candidates).
async fn entity_is_non_root(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    schema_title: &str,
) -> bool {
    if let Ok(Some(schema)) = db.get_schema_in_domain(schema_title, domain).await
        && !schema.is_entity
    {
        return true;
    }
    if config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(schema_title))
        .and_then(|ec| ec.role.as_deref())
        == Some("child")
    {
        return true;
    }
    // Graph parent candidates do NOT make a root entity nested: the
    // router/handler only nest a config `role = "child"` entity, and this
    // function's schema/role checks above cover those. Treating a scalar
    // `refers` candidate as a child zeroed the top-level surface of root
    // entities (the WorkOrder regression).
    false
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

    fn schema(title: &str, domain: &str, is_entity: bool) -> SchemaNode {
        SchemaNode {
            namespace: None,
            schema_id: format!("id:{title}"),
            title: title.to_string(),
            access: None,
            annotations: None,
            description: None,
            schema_type: "object".to_string(),
            classification: if is_entity { "entity" } else { "value_object" }.to_string(),
            domain: Some(domain.to_string()),
            rel_path: format!("{title}.json"),
            pg_type: "UUID".to_string(),
            rust_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            rust_type_name: title.to_string(),
            pg_table_name: codegraph_naming::to_snake_case(title),
            api_path_segment: codegraph_naming::to_kebab_case(title),
            parent_schema: None,
            is_entity,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
            custom_annotations: Default::default(),
        }
    }

    fn config() -> DomainConfig {
        toml::from_str(
            r#"
[domains.benefits]
label = "Benefits"
schema_dir = "benefits"
postgres_schema = "benefits"
entities = ["DependentType"]
"#,
        )
        .unwrap()
    }

    fn config_with_child_role() -> DomainConfig {
        toml::from_str(
            r#"
[domains.benefits]
label = "Benefits"
schema_dir = "benefits"
postgres_schema = "benefits"
entities = ["DependentType"]

[domains.benefits.entity_config.DependentType]
role = "child"
parent = "CoverageType"
"#,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn non_root_child_has_no_top_level_list_or_read() {
        let engine = codegraph_core::mock::MockEngine::builder()
            .with_schema(schema("DependentType", "benefits", true))
            .build();
        let api = resolve_entity_api(&engine, &config_with_child_role(), "Dependent", "v1")
            .await
            .expect("api");
        assert!(
            !api.has_list,
            "non-root list must not fetch a top-level route"
        );
        assert!(!api.has_read);
        assert!(!api.has_create);
    }

    #[tokio::test]
    async fn non_entity_value_object_has_no_top_level_list_or_read() {
        let engine = codegraph_core::mock::MockEngine::builder()
            .with_schema(schema("DependentType", "benefits", false))
            .build();
        let api = resolve_entity_api(&engine, &config(), "Dependent", "v1")
            .await
            .expect("api");
        assert!(!api.has_list);
        assert!(!api.has_read);
    }

    #[tokio::test]
    async fn root_entity_keeps_its_full_surface() {
        let engine = codegraph_core::mock::MockEngine::builder()
            .with_schema(schema("CoverageType", "benefits", true))
            .build();
        let api = resolve_entity_api(&engine, &config(), "Coverage", "v1")
            .await
            .expect("api");
        assert!(api.has_list);
        assert!(api.has_read);
    }
}
