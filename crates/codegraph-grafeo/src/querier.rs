use async_trait::async_trait;
use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{
    ActionNode, CodeList, ColumnInfo, CompositeColumn, CompositeRange, CompositionNode,
    CompositionTree, DetectionSource, EnumValue, EventNode, Extension,
    FkDirection, FkTarget, ParentCandidate, ParameterDefinitionNode, PropertyNode,
    SchemaClassificationData, SchemaNode, StructuredSubField, ViewComponentNode, ViewContainerNode,
};
use std::collections::{HashMap, VecDeque};

/// The RETURN clause for all SchemaNode queries — keeps the 21 columns in one place.
const SCHEMA_RETURN_COLS: &str = "\
    s.schema_id, s.title, s.description, \
    s.schema_type, s.classification, s.domain, s.rel_path, s.pg_type, s.rust_type, \
    s.sea_orm_type, s.rust_type_name, s.pg_table_name, s.api_path_segment, \
    s.parent_schema, s.is_entity, s.is_codelist, s.is_primitive_wrapper, \
    s.has_all_of, s.has_one_of, s.has_any_of, s.has_definitions";

/// The RETURN clause for all PropertyNode queries — keeps the 17 columns in one place.
const PROPERTY_RETURN_COLS: &str = "\
    p.name, p.prop_type, p.description, p.format, \
    p.is_required, p.is_nullable, p.is_array, p.pattern, \
    p.pg_column_name, p.pg_column_type, p.rust_field_name, p.rust_field_type, \
    p.sea_orm_type, p.render_strategy, p.ref_target, p.classification, \
    p.classification_kind";

use crate::conversions::{
    row_to_codelist, row_to_composite_column, row_to_composite_range, row_to_enum_value,
    row_to_extension, row_to_property_node, row_to_schema_node, row_to_structured_sub_field,
    RowReader,
};
use crate::engine::GrafeoEngine;

/// Query result wrapper holding columns and rows from Grafeo.
struct QResult {
    columns: Vec<String>,
    rows: Vec<Vec<grafeo::Value>>,
}

fn query_gql(engine: &GrafeoEngine, gql: &str) -> Result<QResult, GraphError> {
    let session = engine.db().session();
    let result = session
        .execute(gql)
        .map_err(|e| GraphError::Query(format!("{e}")))?;
    Ok(QResult {
        columns: result.columns,
        rows: result.rows,
    })
}

/// Execute a parameterized GQL query. Grafeo can cache query plans for
/// parameterized queries, avoiding repeated parsing of the same template.
fn query_gql_params(
    engine: &GrafeoEngine,
    gql: &str,
    params: HashMap<String, grafeo::Value>,
) -> Result<QResult, GraphError> {
    let result = engine
        .db()
        .execute_with_params(gql, params)
        .map_err(|e| GraphError::Query(format!("{e}")))?;
    Ok(QResult {
        columns: result.columns,
        rows: result.rows,
    })
}

#[async_trait]
impl GraphQuerier for GrafeoEngine {
    async fn get_schema(&self, title: &str) -> Result<Option<SchemaNode>, GraphError> {
        let params = HashMap::from([("title".to_string(), grafeo::Value::String(title.into()))]);
        let result = query_gql_params(
            self,
            &format!("MATCH (s:Schema {{title: $title}}) RETURN {SCHEMA_RETURN_COLS}"),
            params,
        )?;
        if result.rows.is_empty() {
            return Ok(None);
        }
        let reader = RowReader::from_columns(&result.columns);
        if result.rows.len() == 1 {
            return Ok(Some(row_to_schema_node(&reader, &result.rows[0])?));
        }
        // Multiple nodes for same title — pick deterministically by domain (alphabetic first)
        let mut schemas: Vec<SchemaNode> = result
            .rows
            .iter()
            .map(|row| row_to_schema_node(&reader, row))
            .collect::<Result<_, _>>()?;
        schemas.sort_by(|a, b| a.domain.cmp(&b.domain));
        Ok(schemas.into_iter().next())
    }

    async fn get_schema_by_id(&self, schema_id: &str) -> Result<Option<SchemaNode>, GraphError> {
        let params =
            HashMap::from([("sid".to_string(), grafeo::Value::String(schema_id.into()))]);
        let result = query_gql_params(
            self,
            &format!(
                "MATCH (s:Schema {{schema_id: $sid}}) RETURN {SCHEMA_RETURN_COLS}"
            ),
            params,
        )?;
        if result.rows.is_empty() {
            return Ok(None);
        }
        let reader = RowReader::from_columns(&result.columns);
        Ok(Some(row_to_schema_node(&reader, &result.rows[0])?))
    }

    async fn get_schema_in_domain(
        &self,
        title: &str,
        domain: &str,
    ) -> Result<Option<SchemaNode>, GraphError> {
        let params = HashMap::from([
            ("title".to_string(), grafeo::Value::String(title.into())),
            ("domain".to_string(), grafeo::Value::String(domain.into())),
        ]);
        let result = query_gql_params(
            self,
            &format!(
                "MATCH (s:Schema {{title: $title, domain: $domain}}) RETURN {SCHEMA_RETURN_COLS}"
            ),
            params,
        )?;
        if result.rows.is_empty() {
            return Ok(None);
        }
        let reader = RowReader::from_columns(&result.columns);
        Ok(Some(row_to_schema_node(&reader, &result.rows[0])?))
    }

