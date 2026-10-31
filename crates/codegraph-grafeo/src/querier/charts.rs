use codegraph_core::error::GraphError;
use codegraph_core::types::{ChartDefinitionNode, ChartKind, ChartPropertyBinding};

use super::query::query_many;
use crate::conversions::RowReader;
use crate::engine::GrafeoEngine;

impl GrafeoEngine {
    /// All chart definitions, deterministic by `(domain, name)` — the
    /// readback order the generator emits routes in.
    pub(super) async fn query_chart_definitions(
        &self,
    ) -> Result<Vec<ChartDefinitionNode>, GraphError> {
        query_many(
            self,
            "MATCH (c:ChartDefinition) RETURN \
            c.name, c.kind, c.title, c.domain, c.entity, \
            c.entity_title, c.route, c.config_json \
            ORDER BY c.domain, c.name",
            chart_definition_from_row,
        )
        .await
    }

    /// All `(chart_name, field_name, schema_title, role)` property bindings
    /// for chart definitions — the referential surface (issue #577).
    pub(super) async fn query_chart_property_bindings(
        &self,
    ) -> Result<Vec<ChartPropertyBinding>, GraphError> {
        query_many(
            self,
            "MATCH (c:ChartDefinition)-[e:ChartBindsProperty]->(p:Property) RETURN \
            c.name, p.name, p._schema_title, e.role \
            ORDER BY c.name, p.name",
            |reader, row| {
                Ok((
                    reader.get_string(row, "c.name")?,
                    reader.get_string(row, "p.name")?,
                    reader.get_string(row, "p._schema_title")?,
                    reader.get_opt_string(row, "e.role")?,
                ))
            },
        )
        .await
    }
}

fn chart_definition_from_row(
    reader: &RowReader,
    row: &[grafeo::Value],
) -> Result<ChartDefinitionNode, GraphError> {
    let kind_str = reader.get_string(row, "c.kind")?;
    let kind = ChartKind::parse(&kind_str).ok_or_else(|| {
        GraphError::Query(format!(
            "ChartDefinition '{}' carries unknown kind '{kind_str}'",
            reader.get_string(row, "c.name").unwrap_or_default()
        ))
    })?;
    Ok(ChartDefinitionNode {
        name: reader.get_string(row, "c.name")?,
        kind,
        title: reader.get_opt_string(row, "c.title")?,
        domain: reader.get_opt_string(row, "c.domain")?,
        entity: reader.get_opt_string(row, "c.entity")?,
        entity_title: reader.get_opt_string(row, "c.entity_title")?,
        route: reader.get_opt_string(row, "c.route")?,
        config_json: reader.get_opt_string(row, "c.config_json")?,
    })
}
