use async_trait::async_trait;
use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{
    CodeList, CompositeColumn, CompositeRange, EdgeProperties, EdgeType, EnumValue, IngestStats,
    PropertyNode, SchemaNode,
};

use codegraph_type_contracts::RefClassificationKind;

use crate::engine::GrafeoEngine;

/// Escape single quotes in GQL string literals.
pub(crate) fn escape_gql(s: &str) -> String {
    s.replace('\'', "\\'")
}

fn classification_kind_to_str(kind: &RefClassificationKind) -> String {
    match kind {
        RefClassificationKind::PrimitiveWrapper => "primitive_wrapper",
        RefClassificationKind::ArrayWrapper => "array_wrapper",
        RefClassificationKind::RangeWrapper => "range_wrapper",
        RefClassificationKind::CodelistReference => "codelist",
        RefClassificationKind::CodelistCheck => "codelist_check",
        RefClassificationKind::InlineEnum => "inline_enum",
        RefClassificationKind::EntityReference => "entity_reference",
        RefClassificationKind::ValueObject => "value_object",
        RefClassificationKind::CompositeWrapper => "composite_wrapper",
        RefClassificationKind::StructuredWrapper => "structured_wrapper",
        RefClassificationKind::MediaWrapper => "media_wrapper",
    }
    .to_string()
}

/// Format an Option<String> as a GQL value: either 'escaped' or null.
fn opt_str(s: &Option<String>) -> String {
    match s {
        Some(v) => format!("'{}'", escape_gql(v)),
        None => "null".to_string(),
    }
}

fn build_edge_props_string(props: Option<&EdgeProperties>) -> String {
    let Some(p) = props else {
        return String::new();
    };
    let mut fields = Vec::new();
    if let Some(v) = &p.sort_order {
        fields.push(format!("sort_order: {v}"));
    }
    if let Some(v) = &p.ref_path {
        fields.push(format!("ref_path: '{}'", escape_gql(v)));
    }
    if let Some(v) = &p.resolved_classification {
        fields.push(format!("resolved_classification: '{}'", escape_gql(v)));
    }
    if let Some(v) = &p.composition_type {
        fields.push(format!("composition_type: '{}'", escape_gql(v)));
    }
    if let Some(v) = &p.dependency_type {
        fields.push(format!("dependency_type: '{}'", escape_gql(v)));
    }
    if let Some(v) = &p.render_as {
        fields.push(format!("render_as: '{}'", escape_gql(v)));
    }
    if let Some(v) = &p.role {
        fields.push(format!("role: '{}'", escape_gql(v)));
    }
    if let Some(v) = &p.def_name {
        fields.push(format!("def_name: '{}'", escape_gql(v)));
    }
    if fields.is_empty() {
        String::new()
    } else {
        format!(" {{{}}}", fields.join(", "))
    }
}

/// Split a compound ID of the form `"part1::part2"`, returning an error
/// that names the edge label on failure.
fn split_compound_id<'a>(id: &'a str, edge_label: &str) -> Result<(&'a str, &'a str), GraphError> {
    id.split_once("::").ok_or_else(|| {
        GraphError::Ingest(format!("{edge_label} id must be 'part1::part2', got: {id}"))
    })
}

fn count_from_gql(engine: &GrafeoEngine, gql: &str) -> Result<usize, GraphError> {
    let session = engine.db().session();
    let result = session
        .execute(gql)
        .map_err(|e| GraphError::Query(e.to_string()))?;
    if result.rows.is_empty() {
        return Ok(0);
    }
    result.rows[0][0]
        .as_int64()
        .map(|v| v as usize)
        .ok_or_else(|| GraphError::Query("count query did not return an integer".into()))
}

