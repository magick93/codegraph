use crate::error::GraphError;
use crate::traits::{GraphIngestor, GraphQuerier};
use crate::types::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;
use uuid::Uuid;

pub struct MockEngine {
    schemas: Mutex<HashMap<String, SchemaNode>>,
    properties: Mutex<HashMap<String, Vec<PropertyNode>>>,
    codelists: Mutex<HashMap<String, CodeList>>,
    enum_values: Mutex<HashMap<String, Vec<EnumValue>>>,
    trees: Mutex<HashMap<String, CompositionTree>>,
    composite_ranges: Mutex<HashMap<String, CompositeRange>>,
    consumed_fields: Mutex<HashMap<String, Vec<(PropertyNode, String)>>>,
    /// Maps (property_name, schema_title) -> target SchemaNode for $ref resolution
    ref_targets: Mutex<HashMap<(String, String), SchemaNode>>,
    parent_candidates: Mutex<Vec<ParentCandidate>>,
    extends_map: Mutex<HashMap<String, Vec<SchemaNode>>>,
    allof_targets: Mutex<HashMap<String, Vec<String>>>,
    view_containers: Mutex<HashMap<String, ViewContainerNode>>,
    view_components: Mutex<HashMap<String, ViewComponentNode>>,
    events: Mutex<HashMap<String, EventNode>>,
    action_nodes: Mutex<HashMap<String, ActionNode>>,
    parameter_definitions: Mutex<HashMap<String, ParameterDefinitionNode>>,
    data_bindings: Mutex<HashMap<String, DataBindingNode>>,
    start_time: Instant,
}

impl MockEngine {
    pub fn new() -> Self {
        Self {
            schemas: Mutex::new(HashMap::new()),
            properties: Mutex::new(HashMap::new()),
            codelists: Mutex::new(HashMap::new()),
            enum_values: Mutex::new(HashMap::new()),
            trees: Mutex::new(HashMap::new()),
            composite_ranges: Mutex::new(HashMap::new()),
            consumed_fields: Mutex::new(HashMap::new()),
            ref_targets: Mutex::new(HashMap::new()),
            parent_candidates: Mutex::new(Vec::new()),
            extends_map: Mutex::new(HashMap::new()),
            allof_targets: Mutex::new(HashMap::new()),
            view_containers: Mutex::new(HashMap::new()),
            view_components: Mutex::new(HashMap::new()),
            events: Mutex::new(HashMap::new()),
            action_nodes: Mutex::new(HashMap::new()),
            parameter_definitions: Mutex::new(HashMap::new()),
            data_bindings: Mutex::new(HashMap::new()),
            start_time: Instant::now(),
        }
    }

    pub fn builder() -> MockEngineBuilder {
        MockEngineBuilder::default()
    }
}

impl Default for MockEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
pub struct MockEngineBuilder {
    schemas: Vec<SchemaNode>,
    properties: HashMap<String, Vec<PropertyNode>>,
    trees: HashMap<String, CompositionTree>,
    composite_ranges: HashMap<String, CompositeRange>,
    consumed_fields: HashMap<String, Vec<(PropertyNode, String)>>,
    ref_targets: HashMap<(String, String), SchemaNode>,
    parent_candidates: Vec<ParentCandidate>,
    extends_map: HashMap<String, Vec<SchemaNode>>,
    allof_targets: HashMap<String, Vec<String>>,
    enum_values: HashMap<String, Vec<EnumValue>>,
}

impl MockEngineBuilder {
    pub fn with_schema(mut self, schema: SchemaNode) -> Self {
        self.schemas.push(schema);
        self
    }

    pub fn with_properties(mut self, schema_title: &str, props: Vec<PropertyNode>) -> Self {
        self.properties.insert(schema_title.to_string(), props);
        self
    }

    pub fn with_composition_tree(mut self, schema_title: &str, tree: CompositionTree) -> Self {
        self.trees.insert(schema_title.to_string(), tree);
        self
    }

    pub fn with_composite_range(mut self, schema_title: &str, range: CompositeRange) -> Self {
        self.composite_ranges
            .insert(schema_title.to_string(), range);
        self
    }

    pub fn with_consumed_fields(
        mut self,
        schema_title: &str,
        fields: Vec<(PropertyNode, String)>,
    ) -> Self {
        self.consumed_fields
            .insert(schema_title.to_string(), fields);
        self
    }

