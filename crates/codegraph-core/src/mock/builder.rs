use super::MockEngine;
use crate::types::*;
use std::collections::HashMap;

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
    /// (schema_title, lexicon_nsid) pairs for get_lexicon_by_schema lookups.
    schema_lexicons: HashMap<String, String>,
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

    pub fn with_lexicon_mapping(mut self, schema_title: &str, lexicon_nsid: &str) -> Self {
        self.schema_lexicons
            .insert(schema_title.to_string(), lexicon_nsid.to_string());
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
        {
            let mut schema_lexicons = engine.schema_lexicons.lock().unwrap();
            for (k, v) in self.schema_lexicons {
                schema_lexicons.insert(k, v);
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
            if let Some(ts) = target
                && !visited.contains(&ts.title)
            {
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
            continue;
        }

        let fk_target = match classification {
            Some(codegraph_type_contracts::RefClassificationKind::CodelistReference)
                if !p.is_array =>
            {
                p.ref_target.as_ref().map(|rt| FkTarget {
                    schema: mock_ref_schema(rt),
                    table: mock_ref_table(rt),
                    column: "code".to_string(),
                    on_delete: "RESTRICT".to_string(),
                })
            }
            Some(codegraph_type_contracts::RefClassificationKind::EntityReference)
                if !p.is_array =>
            {
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

/// Strip a leading API-metamodel node prefix (`ar:` / `ao:` / `pl:` / `pm:` /
/// `ed:`) from an id, if present.
pub(super) fn strip_api_prefix(id: &str) -> &str {
    for prefix in ["ar:", "ao:", "pl:", "pm:", "ed:"] {
        if let Some(rest) = id.strip_prefix(prefix) {
            return rest;
        }
    }
    id
}
