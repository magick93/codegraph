use std::collections::HashMap;

use codegraph_core::error::GraphError;
use codegraph_core::types::ChartDefinitionNode;

use super::gql::opt_to_grafeo_value;
use super::registry;
use crate::engine::GrafeoEngine;

impl GrafeoEngine {
    /// Insert (upsert-by-name) one ChartDefinition node. The resolved
    /// entity title rides the node so readback can skip re-resolution;
    /// the binding edges land via `ingest_edge` from the ingest bridge.
    pub(super) async fn insert_chart_definition(
        &self,
        node: &ChartDefinitionNode,
    ) -> Result<String, GraphError> {
        let id = format!("chart:{}", node.name);
        let props = HashMap::from([
            (
                "name".to_string(),
                grafeo::Value::String(node.name.clone().into()),
            ),
            (
                "kind".to_string(),
                grafeo::Value::String(node.kind.as_str().into()),
            ),
            ("title".to_string(), opt_to_grafeo_value(&node.title)),
            ("domain".to_string(), opt_to_grafeo_value(&node.domain)),
            ("entity".to_string(), opt_to_grafeo_value(&node.entity)),
            (
                "entity_title".to_string(),
                opt_to_grafeo_value(&node.entity_title),
            ),
            ("route".to_string(), opt_to_grafeo_value(&node.route)),
            (
                "config_json".to_string(),
                opt_to_grafeo_value(&node.config_json),
            ),
        ]);
        self.create_registered_node(
            "ChartDefinition",
            &[registry::keys::chart(&node.name)],
            &props,
        )?;
        Ok(id)
    }
}
