use codegraph_core::error::GraphError;
use codegraph_core::types::{
    CodeList, CompositeColumn, CompositeRange, EnumValue, Extension, PropertyNode, SchemaNode,
    StructuredSubField,
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