    async fn list_schemas(&self, domain: Option<&str>) -> Result<Vec<SchemaNode>, GraphError> {
        let result = match domain {
            Some(d) => {
                let params =
                    HashMap::from([("domain".to_string(), grafeo::Value::String(d.into()))]);
                query_gql_params(
                    self,
                    &format!("MATCH (s:Schema {{domain: $domain}}) RETURN {SCHEMA_RETURN_COLS}"),
                    params,
                )?
            }
            None => query_gql(
                self,
                &format!("MATCH (s:Schema) RETURN {SCHEMA_RETURN_COLS}"),
            )?,
        };
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_schema_node(&reader, row))
            .collect()
    }

    async fn get_properties(&self, schema_title: &str) -> Result<Vec<PropertyNode>, GraphError> {
        let params = HashMap::from([(
            "title".to_string(),
            grafeo::Value::String(schema_title.into()),
        )]);
        let result = query_gql_params(
            self,
            &format!(
                "MATCH (:Schema {{title: $title}})-[:HasProperty]->(p:Property) \
                 RETURN {PROPERTY_RETURN_COLS}"
            ),
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_property_node(&reader, row))
            .collect()
    }

    async fn get_properties_in_domain(
        &self,
        schema_title: &str,
        domain: &str,
    ) -> Result<Vec<PropertyNode>, GraphError> {
        let params = HashMap::from([
            (
                "title".to_string(),
                grafeo::Value::String(schema_title.into()),
            ),
            ("domain".to_string(), grafeo::Value::String(domain.into())),
        ]);
        let result = query_gql_params(
            self,
            &format!(
                "MATCH (:Schema {{title: $title, domain: $domain}})-[:HasProperty]->(p:Property) \
                 RETURN {PROPERTY_RETURN_COLS}"
            ),
            params,
        )?;
        if result.rows.is_empty() {
            // Fallback: schema may not exist in this domain, use title-only query
            return self.get_properties(schema_title).await;
        }
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_property_node(&reader, row))
            .collect()
    }

    async fn get_child_schemas(&self, schema_title: &str) -> Result<Vec<SchemaNode>, GraphError> {
        let params =
            HashMap::from([("ps".to_string(), grafeo::Value::String(schema_title.into()))]);
        let result = query_gql_params(
            self,
            &format!("MATCH (s:Schema {{parent_schema: $ps}}) RETURN {SCHEMA_RETURN_COLS}"),
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_schema_node(&reader, row))
            .collect()
    }

    async fn get_classification_data(&self) -> Result<Vec<SchemaClassificationData>, GraphError> {
        let schemas = self.list_schemas(None).await?;

        // Bulk query: all properties with their schema title and required flag.
        // This replaces N individual get_properties() calls.
        let prop_result = query_gql(
            self,
            "MATCH (s:Schema)-[:HasProperty]->(p:Property) RETURN s.title, p.is_required",
        )?;
        let prop_reader = RowReader::from_columns(&prop_result.columns);
        let mut field_counts: HashMap<String, usize> = HashMap::new();
        let mut required_counts: HashMap<String, usize> = HashMap::new();
        for row in &prop_result.rows {
            let title = prop_reader.get_string(row, "s.title")?;
            *field_counts.entry(title.clone()).or_default() += 1;
            let is_req = prop_reader.get_bool(row, "p.is_required").unwrap_or(false);
            if is_req {
                *required_counts.entry(title).or_default() += 1;
            }
        }

        // Bulk query: ref counts (properties that reference another schema).
        let ref_result = query_gql(
            self,
            "MATCH (s:Schema)-[:HasProperty]->(p:Property)-[:ReferencesSchema]->() RETURN s.title",
        )?;
        let ref_reader = RowReader::from_columns(&ref_result.columns);
        let mut ref_counts: HashMap<String, usize> = HashMap::new();
        for row in &ref_result.rows {
            let title = ref_reader.get_string(row, "s.title")?;
            *ref_counts.entry(title).or_default() += 1;
        }

        // Bulk query: in-degree (schemas referenced by other properties).
        let in_result = query_gql(
            self,
            "MATCH ()-[:ReferencesSchema]->(s:Schema) RETURN s.title",
        )?;
        let in_reader = RowReader::from_columns(&in_result.columns);
        let mut in_degrees: HashMap<String, usize> = HashMap::new();
        for row in &in_result.rows {
            let title = in_reader.get_string(row, "s.title")?;
            *in_degrees.entry(title).or_default() += 1;
        }

        // Bulk query: schemas with ExtendsSchema edges (composition check).
        let ext_result = query_gql(self, "MATCH (s:Schema)-[:ExtendsSchema]->() RETURN s.title")?;
        let ext_reader = RowReader::from_columns(&ext_result.columns);
        let mut extends_set: std::collections::HashSet<String> = std::collections::HashSet::new();
        for row in &ext_result.rows {
            let title = ext_reader.get_string(row, "s.title")?;
            extends_set.insert(title);
        }

        // Assemble results using the bulk data
        let mut results = Vec::with_capacity(schemas.len());
        for schema in &schemas {
            let title = &schema.title;
            let field_count = field_counts.get(title).copied().unwrap_or(0);
            let required_field_count = required_counts.get(title).copied().unwrap_or(0);
            let ref_count = ref_counts.get(title).copied().unwrap_or(0);
            let in_degree = in_degrees.get(title).copied().unwrap_or(0);
            let composes_noun_type = extends_set.contains(title);

            results.push(SchemaClassificationData {
                title: title.clone(),
                domain: schema.domain.clone(),
                rel_path: schema.rel_path.clone(),
                schema_type: schema.schema_type.clone(),
                is_codelist: schema.is_codelist,
                is_primitive_wrapper: schema.is_primitive_wrapper,
                has_all_of: schema.has_all_of,
                composes_noun_type,
                field_count,
                required_field_count,
                ref_count,
                in_degree,
                is_enum: schema.has_one_of && field_count == 0,
                is_string_type: schema.schema_type == "string",
            });
        }

        Ok(results)
    }

    async fn get_entity_names(&self) -> Result<Vec<String>, GraphError> {
        let result = query_gql(self, "MATCH (s:Schema {is_entity: true}) RETURN s.title")?;
        let reader = RowReader::from_columns(&result.columns);
        let mut names: Vec<String> = result
            .rows
            .iter()
            .map(|row| reader.get_string(row, "s.title"))
            .collect::<Result<_, _>>()?;
        names.sort();
        names.dedup();
        Ok(names)
    }

    async fn get_entity_schema_map(&self) -> Result<HashMap<String, String>, GraphError> {
        let result = query_gql(
            self,
            "MATCH (s:Schema {is_entity: true}) RETURN s.title, s.rel_path",
        )?;
        let reader = RowReader::from_columns(&result.columns);
        let mut map = HashMap::new();
        for row in &result.rows {
            let title = reader.get_string(row, "s.title")?;
            let rel_path = reader.get_string(row, "s.rel_path")?;
            map.insert(title, rel_path);
        }
        Ok(map)
    }

    async fn get_value_object_schemas(&self) -> Result<Vec<SchemaNode>, GraphError> {
        let gql = &format!(
            "MATCH (s:Schema) WHERE s.is_entity = false AND s.is_codelist = false AND s.schema_type = 'object' \
             RETURN {SCHEMA_RETURN_COLS}"
        );
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_schema_node(&reader, row))
            .collect()
    }

    async fn get_parent_candidates(&self) -> Result<Vec<ParentCandidate>, GraphError> {
        let gql = "MATCH (child:Schema)-[:HasProperty]->(p:Property {is_array: false})-[:ReferencesSchema]->(parent:Schema {is_entity: true}) \
                   RETURN DISTINCT child.title, parent.title, p.name";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut candidates = Vec::new();
        for row in &result.rows {
            candidates.push(ParentCandidate {
                child_title: reader.get_string(row, "child.title")?,
                parent_title: reader.get_string(row, "parent.title")?,
                field_name: reader.get_string(row, "p.name")?,
                source: DetectionSource::ScalarRef,
            });
        }

        // Detect one-to-many relationships: parent entity has an array property
        // whose items reference a child entity (ItemsOf edge).
        let array_gql = "MATCH (parent:Schema {is_entity: true})-[:HasProperty]->(p:Property {is_array: true})-[:ItemsOf]->(child:Schema {is_entity: true}) \
                          RETURN DISTINCT child.title, parent.title, p.name";
        let array_result = query_gql(self, array_gql)?;
        let array_reader = RowReader::from_columns(&array_result.columns);
        let scalar_keys: std::collections::HashSet<(String, String)> = candidates
            .iter()
            .map(|c| (c.child_title.clone(), c.parent_title.clone()))
            .collect();
        for row in &array_result.rows {
            let child_title = array_reader.get_string(row, "child.title")?;
            let parent_title = array_reader.get_string(row, "parent.title")?;
            let field_name = array_reader.get_string(row, "p.name")?;
            if scalar_keys.contains(&(child_title.clone(), parent_title.clone())) {
                continue;
            }
            candidates.push(ParentCandidate {
                child_title,
                parent_title,
                field_name,
                source: DetectionSource::ArrayItems,
            });
        }

        Ok(candidates)
    }

    async fn get_codelist(&self, name: &str) -> Result<Option<CodeList>, GraphError> {
        let params = HashMap::from([("name".to_string(), grafeo::Value::String(name.into()))]);
        let result = query_gql_params(
            self,
            "MATCH (c:CodeList {name: $name}) RETURN c.name, c.description, \
             c.pg_table_name, c.render_as, c.check_expression",
            params,
        )?;
        if result.rows.is_empty() {
            return Ok(None);
        }
        let reader = RowReader::from_columns(&result.columns);
        Ok(Some(row_to_codelist(&reader, &result.rows[0])?))
    }

    async fn list_codelists(&self) -> Result<Vec<CodeList>, GraphError> {
        let gql =
            "MATCH (c:CodeList) RETURN c.name, c.description, c.pg_table_name, c.render_as, c.check_expression";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_codelist(&reader, row))
            .collect()
    }

    async fn get_enum_values(&self, codelist_name: &str) -> Result<Vec<EnumValue>, GraphError> {
        let params = HashMap::from([(
            "name".to_string(),
            grafeo::Value::String(codelist_name.into()),
        )]);
        let result = query_gql_params(
            self,
            "MATCH (:CodeList {name: $name})-[:HasEnumValue]->(v:EnumValue) \
             RETURN v.value, v.display_name, v.sort_order",
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_enum_value(&reader, row))
            .collect()
    }

    async fn get_composite_columns(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Vec<CompositeColumn>, GraphError> {
        let params = HashMap::from([
            (
                "pname".to_string(),
                grafeo::Value::String(property_name.into()),
            ),
            (
                "stitle".to_string(),
                grafeo::Value::String(schema_title.into()),
            ),
        ]);
        let result = query_gql_params(
            self,
            "MATCH (:Property {name: $pname, _schema_title: $stitle})-[:ExpandsTo]->(cc:CompositeColumn) \
             RETURN cc.suffix, cc.pg_type, cc.rust_type, cc.sea_orm_type, cc.fk_target, cc.dto_rust_type, cc.wrapper_schema",
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_composite_column(&reader, row))
            .collect()
    }

    async fn get_structured_sub_fields(
        &self,
        schema_title: &str,
    ) -> Result<Vec<StructuredSubField>, GraphError> {
        let params = HashMap::from([(
            "title".to_string(),
            grafeo::Value::String(schema_title.into()),
        )]);
        let result = query_gql_params(
            self,
            "MATCH (:Schema {title: $title})-[:HasProperty]->(p:Property) \
             RETURN p.name, p.description, p.is_required \
             ORDER BY p.is_required DESC, p.name ASC",
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_structured_sub_field(&reader, row))
            .collect()
    }

    async fn get_composite_range(
        &self,
        schema_title: &str,
    ) -> Result<Option<CompositeRange>, GraphError> {
        let params = HashMap::from([(
            "title".to_string(),
            grafeo::Value::String(schema_title.into()),
        )]);
        // First try direct CollapsesTo edge on this schema
        let result = query_gql_params(
            self,
            "MATCH (:Schema {title: $title})-[:CollapsesTo]->(r:CompositeRange) \
             RETURN r.pg_column_name, r.pg_type, r.rust_type, r.start_field, r.end_field, r.open_end",
            params.clone(),
        )?;
        if !result.rows.is_empty() {
            let reader = RowReader::from_columns(&result.columns);
            return Ok(Some(row_to_composite_range(&reader, &result.rows[0])?));
        }

        // Follow allOf/ExtendsSchema inheritance to find composite range on parent schemas
        let parent_result = query_gql_params(
            self,
            "MATCH (:Schema {title: $title})-[:ExtendsSchema]->(:Schema)-[:CollapsesTo]->(r:CompositeRange) \
             RETURN r.pg_column_name, r.pg_type, r.rust_type, r.start_field, r.end_field, r.open_end",
            params,
        )?;
        if !parent_result.rows.is_empty() {
            let reader = RowReader::from_columns(&parent_result.columns);
            return Ok(Some(row_to_composite_range(
                &reader,
                &parent_result.rows[0],
            )?));
        }

        Ok(None)
    }

    async fn get_consumed_fields(
        &self,
        schema_title: &str,
    ) -> Result<Vec<(PropertyNode, String)>, GraphError> {
        let params = HashMap::from([(
            "title".to_string(),
            grafeo::Value::String(schema_title.into()),
        )]);
        // First try direct CollapsesTo edge
        let result = query_gql_params(
            self,
            &format!(
                "MATCH (:Schema {{title: $title}})-[:CollapsesTo]->(r:CompositeRange)-[cf:ConsumesField]->(p:Property) \
                 RETURN {PROPERTY_RETURN_COLS}, cf.role"
            ),
            params.clone(),
        )?;
        if !result.rows.is_empty() {
            let reader = RowReader::from_columns(&result.columns);
            let mut pairs = Vec::new();
            for row in &result.rows {
                let prop = row_to_property_node(&reader, row)?;
                let role = reader.get_string(row, "cf.role")?;
                pairs.push((prop, role));
            }
            return Ok(pairs);
        }

        // Follow allOf/ExtendsSchema inheritance
        let parent_result = query_gql_params(
            self,
            &format!(
                "MATCH (:Schema {{title: $title}})-[:ExtendsSchema]->(:Schema)-[:CollapsesTo]->(r:CompositeRange)-[cf:ConsumesField]->(p:Property) \
                 RETURN {PROPERTY_RETURN_COLS}, cf.role"
            ),
            params,
        )?;
        let reader = RowReader::from_columns(&parent_result.columns);
        let mut pairs = Vec::new();
        for row in &parent_result.rows {
            let prop = row_to_property_node(&reader, row)?;
            let role = reader.get_string(row, "cf.role")?;
            pairs.push((prop, role));
        }
        Ok(pairs)
    }

    async fn get_codelist_for_property(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Option<(CodeList, String)>, GraphError> {
        let params = HashMap::from([
            (
                "pname".to_string(),
                grafeo::Value::String(property_name.into()),
            ),
            (
                "stitle".to_string(),
                grafeo::Value::String(schema_title.into()),
            ),
        ]);
        let result = query_gql_params(
            self,
            "MATCH (:Property {name: $pname, _schema_title: $stitle})-[u:UsesCodeList]->(c:CodeList) \
             RETURN c.name, c.description, c.pg_table_name, c.render_as, c.check_expression, u.render_as",
            params,
        )?;
        if result.rows.is_empty() {
            return Ok(None);
        }
        let reader = RowReader::from_columns(&result.columns);
        let codelist = row_to_codelist(&reader, &result.rows[0])?;
        let render_as = reader.get_string(&result.rows[0], "u.render_as")?;
        Ok(Some((codelist, render_as)))
    }

    async fn get_required_extensions(
        &self,
        schema_title: &str,
    ) -> Result<Vec<Extension>, GraphError> {
        let params = HashMap::from([(
            "title".to_string(),
            grafeo::Value::String(schema_title.into()),
        )]);
        let result = query_gql_params(
            self,
            "MATCH (:Schema {title: $title})-[:RequiresExtension]->(e:Extension) RETURN e.name",
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_extension(&reader, row))
            .collect()
    }

    async fn get_composition_tree(
        &self,
        schema_title: &str,
    ) -> Result<CompositionTree, GraphError> {
        let mut visited = std::collections::HashSet::new();
        let root = self
            .build_composition_node(schema_title, schema_title, None, false, &mut visited, 0)
            .await?;
        Ok(CompositionTree { root })
    }

    async fn get_allof_targets(&self, schema_title: &str) -> Result<Vec<String>, GraphError> {
        let params = HashMap::from([(
            "title".to_string(),
            grafeo::Value::String(schema_title.into()),
        )]);
        let result = query_gql_params(
            self,
            "MATCH (:Schema {title: $title})-[:ExtendsSchema {composition_type: 'allOf'}]->(t:Schema) \
             RETURN t.title",
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| reader.get_string(row, "t.title"))
            .collect()
    }

    async fn get_schemas_that_extend(&self, parent_title: &str) -> Result<Vec<SchemaNode>, GraphError> {
        let params = HashMap::from([(
            "title".to_string(),
            grafeo::Value::String(parent_title.into()),
        )]);
        let result = query_gql_params(
            self,
            &format!(
                "MATCH (s:Schema)-[:ExtendsSchema]->(:Schema {{title: $title}}) RETURN {}",
                SCHEMA_RETURN_COLS
            ),
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_schema_node(&reader, row))
            .collect()
    }

    async fn get_referencing_schemas(&self, schema_title: &str) -> Result<Vec<String>, GraphError> {
        let params = HashMap::from([(
            "title".to_string(),
            grafeo::Value::String(schema_title.into()),
        )]);
        let result = query_gql_params(
            self,
            "MATCH (p:Property)-[:ReferencesSchema]->(:Schema {title: $title}) \
             RETURN DISTINCT p._schema_title",
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| reader.get_string(row, "p._schema_title"))
            .collect()
    }

    async fn get_referenced_schemas(&self, schema_title: &str) -> Result<Vec<SchemaNode>, GraphError> {
        let params = HashMap::from([(
            "title".to_string(),
            grafeo::Value::String(schema_title.into()),
        )]);
        let result = query_gql_params(
            self,
            &format!(
                "MATCH (:Schema {{title: $title}})-[:HasProperty]->(p:Property)-[:ReferencesSchema]->(s:Schema) \
                 RETURN DISTINCT {SCHEMA_RETURN_COLS}"
            ),
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_schema_node(&reader, row))
            .collect()
    }

    async fn get_property_ref_target(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Option<SchemaNode>, GraphError> {
        let params = HashMap::from([
            (
                "pname".to_string(),
                grafeo::Value::String(property_name.into()),
            ),
            (
                "stitle".to_string(),
                grafeo::Value::String(schema_title.into()),
            ),
        ]);
        let result = query_gql_params(
            self,
            &format!(
                "MATCH (:Property {{name: $pname, _schema_title: $stitle}})-[:ReferencesSchema]->(s:Schema) \
                 RETURN {SCHEMA_RETURN_COLS}"
            ),
            params,
        )?;
        if result.rows.is_empty() {
            return Ok(None);
        }
        let reader = RowReader::from_columns(&result.columns);
        Ok(Some(row_to_schema_node(&reader, &result.rows[0])?))
    }

    async fn get_property_ref_target_by_id(
        &self,
        property_name: &str,
        schema_id: &str,
    ) -> Result<Option<SchemaNode>, GraphError> {
        let params = HashMap::from([
            (
                "pname".to_string(),
                grafeo::Value::String(property_name.into()),
            ),
            ("sid".to_string(), grafeo::Value::String(schema_id.into())),
        ]);
        let result = query_gql_params(
            self,
            &format!(
                "MATCH (:Property {{name: $pname, _schema_id: $sid}})-[:ReferencesSchema]->(s:Schema) \
                 RETURN {SCHEMA_RETURN_COLS}"
            ),
            params,
        )?;
        if result.rows.is_empty() {
            return Ok(None);
        }
        let reader = RowReader::from_columns(&result.columns);
        Ok(Some(row_to_schema_node(&reader, &result.rows[0])?))
    }

    async fn get_properties_by_schema_id(
        &self,
        schema_id: &str,
    ) -> Result<Vec<PropertyNode>, GraphError> {
        let params =
            HashMap::from([("sid".to_string(), grafeo::Value::String(schema_id.into()))]);
        let result = query_gql_params(
            self,
            &format!(
                "MATCH (p:Property {{_schema_id: $sid}}) RETURN {PROPERTY_RETURN_COLS}"
            ),
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_property_node(&reader, row))
            .collect()
    }

    async fn get_array_item_schema(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Option<SchemaNode>, GraphError> {
        let params = HashMap::from([
            (
                "pname".to_string(),
                grafeo::Value::String(property_name.into()),
            ),
            (
                "stitle".to_string(),
                grafeo::Value::String(schema_title.into()),
            ),
        ]);
        let result = query_gql_params(
            self,
            &format!(
                "MATCH (:Property {{name: $pname, _schema_title: $stitle}})-[:ItemsOf]->(s:Schema) \
                 RETURN {SCHEMA_RETURN_COLS}"
            ),
            params,
        )?;
        if result.rows.is_empty() {
            return Ok(None);
        }
        let reader = RowReader::from_columns(&result.columns);
        Ok(Some(row_to_schema_node(&reader, &result.rows[0])?))
    }

    async fn get_generation_order(&self) -> Result<Vec<String>, GraphError> {
        // Get all schema titles
        let all_result = query_gql(self, "MATCH (s:Schema) RETURN s.title")?;
        let reader = RowReader::from_columns(&all_result.columns);
        let all_titles: Vec<String> = all_result
            .rows
            .iter()
            .map(|row| reader.get_string(row, "s.title"))
            .collect::<Result<_, _>>()?;

        // Get DependsOn edges
        let edge_result = query_gql(
            self,
            "MATCH (a:Schema)-[:DependsOn]->(b:Schema) RETURN a.title, b.title",
        )?;
        let edge_reader = RowReader::from_columns(&edge_result.columns);

        // Build adjacency and in-degree
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();

        for title in &all_titles {
            in_degree.insert(title.clone(), 0);
            adjacency.entry(title.clone()).or_default();
        }

        for row in &edge_result.rows {
            let from = edge_reader.get_string(row, "a.title")?;
            let to = edge_reader.get_string(row, "b.title")?;
            // from depends on to, so to must come first
            adjacency.entry(to.clone()).or_default().push(from.clone());
            *in_degree.entry(from).or_default() += 1;
        }

        // Kahn's algorithm
        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(t, _)| t.clone())
            .collect();
        queue.make_contiguous().sort(); // deterministic ordering

        let mut order = Vec::new();
        while let Some(current) = queue.pop_front() {
            order.push(current.clone());
            if let Some(dependents) = adjacency.get(&current) {
                for dep in dependents {
                    if let Some(deg) = in_degree.get_mut(dep) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(dep.clone());
                        }
                    }
                }
            }
        }

        // Append any remaining (cycles)
        for title in &all_titles {
            if !order.contains(title) {
                order.push(title.clone());
            }
        }

        Ok(order)
    }

    async fn list_all_schema_references(&self) -> Result<Vec<(String, String)>, GraphError> {
        let gql = "MATCH (s:Schema)-[:HasProperty]->(:Property)-[:ReferencesSchema]->(t:Schema) \
                   RETURN DISTINCT s.title, t.title";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut refs = Vec::new();
        for row in &result.rows {
            let src = reader.get_string(row, "s.title")?;
            let tgt = reader.get_string(row, "t.title")?;
            refs.push((src, tgt));
        }
        Ok(refs)
    }

    async fn list_all_properties(&self) -> Result<HashMap<String, Vec<PropertyNode>>, GraphError> {
        let gql = format!(
            "MATCH (s:Schema)-[:HasProperty]->(p:Property) RETURN s.title, {PROPERTY_RETURN_COLS}"
        );
        let result = query_gql(self, &gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut map: HashMap<String, Vec<PropertyNode>> = HashMap::new();
        for row in &result.rows {
            let schema_title = reader.get_string(row, "s.title")?;
            let prop = row_to_property_node(&reader, row)?;
            map.entry(schema_title).or_default().push(prop);
        }
        Ok(map)
    }

    // ── IFML query methods ─────────────────────────────────────────────

    async fn get_ifml_view_containers(&self) -> Result<Vec<ViewContainerNode>, GraphError> {
        let gql = "MATCH (vc:ViewContainer) RETURN \
            vc.name, vc.label, vc.is_xor, vc.is_default, \
            vc.is_landmark, vc.is_modal, vc.domain \
            ORDER BY vc.name";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            nodes.push(ViewContainerNode {
                name: reader.get_string(row, "vc.name")?,
                label: reader.get_opt_string(row, "vc.label")?,
                is_xor: reader.get_bool(row, "vc.is_xor")?,
                is_default: reader.get_bool(row, "vc.is_default")?,
                is_landmark: reader.get_bool(row, "vc.is_landmark")?,
                is_modal: reader.get_bool(row, "vc.is_modal")?,
                domain: reader.get_opt_string(row, "vc.domain")?,
            });
        }
        Ok(nodes)
    }

    async fn get_ifml_view_components(
        &self,
        container_name: &str,
    ) -> Result<Vec<ViewComponentNode>, GraphError> {
        let escaped = container_name.replace('\'', "\\'");
        let gql = format!(
            "MATCH (vc:ViewContainer {{name: '{escaped}'}})-[:ContainsViewComponent]->(comp:ViewComponent) \
             RETURN comp.name, comp.component_type, comp.mode, comp.entity, \
             comp.fields, comp.filter, comp.domain ORDER BY comp.name"
        );
        let result = query_gql(self, &gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            let fields_str: Option<String> = reader.get_opt_string(row, "comp.fields")?;
            let fields: Option<Vec<String>> = fields_str
                .and_then(|s| serde_json::from_str(&s).ok());
            nodes.push(ViewComponentNode {
                name: reader.get_string(row, "comp.name")?,
                component_type: reader.get_string(row, "comp.component_type")?,
                mode: reader.get_opt_string(row, "comp.mode")?,
                entity: reader.get_opt_string(row, "comp.entity")?,
                fields,
                filter: reader.get_opt_string(row, "comp.filter")?,
                domain: reader.get_opt_string(row, "comp.domain")?,
            });
        }
        Ok(nodes)
    }

    async fn get_ifml_events(&self, parent_id: &str) -> Result<Vec<EventNode>, GraphError> {
        let escaped = parent_id.replace('\'', "\\'");
        let gql = format!(
            "MATCH (parent)-[:HasEvent]->(evt:Event) \
             WHERE parent.name = '{escaped}' \
             RETURN evt.name, evt.event_type, evt.params, evt.domain ORDER BY evt.name"
        );
        let result = query_gql(self, &gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            let params_str: Option<String> = reader.get_opt_string(row, "evt.params")?;
            let params: Option<Vec<String>> = params_str
                .and_then(|s| serde_json::from_str(&s).ok());
            nodes.push(EventNode {
                name: reader.get_string(row, "evt.name")?,
                event_type: reader.get_string(row, "evt.event_type")?,
                params,
                domain: reader.get_opt_string(row, "evt.domain")?,
            });
        }
        Ok(nodes)
    }

    async fn get_ifml_navigation_flows(
        &self,
    ) -> Result<Vec<(String, String, String)>, GraphError> {
        let gql = "MATCH (source)-[:HasEvent]->(evt:Event)-[flow:NavigationFlow]->(target:ViewContainer) \
                   RETURN source.name, evt.name, target.name";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut flows = Vec::new();
        for row in &result.rows {
            flows.push((
                reader.get_string(row, "source.name")?,
                reader.get_string(row, "evt.name")?,
                reader.get_string(row, "target.name")?,
            ));
        }
        Ok(flows)
    }

    async fn get_ifml_data_flows(
        &self,
    ) -> Result<Vec<(String, String, Option<String>, Option<String>)>, GraphError> {
        let gql = "MATCH (source)-[flow:DataFlow]->(target) \
                   RETURN source.name, target.name, flow.source_param, flow.target_param";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut flows = Vec::new();
        for row in &result.rows {
            flows.push((
                reader.get_string(row, "source.name")?,
                reader.get_string(row, "target.name")?,
                reader.get_opt_string(row, "flow.source_param")?,
                reader.get_opt_string(row, "flow.target_param")?,
            ));
        }
        Ok(flows)
    }

    async fn get_ifml_actions(&self) -> Result<Vec<ActionNode>, GraphError> {
        let gql = "MATCH (a:ActionNode) RETURN a.name, a.domain ORDER BY a.name";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            nodes.push(ActionNode {
                name: reader.get_string(row, "a.name")?,
                domain: reader.get_opt_string(row, "a.domain")?,
            });
        }
        Ok(nodes)
    }

    async fn get_ifml_parameters(&self) -> Result<Vec<ParameterDefinitionNode>, GraphError> {
        let gql = "MATCH (p:ParameterDefinition) RETURN p.name, p.direction, p.type_ref, p.domain ORDER BY p.name";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            nodes.push(ParameterDefinitionNode {
                name: reader.get_string(row, "p.name")?,
                direction: reader.get_string(row, "p.direction")?,
                type_ref: reader.get_string(row, "p.type_ref")?,
                domain: reader.get_opt_string(row, "p.domain")?,
            });
        }
        Ok(nodes)
    }
}

