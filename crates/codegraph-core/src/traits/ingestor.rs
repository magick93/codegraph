use crate::error::GraphError;
use crate::types::{
    CodeList, CompositeColumn, CompositeRange, EdgeProperties, EdgeType, EnumValue, IngestStats,
    PropertyNode, SchemaNode,
};
use async_trait::async_trait;

#[async_trait]
pub trait GraphIngestor: Send + Sync {
    async fn ingest_schema(&self, node: &SchemaNode) -> Result<String, GraphError>;

    async fn ingest_schemas(&self, nodes: &[SchemaNode]) -> Result<Vec<String>, GraphError> {
        let mut ids = Vec::with_capacity(nodes.len());
        for node in nodes {
            ids.push(self.ingest_schema(node).await?);
        }
        Ok(ids)
    }

    async fn ingest_property(
        &self,
        schema_title: &str,
        prop: &PropertyNode,
    ) -> Result<(), GraphError>;

    async fn ingest_codelist(&self, codelist: &CodeList) -> Result<(), GraphError>;

    async fn ingest_enum_value(
        &self,
        codelist_name: &str,
        value: &EnumValue,
    ) -> Result<(), GraphError>;

    async fn ingest_composite_column(&self, col: &CompositeColumn) -> Result<(), GraphError>;

    async fn ingest_composite_range(&self, range: &CompositeRange) -> Result<(), GraphError>;

    async fn ingest_extension(&self, name: &str) -> Result<(), GraphError>;

    async fn ingest_edge(
        &self,
        from_id: &str,
        to_id: &str,
        edge_type: EdgeType,
        props: Option<&EdgeProperties>,
    ) -> Result<(), GraphError>;

    async fn finalize(&self) -> Result<IngestStats, GraphError>;

    /// Update the is_entity flag on an already-ingested schema node.
    async fn update_entity_flag(&self, title: &str, is_entity: bool) -> Result<(), GraphError>;

    /// Update the classification kind on an already-ingested property node.
    async fn update_property_classification(
        &self,
        schema_title: &str,
        property_name: &str,
        kind: &str,
    ) -> Result<(), GraphError>;
}