    /// Register a $ref target: when `get_property_ref_target(property_name, schema_title)`
    /// or `get_array_item_schema(property_name, schema_title)` is called, return `target`.
    pub fn with_ref_target(
        mut self,
        property_name: &str,
        schema_title: &str,
        target: SchemaNode,
    ) -> Self {
        self.ref_targets.insert(
            (property_name.to_string(), schema_title.to_string()),
            target,
        );
        self
    }

    pub fn with_extending_schema(mut self, parent_title: &str, schema: SchemaNode) -> Self {
        self.extends_map
            .entry(parent_title.to_string())
            .or_default()
            .push(schema);
        self
    }

    /// Register allOf targets for a schema. When `get_allof_targets(schema_title)`
    /// is called, these parent titles are returned.
    pub fn with_allof_targets(mut self, schema_title: &str, targets: Vec<String>) -> Self {
        self.allof_targets.insert(schema_title.to_string(), targets);
        self
    }

    pub fn with_parent_candidate(mut self, pc: ParentCandidate) -> Self {
        self.parent_candidates.push(pc);
        self
    }

    pub fn with_enum_values(mut self, schema_title: &str, values: Vec<EnumValue>) -> Self {
        self.enum_values.insert(schema_title.to_string(), values);
        self
    }

    pub fn build(self) -> MockEngine {
        let engine = MockEngine::new();
        {
            let mut schemas = engine.schemas.lock().unwrap();
            for s in &self.schemas {
                schemas.insert(s.title.clone(), s.clone());
            }
        }
        {
            let mut properties = engine.properties.lock().unwrap();
            for (k, v) in &self.properties {
                properties.insert(k.clone(), v.clone());
            }
        }
        {
            let mut trees = engine.trees.lock().unwrap();
            for (k, v) in &self.trees {
                trees.insert(k.clone(), v.clone());
            }
            // Auto-generate composition trees for schemas that don't have one.
            for s in &self.schemas {
                if trees.contains_key(&s.title) {
                    continue;
                }
                let mut visited = std::collections::HashSet::new();
                let root = build_mock_node(
                    s,
                    &s.pg_table_name,
                    None,
                    false,
                    &self.properties,
                    &self.ref_targets,
                    &self.composite_ranges,
                    &self.consumed_fields,
                    &mut visited,
                    0,
                );
                trees.insert(s.title.clone(), CompositionTree { root });
            }
        }
        {
            let mut composite_ranges = engine.composite_ranges.lock().unwrap();
            for (k, v) in self.composite_ranges {
                composite_ranges.insert(k, v);
            }
        }
        {
            let mut consumed_fields = engine.consumed_fields.lock().unwrap();
            for (k, v) in self.consumed_fields {
                consumed_fields.insert(k, v);
            }
        }
        {
            let mut ref_targets = engine.ref_targets.lock().unwrap();
            for (k, v) in self.ref_targets {
                ref_targets.insert(k, v);
            }
        }
        {
            let mut pcs = engine.parent_candidates.lock().unwrap();
            for pc in &self.parent_candidates {
                pcs.push(pc.clone());
            }
        }
        {
            let mut extends_map = engine.extends_map.lock().unwrap();
            for (k, v) in &self.extends_map {
                extends_map.insert(k.clone(), v.clone());
            }
        }
        {
            let mut allof_targets = engine.allof_targets.lock().unwrap();
            for (k, v) in &self.allof_targets {
                allof_targets.insert(k.clone(), v.clone());
            }
        }
        {
            let mut enum_values = engine.enum_values.lock().unwrap();
            for (k, v) in self.enum_values {
                enum_values.insert(k, v);
            }
        }
        engine
    }
}

