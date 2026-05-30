use std::collections::HashMap;

use async_trait::async_trait;
use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphQuerier;

use super::context::*;

/// Trait for querying the IFML model from the graph
#[async_trait]
pub trait IfmlQuerier: Send + Sync {
    /// Get the full IFML model with all resolved dependencies
    async fn get_ifml_model(&self) -> Result<IfmlModel, GraphError>;

    /// Get all view containers
    async fn get_view_containers(&self) -> Result<Vec<IfmlViewContainer>, GraphError>;

    /// Get a specific view container by name
    async fn get_view_container(&self, name: &str) -> Result<Option<IfmlViewContainer>, GraphError>;

    /// Get all navigation edges between containers
    async fn get_navigation_edges(&self) -> Result<Vec<NavigationEdge>, GraphError>;

    /// Get all data flow edges between elements
    async fn get_data_flows(&self) -> Result<Vec<DataFlowEdge>, GraphError>;

    /// Get all action definitions
    async fn get_actions(&self) -> Result<Vec<IfmlActionDef>, GraphError>;

    /// Compute topological generation order (Kahn's algorithm)
    /// Targets before sources (so routes exist before they're referenced)
    async fn compute_generation_order(&self) -> Result<Vec<String>, GraphError>;
}

/// Implementation that queries the Grafeo graph
pub struct IfmlGraphQuerier<'a> {
    db: &'a dyn GraphQuerier,
}

impl<'a> IfmlGraphQuerier<'a> {
    pub fn new(db: &'a dyn GraphQuerier) -> Self {
        Self { db }
    }
}

#[async_trait]
impl<'a> IfmlQuerier for IfmlGraphQuerier<'a> {
    async fn get_ifml_model(&self) -> Result<IfmlModel, GraphError> {
        let view_containers = self.get_view_containers().await?;
        let actions = self.get_actions().await?;
        let navigation_edges = self.get_navigation_edges().await?;
        let data_flows = self.get_data_flows().await?;
        let generation_order = self.compute_generation_order().await?;

        Ok(IfmlModel {
            view_containers,
            actions,
            navigation_edges,
            data_flows,
            generation_order,
        })
    }

    async fn get_view_containers(&self) -> Result<Vec<IfmlViewContainer>, GraphError> {
        // TODO: When GraphQuerier has IFML-specific methods, replace this with:
        //   self.db.get_view_containers().await?
        // For now, query via the generic mechanism: list schemas that have an IFML
        // view container link in the graph metadata.
        let schemas = self
            .db
            .list_schemas(None)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        let mut containers = Vec::new();
        for schema in &schemas {
            // TODO: Check if schema has an associated ViewContainer node in the graph.
            // When IFML is ingested, each ViewContainer node will reference the schema
            // it renders. Use self.db.get_schema() to find linked view definitions.
            //
            // For now, emit a minimal container placeholder:
            containers.push(IfmlViewContainer {
                name: schema.title.clone(),
                label: None,
                is_xor: false,
                is_default: false,
                is_landmark: false,
                is_modal: false,
                params: Vec::new(),
                components: Vec::new(),
                events: Vec::new(),
                containers: Vec::new(),
            });
        }

        Ok(containers)
    }

    async fn get_view_container(&self, name: &str) -> Result<Option<IfmlViewContainer>, GraphError> {
        // TODO: When GraphQuerier has IFML-specific methods, replace this with:
        //   self.db.get_view_container(name).await?
        // For now, look up via the schema list.
        let schema = self.db.get_schema(name).await?;
        match schema {
            Some(s) => Ok(Some(IfmlViewContainer {
                name: s.title.clone(),
                label: None,
                is_xor: false,
                is_default: false,
                is_landmark: false,
                is_modal: false,
                params: Vec::new(),
                components: Vec::new(),
                events: Vec::new(),
                containers: Vec::new(),
            })),
            None => Ok(None),
        }
    }

    async fn get_navigation_edges(&self) -> Result<Vec<NavigationEdge>, GraphError> {
        // TODO: When GraphQuerier has IFML-specific methods, replace this with:
        //   self.db.get_navigation_edges().await?
        // For now, derive navigation from entity relationships.
        // Query parent-child relationships between schemas as a proxy.
        let all_refs = self
            .db
            .list_all_schema_references()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        let mut edges = Vec::new();
        for (source, target) in &all_refs {
            edges.push(NavigationEdge {
                source_container: source.clone(),
                source_event: String::new(),
                target_container: target.clone(),
                parameter_binding: HashMap::new(),
                conditional_expression: None,
            });
        }

        Ok(edges)
    }

    async fn get_data_flows(&self) -> Result<Vec<DataFlowEdge>, GraphError> {
        // TODO: When GraphQuerier has IFML-specific methods, replace this with:
        //   self.db.get_data_flows().await?
        // For now, derive from schema reference edges.
        let all_refs = self
            .db
            .list_all_schema_references()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        let mut edges = Vec::new();
        for (source, target) in &all_refs {
            edges.push(DataFlowEdge {
                source_element: source.clone(),
                target_element: target.clone(),
                source_param: None,
                target_param: None,
            });
        }

        Ok(edges)
    }

    async fn get_actions(&self) -> Result<Vec<IfmlActionDef>, GraphError> {
        // TODO: When GraphQuerier has IFML-specific methods, replace this with:
        //   self.db.get_actions().await?
        // For now, return empty — actions are pure IFML concepts not yet in the graph.
        Ok(Vec::new())
    }

    async fn compute_generation_order(&self) -> Result<Vec<String>, GraphError> {
        let navigation_edges = self.get_navigation_edges().await?;

        let edge_pairs: Vec<(String, String)> = navigation_edges
            .into_iter()
            .map(|e| (e.source_container, e.target_container))
            .collect();

        Ok(super::dependency_graph::compute_view_generation_order(&edge_pairs))
    }
}
