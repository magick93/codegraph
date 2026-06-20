use crate::error::GraphError;
use crate::types::{
    ActionNode, CodeList, CompositeColumn, CompositeRange, CompositionTree, EventNode, EnumValue,
    Extension, ParentCandidate, ParameterDefinitionNode, PropertyNode, SchemaClassificationData,
    SchemaNode, StructuredSubField, ViewComponentNode, ViewContainerNode,
};
use async_trait::async_trait;
use std::collections::HashMap;

#[async_trait]
pub trait GraphQuerier: Send + Sync {
    async fn get_schema(&self, title: &str) -> Result<Option<SchemaNode>, GraphError>;
    async fn get_schema_by_id(&self, schema_id: &str) -> Result<Option<SchemaNode>, GraphError> {
        Ok(None)
    }

    async fn get_schema_in_domain(
        &self,
        title: &str,
        domain: &str,
    ) -> Result<Option<SchemaNode>, GraphError> {
        self.get_schema(title).await
    }

    async fn list_schemas(&self, domain: Option<&str>) -> Result<Vec<SchemaNode>, GraphError>;
    async fn get_properties(&self, schema_title: &str) -> Result<Vec<PropertyNode>, GraphError>;

    /// Fetch properties for a schema, restricting to a specific domain when
    /// multiple schemas share the same title across domains.
    /// Default implementation falls back to `get_properties`.
    async fn get_properties_in_domain(
        &self,
        schema_title: &str,
        _domain: &str,
    ) -> Result<Vec<PropertyNode>, GraphError> {
        self.get_properties(schema_title).await
    }

    async fn get_child_schemas(&self, schema_title: &str) -> Result<Vec<SchemaNode>, GraphError>;

    async fn get_classification_data(&self) -> Result<Vec<SchemaClassificationData>, GraphError>;
    async fn get_entity_names(&self) -> Result<Vec<String>, GraphError>;
    async fn get_entity_schema_map(&self) -> Result<HashMap<String, String>, GraphError>;
    async fn get_value_object_schemas(&self) -> Result<Vec<SchemaNode>, GraphError>;
    async fn get_parent_candidates(&self) -> Result<Vec<ParentCandidate>, GraphError>;

    async fn get_codelist(&self, name: &str) -> Result<Option<CodeList>, GraphError>;
    async fn list_codelists(&self) -> Result<Vec<CodeList>, GraphError>;
    async fn get_enum_values(&self, codelist_name: &str) -> Result<Vec<EnumValue>, GraphError>;