#[allow(clippy::too_many_arguments)]
fn build_mock_node(
    schema: &SchemaNode,
    field_name: &str,
    fk: Option<FkDirection>,
    is_collection: bool,
    all_properties: &HashMap<String, Vec<PropertyNode>>,
    ref_targets: &HashMap<(String, String), SchemaNode>,
    composite_ranges: &HashMap<String, CompositeRange>,
    consumed_fields_map: &HashMap<String, Vec<(PropertyNode, String)>>,
    visited: &mut std::collections::HashSet<String>,
    depth: usize,
) -> CompositionNode {
    visited.insert(schema.title.clone());
    let default_schema = schema
        .domain
        .clone()
        .unwrap_or_else(|| "public".to_string());
    let props = all_properties
        .get(&schema.title)
        .cloned()
        .unwrap_or_default();

    let cr = composite_ranges.get(&schema.title).cloned();
    let cf: Vec<String> = consumed_fields_map
        .get(&schema.title)
        .map(|v| v.iter().map(|(p, _)| p.name.clone()).collect())
        .unwrap_or_default();
    let cf_set: std::collections::HashSet<&str> = cf.iter().map(|s| s.as_str()).collect();

    let mut columns = Vec::new();
    let mut children = Vec::new();

    for p in &props {
        if cf_set.contains(p.name.as_str()) {
            continue;
        }
        let classification = p.effective_kind();

        // ValueObject → recurse into child node
        if classification == Some(codegraph_type_contracts::RefClassificationKind::ValueObject)
            && depth < 10
        {
            let target = ref_targets.get(&(p.name.clone(), schema.title.clone()));
            if let Some(ts) = target {
                if !visited.contains(&ts.title) {
                    let child_fk = Some(FkDirection::OnChild {
                        column: format!(
                            "{}_id",
                            codegraph_naming::truncate_pg_identifier(&schema.pg_table_name)
                        ),
                    });
                    let child_node = build_mock_node(
                        ts,
                        &p.pg_column_name,
                        child_fk,
                        p.is_array,
                        all_properties,
                        ref_targets,
                        composite_ranges,
                        consumed_fields_map,
                        visited,
                        depth + 1,
                    );
                    children.push(child_node);
                }
            }
            continue;
        }

        let fk_target = match classification {
            Some(codegraph_type_contracts::RefClassificationKind::CodelistReference) if !p.is_array => {
                p.ref_target.as_ref().map(|rt| FkTarget {
                    schema: mock_ref_schema(rt),
                    table: mock_ref_table(rt),
                    column: "code".to_string(),
                    on_delete: "RESTRICT".to_string(),
                })
            }
            Some(codegraph_type_contracts::RefClassificationKind::EntityReference) if !p.is_array => {
                p.ref_target.as_ref().map(|rt| FkTarget {
                    schema: mock_ref_schema(rt),
                    table: mock_ref_table(rt),
                    column: "id".to_string(),
                    on_delete: "SET NULL".to_string(),
                })
            }
            _ => None,
        };
        let is_codelist_fk = matches!(
            classification,
            Some(codegraph_type_contracts::RefClassificationKind::CodelistReference)
        );

        columns.push(ColumnInfo {
            name: p.pg_column_name.clone(),
            description: p.description.clone(),
            rust_type: p.rust_field_type.clone(),
            postgres_type: p.pg_column_type.clone(),
            is_optional: !p.is_required,
            is_codelist_fk,
            composite_columns: vec![],
            is_array: p.is_array,
            classification,
            fk_target,
            check_values: vec![],
        });
    }

    CompositionNode {
        field_name: field_name.to_string(),
        schema_title: schema.title.clone(),
        table_schema: default_schema,
        table_name: schema.pg_table_name.clone(),
        fk,
        is_collection,
        columns,
        jsonb_columns: vec![],
        children,
        composite_range: cr,
        consumed_fields: cf,
    }
}

/// Extract schema name from a $ref path for mock FK resolution.
fn mock_ref_schema(ref_target: &str) -> String {
    let segments: Vec<&str> = ref_target
        .split('/')
        .filter(|s| !s.is_empty() && *s != "..")
        .collect();
    for (i, seg) in segments.iter().enumerate() {
        if *seg == "json" && i > 0 {
            return segments[i - 1].to_string();
        }
    }
    segments
        .first()
        .filter(|s| **s != "codelist")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "common".to_string())
}

/// Extract table name from a $ref path for mock FK resolution.
fn mock_ref_table(ref_target: &str) -> String {
    let filename = ref_target.rsplit('/').next().unwrap_or("");
    let stem = filename
        .strip_suffix(".json#")
        .or_else(|| filename.strip_suffix(".json"))
        .unwrap_or(filename);
    codegraph_naming::to_snake_case(&codegraph_naming::strip_suffix(stem, "Type"))
}

#[async_trait]
impl GraphIngestor for MockEngine {
    async fn ingest_schema(&self, node: &SchemaNode) -> Result<String, GraphError> {
        let id = node.schema_id.clone();
        self.schemas
            .lock()
            .unwrap()
            .insert(node.title.clone(), node.clone());
        Ok(id)
    }

