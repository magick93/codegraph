use codegraph_core::error::GraphError;
use codegraph_core::types::{
    Cardinality, CodeList, CompositeColumn, CompositeRange, EnumValue, Extension, ForeignKeySpec,
    MembershipNode, MembershipStatus, Ownership, PolicyKind, PolicyNode, PropagationRule,
    PropertyNode, RelationshipNode, SchemaNode, SecurityIdentityNode, StructuredSubField,
    TenantNode, TenantStrategy,
};
use codegraph_type_contracts::RefClassificationKind;
use std::collections::HashMap;

fn parse_classification_kind(s: &str) -> Option<RefClassificationKind> {
    match s {
        "primitive_wrapper" => Some(RefClassificationKind::PrimitiveWrapper),
        "array_wrapper" => Some(RefClassificationKind::ArrayWrapper),
        "range_wrapper" => Some(RefClassificationKind::RangeWrapper),
        "codelist" => Some(RefClassificationKind::CodelistReference),
        "codelist_check" => Some(RefClassificationKind::CodelistCheck),
        "inline_enum" => Some(RefClassificationKind::InlineEnum),
        "entity_reference" => Some(RefClassificationKind::EntityReference),
        "value_object" => Some(RefClassificationKind::ValueObject),
        "composite_wrapper" => Some(RefClassificationKind::CompositeWrapper),
        _ => None,
    }
}

/// A helper that maps column names to indices for a QueryResult.
pub struct RowReader {
    col_map: HashMap<String, usize>,
}

impl RowReader {
    pub fn from_columns(columns: &[String]) -> Self {
        let col_map = columns
            .iter()
            .enumerate()
            .map(|(i, name)| (name.clone(), i))
            .collect();
        Self { col_map }
    }

    fn idx(&self, col: &str) -> Result<usize, GraphError> {
        self.col_map
            .get(col)
            .copied()
            .ok_or_else(|| GraphError::Query(format!("column '{col}' not found in result")))
    }

    pub fn get_string(&self, row: &[grafeo::Value], col: &str) -> Result<String, GraphError> {
        let i = self.idx(col)?;
        row[i]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| GraphError::Query(format!("column '{col}' is not a string")))
    }

    pub fn get_opt_string(
        &self,
        row: &[grafeo::Value],
        col: &str,
    ) -> Result<Option<String>, GraphError> {
        let i = self.idx(col)?;
        if row[i].is_null() {
            Ok(None)
        } else {
            Ok(row[i].as_str().map(|s| s.to_string()))
        }
    }

    pub fn get_bool(&self, row: &[grafeo::Value], col: &str) -> Result<bool, GraphError> {
        let i = self.idx(col)?;
        row[i]
            .as_bool()
            .ok_or_else(|| GraphError::Query(format!("column '{col}' is not a bool")))
    }

    pub fn get_i64(&self, row: &[grafeo::Value], col: &str) -> Result<i64, GraphError> {
        let i = self.idx(col)?;
        row[i]
            .as_int64()
            .ok_or_else(|| GraphError::Query(format!("column '{col}' is not an integer")))
    }

    pub fn get_i32(&self, row: &[grafeo::Value], col: &str) -> Result<i32, GraphError> {
        self.get_i64(row, col).map(|v| v as i32)
    }

    pub fn get_usize(&self, row: &[grafeo::Value], col: &str) -> Result<usize, GraphError> {
        self.get_i64(row, col).map(|v| v as usize)
    }
}

pub fn row_to_schema_node(
    reader: &RowReader,
    row: &[grafeo::Value],
) -> Result<SchemaNode, GraphError> {
    Ok(SchemaNode {
        schema_id: reader.get_string(row, "s.schema_id")?,
        title: reader.get_string(row, "s.title")?,
        description: reader.get_opt_string(row, "s.description")?,
        schema_type: reader.get_string(row, "s.schema_type")?,
        classification: reader.get_string(row, "s.classification")?,
        domain: reader.get_opt_string(row, "s.domain")?,
        rel_path: reader.get_string(row, "s.rel_path")?,
        pg_type: reader.get_string(row, "s.pg_type")?,
        rust_type: reader.get_string(row, "s.rust_type")?,
        sea_orm_type: reader.get_string(row, "s.sea_orm_type")?,
        rust_type_name: reader.get_string(row, "s.rust_type_name")?,
        pg_table_name: reader.get_string(row, "s.pg_table_name")?,
        api_path_segment: reader.get_string(row, "s.api_path_segment")?,
        parent_schema: reader.get_opt_string(row, "s.parent_schema")?,
        is_entity: reader.get_bool(row, "s.is_entity")?,
        is_codelist: reader.get_bool(row, "s.is_codelist")?,
        is_primitive_wrapper: reader.get_bool(row, "s.is_primitive_wrapper")?,
        has_all_of: reader.get_bool(row, "s.has_all_of")?,
        has_one_of: reader.get_bool(row, "s.has_one_of")?,
        has_any_of: reader.get_bool(row, "s.has_any_of")?,
        has_definitions: reader.get_bool(row, "s.has_definitions")?,
        custom_annotations: serde_json::from_str(&reader.get_string(row, "s.custom_annotations")?)
            .unwrap_or_default(),
    })
}

