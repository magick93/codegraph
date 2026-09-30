use std::collections::HashMap;

use codegraph_core::error::GraphError;
use codegraph_core::types::{
    CodeList, ColumnInfo, CompositeColumn, CompositeRange, CompositionNode, CompositionTree,
    Extension, FkDirection, FkTarget, PropertyNode, SchemaNode, StructuredSubField,
};

use super::{query_gql_params, PROPERTY_RETURN_COLS, SCHEMA_RETURN_COLS};
use crate::conversions::{
    row_to_codelist, row_to_composite_column, row_to_composite_range, row_to_extension,
    row_to_property_node, row_to_schema_node, row_to_structured_sub_field, RowReader,
};
use crate::engine::GrafeoEngine;

impl GrafeoEngine {
    pub(super) async fn query_composite_columns(
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

    pub(super) async fn query_structured_sub_fields(
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

    pub(super) async fn query_composite_range(
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

    pub(super) async fn query_consumed_fields(
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

    pub(super) async fn query_codelist_for_property(
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

    pub(super) async fn query_required_extensions(
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

    pub(super) async fn query_composition_tree(
        &self,
        schema_title: &str,
    ) -> Result<CompositionTree, GraphError> {
        let mut visited = std::collections::HashSet::new();
        let mut root = self
            .build_composition_node(schema_title, schema_title, None, false, &mut visited, 0)
            .await?;
        root.dedup_fields();
        Ok(CompositionTree { root })
    }

    pub(super) async fn query_allof_targets(&self, schema_title: &str) -> Result<Vec<String>, GraphError> {
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

    pub(super) async fn query_schemas_that_extend(
        &self,
        parent_title: &str,
    ) -> Result<Vec<SchemaNode>, GraphError> {
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

    pub(super) async fn query_referencing_schemas(&self, schema_title: &str) -> Result<Vec<String>, GraphError> {
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

    pub(super) async fn query_referenced_schemas(
        &self,
        schema_title: &str,
    ) -> Result<Vec<SchemaNode>, GraphError> {
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

    pub(super) async fn query_property_ref_target(
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

    pub(super) async fn query_property_ref_target_by_id(
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

    pub(super) async fn query_properties_by_schema_id(
        &self,
        schema_id: &str,
    ) -> Result<Vec<PropertyNode>, GraphError> {
        let params = HashMap::from([("sid".to_string(), grafeo::Value::String(schema_id.into()))]);
        let result = query_gql_params(
            self,
            &format!("MATCH (p:Property {{_schema_id: $sid}}) RETURN {PROPERTY_RETURN_COLS}"),
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_property_node(&reader, row))
            .collect()
    }

    pub(super) async fn query_array_item_schema(
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

}

/// Maximum nesting depth for recursive composition tree building.
const MAX_COMPOSITION_DEPTH: usize = 10;

/// Build the synthetic codelist-array child node (single "code" column) for a
/// codelist array property and push it onto `children`.
fn push_codelist_array_child(
    prop: &PropertyNode,
    col: &ColumnInfo,
    schema: &SchemaNode,
    default_schema: &str,
    children: &mut Vec<CompositionNode>,
) {
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
        table_schema: default_schema.to_string(),
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
}

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
            .query_schema(schema_title)
            .await?
            .ok_or_else(|| GraphError::NotFound(format!("Schema '{schema_title}'")))?;

        let default_schema = schema
            .domain
            .clone()
            .unwrap_or_else(|| "public".to_string());

        let properties = self.query_properties(schema_title).await?;
        let mut columns = Vec::new();
        let mut jsonb_columns = Vec::new();
        let mut children = Vec::new();

        // Resolve composite range and consumed fields for this node
        let composite_range = self.query_composite_range(schema_title).await.ok().flatten();
        let consumed_fields_raw = self
            .query_consumed_fields(schema_title)
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
                .query_codelist_for_property(&prop.name, schema_title)
                .await?
                .is_some();
            let composite_columns = self.query_composite_columns(&prop.name, schema_title).await?;
            let classification = prop.effective_kind();

            // Resolve FK target for reference columns
            let fk_target = self
                .resolve_property_fk_target(prop, schema_title, &default_schema, &classification)
                .await;

            // Resolve enum values for check-constraint columns
            let check_values = self
                .resolve_property_check_values(prop, &classification)
                .await;

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
            if classification == Some(codegraph_type_contracts::RefClassificationKind::ValueObject)
            {
                self.push_value_object_child(
                    prop,
                    col,
                    schema_title,
                    &schema,
                    &default_schema,
                    visited,
                    depth,
                    &mut columns,
                    &mut children,
                )
                .await?;
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
                push_codelist_array_child(prop, &col, &schema, &default_schema, &mut children);
                continue;
            }

            // Array of entity refs: the relationship cannot be represented by a
            // column on the parent table. Two shapes are supported:
            //   1. FK-on-child: the target schema carries a back-reference
            //      column to this schema (e.g. party.case_id for case.party_ids).
            //      The child-side FK is injected by the DDL generator's parent
            //      candidate handling, so the parent side is skipped entirely.
            //   2. Junction table: no back-reference exists. Emit a
            //      `<parent>_<field>` junction child table holding parent_id +
            //      child_id FKs (many-to-many).
            if prop.is_array
                && classification
                    == Some(codegraph_type_contracts::RefClassificationKind::EntityReference)
            {
                self.push_junction_array_child(
                    prop,
                    schema_title,
                    &schema,
                    &default_schema,
                    &mut children,
                )
                .await?;
                continue;
            }

            if let Some(ref_target) = &prop.ref_target {
                if let Some(target_schema) = self.query_schema(ref_target).await? {
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
        self.push_allof_children(schema_title, visited, depth, &mut children)
            .await?;

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

    async fn resolve_property_fk_target(
        &self,
        prop: &PropertyNode,
        schema_title: &str,
        default_schema: &str,
        classification: &Option<codegraph_type_contracts::RefClassificationKind>,
    ) -> Option<FkTarget> {
        match classification {
            Some(codegraph_type_contracts::RefClassificationKind::CodelistReference) => {
                // Resolve FK for both scalar and array codelists —
                // array codelists need the FK target for their child table's code column.
                self.resolve_fk_target(
                    &prop.name,
                    schema_title,
                    default_schema,
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
                        default_schema,
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
        }
    }

    async fn resolve_property_check_values(
        &self,
        prop: &PropertyNode,
        classification: &Option<codegraph_type_contracts::RefClassificationKind>,
    ) -> Vec<String> {
        match classification {
            Some(codegraph_type_contracts::RefClassificationKind::CodelistCheck)
            | Some(codegraph_type_contracts::RefClassificationKind::InlineEnum)
                if !prop.is_array =>
            {
                if let Some(ref codelist_name) = prop.ref_target {
                    self.query_enum_values(codelist_name)
                        .await
                        .ok()
                        .map(|vals| vals.into_iter().map(|v| v.value).collect())
                        .unwrap_or_default()
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn push_value_object_child(
        &self,
        prop: &PropertyNode,
        col: ColumnInfo,
        schema_title: &str,
        schema: &SchemaNode,
        default_schema: &str,
        visited: &mut std::collections::HashSet<String>,
        depth: usize,
        columns: &mut Vec<ColumnInfo>,
        children: &mut Vec<CompositionNode>,
    ) -> Result<(), GraphError> {
        if depth < MAX_COMPOSITION_DEPTH {
            // Resolve target schema
            let target = if prop.is_array {
                self.query_array_item_schema(&prop.name, schema_title)
                    .await
                    .ok()
                    .flatten()
            } else {
                self.query_property_ref_target(&prop.name, schema_title)
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
                    // VO→entity synthetic FK columns are always nullable: the
                    // DTO and repository generators model the VO as a nested
                    // child table, so no create command ever supplies a value
                    // for this column. Genuine EntityReference columns honor
                    // the schema's `required` (is_optional was already derived
                    // from !prop.is_required above).
                    if vo_entity.is_some() {
                        entity_col.is_optional = true;
                    }
                    if let Some(entity) = &vo_entity {
                        entity_col.fk_target = Some(FkTarget {
                            schema: entity
                                .domain
                                .clone()
                                .unwrap_or_else(|| default_schema.to_string()),
                            table: entity.pg_table_name.clone(),
                            column: "id".to_string(),
                            on_delete: "SET NULL".to_string(),
                        });
                    } else {
                        entity_col.fk_target = self
                            .resolve_fk_target(
                                &prop.name,
                                schema_title,
                                default_schema,
                                prop.ref_target.as_deref(),
                                "id",
                                "SET NULL",
                            )
                            .await;
                    }
                    columns.push(entity_col);
                }
                if !target_schema.is_entity && !visited.contains(&target_schema.title) {
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
        Ok(())
    }

    async fn push_junction_array_child(
        &self,
        prop: &PropertyNode,
        schema_title: &str,
        schema: &SchemaNode,
        default_schema: &str,
        children: &mut Vec<CompositionNode>,
    ) -> Result<(), GraphError> {
        let target_title = self
            .query_array_item_schema(&prop.name, schema_title)
            .await
            .ok()
            .flatten();
        let has_back_ref = match &target_title {
            Some(target_schema) => {
                let back_ref = format!(
                    "{}_id",
                    codegraph_naming::truncate_pg_identifier(&schema.pg_table_name)
                );
                self.query_properties(&target_schema.title)
                    .await
                    .map(|ps| {
                        ps.iter().any(|p| {
                            p.pg_column_name == back_ref
                                || p.pg_column_name
                                    == format!(
                                        "{}_id",
                                        codegraph_naming::to_snake_case(&schema.title)
                                    )
                        })
                    })
                    .unwrap_or(false)
            }
            None => false,
        };

        if !has_back_ref {
            if let Some(target_schema) = target_title {
                let child_table = codegraph_naming::truncate_pg_identifier(&format!(
                    "{}_{}",
                    schema.pg_table_name, prop.pg_column_name
                ));
                let child_fk_col = codegraph_naming::truncate_pg_identifier(&format!(
                    "{}_id",
                    schema.pg_table_name
                ));
                let child_id_col = codegraph_naming::truncate_pg_identifier(&format!(
                    "{}_id",
                    target_schema.pg_table_name
                ));

                let child_col = ColumnInfo {
                    name: child_id_col.clone(),
                    description: Some(format!(
                        "FK to {}.{}",
                        target_schema
                            .domain
                            .clone()
                            .unwrap_or_else(|| default_schema.to_string()),
                        target_schema.pg_table_name
                    )),
                    rust_type: "uuid::Uuid".to_string(),
                    postgres_type: "UUID".to_string(),
                    is_optional: false,
                    is_codelist_fk: false,
                    composite_columns: vec![],
                    is_array: false,
                    classification: Some(
                        codegraph_type_contracts::RefClassificationKind::EntityReference,
                    ),
                    fk_target: Some(FkTarget {
                        schema: target_schema
                            .domain
                            .clone()
                            .unwrap_or_else(|| default_schema.to_string()),
                        table: target_schema.pg_table_name.clone(),
                        column: "id".to_string(),
                        on_delete: "CASCADE".to_string(),
                    }),
                    check_values: vec![],
                };

                children.push(CompositionNode {
                    field_name: prop.pg_column_name.clone(),
                    schema_title: target_schema.title.clone(),
                    table_schema: default_schema.to_string(),
                    table_name: child_table,
                    fk: Some(FkDirection::OnChild {
                        column: child_fk_col,
                    }),
                    is_collection: true,
                    columns: vec![child_col],
                    jsonb_columns: vec![],
                    children: vec![],
                    composite_range: None,
                    consumed_fields: vec![],
                });
            }
        }
        Ok(())
    }

    async fn push_allof_children(
        &self,
        schema_title: &str,
        visited: &mut std::collections::HashSet<String>,
        depth: usize,
        children: &mut Vec<CompositionNode>,
    ) -> Result<(), GraphError> {
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
        Ok(())
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
            .query_property_ref_target(property_name, schema_title)
            .await
        {
            Some(ts)
        } else if let Ok(Some(ts)) = self
            .query_array_item_schema(property_name, schema_title)
            .await
        {
            Some(ts)
        } else {
            None
        };

        if let Some(ts) = target_schema {
            // Only create FK targets for entities (types with their own tables).
            // VOs are embedded as child tables, not referenced via FK.
            // Codelists always live in the "common" schema regardless of source domain.
            let schema_name = if ts.is_codelist {
                "common".to_string()
            } else if !ts.is_entity {
                return None;
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

        // Fallback: parse the ref_target string path.
        // This is a last-resort heuristic when graph edges are missing.
        let ref_str = ref_target.unwrap_or("");
        if ref_str.is_empty() {
            return None;
        }
        let is_codelist_ref = ref_str.contains("/codelist/") || ref_str.starts_with("codelist/");
        let schema_name = if is_codelist_ref {
            "common".to_string()
        } else {
            let domain = extract_ref_domain(ref_str).unwrap_or(default_schema);
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

        // Verify the target exists as an entity in the graph before emitting FK.
        // Try to find the target by table name in the resolved schema domain.
        if let Ok(Some(target_check)) = self.query_schema_in_domain(&table, &schema_name).await {
            if !target_check.is_entity {
                return None;
            }
        }
        // If the schema doesn't exist in the resolved domain, try the default domain
        // as a fallback (cross-domain allOf references).
        if schema_name != default_schema {
            if let Ok(Some(target_check)) = self.query_schema_in_domain(&table, default_schema).await
            {
                if target_check.is_entity {
                    return Some(FkTarget {
                        schema: default_schema.to_string(),
                        table,
                        column: target_column.to_string(),
                        on_delete: on_delete.to_string(),
                    });
                }
            }
        }

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
    Some(codegraph_naming::to_snake_case(
        &codegraph_naming::strip_suffix(stem, "Type"),
    ))
}