    async fn ingest_property(
        &self,
        schema_title: &str,
        _schema_id: &str,
        prop: &PropertyNode,
    ) -> Result<(), GraphError> {
        self.properties
            .lock()
            .unwrap()
            .entry(schema_title.to_string())
            .or_default()
            .push(prop.clone());
        Ok(())
    }

    async fn ingest_codelist(&self, codelist: &CodeList) -> Result<(), GraphError> {
        self.codelists
            .lock()
            .unwrap()
            .insert(codelist.name.clone(), codelist.clone());
        Ok(())
    }

    async fn ingest_enum_value(
        &self,
        codelist_name: &str,
        value: &EnumValue,
    ) -> Result<(), GraphError> {
        self.enum_values
            .lock()
            .unwrap()
            .entry(codelist_name.to_string())
            .or_default()
            .push(value.clone());
        Ok(())
    }

    async fn ingest_composite_column(&self, _col: &CompositeColumn) -> Result<(), GraphError> {
        Ok(())
    }

    async fn ingest_composite_range(&self, _range: &CompositeRange) -> Result<(), GraphError> {
        Ok(())
    }

    async fn ingest_extension(&self, _name: &str) -> Result<(), GraphError> {
        Ok(())
    }

    async fn ingest_edge(
        &self,
        _from_id: &str,
        _to_id: &str,
        _edge_type: EdgeType,
        _props: Option<&EdgeProperties>,
    ) -> Result<(), GraphError> {
        Ok(())
    }

    async fn ingest_view_container(&self, node: &ViewContainerNode) -> Result<String, GraphError> {
        let id = format!("vc:{}", node.name);
        self.view_containers
            .lock()
            .unwrap()
            .insert(node.name.clone(), node.clone());
        Ok(id)
    }

    async fn ingest_view_component(&self, node: &ViewComponentNode) -> Result<String, GraphError> {
        let id = format!("comp:{}", node.name);
        self.view_components
            .lock()
            .unwrap()
            .insert(node.name.clone(), node.clone());
        Ok(id)
    }

    async fn ingest_event(&self, node: &EventNode) -> Result<String, GraphError> {
        let id = format!("evt:{}", node.name);
        self.events
            .lock()
            .unwrap()
            .insert(node.name.clone(), node.clone());
        Ok(id)
    }

    async fn ingest_action_node(&self, node: &ActionNode) -> Result<String, GraphError> {
        let id = format!("action:{}", node.name);
        self.action_nodes
            .lock()
            .unwrap()
            .insert(node.name.clone(), node.clone());
        Ok(id)
    }

    async fn ingest_parameter_definition(
        &self,
        node: &ParameterDefinitionNode,
    ) -> Result<String, GraphError> {
        let id = format!("param:{}", node.name);
        self.parameter_definitions
            .lock()
            .unwrap()
            .insert(node.name.clone(), node.clone());
        Ok(id)
    }

    async fn ingest_data_binding(&self, _node: &DataBindingNode) -> Result<String, GraphError> {
        let id = format!("db:{}", Uuid::new_v4());
        Ok(id)
    }

    async fn finalize(&self) -> Result<IngestStats, GraphError> {
        let schemas = self.schemas.lock().unwrap();
        let properties = self.properties.lock().unwrap();
        let codelists = self.codelists.lock().unwrap();
        let enum_values = self.enum_values.lock().unwrap();
        let vc = self.view_containers.lock().unwrap();
        let vcomp = self.view_components.lock().unwrap();
        let evt = self.events.lock().unwrap();
        let act = self.action_nodes.lock().unwrap();
        let param = self.parameter_definitions.lock().unwrap();

        let ifml_count =
            vc.len() + vcomp.len() + evt.len() + act.len() + param.len();

        Ok(IngestStats {
            schema_count: schemas.len(),
            property_count: properties.values().map(|v| v.len()).sum(),
            codelist_count: codelists.len(),
            enum_value_count: enum_values.values().map(|v| v.len()).sum(),
            ifml_node_count: ifml_count,
            duration: self.start_time.elapsed(),
            ..Default::default()
        })
    }

    async fn update_entity_flag(&self, title: &str, is_entity: bool) -> Result<(), GraphError> {
        let mut schemas = self.schemas.lock().unwrap();
        if let Some(schema) = schemas.get_mut(title) {
            schema.is_entity = is_entity;
        }
        Ok(())
    }

    async fn update_property_classification(
        &self,
        _schema_title: &str,
        _property_name: &str,
        _kind: &str,
    ) -> Result<(), GraphError> {
        // Mock: no-op for property classification updates
        Ok(())
    }
}