pub fn row_to_property_node(
    reader: &RowReader,
    row: &[grafeo::Value],
) -> Result<PropertyNode, GraphError> {
    Ok(PropertyNode {
        name: reader.get_string(row, "p.name")?,
        prop_type: reader.get_string(row, "p.prop_type")?,
        description: reader.get_opt_string(row, "p.description")?,
        format: reader.get_opt_string(row, "p.format")?,
        is_required: reader.get_bool(row, "p.is_required")?,
        is_nullable: reader.get_bool(row, "p.is_nullable")?,
        is_array: reader.get_bool(row, "p.is_array")?,
        pattern: reader.get_opt_string(row, "p.pattern")?,
        min_length: reader
            .get_opt_string(row, "p.min_length")
            .ok()
            .flatten()
            .and_then(|s| s.parse::<u64>().ok()),
        max_length: reader
            .get_opt_string(row, "p.max_length")
            .ok()
            .flatten()
            .and_then(|s| s.parse::<u64>().ok()),
        minimum: reader
            .get_opt_string(row, "p.minimum")
            .ok()
            .flatten()
            .and_then(|s| s.parse::<rust_decimal::Decimal>().ok()),
        maximum: reader
            .get_opt_string(row, "p.maximum")
            .ok()
            .flatten()
            .and_then(|s| s.parse::<rust_decimal::Decimal>().ok()),
        pg_column_name: reader.get_string(row, "p.pg_column_name")?,
        pg_column_type: reader.get_string(row, "p.pg_column_type")?,
        rust_field_name: reader.get_string(row, "p.rust_field_name")?,
        rust_field_type: reader.get_string(row, "p.rust_field_type")?,
        sea_orm_type: reader.get_string(row, "p.sea_orm_type")?,
        render_strategy: reader.get_string(row, "p.render_strategy")?,
        ref_target: reader.get_opt_string(row, "p.ref_target")?,
        classification: reader.get_opt_string(row, "p.classification")?,
        projection: None,
        classification_kind: reader
            .get_opt_string(row, "p.classification_kind")?
            .as_deref()
            .and_then(parse_classification_kind),
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    })
}

pub fn row_to_codelist(reader: &RowReader, row: &[grafeo::Value]) -> Result<CodeList, GraphError> {
    Ok(CodeList {
        name: reader.get_string(row, "c.name")?,
        description: reader.get_opt_string(row, "c.description")?,
        pg_table_name: reader.get_string(row, "c.pg_table_name")?,
        render_as: reader.get_string(row, "c.render_as")?,
        check_expression: reader.get_opt_string(row, "c.check_expression")?,
    })
}

pub fn row_to_enum_value(
    reader: &RowReader,
    row: &[grafeo::Value],
) -> Result<EnumValue, GraphError> {
    Ok(EnumValue {
        value: reader.get_string(row, "v.value")?,
        display_name: reader.get_opt_string(row, "v.display_name")?,
        sort_order: reader.get_i32(row, "v.sort_order")?,
    })
}

pub fn row_to_composite_column(
    reader: &RowReader,
    row: &[grafeo::Value],
) -> Result<CompositeColumn, GraphError> {
    Ok(CompositeColumn {
        suffix: reader.get_string(row, "cc.suffix")?,
        pg_type: reader.get_string(row, "cc.pg_type")?,
        rust_type: reader.get_string(row, "cc.rust_type")?,
        sea_orm_type: reader.get_string(row, "cc.sea_orm_type")?,
        fk_target: reader.get_opt_string(row, "cc.fk_target")?,
        dto_rust_type: reader.get_opt_string(row, "cc.dto_rust_type")?,
        wrapper_schema: reader
            .get_string(row, "cc.wrapper_schema")
            .unwrap_or_default(),
    })
}

pub fn row_to_composite_range(
    reader: &RowReader,
    row: &[grafeo::Value],
) -> Result<CompositeRange, GraphError> {
    Ok(CompositeRange {
        pg_column_name: reader.get_string(row, "r.pg_column_name")?,
        pg_type: reader.get_string(row, "r.pg_type")?,
        rust_type: reader.get_string(row, "r.rust_type")?,
        start_field: reader.get_string(row, "r.start_field")?,
        end_field: reader.get_string(row, "r.end_field")?,
        open_end: reader.get_bool(row, "r.open_end")?,
    })
}

