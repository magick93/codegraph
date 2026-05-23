use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;

use crate::error::GraphError;
use crate::traits::GraphQuerier;
use crate::types::{
    CodeList, CompositeColumn, CompositeRange, CompositionTree, EnumValue, Extension,
    ParentCandidate, PropertyNode, SchemaClassificationData, SchemaNode, StructuredSubField,
};

/// Cached codelist-for-property value: `Option<(CodeList, render_as)>`.
type CodelistPropertyVal = Option<(CodeList, String)>;

/// A caching wrapper around a `&dyn GraphQuerier`.
///
/// Caches the results of frequently-called query methods (get_schema,
/// get_properties, etc.) so that multiple generators querying the same
/// entity do not re-execute identical graph queries.
///
/// The cache is populated lazily on first access and lives for the
/// duration of the wrapper (typically one full generation run).
pub struct CachingQuerier<'a> {
    inner: &'a dyn GraphQuerier,
    schema_cache: RwLock<HashMap<String, Option<SchemaNode>>>,
    properties_cache: RwLock<HashMap<String, Vec<PropertyNode>>>,
    child_schemas_cache: RwLock<HashMap<String, Vec<SchemaNode>>>,
    composite_columns_cache: RwLock<HashMap<(String, String), Vec<CompositeColumn>>>,
    structured_sub_fields_cache: RwLock<HashMap<String, Vec<StructuredSubField>>>,
    composite_range_cache: RwLock<HashMap<String, Option<CompositeRange>>>,
    consumed_fields_cache: RwLock<HashMap<String, Vec<(PropertyNode, String)>>>,
    codelist_for_property_cache: RwLock<HashMap<(String, String), CodelistPropertyVal>>,
    codelist_cache: RwLock<HashMap<String, Option<CodeList>>>,
    enum_values_cache: RwLock<HashMap<String, Vec<EnumValue>>>,
    extensions_cache: RwLock<HashMap<String, Vec<Extension>>>,
    composition_tree_cache: RwLock<HashMap<String, CompositionTree>>,
    allof_targets_cache: RwLock<HashMap<String, Vec<String>>>,
    referencing_cache: RwLock<HashMap<String, Vec<String>>>,
    referenced_cache: RwLock<HashMap<String, Vec<String>>>,
    property_ref_target_cache: RwLock<HashMap<(String, String), Option<SchemaNode>>>,
    array_item_schema_cache: RwLock<HashMap<(String, String), Option<SchemaNode>>>,
}