#[async_trait]
impl GraphQuerier for MockEngine {
    async fn get_schema(&self, title: &str) -> Result<Option<SchemaNode>, GraphError> {
        Ok(self.schemas.lock().unwrap().get(title).cloned())
    }

    async fn get_schema_by_id(&self, schema_id: &str) -> Result<Option<SchemaNode>, GraphError> {
        let schemas = self.schemas.lock().unwrap();
        Ok(schemas.values().find(|s| s.schema_id == schema_id).cloned())
    }

    async fn get_schema_in_domain(
        &self,
        title: &str,
        domain: &str,
    ) -> Result<Option<SchemaNode>, GraphError> {
        let schemas = self.schemas.lock().unwrap();
        Ok(schemas
            .values()
            .find(|s| s.title == title && s.domain.as_deref() == Some(domain))
            .cloned())
    }

    async fn list_schemas(&self, domain: Option<&str>) -> Result<Vec<SchemaNode>, GraphError> {
        let schemas = self.schemas.lock().unwrap();
        let result: Vec<_> = schemas
            .values()
            .filter(|s| match domain {
                Some(d) => s.domain.as_deref() == Some(d),
                None => true,
            })
            .cloned()
            .collect();
        Ok(result)
    }

    async fn get_properties(&self, schema_title: &str) -> Result<Vec<PropertyNode>, GraphError> {
        Ok(self
            .properties
            .lock()
            .unwrap()
            .get(schema_title)
            .cloned()
            .unwrap_or_default())
    }

    async fn get_child_schemas(&self, schema_title: &str) -> Result<Vec<SchemaNode>, GraphError> {
        let schemas = self.schemas.lock().unwrap();
        Ok(schemas
            .values()
            .filter(|s| s.parent_schema.as_deref() == Some(schema_title))
            .cloned()
            .collect())
    }

    async fn get_classification_data(&self) -> Result<Vec<SchemaClassificationData>, GraphError> {
        Ok(vec![])
    }

    async fn get_entity_names(&self) -> Result<Vec<String>, GraphError> {
        let schemas = self.schemas.lock().unwrap();
        Ok(schemas
            .values()
            .filter(|s| s.is_entity)
            .map(|s| s.title.clone())
            .collect())
    }

    async fn get_entity_schema_map(&self) -> Result<HashMap<String, String>, GraphError> {
        let schemas = self.schemas.lock().unwrap();
        Ok(schemas
            .values()
            .filter(|s| s.is_entity)
            .map(|s| (s.title.clone(), s.rel_path.clone()))
            .collect())
    }

    async fn get_value_object_schemas(&self) -> Result<Vec<SchemaNode>, GraphError> {
        let schemas = self.schemas.lock().unwrap();
        Ok(schemas
            .values()
            .filter(|s| !s.is_entity && !s.is_codelist && s.schema_type == "object")
            .cloned()
            .collect())
    }

    async fn get_parent_candidates(&self) -> Result<Vec<ParentCandidate>, GraphError> {
        Ok(self.parent_candidates.lock().unwrap().clone())
    }

    async fn get_codelist(&self, name: &str) -> Result<Option<CodeList>, GraphError> {
        Ok(self.codelists.lock().unwrap().get(name).cloned())
    }

    async fn list_codelists(&self) -> Result<Vec<CodeList>, GraphError> {
        Ok(self.codelists.lock().unwrap().values().cloned().collect())
    }

    async fn get_enum_values(&self, codelist_name: &str) -> Result<Vec<EnumValue>, GraphError> {
        Ok(self
            .enum_values
            .lock()
            .unwrap()
            .get(codelist_name)
            .cloned()
            .unwrap_or_default())
    }

    async fn get_composite_columns(
        &self,
        _property_name: &str,
        _schema_title: &str,
    ) -> Result<Vec<CompositeColumn>, GraphError> {
        Ok(vec![])
    }

    async fn get_structured_sub_fields(
        &self,
        _schema_title: &str,
    ) -> Result<Vec<StructuredSubField>, GraphError> {
        Ok(vec![])
    }

    async fn get_composite_range(
        &self,
        schema_title: &str,
    ) -> Result<Option<CompositeRange>, GraphError> {
        Ok(self
            .composite_ranges
            .lock()
            .unwrap()
            .get(schema_title)
            .cloned())
    }