pub fn row_to_extension(
    reader: &RowReader,
    row: &[grafeo::Value],
) -> Result<Extension, GraphError> {
    Ok(Extension {
        name: reader.get_string(row, "e.name")?,
    })
}

pub fn row_to_structured_sub_field(
    reader: &RowReader,
    row: &[grafeo::Value],
) -> Result<StructuredSubField, GraphError> {
    Ok(StructuredSubField {
        name: reader.get_string(row, "p.name")?,
        description: reader
            .get_opt_string(row, "p.description")?
            .unwrap_or_default(),
        is_required: reader.get_bool(row, "p.is_required")?,
    })
}

pub fn row_to_policy_node(
    reader: &RowReader,
    row: &[grafeo::Value],
) -> Result<PolicyNode, GraphError> {
    let kind_json = reader.get_string(row, "kind_json")?;
    let kind: PolicyKind = serde_json::from_str(&kind_json)
        .map_err(|e| GraphError::Query(format!("Failed to parse policy kind: {}", e)))?;
    Ok(PolicyNode {
        name: reader.get_string(row, "name")?,
        kind,
        target_schema: reader.get_string(row, "target_schema")?,
        domain: reader.get_opt_string(row, "domain")?,
    })
}

pub fn row_to_relationship_node(
    reader: &RowReader,
    row: &[grafeo::Value],
) -> Result<RelationshipNode, GraphError> {
    let cardinality_str = reader.get_string(row, "cardinality")?;
    let ownership_str = reader.get_string(row, "ownership")?;
    let fk_json = reader.get_string(row, "fk_json")?;
    let propagation_json = reader.get_string(row, "propagation_json")?;

    let cardinality: Cardinality = serde_json::from_str(&format!("\"{}\"", cardinality_str))
        .or_else(|_| serde_json::from_str(&cardinality_str))
        .unwrap_or(Cardinality::OneToMany);
    let ownership: Ownership = serde_json::from_str(&format!("\"{}\"", ownership_str))
        .or_else(|_| serde_json::from_str(&ownership_str))
        .unwrap_or(Ownership::References);
    let foreign_key: Option<ForeignKeySpec> = serde_json::from_str(&fk_json).ok();
    let propagation: Vec<PropagationRule> =
        serde_json::from_str(&propagation_json).unwrap_or_default();

    Ok(RelationshipNode {
        name: reader.get_string(row, "name")?,
        source_schema: reader.get_string(row, "source_schema")?,
        target_schema: reader.get_string(row, "target_schema")?,
        cardinality,
        ownership,
        foreign_key,
        propagation,
        domain: reader.get_opt_string(row, "domain")?,
    })
}

pub fn row_to_security_identity_node(
    reader: &RowReader,
    row: &[grafeo::Value],
) -> Result<SecurityIdentityNode, GraphError> {
    Ok(SecurityIdentityNode {
        name: reader.get_string(row, "name")?,
        subject: reader.get_string(row, "subject")?,
        domain: reader.get_opt_string(row, "domain")?,
    })
}

pub fn row_to_membership_node(
    reader: &RowReader,
    row: &[grafeo::Value],
) -> Result<MembershipNode, GraphError> {
    let status_str = reader.get_string(row, "status")?;
    let roles_json = reader.get_string(row, "roles_json")?;

    let status: MembershipStatus = serde_json::from_str(&format!("\"{}\"", status_str))
        .or_else(|_| serde_json::from_str(&status_str))
        .unwrap_or(MembershipStatus::Active);
    let roles: Vec<String> = serde_json::from_str(&roles_json).unwrap_or_default();

    Ok(MembershipNode {
        identity: reader.get_string(row, "identity")?,
        tenant: reader.get_string(row, "tenant")?,
        status,
        roles,
        valid_from: reader.get_opt_string(row, "valid_from")?,
        valid_until: reader.get_opt_string(row, "valid_until")?,
    })
}

pub fn row_to_tenant_node(
    reader: &RowReader,
    row: &[grafeo::Value],
) -> Result<TenantNode, GraphError> {
    let strategy_json = reader.get_string(row, "strategy_json")?;
    let strategy: TenantStrategy = serde_json::from_str(&strategy_json)
        .map_err(|e| GraphError::Query(format!("Failed to parse tenant strategy: {}", e)))?;
    Ok(TenantNode {
        name: reader.get_string(row, "name")?,
        label: reader.get_string(row, "label")?,
        strategy,
        domain: reader.get_opt_string(row, "domain")?,
    })
}