    async fn get_composite_columns(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Vec<CompositeColumn>, GraphError>;
    async fn get_structured_sub_fields(
        &self,
        schema_title: &str,
    ) -> Result<Vec<StructuredSubField>, GraphError>;
    async fn get_composite_range(
        &self,
        schema_title: &str,
    ) -> Result<Option<CompositeRange>, GraphError>;
    async fn get_consumed_fields(
        &self,
        schema_title: &str,
    ) -> Result<Vec<(PropertyNode, String)>, GraphError>;
    async fn get_codelist_for_property(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Option<(CodeList, String)>, GraphError>;

    async fn get_required_extensions(
        &self,
        schema_title: &str,
    ) -> Result<Vec<Extension>, GraphError>;

    async fn get_composition_tree(&self, schema_title: &str)
        -> Result<CompositionTree, GraphError>;
    async fn get_allof_targets(&self, schema_title: &str) -> Result<Vec<String>, GraphError>;
    /// Get all schemas that have an ExtendsSchema edge pointing to the given parent
    /// title — the reverse of `get_allof_targets`. Returns full SchemaNode objects
    /// so callers can inspect `is_entity`, `pg_table_name`, etc.
    ///
    /// For example, if both PersonType (entity) and PersonLegalType (VO) allOf-compose
    /// PersonBaseType, then `get_schemas_that_extend("PersonBaseType")` returns both.
    async fn get_schemas_that_extend(&self, parent_title: &str) -> Result<Vec<SchemaNode>, GraphError> {
        Ok(Vec::new())
    }
    async fn get_referencing_schemas(&self, schema_title: &str) -> Result<Vec<String>, GraphError>;
    async fn get_referenced_schemas(&self, schema_title: &str) -> Result<Vec<SchemaNode>, GraphError>;
    async fn get_property_ref_target(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Option<SchemaNode>, GraphError>;

    /// Like get_property_ref_target but matches by _schema_id (unique) instead of _schema_title.
    async fn get_property_ref_target_by_id(
        &self,
        property_name: &str,
        schema_id: &str,
    ) -> Result<Option<SchemaNode>, GraphError> {
        // Default: delegate to title-based version
        self.get_property_ref_target(property_name, schema_id).await
    }

    /// Get properties associated with a schema by its unique schema_id (not title).
    async fn get_properties_by_schema_id(
        &self,
        schema_id: &str,
    ) -> Result<Vec<PropertyNode>, GraphError> {
        // Default: return empty
        Ok(Vec::new())
    }

    async fn get_array_item_schema(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Option<SchemaNode>, GraphError>;

    async fn get_generation_order(&self) -> Result<Vec<String>, GraphError>;

    /// Bulk-fetch all schema→schema reference edges in one query.
    ///
    /// Returns `(source_title, target_title)` pairs for every
    /// `Schema -[:HasProperty]-> Property -[:ReferencesSchema]-> Schema` path.
    /// Implementations should use a single graph query instead of per-entity calls.
    /// Default implementation falls back to per-entity `get_referenced_schemas`.
    async fn list_all_schema_references(&self) -> Result<Vec<(String, String)>, GraphError> {
        let schemas = self.list_schemas(None).await?;
        let mut refs = Vec::new();
        for s in &schemas {
            if let Ok(targets) = self.get_referenced_schemas(&s.title).await {
                for t in &targets {
                    refs.push((s.title.clone(), t.title.clone()));
                }
            }
        }
        Ok(refs)
    }

    /// Bulk-fetch all properties keyed by schema title.
    ///
    /// Default implementation falls back to per-schema `get_properties`.
    async fn list_all_properties(&self) -> Result<HashMap<String, Vec<PropertyNode>>, GraphError> {
        let schemas = self.list_schemas(None).await?;
        let mut map = HashMap::new();
        for s in &schemas {
            let props = self.get_properties(&s.title).await?;
            if !props.is_empty() {
                map.insert(s.title.clone(), props);
            }
        }
        Ok(map)
    }

    // ── IFML query methods (default: no IFML data) ─────────────────────

    /// Get all IFML ViewContainer nodes.
    async fn get_ifml_view_containers(&self) -> Result<Vec<ViewContainerNode>, GraphError> {
        Ok(Vec::new())
    }

    /// Get all ViewComponent nodes inside a container.
    async fn get_ifml_view_components(
        &self,
        _container_name: &str,
    ) -> Result<Vec<ViewComponentNode>, GraphError> {
        Ok(Vec::new())
    }

    /// Get all Event nodes for a given parent element.
    async fn get_ifml_events(
        &self,
        _parent_id: &str,
    ) -> Result<Vec<EventNode>, GraphError> {
        Ok(Vec::new())
    }

    /// Get NavigationFlow edges: (source_element, source_event, target_container).
    async fn get_ifml_navigation_flows(
        &self,
    ) -> Result<Vec<(String, String, String)>, GraphError> {
        Ok(Vec::new())
    }

    /// Get DataFlow edges: (source, target, source_param, target_param).
    async fn get_ifml_data_flows(
        &self,
    ) -> Result<Vec<(String, String, Option<String>, Option<String>)>, GraphError> {
        Ok(Vec::new())
    }

    /// Get all ActionNode definitions.
    async fn get_ifml_actions(&self) -> Result<Vec<ActionNode>, GraphError> {
        Ok(Vec::new())
    }

    /// Get all ParameterDefinition nodes.
    async fn get_ifml_parameters(&self) -> Result<Vec<ParameterDefinitionNode>, GraphError> {
        Ok(Vec::new())
    }
}

/// Check if a VO (value object) schema extends an entity through its allOf
/// composition chain. Traverses: VO → its allOf parents → schemas that extend
/// those parents → first entity found (excluding the VO itself).
///
/// Example: PersonLegalType (VO) allOf → [PersonBaseType, PersonLegalInclusion].
/// PersonType (entity) also allOf → PersonBaseType and PersonLegalInclusion.
/// `find_entity_extended_by_vo(db, "PersonLegalType")` returns PersonType.
pub async fn find_entity_extended_by_vo(
    db: &dyn GraphQuerier,
    vo_title: &str,
) -> Result<Option<SchemaNode>, GraphError> {
    let allof_targets = db.get_allof_targets(vo_title).await?;
    for parent_def in &allof_targets {
        if let Ok(extenders) = db.get_schemas_that_extend(parent_def).await {
            for extender in &extenders {
                if extender.title != vo_title
                    && extender.is_entity
                    && !extender.pg_table_name.is_empty()
                {
                    if let Some(auth) = db.get_schema_by_id(&extender.schema_id).await? {
                        return Ok(Some(auth));
                    }
                    return Ok(Some(extender.clone()));
                }
            }
        }
    }
    Ok(None)
}