#[async_trait]
impl GraphIngestor for GrafeoEngine {
    async fn ingest_schema(&self, node: &SchemaNode) -> Result<String, GraphError> {
        let session = self.db().session();
        let gql = format!(
            "INSERT (:Schema {{\
                schema_id: '{schema_id}', title: '{title}', description: {description}, \
                schema_type: '{schema_type}', classification: '{classification}', \
                pg_type: '{pg_type}', rust_type: '{rust_type}', sea_orm_type: '{sea_orm_type}', \
                domain: {domain}, rel_path: '{rel_path}', \
                rust_type_name: '{rust_type_name}', pg_table_name: '{pg_table_name}', \
                api_path_segment: '{api_path_segment}', \
                parent_schema: {parent_schema}, \
                is_entity: {is_entity}, is_codelist: {is_codelist}, \
                is_primitive_wrapper: {is_primitive_wrapper}, \
                has_all_of: {has_all_of}, has_one_of: {has_one_of}, \
                has_any_of: {has_any_of}, has_definitions: {has_definitions}\
            }})",
            schema_id = escape_gql(&node.schema_id),
            title = escape_gql(&node.title),
            description = opt_str(&node.description),
            schema_type = escape_gql(&node.schema_type),
            classification = escape_gql(&node.classification),
            pg_type = escape_gql(&node.pg_type),
            rust_type = escape_gql(&node.rust_type),
            sea_orm_type = escape_gql(&node.sea_orm_type),
            domain = opt_str(&node.domain),
            rel_path = escape_gql(&node.rel_path),
            rust_type_name = escape_gql(&node.rust_type_name),
            pg_table_name = escape_gql(&node.pg_table_name),
            api_path_segment = escape_gql(&node.api_path_segment),
            parent_schema = opt_str(&node.parent_schema),
            is_entity = node.is_entity,
            is_codelist = node.is_codelist,
            is_primitive_wrapper = node.is_primitive_wrapper,
            has_all_of = node.has_all_of,
            has_one_of = node.has_one_of,
            has_any_of = node.has_any_of,
            has_definitions = node.has_definitions,
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_schema failed: {e}")))?;
        Ok(node.schema_id.clone())
    }

    async fn ingest_property(
        &self,
        schema_title: &str,
        prop: &PropertyNode,
    ) -> Result<(), GraphError> {
        let session = self.db().session();
        let gql = format!(
            "INSERT (:Property {{\
                name: '{name}', prop_type: '{prop_type}', description: {description}, \
                format: {format}, \
                is_required: {is_required}, is_nullable: {is_nullable}, \
                is_array: {is_array}, pattern: {pattern}, \
                pg_column_name: '{pg_column_name}', pg_column_type: '{pg_column_type}', \
                rust_field_name: '{rust_field_name}', rust_field_type: '{rust_field_type}', \
                sea_orm_type: '{sea_orm_type}', render_strategy: '{render_strategy}', \
                ref_target: {ref_target}, classification: {classification}, \
                classification_kind: {classification_kind}, \
                _schema_title: '{schema_title}'\
            }})",
            name = escape_gql(&prop.name),
            prop_type = escape_gql(&prop.prop_type),
            description = opt_str(&prop.description),
            format = opt_str(&prop.format),
            is_required = prop.is_required,
            is_nullable = prop.is_nullable,
            is_array = prop.is_array,
            pattern = opt_str(&prop.pattern),
            pg_column_name = escape_gql(&prop.pg_column_name),
            pg_column_type = escape_gql(&prop.pg_column_type),
            rust_field_name = escape_gql(&prop.rust_field_name),
            rust_field_type = escape_gql(&prop.rust_field_type),
            sea_orm_type = escape_gql(&prop.sea_orm_type),
            render_strategy = escape_gql(&prop.render_strategy),
            ref_target = opt_str(&prop.ref_target),
            classification = opt_str(&prop.classification),
            classification_kind = opt_str(
                &prop
                    .classification_kind
                    .as_ref()
                    .map(classification_kind_to_str)
            ),
            schema_title = escape_gql(schema_title),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_property INSERT failed: {e}")))?;

        let edge_gql = format!(
            "MATCH (s:Schema {{title: '{st}'}}), (p:Property {{name: '{pn}', _schema_title: '{st}'}}) \
             INSERT (s)-[:HasProperty]->(p)",
            st = escape_gql(schema_title),
            pn = escape_gql(&prop.name),
        );
        session.execute(&edge_gql).map_err(|e| {
            GraphError::Ingest(format!("ingest_property HasProperty edge failed: {e}"))
        })?;
        Ok(())
    }

    async fn ingest_codelist(&self, codelist: &CodeList) -> Result<(), GraphError> {
        let session = self.db().session();
        let gql = format!(
            "INSERT (:CodeList {{name: '{name}', description: {description}, \
             pg_table_name: '{pg_table_name}', render_as: '{render_as}', \
             check_expression: {check_expression}}})",
            name = escape_gql(&codelist.name),
            description = opt_str(&codelist.description),
            pg_table_name = escape_gql(&codelist.pg_table_name),
            render_as = escape_gql(&codelist.render_as),
            check_expression = opt_str(&codelist.check_expression),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(e.to_string()))?;
        Ok(())
    }

    async fn ingest_enum_value(
        &self,
        codelist_name: &str,
        value: &EnumValue,
    ) -> Result<(), GraphError> {
        let session = self.db().session();
        let gql = format!(
            "INSERT (:EnumValue {{value: '{val}', display_name: {dn}, sort_order: {so}, \
             _codelist_name: '{cn}'}})",
            val = escape_gql(&value.value),
            dn = opt_str(&value.display_name),
            so = value.sort_order,
            cn = escape_gql(codelist_name),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(e.to_string()))?;

        let edge_gql = format!(
            "MATCH (c:CodeList {{name: '{cn}'}}), \
             (v:EnumValue {{value: '{val}', _codelist_name: '{cn}'}}) \
             INSERT (c)-[:HasEnumValue]->(v)",
            cn = escape_gql(codelist_name),
            val = escape_gql(&value.value),
        );
        session
            .execute(&edge_gql)
            .map_err(|e| GraphError::Ingest(e.to_string()))?;
        Ok(())
    }

    async fn ingest_composite_column(&self, col: &CompositeColumn) -> Result<(), GraphError> {
        let session = self.db().session();
        let gql = format!(
            "MERGE (:CompositeColumn {{suffix: '{suffix}', wrapper_schema: '{wrapper_schema}', \
             pg_type: '{pg_type}', rust_type: '{rust_type}', sea_orm_type: '{sea_orm_type}', \
             fk_target: {fk_target}, dto_rust_type: {dto_rust_type}}})",
            suffix = escape_gql(&col.suffix),
            wrapper_schema = escape_gql(&col.wrapper_schema),
            pg_type = escape_gql(&col.pg_type),
            rust_type = escape_gql(&col.rust_type),
            sea_orm_type = escape_gql(&col.sea_orm_type),
            fk_target = opt_str(&col.fk_target),
            dto_rust_type = opt_str(&col.dto_rust_type),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(e.to_string()))?;
        Ok(())
    }

    async fn ingest_extension(&self, name: &str) -> Result<(), GraphError> {
        let session = self.db().session();
        let gql = format!("MERGE (:Extension {{name: '{}'}})", escape_gql(name),);
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(e.to_string()))?;
        Ok(())
    }

    async fn ingest_composite_range(&self, range: &CompositeRange) -> Result<(), GraphError> {
        let session = self.db().session();
        let gql = format!(
            "INSERT (:CompositeRange {{pg_column_name: '{pg_col}', pg_type: '{pg_type}', \
             rust_type: '{rust_type}', start_field: '{start}', end_field: '{end}', \
             open_end: {open_end}}})",
            pg_col = escape_gql(&range.pg_column_name),
            pg_type = escape_gql(&range.pg_type),
            rust_type = escape_gql(&range.rust_type),
            start = escape_gql(&range.start_field),
            end = escape_gql(&range.end_field),
            open_end = range.open_end,
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(e.to_string()))?;
        Ok(())
    }

    async fn ingest_edge(
        &self,
        from_id: &str,
        to_id: &str,
        edge_type: EdgeType,
        props: Option<&EdgeProperties>,
    ) -> Result<(), GraphError> {
        let session = self.db().session();
        let label = match &edge_type {
            EdgeType::HasProperty => "HasProperty",
            EdgeType::ReferencesSchema => "ReferencesSchema",
            EdgeType::ItemsOf => "ItemsOf",
            EdgeType::ExtendsSchema => "ExtendsSchema",
            EdgeType::DependsOn => "DependsOn",
            EdgeType::HasEnumValue => "HasEnumValue",
            EdgeType::UsesCodeList => "UsesCodeList",
            EdgeType::ExpandsTo => "ExpandsTo",
            EdgeType::CollapsesTo => "CollapsesTo",
            EdgeType::ConsumesField => "ConsumesField",
            EdgeType::ContainsDef => "ContainsDef",
            EdgeType::RequiresExtension => "RequiresExtension",
            EdgeType::InDomain => "InDomain",
            EdgeType::DomainDepends => "DomainDepends",
        };

        let match_clause = match &edge_type {
            EdgeType::HasProperty => {
                let (prop_name, schema_title) = split_compound_id(to_id, "HasProperty")?;
                format!(
                    "MATCH (a:Schema {{title: '{}'}}), (b:Property {{name: '{}', _schema_title: '{}'}})",
                    escape_gql(from_id),
                    escape_gql(prop_name),
                    escape_gql(schema_title),
                )
            }
            EdgeType::ReferencesSchema => {
                let (prop_name, schema_title) = split_compound_id(from_id, "ReferencesSchema")?;
                format!(
                    "MATCH (a:Property {{name: '{}', _schema_title: '{}'}}), (b:Schema {{title: '{}'}})",
                    escape_gql(prop_name),
                    escape_gql(schema_title),
                    escape_gql(to_id),
                )
            }
            EdgeType::HasEnumValue => {
                let (value, codelist) = split_compound_id(to_id, "HasEnumValue")?;
                format!(
                    "MATCH (a:CodeList {{name: '{}'}}), (b:EnumValue {{value: '{}', _codelist_name: '{}'}})",
                    escape_gql(from_id),
                    escape_gql(value),
                    escape_gql(codelist),
                )
            }
            EdgeType::ItemsOf => {
                let (prop_name, schema_title) = split_compound_id(from_id, "ItemsOf")?;
                format!(
                    "MATCH (a:Property {{name: '{}', _schema_title: '{}'}}), (b:Schema {{title: '{}'}})",
                    escape_gql(prop_name),
                    escape_gql(schema_title),
                    escape_gql(to_id),
                )
            }
            EdgeType::ExtendsSchema | EdgeType::DependsOn => {
                format!(
                    "MATCH (a:Schema {{title: '{}'}}), (b:Schema {{title: '{}'}})",
                    escape_gql(from_id),
                    escape_gql(to_id),
                )
            }
            EdgeType::UsesCodeList => {
                let (prop_name, schema_title) = split_compound_id(from_id, "UsesCodeList")?;
                format!(
                    "MATCH (a:Property {{name: '{}', _schema_title: '{}'}}), (b:CodeList {{name: '{}'}})",
                    escape_gql(prop_name),
                    escape_gql(schema_title),
                    escape_gql(to_id),
                )
            }
            EdgeType::ExpandsTo => {
                let (prop_name, schema_title) = split_compound_id(from_id, "ExpandsTo")?;
                let (suffix, wrapper_schema) = split_compound_id(to_id, "ExpandsTo(target)")?;
                format!(
                    "MATCH (a:Property {{name: '{}', _schema_title: '{}'}}), \
                     (b:CompositeColumn {{suffix: '{}', wrapper_schema: '{}'}})",
                    escape_gql(prop_name),
                    escape_gql(schema_title),
                    escape_gql(suffix),
                    escape_gql(wrapper_schema),
                )
            }
            EdgeType::CollapsesTo => {
                format!(
                    "MATCH (a:Schema {{title: '{}'}}), (b:CompositeRange {{pg_column_name: '{}'}})",
                    escape_gql(from_id),
                    escape_gql(to_id),
                )
            }
            EdgeType::ConsumesField => {
                let (prop_name, schema_title) = split_compound_id(to_id, "ConsumesField")?;
                format!(
                    "MATCH (a:CompositeRange {{pg_column_name: '{}'}}), (b:Property {{name: '{}', _schema_title: '{}'}})",
                    escape_gql(from_id),
                    escape_gql(prop_name),
                    escape_gql(schema_title),
                )
            }
            EdgeType::ContainsDef => {
                format!(
                    "MATCH (a:Schema {{title: '{}'}}), (b:Schema {{title: '{}'}})",
                    escape_gql(from_id),
                    escape_gql(to_id),
                )
            }
            EdgeType::RequiresExtension => {
                format!(
                    "MATCH (a:Schema {{title: '{}'}}), (b:Extension {{name: '{}'}})",
                    escape_gql(from_id),
                    escape_gql(to_id),
                )
            }
            EdgeType::InDomain => {
                format!(
                    "MATCH (a:Schema {{title: '{}'}}), (b:Domain {{name: '{}'}})",
                    escape_gql(from_id),
                    escape_gql(to_id),
                )
            }
            EdgeType::DomainDepends => {
                format!(
                    "MATCH (a:Domain {{name: '{}'}}), (b:Domain {{name: '{}'}})",
                    escape_gql(from_id),
                    escape_gql(to_id),
                )
            }
        };

        let props_str = build_edge_props_string(props);
        let gql = format!("{match_clause} INSERT (a)-[:{label}{props_str}]->(b)");
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_edge {label} failed: {e}")))?;
        Ok(())
    }

    async fn finalize(&self) -> Result<IngestStats, GraphError> {
        Ok(IngestStats {
            schema_count: count_from_gql(self, "MATCH (s:Schema) RETURN count(s) AS cnt")?,
            property_count: count_from_gql(self, "MATCH (p:Property) RETURN count(p) AS cnt")?,
            reference_edge_count: count_from_gql(
                self,
                "MATCH ()-[r:ReferencesSchema]->() RETURN count(r) AS cnt",
            )?,
            composition_edge_count: count_from_gql(
                self,
                "MATCH ()-[r:ExtendsSchema]->() RETURN count(r) AS cnt",
            )?,
            codelist_count: count_from_gql(self, "MATCH (c:CodeList) RETURN count(c) AS cnt")?,
            enum_value_count: count_from_gql(self, "MATCH (v:EnumValue) RETURN count(v) AS cnt")?,
            composite_column_count: count_from_gql(
                self,
                "MATCH (c:CompositeColumn) RETURN count(c) AS cnt",
            )?,
            composite_range_count: count_from_gql(
                self,
                "MATCH (r:CompositeRange) RETURN count(r) AS cnt",
            )?,
            domain_count: count_from_gql(self, "MATCH (d:Domain) RETURN count(d) AS cnt")?,
            duration: self.start_time().elapsed(),
        })
    }

    async fn update_entity_flag(&self, title: &str, is_entity: bool) -> Result<(), GraphError> {
        let session = self.db().session();
        let query = format!(
            "MATCH (s:Schema {{title: '{}'}}) SET s.is_entity = {}",
            title.replace('\'', "\\'"),
            is_entity
        );
        session
            .execute(&query)
            .map_err(|e| GraphError::Query(e.to_string()))?;
        Ok(())
    }

    async fn update_property_classification(
        &self,
        schema_title: &str,
        property_name: &str,
        kind: &str,
    ) -> Result<(), GraphError> {
        let session = self.db().session();
        let query = format!(
            "MATCH (s:Schema {{title: '{}'}})-[:HasProperty]->(p:Property {{name: '{}'}}) SET p.classification_kind = '{}'",
            schema_title.replace('\'', "\\'"),
            property_name.replace('\'', "\\'"),
            kind.replace('\'', "\\'"),
        );
        session
            .execute(&query)
            .map_err(|e| GraphError::Query(e.to_string()))?;
        Ok(())
    }
}