/// Maximum nesting depth for recursive composition tree building.
const MAX_COMPOSITION_DEPTH: usize = 10;

impl GrafeoEngine {
    async fn build_composition_node(
        &self,
        schema_title: &str,
        field_name: &str,
        fk: Option<FkDirection>,
        is_collection: bool,
        visited: &mut std::collections::HashSet<String>,
        depth: usize,
    ) -> Result<CompositionNode, GraphError> {
        let schema = self
            .get_schema(schema_title)
            .await?
            .ok_or_else(|| GraphError::NotFound(format!("Schema '{schema_title}'")))?;

        let default_schema = schema
            .domain
            .clone()
            .unwrap_or_else(|| "public".to_string());

        let properties = self.get_properties(schema_title).await?;
        let mut columns = Vec::new();
        let mut jsonb_columns = Vec::new();
        let mut children = Vec::new();

        // Resolve composite range and consumed fields for this node
        let composite_range = self.get_composite_range(schema_title).await.ok().flatten();
        let consumed_fields_raw = self
            .get_consumed_fields(schema_title)
            .await
            .unwrap_or_default();
        let consumed_field_names: Vec<String> = consumed_fields_raw
            .iter()
            .map(|(p, _)| p.name.clone())
            .collect();
        let consumed_set: std::collections::HashSet<&str> =
            consumed_field_names.iter().map(|s| s.as_str()).collect();

        for prop in &properties {
            // Skip fields consumed by composite ranges
            if consumed_set.contains(prop.name.as_str()) {
                continue;
            }

            let is_codelist_fk = self
                .get_codelist_for_property(&prop.name, schema_title)
                .await?
                .is_some();
            let composite_columns = self.get_composite_columns(&prop.name, schema_title).await?;
            let classification = prop.effective_kind();

            // Resolve FK target for reference columns
            let fk_target = match classification {
                Some(codegraph_type_contracts::RefClassificationKind::CodelistReference) => {
                    // Resolve FK for both scalar and array codelists —
                    // array codelists need the FK target for their child table's code column.
                    self.resolve_fk_target(
                        &prop.name,
                        schema_title,
                        &default_schema,
                        prop.ref_target.as_deref(),
                        "code",
                        "RESTRICT",
                    )
                    .await
                }
                Some(codegraph_type_contracts::RefClassificationKind::EntityReference) => {
                    if !prop.is_array {
                        self.resolve_fk_target(
                            &prop.name,
                            schema_title,
                            &default_schema,
                            prop.ref_target.as_deref(),
                            "id",
                            "SET NULL",
                        )
                        .await
                    } else {
                        None
                    }
                }
                _ => None,
            };

            // Resolve enum values for check-constraint columns
            let check_values = match classification {
                Some(codegraph_type_contracts::RefClassificationKind::CodelistCheck)
                | Some(codegraph_type_contracts::RefClassificationKind::InlineEnum)
                    if !prop.is_array =>
                {
                    if let Some(ref codelist_name) = prop.ref_target {
                        self.get_enum_values(codelist_name)
                            .await
                            .ok()
                            .map(|vals| vals.into_iter().map(|v| v.value).collect())
                            .unwrap_or_default()
                    } else {
                        vec![]
                    }
                }
                _ => vec![],
            };

            let col = ColumnInfo {
                name: prop.pg_column_name.clone(),
                description: prop.description.clone(),
                rust_type: prop.rust_field_type.clone(),
                postgres_type: prop.pg_column_type.clone(),
                is_optional: !prop.is_required,
                is_codelist_fk,
                composite_columns,
                is_array: prop.is_array,
                classification: classification.clone(),
                fk_target,
                check_values,
            };

            // ValueObject properties → recurse into child nodes instead of
            // flattening into jsonb_columns. This matches the DDL child-table
            // hierarchy: each ValueObject becomes a separate SQL table.
            if classification == Some(codegraph_type_contracts::RefClassificationKind::ValueObject) {
                if depth < MAX_COMPOSITION_DEPTH {
                    // Resolve target schema
                    let target = if prop.is_array {
                        self.get_array_item_schema(&prop.name, schema_title)
                            .await
                            .ok()
                            .flatten()
                    } else {
                        self.get_property_ref_target(&prop.name, schema_title)
                            .await
                            .ok()
                            .flatten()
                    };

                    if let Some(target_schema) = target {
                        // Non-array entity targets get a FK column on this node.
                        // Array entity targets are SKIPPED — a one-to-many relationship
                        // cannot be represented by a single UUID FK on the parent. The FK
                        // lives on the child entity's table instead (configured via
                        // parent_ref in domains.toml).
                        let vo_entity = if !target_schema.is_entity {
                            codegraph_core::traits::find_entity_extended_by_vo(self, &target_schema.title)
                                .await
                                .ok()
                                .flatten()
                        } else {
                            None
                        };

                        if (target_schema.is_entity || vo_entity.is_some()) && !prop.is_array {
                            let mut entity_col = col;
                            entity_col.classification =
                                Some(codegraph_type_contracts::RefClassificationKind::EntityReference);
                            if let Some(entity) = &vo_entity {
                                entity_col.fk_target = Some(FkTarget {
                                    schema: entity.domain.clone().unwrap_or_else(|| default_schema.to_string()),
                                    table: entity.pg_table_name.clone(),
                                    column: "id".to_string(),
                                    on_delete: "SET NULL".to_string(),
                                });
                            } else {
                                entity_col.fk_target = self
                                    .resolve_fk_target(
                                        &prop.name,
                                        schema_title,
                                        &default_schema,
                                        prop.ref_target.as_deref(),
                                        "id",
                                        "SET NULL",
                                    )
                                    .await;
                            }
                            columns.push(entity_col);
                        }
                        if !target_schema.is_entity
                            && !visited.contains(&target_schema.title)
                        {
                            // Recurse into ValueObject as a child node.
                            // Use a fresh visited set (seeded with the current
                            // path) so sibling VO properties referencing the same
                            // schema type each get their own child table — matching
                            // the entity generator's per-property visited approach.
                            let mut child_visited = visited.clone();
                            child_visited.insert(target_schema.title.clone());
                            let child_fk = Some(FkDirection::OnChild {
                                column: format!(
                                    "{}_id",
                                    codegraph_naming::truncate_pg_identifier(&schema.pg_table_name)
                                ),
                            });
                            let child_node = Box::pin(self.build_composition_node(
                                &target_schema.title,
                                &prop.pg_column_name,
                                child_fk,
                                prop.is_array,
                                &mut child_visited,
                                depth + 1,
                            ))
                            .await?;
                            children.push(child_node);
                        }
                    }
                }
                continue;
            }

            // Codelist array properties → synthetic child node with a single
            // "code" column.  Codelist schemas are plain enums (no object
            // properties to recurse into), so we build the CompositionNode
            // directly instead of recursing via build_composition_node.
            if prop.is_array
                && matches!(
                    classification,
                    Some(codegraph_type_contracts::RefClassificationKind::CodelistReference)
                        | Some(codegraph_type_contracts::RefClassificationKind::CodelistCheck)
                )
            {
                let child_table = codegraph_naming::truncate_pg_identifier(&format!(
                    "{}_{}",
                    schema.pg_table_name, prop.pg_column_name
                ));
                let child_fk_col = format!(
                    "{}_id",
                    codegraph_naming::truncate_pg_identifier(&schema.pg_table_name)
                );

                let codelist_title = prop
                    .ref_target
                    .as_deref()
                    .map(|r| {
                        r.rsplit('/')
                            .next()
                            .unwrap_or(r)
                            .trim_end_matches(".json#")
                            .trim_end_matches(".json")
                            .to_string()
                    })
                    .unwrap_or_else(|| prop.name.clone());

                let code_col = ColumnInfo {
                    name: "code".to_string(),
                    description: col.description.clone(),
                    rust_type: "String".to_string(),
                    postgres_type: "TEXT".to_string(),
                    is_optional: false,
                    is_codelist_fk: true,
                    composite_columns: vec![],
                    is_array: false,
                    classification: col.classification.clone(),
                    fk_target: col.fk_target.clone(),
                    check_values: col.check_values.clone(),
                };

                children.push(CompositionNode {
                    field_name: prop.pg_column_name.clone(),
                    schema_title: codelist_title,
                    table_schema: default_schema.clone(),
                    table_name: child_table,
                    fk: Some(FkDirection::OnChild {
                        column: child_fk_col,
                    }),
                    is_collection: true,
                    columns: vec![code_col],
                    jsonb_columns: vec![],
                    children: vec![],
                    composite_range: None,
                    consumed_fields: vec![],
                });
                continue;
            }

            if let Some(ref_target) = &prop.ref_target {
                if let Some(target_schema) = self.get_schema(ref_target).await? {
                    if !target_schema.is_entity
                        && !target_schema.is_codelist
                        && target_schema.schema_type == "object"
                    {
                        jsonb_columns.push(col);
                        continue;
                    }
                }
            }
            columns.push(col);
        }

        // Query ExtendsSchema edges for children (allOf composition)
        let params = HashMap::from([(
            "title".to_string(),
            grafeo::Value::String(schema_title.into()),
        )]);
        let child_result = query_gql_params(
            self,
            "MATCH (:Schema {title: $title})-[e:ExtendsSchema]->(child:Schema) \
             RETURN child.title, e.composition_type",
            params,
        )?;

        if !child_result.rows.is_empty() {
            let child_reader = RowReader::from_columns(&child_result.columns);
            for row in &child_result.rows {
                let child_title = child_reader.get_string(row, "child.title")?;
                let comp_type = child_reader.get_opt_string(row, "e.composition_type")?;

                if visited.contains(&child_title) {
                    continue;
                }
                visited.insert(child_title.clone());

                let child_fk = Some(FkDirection::OnChild {
                    column: format!("{}_id", schema_title.to_lowercase()),
                });
                let child_is_collection = comp_type.as_deref() == Some("collection");
                let child_field_name = child_title.to_lowercase();

                let child_node = Box::pin(self.build_composition_node(
                    &child_title,
                    &child_field_name,
                    child_fk,
                    child_is_collection,
                    visited,
                    depth + 1,
                ))
                .await?;
                children.push(child_node);
            }
        }

        Ok(CompositionNode {
            field_name: field_name.to_string(),
            schema_title: schema_title.to_string(),
            table_schema: default_schema,
            table_name: schema.pg_table_name.clone(),
            fk,
            is_collection,
            columns,
            jsonb_columns,
            children,
            composite_range,
            consumed_fields: consumed_field_names,
        })
    }