impl<'a> CachingQuerier<'a> {
    pub fn new(inner: &'a dyn GraphQuerier) -> Self {
        Self {
            inner,
            schema_cache: RwLock::new(HashMap::new()),
            properties_cache: RwLock::new(HashMap::new()),
            child_schemas_cache: RwLock::new(HashMap::new()),
            composite_columns_cache: RwLock::new(HashMap::new()),
            structured_sub_fields_cache: RwLock::new(HashMap::new()),
            composite_range_cache: RwLock::new(HashMap::new()),
            consumed_fields_cache: RwLock::new(HashMap::new()),
            codelist_for_property_cache: RwLock::new(HashMap::new()),
            codelist_cache: RwLock::new(HashMap::new()),
            enum_values_cache: RwLock::new(HashMap::new()),
            extensions_cache: RwLock::new(HashMap::new()),
            composition_tree_cache: RwLock::new(HashMap::new()),
            allof_targets_cache: RwLock::new(HashMap::new()),
            referencing_cache: RwLock::new(HashMap::new()),
            referenced_cache: RwLock::new(HashMap::new()),
            property_ref_target_cache: RwLock::new(HashMap::new()),
            array_item_schema_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Pre-warm the cache by bulk-loading all schemas, properties, and reference
    /// edges in a small number of graph queries. This avoids hundreds of
    /// individual queries during generation.
    pub async fn warm(&self) -> Result<(), GraphError> {
        // 1. Bulk-load all schemas → schema_cache
        let all_schemas = self.inner.list_schemas(None).await?;
        {
            let mut cache = self.schema_cache.write().unwrap();
            for schema in &all_schemas {
                cache.insert(schema.title.clone(), Some(schema.clone()));
            }
        }

        // 2. Bulk-load all properties → properties_cache
        let all_props = self.inner.list_all_properties().await?;
        {
            let mut cache = self.properties_cache.write().unwrap();
            // Insert empty vecs for schemas that have no properties
            for schema in &all_schemas {
                cache.entry(schema.title.clone()).or_default();
            }
            for (title, props) in all_props {
                cache.insert(title, props);
            }
        }

        // 3. Bulk-load all schema references → referenced_cache
        let all_refs = self.inner.list_all_schema_references().await?;
        {
            let mut cache = self.referenced_cache.write().unwrap();
            // Build per-source map
            let mut ref_map: HashMap<String, Vec<String>> = HashMap::new();
            for (src, tgt) in &all_refs {
                ref_map.entry(src.clone()).or_default().push(tgt.clone());
            }
            for (src, targets) in ref_map {
                cache.insert(src, targets);
            }
            // Also pre-populate empty entries for schemas with no refs
            for schema in &all_schemas {
                cache.entry(schema.title.clone()).or_default();
            }
        }

        // 4. Build child_schemas_cache from all_schemas parent_schema field
        {
            let mut cache = self.child_schemas_cache.write().unwrap();
            let mut children_map: HashMap<String, Vec<SchemaNode>> = HashMap::new();
            for schema in &all_schemas {
                if let Some(ref parent) = schema.parent_schema {
                    children_map
                        .entry(parent.clone())
                        .or_default()
                        .push(schema.clone());
                }
            }
            for (parent, children) in children_map {
                cache.insert(parent, children);
            }
        }

        // 5. Build referencing_cache (reverse of referenced)
        {
            let mut cache = self.referencing_cache.write().unwrap();
            let mut rev_map: HashMap<String, Vec<String>> = HashMap::new();
            for (src, tgt) in &all_refs {
                rev_map.entry(tgt.clone()).or_default().push(src.clone());
            }
            for (tgt, sources) in rev_map {
                cache.insert(tgt, sources);
            }
        }

        Ok(())
    }
}

/// Helper macro to implement a cached method with a single string key.
macro_rules! cached_single {
    ($self:ident, $cache:ident, $key:expr, $inner_call:expr) => {{
        {
            let cache = $self.$cache.read().unwrap();
            if let Some(val) = cache.get($key) {
                return Ok(val.clone());
            }
        }
        let result = $inner_call.await?;
        {
            let mut cache = $self.$cache.write().unwrap();
            cache.insert($key.to_string(), result.clone());
        }
        Ok(result)
    }};
}

/// Helper macro for cached methods with a two-string key.
macro_rules! cached_pair {
    ($self:ident, $cache:ident, $k1:expr, $k2:expr, $inner_call:expr) => {{
        let key = ($k1.to_string(), $k2.to_string());
        {
            let cache = $self.$cache.read().unwrap();
            if let Some(val) = cache.get(&key) {
                return Ok(val.clone());
            }
        }
        let result = $inner_call.await?;
        {
            let mut cache = $self.$cache.write().unwrap();
            cache.insert(key, result.clone());
        }
        Ok(result)
    }};
}

#[async_trait]
impl GraphQuerier for CachingQuerier<'_> {
    async fn get_schema(&self, title: &str) -> Result<Option<SchemaNode>, GraphError> {
        cached_single!(self, schema_cache, title, self.inner.get_schema(title))
    }

    async fn list_schemas(&self, domain: Option<&str>) -> Result<Vec<SchemaNode>, GraphError> {
        self.inner.list_schemas(domain).await
    }

    async fn get_properties(&self, schema_title: &str) -> Result<Vec<PropertyNode>, GraphError> {
        cached_single!(
            self,
            properties_cache,
            schema_title,
            self.inner.get_properties(schema_title)
        )
    }

    async fn get_child_schemas(&self, schema_title: &str) -> Result<Vec<SchemaNode>, GraphError> {
        cached_single!(
            self,
            child_schemas_cache,
            schema_title,
            self.inner.get_child_schemas(schema_title)
        )
    }

    async fn get_classification_data(&self) -> Result<Vec<SchemaClassificationData>, GraphError> {
        self.inner.get_classification_data().await
    }

    async fn get_entity_names(&self) -> Result<Vec<String>, GraphError> {
        self.inner.get_entity_names().await
    }

    async fn get_entity_schema_map(&self) -> Result<HashMap<String, String>, GraphError> {
        self.inner.get_entity_schema_map().await
    }

    async fn get_value_object_schemas(&self) -> Result<Vec<SchemaNode>, GraphError> {
        self.inner.get_value_object_schemas().await
    }

    async fn get_parent_candidates(&self) -> Result<Vec<ParentCandidate>, GraphError> {
        self.inner.get_parent_candidates().await
    }

    async fn get_codelist(&self, name: &str) -> Result<Option<CodeList>, GraphError> {
        cached_single!(self, codelist_cache, name, self.inner.get_codelist(name))
    }

    async fn list_codelists(&self) -> Result<Vec<CodeList>, GraphError> {
        self.inner.list_codelists().await
    }

    async fn get_enum_values(&self, codelist_name: &str) -> Result<Vec<EnumValue>, GraphError> {
        cached_single!(
            self,
            enum_values_cache,
            codelist_name,
            self.inner.get_enum_values(codelist_name)
        )
    }

    async fn get_composite_columns(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Vec<CompositeColumn>, GraphError> {
        cached_pair!(
            self,
            composite_columns_cache,
            property_name,
            schema_title,
            self.inner
                .get_composite_columns(property_name, schema_title)
        )
    }

    async fn get_structured_sub_fields(
        &self,
        schema_title: &str,
    ) -> Result<Vec<StructuredSubField>, GraphError> {
        cached_single!(
            self,
            structured_sub_fields_cache,
            schema_title,
            self.inner.get_structured_sub_fields(schema_title)
        )
    }

    async fn get_composite_range(
        &self,
        schema_title: &str,
    ) -> Result<Option<CompositeRange>, GraphError> {
        cached_single!(
            self,
            composite_range_cache,
            schema_title,
            self.inner.get_composite_range(schema_title)
        )
    }

    async fn get_consumed_fields(
        &self,
        schema_title: &str,
    ) -> Result<Vec<(PropertyNode, String)>, GraphError> {
        cached_single!(
            self,
            consumed_fields_cache,
            schema_title,
            self.inner.get_consumed_fields(schema_title)
        )
    }

    async fn get_codelist_for_property(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Option<(CodeList, String)>, GraphError> {
        cached_pair!(
            self,
            codelist_for_property_cache,
            property_name,
            schema_title,
            self.inner
                .get_codelist_for_property(property_name, schema_title)
        )
    }

    async fn get_required_extensions(
        &self,
        schema_title: &str,
    ) -> Result<Vec<Extension>, GraphError> {
        cached_single!(
            self,
            extensions_cache,
            schema_title,
            self.inner.get_required_extensions(schema_title)
        )
    }

    async fn get_composition_tree(
        &self,
        schema_title: &str,
    ) -> Result<CompositionTree, GraphError> {
        cached_single!(
            self,
            composition_tree_cache,
            schema_title,
            self.inner.get_composition_tree(schema_title)
        )
    }

    async fn get_allof_targets(&self, schema_title: &str) -> Result<Vec<String>, GraphError> {
        cached_single!(
            self,
            allof_targets_cache,
            schema_title,
            self.inner.get_allof_targets(schema_title)
        )
    }

    async fn get_referencing_schemas(&self, schema_title: &str) -> Result<Vec<String>, GraphError> {
        cached_single!(
            self,
            referencing_cache,
            schema_title,
            self.inner.get_referencing_schemas(schema_title)
        )
    }

    async fn get_referenced_schemas(&self, schema_title: &str) -> Result<Vec<String>, GraphError> {
        cached_single!(
            self,
            referenced_cache,
            schema_title,
            self.inner.get_referenced_schemas(schema_title)
        )
    }

    async fn get_property_ref_target(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Option<SchemaNode>, GraphError> {
        cached_pair!(
            self,
            property_ref_target_cache,
            property_name,
            schema_title,
            self.inner
                .get_property_ref_target(property_name, schema_title)
        )
    }

    async fn get_array_item_schema(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Option<SchemaNode>, GraphError> {
        cached_pair!(
            self,
            array_item_schema_cache,
            property_name,
            schema_title,
            self.inner
                .get_array_item_schema(property_name, schema_title)
        )
    }

    async fn get_generation_order(&self) -> Result<Vec<String>, GraphError> {
        self.inner.get_generation_order().await
    }

    async fn list_all_schema_references(&self) -> Result<Vec<(String, String)>, GraphError> {
        self.inner.list_all_schema_references().await
    }

    async fn list_all_properties(&self) -> Result<HashMap<String, Vec<PropertyNode>>, GraphError> {
        self.inner.list_all_properties().await
    }
}
