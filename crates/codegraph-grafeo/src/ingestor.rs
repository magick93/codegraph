use async_trait::async_trait;
use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::strip_ifml_prefix;
use codegraph_core::types::{
    ActionNode, ApiOperationNode, ApiResourceNode, CodeList, CollectionNode, CompositeColumn,
    CompositeRange, DataBindingNode, EdgeProperties, EdgeType, EnumValue, ErrorDefinitionNode,
    EventNode, HttpEndpointNode, IngestStats, InteractionNode, LexiconNode, MembershipNode,
    NamespaceNode, ParameterDefinitionNode, PermissionNode, PipelineNode, PolicyNode, PropertyNode,
    RelationshipNode, RepositoryNode, SchemaNode, SecurityIdentityNode, TenantNode,
    ViewComponentNode, ViewContainerNode,
};

use codegraph_type_contracts::RefClassificationKind;

use crate::engine::GrafeoEngine;

/// Escape single quotes in GQL string literals.
pub(crate) fn escape_gql(s: &str) -> String {
    s.replace('\'', "\\'")
}

/// Strip a leading API-metamodel node prefix (`ar:` / `ao:` / `pl:` / `pm:` /
/// `ed:`) from an id, returning the id unchanged when no prefix is present.
/// Interaction/HttpEndpoint ids (`ia:` / `he:`) are deliberately not stripped —
/// those nodes store the full prefixed id as their `name`.
fn strip_api_prefix(id: &str) -> &str {
    for prefix in ["ar:", "ao:", "pl:", "pm:", "ed:"] {
        if let Some(rest) = id.strip_prefix(prefix) {
            return rest;
        }
    }
    id
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

fn serde_enum_str<T: serde::Serialize>(v: &T) -> String {
    let json = serde_json::to_string(v).unwrap_or_default();
    json.trim_matches('"').to_string()
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
    if let Some(v) = &p.target_param_binding {
        fields.push(format!("target_param_binding: '{}'", escape_gql(v)));
    }
    if let Some(v) = &p.source_param {
        fields.push(format!("source_param: '{}'", escape_gql(v)));
    }
    if let Some(v) = &p.event_type {
        fields.push(format!("event_type: '{}'", escape_gql(v)));
    }
    if let Some(v) = &p.outcome {
        fields.push(format!("outcome: '{}'", escape_gql(v)));
    }
    if let Some(v) = &p.component_type {
        fields.push(format!("component_type: '{}'", escape_gql(v)));
    }
    if let Some(v) = &p.direction {
        fields.push(format!("direction: '{}'", escape_gql(v)));
    }
    if let Some(v) = &p.expression {
        fields.push(format!("expression: '{}'", escape_gql(v)));
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
                has_any_of: {has_any_of}, has_definitions: {has_definitions}, \
                custom_annotations: '{custom_annotations}'\
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
            custom_annotations = escape_gql(
                &serde_json::to_string(&node.custom_annotations)
                    .unwrap_or_else(|_| "{}".to_string()),
            ),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_schema failed: {e}")))?;
        Ok(node.schema_id.clone())
    }

    async fn ingest_property(
        &self,
        schema_title: &str,
        schema_id: &str,
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
                _schema_title: '{schema_title}', _schema_id: '{schema_id}'\
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
            schema_id = escape_gql(schema_id),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_property INSERT failed: {e}")))?;

        let edge_gql = format!(
            "MATCH (s:Schema {{title: '{st}'}}), (p:Property {{name: '{pn}', _schema_title: '{st2}'}}) \
             INSERT (s)-[:HasProperty]->(p)",
            st = escape_gql(schema_title),
            st2 = escape_gql(schema_title),
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
            EdgeType::ContainsViewContainer => "ContainsViewContainer",
            EdgeType::ContainsViewComponent => "ContainsViewComponent",
            EdgeType::HasEvent => "HasEvent",
            EdgeType::NavigationFlow => "NavigationFlow",
            EdgeType::DataFlow => "DataFlow",
            EdgeType::HasParameter => "HasParameter",
            EdgeType::ParameterBindingGroup => "ParameterBindingGroup",
            EdgeType::ParameterBinding => "ParameterBinding",
            EdgeType::HasDataBinding => "HasDataBinding",
            EdgeType::BindsToEntity => "BindsToEntity",
            EdgeType::BindsToProperty => "BindsToProperty",
            EdgeType::BindsToOperation => "BindsToOperation",
            EdgeType::TriggersAction => "TriggersAction",
            EdgeType::ActionEvent => "ActionEvent",
            EdgeType::HasModuleDefinition => "HasModuleDefinition",
            EdgeType::HasViewComponentPart => "HasViewComponentPart",
            EdgeType::HasConditionalExpr => "HasConditionalExpr",
            EdgeType::InNamespace => "InNamespace",
            EdgeType::ProjectsToLexicon => "ProjectsToLexicon",
            EdgeType::DefinesCollection => "DefinesCollection",
            EdgeType::LexiconReferences => "LexiconReferences",
            EdgeType::StoredInRepository => "StoredInRepository",
            EdgeType::ExposesResource => "ExposesResource",
            EdgeType::BindsToSchema => "BindsToSchema",
            EdgeType::HasOperation => "HasOperation",
            EdgeType::InputBoundTo => "InputBoundTo",
            EdgeType::OutputBoundTo => "OutputBoundTo",
            EdgeType::CanReturnError => "CanReturnError",
            EdgeType::RequiresPermission => "RequiresPermission",
            EdgeType::HasInteraction => "HasInteraction",
            EdgeType::BindsHttpEndpoint => "BindsHttpEndpoint",
            EdgeType::UsesPipeline => "UsesPipeline",
            EdgeType::HasPolicy => "HasPolicy",
            EdgeType::PolicyAppliesTo => "PolicyAppliesTo",
            EdgeType::HasRelationship => "HasRelationship",
            EdgeType::RelationshipSource => "RelationshipSource",
            EdgeType::RelationshipTarget => "RelationshipTarget",
            EdgeType::PolicyOnRelationship => "PolicyOnRelationship",
            EdgeType::TenantOwns => "TenantOwns",
            EdgeType::HasMembership => "HasMembership",
            EdgeType::MembershipInTenant => "MembershipInTenant",
            EdgeType::HasRole => "HasRole",
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
                    "MATCH (a:Property {{name: '{}', _schema_title: '{}'}}), (b:Schema {{schema_id: '{}'}})",
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
                    "MATCH (a:Property {{name: '{}', _schema_title: '{}'}}), (b:Schema {{schema_id: '{}'}})",
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

            // IFML edge types — simple MATCH on node label + name
            EdgeType::ContainsViewContainer => {
                format!(
                    "MATCH (a:ViewContainer {{name: '{}'}}), (b:ViewContainer {{name: '{}'}})",
                    escape_gql(strip_ifml_prefix(from_id)),
                    escape_gql(strip_ifml_prefix(to_id)),
                )
            }
            EdgeType::ContainsViewComponent => {
                format!(
                    "MATCH (a:ViewContainer {{name: '{}'}}), (b:ViewComponent {{name: '{}'}})",
                    escape_gql(strip_ifml_prefix(from_id)),
                    escape_gql(strip_ifml_prefix(to_id)),
                )
            }
            EdgeType::HasEvent => {
                format!(
                    "MATCH (a {{name: '{}'}}), (b:Event {{name: '{}'}})",
                    escape_gql(strip_ifml_prefix(from_id)),
                    escape_gql(strip_ifml_prefix(to_id)),
                )
            }
            EdgeType::NavigationFlow => {
                format!(
                    "MATCH (a:Event {{name: '{}'}}), (b:ViewContainer {{name: '{}'}})",
                    escape_gql(strip_ifml_prefix(from_id)),
                    escape_gql(strip_ifml_prefix(to_id)),
                )
            }
            EdgeType::DataFlow => {
                format!(
                    "MATCH (a {{name: '{}'}}), (b {{name: '{}'}})",
                    escape_gql(strip_ifml_prefix(from_id)),
                    escape_gql(strip_ifml_prefix(to_id)),
                )
            }
            EdgeType::HasParameter => {
                format!(
                    "MATCH (a {{name: '{}'}}), (b:ParameterDefinition {{name: '{}'}})",
                    escape_gql(strip_ifml_prefix(from_id)),
                    escape_gql(strip_ifml_prefix(to_id)),
                )
            }
            EdgeType::ParameterBindingGroup => {
                format!(
                    "MATCH (a {{name: '{}'}}), (b {{name: '{}'}})",
                    escape_gql(strip_ifml_prefix(from_id)),
                    escape_gql(strip_ifml_prefix(to_id)),
                )
            }
            EdgeType::ParameterBinding => {
                format!(
                    "MATCH (a {{name: '{}'}}), (b {{name: '{}'}})",
                    escape_gql(strip_ifml_prefix(from_id)),
                    escape_gql(strip_ifml_prefix(to_id)),
                )
            }
            EdgeType::HasDataBinding => {
                format!(
                    "MATCH (a:ViewComponent {{name: '{}'}}), (b:DataBinding {{name: '{}'}})",
                    escape_gql(strip_ifml_prefix(from_id)),
                    escape_gql(strip_ifml_prefix(to_id)),
                )
            }
            EdgeType::BindsToEntity => {
                format!(
                    "MATCH (a:DataBinding {{name: '{}'}}), (b:Schema {{title: '{}'}})",
                    escape_gql(strip_ifml_prefix(from_id)),
                    escape_gql(to_id),
                )
            }
            EdgeType::BindsToProperty => {
                let (prop_name, schema_title) = split_compound_id(to_id, "BindsToProperty")?;
                format!(
                    "MATCH (a:ViewComponent {{name: '{}'}}), \
                     (b:Property {{name: '{}', _schema_title: '{}'}})",
                    escape_gql(strip_ifml_prefix(from_id)),
                    escape_gql(prop_name),
                    escape_gql(schema_title),
                )
            }
            EdgeType::BindsToOperation => {
                format!(
                    "MATCH (a:ViewComponent {{name: '{}'}}), (b:ApiOperation {{name: '{}'}})",
                    escape_gql(strip_ifml_prefix(from_id)),
                    escape_gql(strip_ifml_prefix(to_id)),
                )
            }
            EdgeType::TriggersAction => {
                format!(
                    "MATCH (a:Event {{name: '{}'}}), (b:ActionNode {{name: '{}'}})",
                    escape_gql(strip_ifml_prefix(from_id)),
                    escape_gql(strip_ifml_prefix(to_id)),
                )
            }
            EdgeType::ActionEvent => {
                format!(
                    "MATCH (a:ActionNode {{name: '{}'}}), (b:Event {{name: '{}'}})",
                    escape_gql(strip_ifml_prefix(from_id)),
                    escape_gql(strip_ifml_prefix(to_id)),
                )
            }
            EdgeType::HasModuleDefinition => {
                format!(
                    "MATCH (a:ViewContainer {{name: '{}'}}), (b:ModuleDefinition {{name: '{}'}})",
                    escape_gql(strip_ifml_prefix(from_id)),
                    escape_gql(strip_ifml_prefix(to_id)),
                )
            }
            EdgeType::HasViewComponentPart => {
                format!(
                    "MATCH (a:ViewComponent {{name: '{}'}}), (b:ViewComponent {{name: '{}'}})",
                    escape_gql(strip_ifml_prefix(from_id)),
                    escape_gql(strip_ifml_prefix(to_id)),
                )
            }
            EdgeType::HasConditionalExpr => {
                format!(
                    "MATCH (a {{name: '{}'}}), (b {{name: '{}'}})",
                    escape_gql(strip_ifml_prefix(from_id)),
                    escape_gql(strip_ifml_prefix(to_id)),
                )
            }
            // API metamodel edges. Name-bearing nodes (ApiResource,
            // ApiOperation, Pipeline, Permission, ErrorDefinition) store plain
            // names; Interaction/HttpEndpoint store the full prefixed id as
            // `name`; Schema targets match by title.
            EdgeType::ExposesResource => {
                format!(
                    "MATCH (a:ApiResource {{name: '{}'}}), (b:ApiResource {{name: '{}'}})",
                    escape_gql(strip_api_prefix(from_id)),
                    escape_gql(strip_api_prefix(to_id)),
                )
            }
            EdgeType::BindsToSchema => {
                format!(
                    "MATCH (a:ApiResource {{name: '{}'}}), (b:Schema {{title: '{}'}})",
                    escape_gql(strip_api_prefix(from_id)),
                    escape_gql(to_id),
                )
            }
            EdgeType::HasOperation => {
                format!(
                    "MATCH (a:ApiResource {{name: '{}'}}), (b:ApiOperation {{name: '{}'}})",
                    escape_gql(strip_api_prefix(from_id)),
                    escape_gql(strip_api_prefix(to_id)),
                )
            }
            EdgeType::InputBoundTo | EdgeType::OutputBoundTo => {
                format!(
                    "MATCH (a:ApiOperation {{name: '{}'}}), (b:Schema {{title: '{}'}})",
                    escape_gql(strip_api_prefix(from_id)),
                    escape_gql(to_id),
                )
            }
            EdgeType::CanReturnError => {
                format!(
                    "MATCH (a:ApiOperation {{name: '{}'}}), (b:ErrorDefinition {{code: '{}'}})",
                    escape_gql(strip_api_prefix(from_id)),
                    escape_gql(strip_api_prefix(to_id)),
                )
            }
            EdgeType::RequiresPermission => {
                format!(
                    "MATCH (a:ApiOperation {{name: '{}'}}), (b:Permission {{name: '{}'}})",
                    escape_gql(strip_api_prefix(from_id)),
                    escape_gql(strip_api_prefix(to_id)),
                )
            }
            EdgeType::HasInteraction => {
                format!(
                    "MATCH (a:ApiOperation {{name: '{}'}}), (b:Interaction {{name: '{}'}})",
                    escape_gql(strip_api_prefix(from_id)),
                    escape_gql(to_id),
                )
            }
            EdgeType::BindsHttpEndpoint => {
                format!(
                    "MATCH (a:Interaction {{name: '{}'}}), (b:HttpEndpoint {{name: '{}'}})",
                    escape_gql(from_id),
                    escape_gql(to_id),
                )
            }
            EdgeType::UsesPipeline => {
                format!(
                    "MATCH (a:HttpEndpoint {{name: '{}'}}), (b:Pipeline {{name: '{}'}})",
                    escape_gql(from_id),
                    escape_gql(strip_api_prefix(to_id)),
                )
            }
            // AT Protocol edges — nodes are matched by their natural keys
            // (Lexicon/Collection by nsid, Namespace by authority, Repository by did).
            EdgeType::InNamespace => {
                format!(
                    "MATCH (a:Lexicon {{nsid: '{}'}}), (b:Namespace {{authority: '{}'}})",
                    escape_gql(from_id),
                    escape_gql(to_id),
                )
            }
            EdgeType::ProjectsToLexicon => {
                format!(
                    "MATCH (a:Schema {{title: '{}'}}), (b:Lexicon {{nsid: '{}'}})",
                    escape_gql(from_id),
                    escape_gql(to_id),
                )
            }
            EdgeType::DefinesCollection => {
                format!(
                    "MATCH (a:Lexicon {{nsid: '{}'}}), (b:Collection {{nsid: '{}'}})",
                    escape_gql(from_id),
                    escape_gql(to_id),
                )
            }
            EdgeType::LexiconReferences => {
                format!(
                    "MATCH (a:Lexicon {{nsid: '{}'}}), (b:Lexicon {{nsid: '{}'}})",
                    escape_gql(from_id),
                    escape_gql(to_id),
                )
            }
            EdgeType::StoredInRepository => {
                format!(
                    "MATCH (a:Collection {{nsid: '{}'}}), (b:Repository {{did: '{}'}})",
                    escape_gql(from_id),
                    escape_gql(to_id),
                )
            }
            EdgeType::HasPolicy
            | EdgeType::PolicyAppliesTo
            | EdgeType::HasRelationship
            | EdgeType::RelationshipSource
            | EdgeType::RelationshipTarget
            | EdgeType::PolicyOnRelationship
            | EdgeType::TenantOwns
            | EdgeType::HasMembership
            | EdgeType::MembershipInTenant
            | EdgeType::HasRole => {
                format!(
                    "MATCH (a {{name: '{}'}}), (b {{name: '{}'}})",
                    escape_gql(strip_api_prefix(from_id)),
                    escape_gql(strip_api_prefix(to_id)),
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

    async fn ingest_view_container(&self, node: &ViewContainerNode) -> Result<String, GraphError> {
        let session = self.db().session();
        let id = format!("vc:{}", node.name);
        let gql = format!(
            "INSERT (:ViewContainer {{ \
                name: '{}', label: {}, is_xor: {}, is_default: {}, \
                is_landmark: {}, is_modal: {}, domain: {} \
            }})",
            escape_gql(&node.name),
            opt_str(&node.label),
            node.is_xor,
            node.is_default,
            node.is_landmark,
            node.is_modal,
            opt_str(&node.domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_view_container failed: {e}")))?;
        Ok(id)
    }

    async fn ingest_view_component(&self, node: &ViewComponentNode) -> Result<String, GraphError> {
        let session = self.db().session();
        let id = format!("comp:{}", node.name);
        let fields_json = node
            .fields
            .as_ref()
            .map(|f| serde_json::to_string(f).unwrap_or_default());
        let gql = format!(
            "INSERT (:ViewComponent {{ \
                name: '{}', component_type: '{}', mode: {}, \
                entity: {}, fields: {}, filter: {}, api_operation: {}, \
                spec: {}, domain: {} \
            }})",
            escape_gql(&node.name),
            escape_gql(&node.component_type),
            opt_str(&node.mode),
            opt_str(&node.entity),
            opt_str(&fields_json),
            opt_str(&node.filter),
            opt_str(&node.api_operation),
            opt_str(&node.spec),
            opt_str(&node.domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_view_component failed: {e}")))?;
        Ok(id)
    }

    async fn ingest_event(&self, node: &EventNode) -> Result<String, GraphError> {
        let session = self.db().session();
        let id = format!("evt:{}", node.name);
        let params_json = node
            .params
            .as_ref()
            .map(|p| serde_json::to_string(p).unwrap_or_default());
        let gql = format!(
            "INSERT (:Event {{ \
                name: '{}', event_type: '{}', params: {}, domain: {} \
            }})",
            escape_gql(&node.name),
            escape_gql(&node.event_type),
            opt_str(&params_json),
            opt_str(&node.domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_event failed: {e}")))?;
        Ok(id)
    }

    async fn ingest_action_node(&self, node: &ActionNode) -> Result<String, GraphError> {
        let session = self.db().session();
        let id = format!("action:{}", node.name);
        let gql = format!(
            "INSERT (:ActionNode {{ name: '{}', domain: {} }})",
            escape_gql(&node.name),
            opt_str(&node.domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_action_node failed: {e}")))?;
        Ok(id)
    }

    async fn ingest_parameter_definition(
        &self,
        node: &ParameterDefinitionNode,
    ) -> Result<String, GraphError> {
        let session = self.db().session();
        let id = format!("param:{}", node.name);
        let gql = format!(
            "INSERT (:ParameterDefinition {{ \
                name: '{}', direction: '{}', type_ref: '{}', domain: {} \
            }})",
            escape_gql(&node.name),
            escape_gql(&node.direction),
            escape_gql(&node.type_ref),
            opt_str(&node.domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_parameter_definition failed: {e}")))?;
        Ok(id)
    }

    async fn ingest_data_binding(&self, node: &DataBindingNode) -> Result<String, GraphError> {
        let session = self.db().session();
        let id = format!("db:{}", node.name);
        let gql = format!(
            "INSERT (:DataBinding {{ \
                name: '{}', conditional_expression: {}, expression_language: '{}', domain: {} \
            }})",
            escape_gql(&node.name),
            opt_str(&node.conditional_expression),
            escape_gql(&node.expression_language),
            opt_str(&node.domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_data_binding failed: {e}")))?;
        Ok(id)
    }

    async fn ingest_namespace(&self, node: &NamespaceNode) -> Result<String, GraphError> {
        let session = self.db().session();
        let gql = format!(
            "INSERT (:Namespace {{ authority: '{}', segment: '{}', domain: '{}' }})",
            escape_gql(&node.authority),
            escape_gql(&node.segment),
            escape_gql(&node.domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_namespace failed: {e}")))?;
        Ok(node.authority.clone())
    }

    async fn ingest_lexicon(&self, node: &LexiconNode) -> Result<String, GraphError> {
        let session = self.db().session();
        let revision_val = match &node.revision {
            Some(v) => v.to_string(),
            None => "null".to_string(),
        };
        let gql = format!(
            "INSERT (:Lexicon {{ nsid: '{}', lex_type: '{}', key_strategy: '{}', \
             revision: {}, description: {}, domain: '{}' }})",
            escape_gql(&node.nsid),
            escape_gql(&node.lex_type),
            escape_gql(&node.key_strategy),
            revision_val,
            opt_str(&node.description),
            escape_gql(&node.domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_lexicon failed: {e}")))?;
        Ok(node.nsid.clone())
    }

    async fn ingest_collection(&self, node: &CollectionNode) -> Result<String, GraphError> {
        let session = self.db().session();
        let gql = format!(
            "INSERT (:Collection {{ nsid: '{}', key_strategy: '{}', domain: '{}' }})",
            escape_gql(&node.nsid),
            escape_gql(&node.key_strategy),
            escape_gql(&node.domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_collection failed: {e}")))?;
        Ok(node.nsid.clone())
    }

    async fn ingest_repository(&self, node: &RepositoryNode) -> Result<String, GraphError> {
        let session = self.db().session();
        let gql = format!(
            "INSERT (:Repository {{ did: '{}', handle: {}, pds_endpoint: '{}', \
             org_name: '{}', tenancy_mode: '{}' }})",
            escape_gql(&node.did),
            opt_str(&node.handle),
            escape_gql(&node.pds_endpoint),
            escape_gql(&node.org_name),
            escape_gql(&node.tenancy_mode),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_repository failed: {e}")))?;
        Ok(node.did.clone())
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
            ifml_node_count: count_from_gql(
                self,
                "MATCH (n) WHERE n:ViewContainer OR n:ViewComponent OR n:Event OR n:ActionNode OR n:ParameterDefinition OR n:DataBinding RETURN count(n) AS cnt",
            )?,
            lexicons_ingested: count_from_gql(
                self,
                "MATCH (l:Lexicon) RETURN count(l) AS cnt",
            )?,
            collections_ingested: count_from_gql(
                self,
                "MATCH (c:Collection) RETURN count(c) AS cnt",
            )?,
            namespaces_ingested: count_from_gql(
                self,
                "MATCH (n:Namespace) RETURN count(n) AS cnt",
            )?,
            repositories_ingested: count_from_gql(
                self,
                "MATCH (r:Repository) RETURN count(r) AS cnt",
            )?,
            api_resource_count: count_from_gql(
                self,
                "MATCH (r:ApiResource) RETURN count(r) AS cnt",
            )?,
            policy_count: count_from_gql(
                self,
                "MATCH (p:Policy) RETURN count(p) AS cnt",
            )?,
            relationship_count: count_from_gql(
                self,
                "MATCH (r:Relationship) RETURN count(r) AS cnt",
            )?,
            security_node_count: count_from_gql(
                self,
                "MATCH (n) WHERE n:SecurityIdentity OR n:Membership OR n:Tenant RETURN count(n) AS cnt",
            )?,
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

    // ── API metamodel ingestion ───────────────────────────────────────

    async fn ingest_api_resource(&self, node: &ApiResourceNode) -> Result<String, GraphError> {
        let session = self.db().session();
        let id = format!("ar:{}", node.name);
        let gql = format!(
            "INSERT (:ApiResource {{ \
                _node_id: '{}', name: '{}', schema_title: '{}', domain: '{}', \
                label: {}, path_segment: '{}' \
            }})",
            escape_gql(&id),
            escape_gql(&node.name),
            escape_gql(&node.schema_title),
            escape_gql(&node.domain),
            opt_str(&node.label),
            escape_gql(&node.path_segment),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_api_resource failed: {e}")))?;
        Ok(id)
    }

    async fn ingest_api_operation(&self, node: &ApiOperationNode) -> Result<String, GraphError> {
        let session = self.db().session();
        let id = format!("ao:{}", node.name);
        let gql = format!(
            "INSERT (:ApiOperation {{ \
                _node_id: '{}', name: '{}', kind: '{}', input_schema: {}, output_schema: '{}', \
                paging: {}, sorting: {}, filtering: {}, domain: {} \
            }})",
            escape_gql(&id),
            escape_gql(&node.name),
            escape_gql(&node.kind),
            opt_str(&node.input_schema),
            escape_gql(&node.output_schema),
            node.paging,
            node.sorting,
            node.filtering,
            opt_str(&node.domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_api_operation failed: {e}")))?;
        Ok(id)
    }

    async fn ingest_interaction(&self, node: &InteractionNode) -> Result<String, GraphError> {
        let session = self.db().session();
        let id = format!("ia:{}", uuid::Uuid::new_v4());
        let gql = format!(
            "INSERT (:Interaction {{ name: '{}', transport: '{}', domain: {} }})",
            escape_gql(&id),
            escape_gql(&node.transport),
            opt_str(&node.domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_interaction failed: {e}")))?;
        Ok(id)
    }

    async fn ingest_http_endpoint(&self, node: &HttpEndpointNode) -> Result<String, GraphError> {
        let session = self.db().session();
        let id = format!("he:{}", uuid::Uuid::new_v4());
        let gql = format!(
            "INSERT (:HttpEndpoint {{ name: '{}', method: '{}', path_template: '{}', domain: {} }})",
            escape_gql(&id),
            escape_gql(&node.method),
            escape_gql(&node.path_template),
            opt_str(&node.domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_http_endpoint failed: {e}")))?;
        Ok(id)
    }

    async fn ingest_pipeline(&self, node: &PipelineNode) -> Result<String, GraphError> {
        let session = self.db().session();
        let id = format!("pl:{}", node.name);
        let middleware_str = node
            .middleware
            .as_ref()
            .map(|m| serde_json::to_string(m).unwrap_or_default());
        let gql = format!(
            "INSERT (:Pipeline {{ _node_id: '{}', name: '{}', middleware: {}, domain: {} }})",
            escape_gql(&id),
            escape_gql(&node.name),
            opt_str(&middleware_str),
            opt_str(&node.domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_pipeline failed: {e}")))?;
        Ok(id)
    }

    async fn ingest_error_definition(
        &self,
        node: &ErrorDefinitionNode,
    ) -> Result<String, GraphError> {
        let session = self.db().session();
        let id = format!("ed:{}", node.code);
        let gql = format!(
            "INSERT (:ErrorDefinition {{ \
                code: '{}', description: '{}', http_status: {}, domain: {} \
            }})",
            escape_gql(&node.code),
            escape_gql(&node.description),
            node.http_status,
            opt_str(&node.domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_error_definition failed: {e}")))?;
        Ok(id)
    }

    async fn ingest_permission(&self, node: &PermissionNode) -> Result<String, GraphError> {
        let session = self.db().session();
        let id = format!("pm:{}", node.name);
        let gql = format!(
            "INSERT (:Permission {{ _node_id: '{}', name: '{}', domain: {} }})",
            escape_gql(&id),
            escape_gql(&node.name),
            opt_str(&node.domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_permission failed: {e}")))?;
        Ok(id)
    }

    // ── Persistence metamodel ────────────────────────────────────────

    async fn ingest_policy(&self, policy: &PolicyNode) -> Result<(), GraphError> {
        let session = self.db().session();
        let kind_json =
            serde_json::to_string(&policy.kind).map_err(|e| GraphError::Ingest(e.to_string()))?;
        let domain = policy.domain.clone().unwrap_or_default();
        let gql = format!(
            "INSERT (:Policy {{ \
                name: '{}', kind_json: '{}', target_schema: '{}', domain: '{}' \
            }})",
            escape_gql(&policy.name),
            escape_gql(&kind_json),
            escape_gql(&policy.target_schema),
            escape_gql(&domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_policy failed: {e}")))?;
        Ok(())
    }

    async fn ingest_relationship(&self, relationship: &RelationshipNode) -> Result<(), GraphError> {
        let session = self.db().session();
        let fk_json = serde_json::to_string(&relationship.foreign_key)
            .map_err(|e| GraphError::Ingest(e.to_string()))?;
        let propagation_json = serde_json::to_string(&relationship.propagation)
            .map_err(|e| GraphError::Ingest(e.to_string()))?;
        let domain = relationship.domain.clone().unwrap_or_default();
        let gql = format!(
            "INSERT (:Relationship {{ \
                name: '{}', source_schema: '{}', target_schema: '{}', \
                cardinality: '{}', ownership: '{}', fk_json: '{}', \
                propagation_json: '{}', domain: '{}' \
            }})",
            escape_gql(&relationship.name),
            escape_gql(&relationship.source_schema),
            escape_gql(&relationship.target_schema),
            escape_gql(&serde_enum_str(&relationship.cardinality)),
            escape_gql(&serde_enum_str(&relationship.ownership)),
            escape_gql(&fk_json),
            escape_gql(&propagation_json),
            escape_gql(&domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_relationship failed: {e}")))?;
        Ok(())
    }

    async fn ingest_security_identity(
        &self,
        identity: &SecurityIdentityNode,
    ) -> Result<(), GraphError> {
        let session = self.db().session();
        let domain = identity.domain.clone().unwrap_or_default();
        let gql = format!(
            "INSERT (:SecurityIdentity {{ \
                name: '{}', subject: '{}', domain: '{}' \
            }})",
            escape_gql(&identity.name),
            escape_gql(&identity.subject),
            escape_gql(&domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_security_identity failed: {e}")))?;
        Ok(())
    }

    async fn ingest_membership(&self, membership: &MembershipNode) -> Result<(), GraphError> {
        let session = self.db().session();
        let roles_json = serde_json::to_string(&membership.roles)
            .map_err(|e| GraphError::Ingest(e.to_string()))?;
        let valid_from = membership.valid_from.clone().unwrap_or_default();
        let valid_until = membership.valid_until.clone().unwrap_or_default();
        let gql = format!(
            "INSERT (:Membership {{ \
                identity: '{}', tenant: '{}', status: '{}', roles_json: '{}', \
                valid_from: '{}', valid_until: '{}' \
            }})",
            escape_gql(&membership.identity),
            escape_gql(&membership.tenant),
            escape_gql(&serde_enum_str(&membership.status)),
            escape_gql(&roles_json),
            escape_gql(&valid_from),
            escape_gql(&valid_until),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_membership failed: {e}")))?;
        Ok(())
    }

    async fn ingest_tenant(&self, tenant: &TenantNode) -> Result<(), GraphError> {
        let session = self.db().session();
        let strategy_json = serde_json::to_string(&tenant.strategy)
            .map_err(|e| GraphError::Ingest(e.to_string()))?;
        let domain = tenant.domain.clone().unwrap_or_default();
        let gql = format!(
            "INSERT (:Tenant {{ \
                name: '{}', label: '{}', strategy_json: '{}', domain: '{}' \
            }})",
            escape_gql(&tenant.name),
            escape_gql(&tenant.label),
            escape_gql(&strategy_json),
            escape_gql(&domain),
        );
        session
            .execute(&gql)
            .map_err(|e| GraphError::Ingest(format!("ingest_tenant failed: {e}")))?;
        Ok(())
    }
}
