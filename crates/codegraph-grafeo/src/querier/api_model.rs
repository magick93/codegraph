use std::collections::HashMap;

use codegraph_core::error::GraphError;
use codegraph_core::types::{ApiOperationNode, ApiResourceNode, HttpEndpointNode, InteractionNode};

use super::{query_gql, query_gql_params};
use crate::conversions::RowReader;
use crate::engine::GrafeoEngine;

impl GrafeoEngine {
    // ── API metamodel query methods ────────────────────────────────────

    pub(super) async fn query_api_resources(&self) -> Result<Vec<ApiResourceNode>, GraphError> {
        let gql = "MATCH (r:ApiResource) RETURN \
            r.name, r.schema_title, r.domain, r.label, r.path_segment \
            ORDER BY r.name";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            nodes.push(ApiResourceNode {
                name: reader.get_string(row, "r.name")?,
                schema_title: reader.get_string(row, "r.schema_title")?,
                domain: reader.get_string(row, "r.domain")?,
                label: reader.get_opt_string(row, "r.label")?,
                path_segment: reader.get_string(row, "r.path_segment")?,
            });
        }
        Ok(nodes)
    }

    pub(super) async fn query_api_resource(&self, name: &str) -> Result<Option<ApiResourceNode>, GraphError> {
        let escaped = name.replace('\'', "\\'");
        let gql = format!(
            "MATCH (r:ApiResource {{name: '{escaped}'}}) RETURN \
            r.name, r.schema_title, r.domain, r.label, r.path_segment"
        );
        let result = query_gql(self, &gql)?;
        if result.rows.is_empty() {
            return Ok(None);
        }
        let reader = RowReader::from_columns(&result.columns);
        let row = &result.rows[0];
        Ok(Some(ApiResourceNode {
            name: reader.get_string(row, "r.name")?,
            schema_title: reader.get_string(row, "r.schema_title")?,
            domain: reader.get_string(row, "r.domain")?,
            label: reader.get_opt_string(row, "r.label")?,
            path_segment: reader.get_string(row, "r.path_segment")?,
        }))
    }

    pub(super) async fn query_api_operations(
        &self,
        resource_name: &str,
    ) -> Result<Vec<ApiOperationNode>, GraphError> {
        let escaped = resource_name.replace('\'', "\\'");
        let gql = format!(
            "MATCH (r:ApiResource {{name: '{escaped}'}})-[:HasOperation]->(op:ApiOperation) \
             RETURN op.name, op.kind, op.input_schema, op.output_schema, \
             op.paging, op.sorting, op.filtering, op.domain \
             ORDER BY op.name"
        );
        let result = query_gql(self, &gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            nodes.push(ApiOperationNode {
                name: reader.get_string(row, "op.name")?,
                kind: reader.get_string(row, "op.kind")?,
                input_schema: reader.get_opt_string(row, "op.input_schema")?,
                output_schema: reader.get_string(row, "op.output_schema")?,
                paging: reader.get_bool(row, "op.paging")?,
                sorting: reader.get_bool(row, "op.sorting")?,
                filtering: reader.get_bool(row, "op.filtering")?,
                domain: reader.get_opt_string(row, "op.domain")?,
            });
        }
        Ok(nodes)
    }

    pub(super) async fn query_api_operation(&self, name: &str) -> Result<Option<ApiOperationNode>, GraphError> {
        let params = HashMap::from([("name".to_string(), grafeo::Value::String(name.into()))]);
        let result = query_gql_params(
            self,
            "MATCH (op:ApiOperation {name: $name}) RETURN \
             op.name, op.kind, op.input_schema, op.output_schema, \
             op.paging, op.sorting, op.filtering, op.domain",
            params,
        )?;
        if result.rows.is_empty() {
            return Ok(None);
        }
        let reader = RowReader::from_columns(&result.columns);
        let row = &result.rows[0];
        Ok(Some(ApiOperationNode {
            name: reader.get_string(row, "op.name")?,
            kind: reader.get_string(row, "op.kind")?,
            input_schema: reader.get_opt_string(row, "op.input_schema")?,
            output_schema: reader.get_string(row, "op.output_schema")?,
            paging: reader.get_bool(row, "op.paging")?,
            sorting: reader.get_bool(row, "op.sorting")?,
            filtering: reader.get_bool(row, "op.filtering")?,
            domain: reader.get_opt_string(row, "op.domain")?,
        }))
    }

    pub(super) async fn query_http_endpoint_for_operation(
        &self,
        operation_name: &str,
    ) -> Result<Option<HttpEndpointNode>, GraphError> {
        let params = HashMap::from([(
            "name".to_string(),
            grafeo::Value::String(operation_name.into()),
        )]);
        let result = query_gql_params(
            self,
            "MATCH (op:ApiOperation {name: $name})-[:HasInteraction]->(ia:Interaction) \
             -[:BindsHttpEndpoint]->(he:HttpEndpoint) \
             RETURN he.method, he.path_template, he.domain",
            params,
        )?;
        if result.rows.is_empty() {
            return Ok(None);
        }
        let reader = RowReader::from_columns(&result.columns);
        let row = &result.rows[0];
        Ok(Some(HttpEndpointNode {
            method: reader.get_string(row, "he.method")?,
            path_template: reader.get_string(row, "he.path_template")?,
            domain: reader.get_opt_string(row, "he.domain")?,
        }))
    }

    pub(super) async fn query_interactions(
        &self,
        _operation_name: &str,
    ) -> Result<Vec<InteractionNode>, GraphError> {
        let gql = "MATCH (ia:Interaction) RETURN ia.transport, ia.domain ORDER BY ia.transport";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            nodes.push(InteractionNode {
                transport: reader.get_string(row, "ia.transport")?,
                domain: reader.get_opt_string(row, "ia.domain")?,
            });
        }
        Ok(nodes)
    }

    pub(super) async fn query_http_endpoints(&self) -> Result<Vec<HttpEndpointNode>, GraphError> {
        let gql =
            "MATCH (he:HttpEndpoint) RETURN he.method, he.path_template, he.domain ORDER BY he.path_template";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            nodes.push(HttpEndpointNode {
                method: reader.get_string(row, "he.method")?,
                path_template: reader.get_string(row, "he.path_template")?,
                domain: reader.get_opt_string(row, "he.domain")?,
            });
        }
        Ok(nodes)
    }
}