    async fn get_consumed_fields(
        &self,
        schema_title: &str,
    ) -> Result<Vec<(PropertyNode, String)>, GraphError> {
        Ok(self
            .consumed_fields
            .lock()
            .unwrap()
            .get(schema_title)
            .cloned()
            .unwrap_or_default())
    }

    async fn get_codelist_for_property(
        &self,
        _property_name: &str,
        _schema_title: &str,
    ) -> Result<Option<(CodeList, String)>, GraphError> {
        Ok(None)
    }

    async fn get_required_extensions(
        &self,
        _schema_title: &str,
    ) -> Result<Vec<Extension>, GraphError> {
        Ok(vec![])
    }

    async fn get_composition_tree(
        &self,
        schema_title: &str,
    ) -> Result<CompositionTree, GraphError> {
        self.trees
            .lock()
            .unwrap()
            .get(schema_title)
            .cloned()
            .ok_or_else(|| GraphError::NotFound(format!("composition tree for {schema_title}")))
    }

    async fn get_allof_targets(&self, schema_title: &str) -> Result<Vec<String>, GraphError> {
        Ok(self
            .allof_targets
            .lock()
            .unwrap()
            .get(schema_title)
            .cloned()
            .unwrap_or_default())
    }

    async fn get_schemas_that_extend(
        &self,
        parent_title: &str,
    ) -> Result<Vec<SchemaNode>, GraphError> {
        Ok(self
            .extends_map
            .lock()
            .unwrap()
            .get(parent_title)
            .cloned()
            .unwrap_or_default())
    }

    async fn list_all_properties(&self) -> Result<HashMap<String, Vec<PropertyNode>>, GraphError> {
        Ok(self.properties.lock().unwrap().clone())
    }

    async fn get_referencing_schemas(
        &self,
        _schema_title: &str,
    ) -> Result<Vec<String>, GraphError> {
        Ok(vec![])
    }

    async fn get_referenced_schemas(&self, schema_title: &str) -> Result<Vec<SchemaNode>, GraphError> {
        let ref_targets = self.ref_targets.lock().unwrap();
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for ((_prop_name, st), schema) in ref_targets.iter() {
            if st == schema_title && seen.insert(schema.schema_id.clone()) {
                result.push(schema.clone());
            }
        }
        Ok(result)
    }

    async fn get_property_ref_target(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Option<SchemaNode>, GraphError> {
        let ref_targets = self.ref_targets.lock().unwrap();
        Ok(ref_targets
            .get(&(property_name.to_string(), schema_title.to_string()))
            .cloned())
    }

    async fn get_property_ref_target_by_id(
        &self,
        property_name: &str,
        schema_id: &str,
    ) -> Result<Option<SchemaNode>, GraphError> {
        let title = {
            let schemas = self.schemas.lock().unwrap();
            schemas
                .values()
                .find(|s| s.schema_id == schema_id)
                .map(|s| s.title.clone())
        };
        let Some(title) = title else {
            return Ok(None);
        };
        let ref_targets = self.ref_targets.lock().unwrap();
        Ok(ref_targets
            .get(&(property_name.to_string(), title))
            .cloned())
    }

    async fn get_properties_by_schema_id(
        &self,
        schema_id: &str,
    ) -> Result<Vec<PropertyNode>, GraphError> {
        let title = {
            let schemas = self.schemas.lock().unwrap();
            schemas
                .values()
                .find(|s| s.schema_id == schema_id)
                .map(|s| s.title.clone())
        };
        let Some(title) = title else {
            return Ok(Vec::new());
        };
        Ok(self
            .properties
            .lock()
            .unwrap()
            .get(&title)
            .cloned()
            .unwrap_or_default())
    }

    async fn get_array_item_schema(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Option<SchemaNode>, GraphError> {
        let ref_targets = self.ref_targets.lock().unwrap();
        Ok(ref_targets
            .get(&(property_name.to_string(), schema_title.to_string()))
            .cloned())
    }

    async fn get_ifml_view_containers(&self) -> Result<Vec<ViewContainerNode>, GraphError> {
        let map = self.view_containers.lock().unwrap();
        Ok(map.values().cloned().collect())
    }

    async fn get_ifml_view_components(
        &self,
        _container_name: &str,
    ) -> Result<Vec<ViewComponentNode>, GraphError> {
        let map = self.view_components.lock().unwrap();
        Ok(map.values().cloned().collect())
    }

    async fn get_generation_order(&self) -> Result<Vec<String>, GraphError> {
        Ok(self.get_entity_names().await?)
    }
}