    /// Resolve a property's FK target to (schema, table, column, on_delete) using graph edges.
    async fn resolve_fk_target(
        &self,
        property_name: &str,
        schema_title: &str,
        default_schema: &str,
        ref_target: Option<&str>,
        target_column: &str,
        on_delete: &str,
    ) -> Option<FkTarget> {
        // Try ReferencesSchema edge, then ItemsOf edge (for array properties)
        let target_schema = if let Ok(Some(ts)) = self
            .get_property_ref_target(property_name, schema_title)
            .await
        {
            Some(ts)
        } else if let Ok(Some(ts)) = self
            .get_array_item_schema(property_name, schema_title)
            .await
        {
            Some(ts)
        } else {
            None
        };

        if let Some(ts) = target_schema {
            let schema_name = if ts.is_codelist {
                "common".to_string()
            } else {
                ts.domain.unwrap_or_else(|| default_schema.to_string())
            };
            if !ts.pg_table_name.is_empty() {
                return Some(FkTarget {
                    schema: schema_name,
                    table: ts.pg_table_name,
                    column: target_column.to_string(),
                    on_delete: on_delete.to_string(),
                });
            }
        }

        // Fallback: parse the ref_target string path
        let ref_str = ref_target.unwrap_or("");
        if ref_str.is_empty() {
            return None;
        }
        let is_codelist_ref = ref_str.contains("/codelist/") || ref_str.starts_with("codelist/");
        let schema_name = if is_codelist_ref {
            "common".to_string()
        } else {
            let domain = extract_ref_domain(ref_str)
                .unwrap_or(default_schema);
            // If the "domain" looks like a JSON schema filename (contains `.json`),
            // the ref_target is a bare filename without path (no domain prefix).
            // Use the default schema instead of the filename.
            if domain.contains(".json") || domain.contains(".json#") {
                default_schema.to_string()
            } else {
                domain.to_string()
            }
        };
        let table = extract_ref_table(ref_str)?;
        Some(FkTarget {
            schema: schema_name,
            table,
            column: target_column.to_string(),
            on_delete: on_delete.to_string(),
        })
    }
}

/// Extract the domain name from a JSON Schema $ref path.
///
/// Examples:
///   "common/json/GenderCodeList.json"       → Some("common")
///   "../../../common/json/codelist/X.json"   → Some("common")
///   "codelist/CandidateRelationshipCodeList.json" → None
fn extract_ref_domain(ref_target: &str) -> Option<&str> {
    let segments: Vec<&str> = ref_target
        .split('/')
        .filter(|s| !s.is_empty() && *s != "..")
        .collect();
    for (i, seg) in segments.iter().enumerate() {
        if *seg == "json" && i > 0 {
            return Some(segments[i - 1]);
        }
    }
    segments.first().copied().filter(|s| *s != "codelist")
}

/// Extract the table name from a JSON Schema $ref path by converting the filename.
fn extract_ref_table(ref_target: &str) -> Option<String> {
    let filename = ref_target.rsplit('/').next()?;
    let stem = filename
        .strip_suffix(".json#")
        .or_else(|| filename.strip_suffix(".json"))
        .unwrap_or(filename);
    Some(codegraph_naming::to_snake_case(&codegraph_naming::strip_type_suffix(
        stem,
    )))
}
