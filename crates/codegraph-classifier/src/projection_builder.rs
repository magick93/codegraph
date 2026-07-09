use std::collections::HashMap;

use codegraph_type_contracts::{
    ColumnType, CompositeColumn, DddFieldProjection, DomainProjection, DtoFieldType,
    DtoProjections, EntityProjection, FkTarget, PgType, RefClassificationKind, RustType,
};

use crate::config::CompositeWrapperColumn;

pub struct ProjectionBuilder;

impl ProjectionBuilder {
    /// Build the complete cross-layer projection for a classified $ref.
    #[allow(clippy::too_many_arguments)]
    pub fn from_classification(
        kind: &RefClassificationKind,
        field_name: &str,
        pg_type_str: Option<&str>,
        _rust_type_str: Option<&str>,
        _sea_orm_type_str: Option<&str>,
        codelist_table_schema: Option<&str>,
        codelist_table_name: Option<&str>,
        codelist_enum_map: &HashMap<String, String>,
        entity_table_schema: Option<&str>,
        entity_table_name: Option<&str>,
        composite_columns: Option<&[CompositeWrapperColumn]>,
        value_is_optional: bool,
    ) -> DddFieldProjection {
        match kind {
            RefClassificationKind::PrimitiveWrapper => {
                Self::build_primitive(field_name, pg_type_str.unwrap_or("TEXT"))
            }
            RefClassificationKind::ArrayWrapper => {
                Self::build_array(field_name, pg_type_str.unwrap_or("TEXT[]"))
            }
            RefClassificationKind::RangeWrapper => {
                Self::build_range(field_name, pg_type_str.unwrap_or("INT4RANGE"))
            }
            RefClassificationKind::CodelistReference => Self::build_codelist(
                field_name,
                codelist_table_schema.unwrap_or(""),
                codelist_table_name.unwrap_or(""),
                codelist_enum_map,
            ),
            RefClassificationKind::CodelistCheck => Self::build_inline_enum(field_name),
            RefClassificationKind::InlineEnum => Self::build_inline_enum(field_name),
            RefClassificationKind::EntityReference => Self::build_entity_ref(
                field_name,
                entity_table_schema.unwrap_or(""),
                entity_table_name.unwrap_or(""),
            ),
            RefClassificationKind::ValueObject => Self::build_value_object(field_name),
            RefClassificationKind::StructuredWrapper => Self::build_structured_wrapper(
                field_name,
                _rust_type_str.unwrap_or("serde_json::Value"),
            ),
            RefClassificationKind::CompositeWrapper => Self::build_composite(
                field_name,
                composite_columns.unwrap_or(&[]),
                value_is_optional,
            ),
            RefClassificationKind::MediaWrapper => Self::build_composite(
                field_name,
                composite_columns.unwrap_or(&[]),
                value_is_optional,
            ),
        }
    }

    /// Build projection for a direct JSON schema primitive (no $ref).
    pub fn from_scalar(
        scalar_kind: &codegraph_type_contracts::ScalarKind,
        field_name: &str,
    ) -> DddFieldProjection {
        let pg = match scalar_kind {
            codegraph_type_contracts::ScalarKind::String => PgType::Text,
            codegraph_type_contracts::ScalarKind::Integer => PgType::BigInt,
            codegraph_type_contracts::ScalarKind::Number => PgType::DoublePrecision,
            codegraph_type_contracts::ScalarKind::Boolean => PgType::Boolean,
            codegraph_type_contracts::ScalarKind::Date => PgType::Date,
            codegraph_type_contracts::ScalarKind::DateTime => PgType::Timestamptz,
            codegraph_type_contracts::ScalarKind::Json => PgType::Jsonb,
        };
        let rust = pg.canonical_rust_type();
        let column_type = ColumnType::from_pg(pg);
        DddFieldProjection {
            entity: EntityProjection::SingleColumn {
                column_name: field_name.to_string(),
                column_type,
                is_fk: false,
                fk_target: None,
            },
            domain: DomainProjection {
                rust_type: rust.clone(),
            },
            dto: DtoProjections {
                create: DtoFieldType::Scalar(rust.clone()),
                update: DtoFieldType::Scalar(rust.clone()),
                response: DtoFieldType::Scalar(rust),
            },
        }
    }

    fn build_primitive(field_name: &str, pg_type_str: &str) -> DddFieldProjection {
        let pg = PgType::from_pg_str(pg_type_str).unwrap_or(PgType::Text);
        let rust = pg.canonical_rust_type();
        let column_type = ColumnType::from_pg(pg);
        DddFieldProjection {
            entity: EntityProjection::SingleColumn {
                column_name: field_name.to_string(),
                column_type,
                is_fk: false,
                fk_target: None,
            },
            domain: DomainProjection {
                rust_type: rust.clone(),
            },
            dto: DtoProjections {
                create: DtoFieldType::Scalar(rust.clone()),
                update: DtoFieldType::Scalar(rust.clone()),
                response: DtoFieldType::Scalar(rust),
            },
        }
    }

    fn build_array(field_name: &str, pg_type_str: &str) -> DddFieldProjection {
        let pg = PgType::from_pg_str(pg_type_str).unwrap_or(PgType::TextArray);
        let rust = pg.canonical_rust_type();
        let column_type = ColumnType::from_pg(pg);
        DddFieldProjection {
            entity: EntityProjection::SingleColumn {
                column_name: field_name.to_string(),
                column_type,
                is_fk: false,
                fk_target: None,
            },
            domain: DomainProjection {
                rust_type: rust.clone(),
            },
            dto: DtoProjections {
                create: DtoFieldType::Scalar(rust.clone()),
                update: DtoFieldType::Scalar(rust.clone()),
                response: DtoFieldType::Scalar(rust),
            },
        }
    }

    fn build_range(field_name: &str, pg_type_str: &str) -> DddFieldProjection {
        let pg = PgType::from_pg_str(pg_type_str).unwrap_or(PgType::Int4Range);
        let rust = pg.canonical_rust_type();
        let column_type = ColumnType::from_pg(pg);
        DddFieldProjection {
            entity: EntityProjection::SingleColumn {
                column_name: field_name.to_string(),
                column_type,
                is_fk: false,
                fk_target: None,
            },
            domain: DomainProjection {
                rust_type: rust.clone(),
            },
            dto: DtoProjections {
                create: DtoFieldType::Scalar(rust.clone()),
                update: DtoFieldType::Scalar(rust.clone()),
                response: DtoFieldType::Scalar(rust),
            },
        }
    }

    fn build_codelist(
        field_name: &str,
        codelist_table_schema: &str,
        codelist_table_name: &str,
        codelist_enum_map: &HashMap<String, String>,
    ) -> DddFieldProjection {
        let enum_name = codelist_enum_map
            .get(codelist_table_name)
            .cloned()
            .unwrap_or_else(|| field_name.to_string());
        let column_type = ColumnType::from_pg(PgType::Text);
        DddFieldProjection {
            entity: EntityProjection::SingleColumn {
                column_name: field_name.to_string(),
                column_type,
                is_fk: true,
                fk_target: Some(FkTarget {
                    schema: codelist_table_schema.to_string(),
                    table: codelist_table_name.to_string(),
                    column: "code".to_string(),
                }),
            },
            domain: DomainProjection {
                rust_type: RustType::String,
            },
            dto: DtoProjections {
                create: DtoFieldType::Codelist {
                    enum_name: enum_name.clone(),
                },
                update: DtoFieldType::Codelist {
                    enum_name: enum_name.clone(),
                },
                response: DtoFieldType::Codelist { enum_name },
            },
        }
    }

    fn build_inline_enum(field_name: &str) -> DddFieldProjection {
        let column_type = ColumnType::from_pg(PgType::Text);
        DddFieldProjection {
            entity: EntityProjection::SingleColumn {
                column_name: field_name.to_string(),
                column_type,
                is_fk: false,
                fk_target: None,
            },
            domain: DomainProjection {
                rust_type: RustType::String,
            },
            dto: DtoProjections {
                create: DtoFieldType::Scalar(RustType::String),
                update: DtoFieldType::Scalar(RustType::String),
                response: DtoFieldType::Scalar(RustType::String),
            },
        }
    }

    fn build_entity_ref(
        field_name: &str,
        entity_table_schema: &str,
        entity_table_name: &str,
    ) -> DddFieldProjection {
        let column_name = format!("{field_name}_id");
        let column_type = ColumnType::from_pg(PgType::Uuid);
        DddFieldProjection {
            entity: EntityProjection::SingleColumn {
                column_name,
                column_type,
                is_fk: true,
                fk_target: Some(FkTarget {
                    schema: entity_table_schema.to_string(),
                    table: entity_table_name.to_string(),
                    column: "id".to_string(),
                }),
            },
            domain: DomainProjection {
                rust_type: RustType::Uuid,
            },
            dto: DtoProjections {
                create: DtoFieldType::EntityRef {
                    entity_name: entity_table_name.to_string(),
                },
                update: DtoFieldType::EntityRef {
                    entity_name: entity_table_name.to_string(),
                },
                response: DtoFieldType::EntityRef {
                    entity_name: entity_table_name.to_string(),
                },
            },
        }
    }

    fn build_structured_wrapper(field_name: &str, rust_type_name: &str) -> DddFieldProjection {
        let column_type = ColumnType::from_pg(PgType::Jsonb);
        let domain_type = RustType::DomainType(rust_type_name.to_string());
        DddFieldProjection {
            entity: EntityProjection::SingleColumn {
                column_name: field_name.to_string(),
                column_type,
                is_fk: false,
                fk_target: None,
            },
            domain: DomainProjection {
                rust_type: domain_type.clone(),
            },
            dto: DtoProjections {
                create: DtoFieldType::Scalar(domain_type.clone()),
                update: DtoFieldType::Scalar(domain_type.clone()),
                response: DtoFieldType::Scalar(domain_type),
            },
        }
    }

    fn build_value_object(field_name: &str) -> DddFieldProjection {
        let column_type = ColumnType::from_pg(PgType::Jsonb);
        DddFieldProjection {
            entity: EntityProjection::SingleColumn {
                column_name: field_name.to_string(),
                column_type,
                is_fk: false,
                fk_target: None,
            },
            domain: DomainProjection {
                rust_type: RustType::Json,
            },
            dto: DtoProjections {
                create: DtoFieldType::NestedDto,
                update: DtoFieldType::NestedDto,
                response: DtoFieldType::NestedDto,
            },
        }
    }

    fn build_composite(
        field_name: &str,
        columns: &[CompositeWrapperColumn],
        _value_is_optional: bool,
    ) -> DddFieldProjection {
        if columns.is_empty() {
            // Fallback: single JSONB column
            return Self::build_value_object(field_name);
        }

        let make_composite_col = |col: &CompositeWrapperColumn| {
            let pg = PgType::from_pg_str(&col.postgres).unwrap_or(PgType::Text);
            let column_type = ColumnType::from_pg(pg);
            let fk_target = if col.fk_table.is_empty() {
                None
            } else {
                Some(parse_fk_table(&col.fk_table))
            };
            CompositeColumn {
                suffix: col.suffix.clone(),
                column_type,
                fk_target,
            }
        };

        let primary = make_composite_col(&columns[0]);
        let secondary: Vec<CompositeColumn> = columns[1..].iter().map(make_composite_col).collect();

        // Domain type is the canonical rust type of the primary column
        let domain_rust_type = primary.column_type.rust().clone();

        DddFieldProjection {
            entity: EntityProjection::CompositeColumns { primary, secondary },
            domain: DomainProjection {
                rust_type: domain_rust_type,
            },
            dto: DtoProjections {
                create: DtoFieldType::Composite {
                    name: field_name.to_string(),
                },
                update: DtoFieldType::Composite {
                    name: field_name.to_string(),
                },
                response: DtoFieldType::Composite {
                    name: field_name.to_string(),
                },
            },
        }
    }
}

/// Parse a FK table reference like "schema.table" or just "table".
fn parse_fk_table(fk_table: &str) -> FkTarget {
    if let Some((schema, table)) = fk_table.split_once('.') {
        // Sanitize the schema name: strip .json extension and # fragment
        // that come from JSON schema filenames (e.g. "OrderType.json#").
        let schema = schema
            .strip_suffix(".json#")
            .or_else(|| schema.strip_suffix(".json"))
            .unwrap_or(schema)
            .to_string();
        FkTarget {
            schema,
            table: table.to_string(),
            column: "id".to_string(),
        }
    } else {
        FkTarget {
            schema: String::new(),
            table: fk_table.to_string(),
            column: "id".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_enum_map() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn primitive_wrapper_text_produces_consistent_projections() {
        let proj = ProjectionBuilder::from_classification(
            &RefClassificationKind::PrimitiveWrapper,
            "code",
            Some("TEXT"),
            None,
            None,
            None,
            None,
            &empty_enum_map(),
            None,
            None,
            None,
            false,
        );

        // Entity: TEXT column, no FK
        match &proj.entity {
            EntityProjection::SingleColumn {
                column_name,
                column_type,
                is_fk,
                fk_target,
            } => {
                assert_eq!(column_name, "code");
                assert_eq!(column_type.pg(), &PgType::Text);
                assert_eq!(column_type.rust(), &RustType::String);
                assert!(!is_fk);
                assert!(fk_target.is_none());
            }
            _ => panic!("expected SingleColumn"),
        }
        assert!(!proj.entity.is_fk());

        // Domain
        assert_eq!(proj.domain.rust_type, RustType::String);

        // DTO
        assert!(matches!(
            proj.dto.create,
            DtoFieldType::Scalar(RustType::String)
        ));
        assert!(matches!(
            proj.dto.update,
            DtoFieldType::Scalar(RustType::String)
        ));
        assert!(matches!(
            proj.dto.response,
            DtoFieldType::Scalar(RustType::String)
        ));
    }

    #[test]
    fn array_wrapper_text_array_produces_consistent_projections() {
        let proj = ProjectionBuilder::from_classification(
            &RefClassificationKind::ArrayWrapper,
            "tags",
            Some("TEXT[]"),
            None,
            None,
            None,
            None,
            &empty_enum_map(),
            None,
            None,
            None,
            false,
        );

        match &proj.entity {
            EntityProjection::SingleColumn {
                column_type, is_fk, ..
            } => {
                assert_eq!(column_type.pg(), &PgType::TextArray);
                assert_eq!(column_type.rust(), &RustType::VecString);
                assert!(!is_fk);
            }
            _ => panic!("expected SingleColumn"),
        }
        assert!(!proj.entity.is_fk());
        assert_eq!(proj.domain.rust_type, RustType::VecString);
        assert!(matches!(
            proj.dto.create,
            DtoFieldType::Scalar(RustType::VecString)
        ));
    }

    #[test]
    fn range_wrapper_int4range_produces_consistent_projections() {
        let proj = ProjectionBuilder::from_classification(
            &RefClassificationKind::RangeWrapper,
            "age_range",
            Some("INT4RANGE"),
            None,
            None,
            None,
            None,
            &empty_enum_map(),
            None,
            None,
            None,
            false,
        );

        match &proj.entity {
            EntityProjection::SingleColumn {
                column_type, is_fk, ..
            } => {
                assert_eq!(column_type.pg(), &PgType::Int4Range);
                assert_eq!(column_type.rust(), &RustType::String);
                assert!(!is_fk);
            }
            _ => panic!("expected SingleColumn"),
        }
        assert!(!proj.entity.is_fk());
        assert_eq!(proj.domain.rust_type, RustType::String);
        assert!(matches!(
            proj.dto.create,
            DtoFieldType::Scalar(RustType::String)
        ));
    }

    #[test]
    fn codelist_reference_produces_fk_and_codelist_dto() {
        let mut enum_map = HashMap::new();
        enum_map.insert("country_code".to_string(), "CountryCode".to_string());

        let proj = ProjectionBuilder::from_classification(
            &RefClassificationKind::CodelistReference,
            "country",
            None,
            None,
            None,
            Some("common"),
            Some("country_code"),
            &enum_map,
            None,
            None,
            None,
            false,
        );

        match &proj.entity {
            EntityProjection::SingleColumn {
                column_type,
                is_fk,
                fk_target,
                ..
            } => {
                assert_eq!(column_type.pg(), &PgType::Text);
                assert!(*is_fk);
                let fk = fk_target.as_ref().unwrap();
                assert_eq!(fk.schema, "common");
                assert_eq!(fk.table, "country_code");
                assert_eq!(fk.column, "code");
            }
            _ => panic!("expected SingleColumn"),
        }
        assert!(proj.entity.is_fk());
        assert_eq!(proj.domain.rust_type, RustType::String);
        assert!(matches!(
            &proj.dto.create,
            DtoFieldType::Codelist { enum_name } if enum_name == "CountryCode"
        ));
    }

    #[test]
    fn codelist_check_produces_text_no_fk() {
        let proj = ProjectionBuilder::from_classification(
            &RefClassificationKind::CodelistCheck,
            "status",
            None,
            None,
            None,
            None,
            None,
            &empty_enum_map(),
            None,
            None,
            None,
            false,
        );

        match &proj.entity {
            EntityProjection::SingleColumn {
                column_type,
                is_fk,
                fk_target,
                ..
            } => {
                assert_eq!(column_type.pg(), &PgType::Text);
                assert!(!is_fk);
                assert!(fk_target.is_none());
            }
            _ => panic!("expected SingleColumn"),
        }
        assert!(!proj.entity.is_fk());
        assert_eq!(proj.domain.rust_type, RustType::String);
        assert!(matches!(
            proj.dto.create,
            DtoFieldType::Scalar(RustType::String)
        ));
    }

    #[test]
    fn inline_enum_produces_text_no_fk() {
        let proj = ProjectionBuilder::from_classification(
            &RefClassificationKind::InlineEnum,
            "priority",
            None,
            None,
            None,
            None,
            None,
            &empty_enum_map(),
            None,
            None,
            None,
            false,
        );

        match &proj.entity {
            EntityProjection::SingleColumn {
                column_type,
                is_fk,
                fk_target,
                ..
            } => {
                assert_eq!(column_type.pg(), &PgType::Text);
                assert!(!is_fk);
                assert!(fk_target.is_none());
            }
            _ => panic!("expected SingleColumn"),
        }
        assert!(!proj.entity.is_fk());
        assert_eq!(proj.domain.rust_type, RustType::String);
        assert!(matches!(
            proj.dto.response,
            DtoFieldType::Scalar(RustType::String)
        ));
    }

    #[test]
    fn entity_reference_produces_uuid_fk() {
        let proj = ProjectionBuilder::from_classification(
            &RefClassificationKind::EntityReference,
            "person",
            None,
            None,
            None,
            None,
            None,
            &empty_enum_map(),
            Some("common"),
            Some("person"),
            None,
            false,
        );

        match &proj.entity {
            EntityProjection::SingleColumn {
                column_name,
                column_type,
                is_fk,
                fk_target,
            } => {
                assert_eq!(column_name, "person_id");
                assert_eq!(column_type.pg(), &PgType::Uuid);
                assert!(*is_fk);
                let fk = fk_target.as_ref().unwrap();
                assert_eq!(fk.schema, "common");
                assert_eq!(fk.table, "person");
                assert_eq!(fk.column, "id");
            }
            _ => panic!("expected SingleColumn"),
        }
        assert!(proj.entity.is_fk());
        assert_eq!(proj.domain.rust_type, RustType::Uuid);
        assert!(matches!(
            &proj.dto.create,
            DtoFieldType::EntityRef { entity_name } if entity_name == "person"
        ));
    }

    #[test]
    fn value_object_produces_jsonb_nested_dto() {
        let proj = ProjectionBuilder::from_classification(
            &RefClassificationKind::ValueObject,
            "address",
            None,
            None,
            None,
            None,
            None,
            &empty_enum_map(),
            None,
            None,
            None,
            false,
        );

        match &proj.entity {
            EntityProjection::SingleColumn {
                column_type,
                is_fk,
                fk_target,
                ..
            } => {
                assert_eq!(column_type.pg(), &PgType::Jsonb);
                assert!(!is_fk);
                assert!(fk_target.is_none());
            }
            _ => panic!("expected SingleColumn"),
        }
        assert!(!proj.entity.is_fk());
        assert_eq!(proj.domain.rust_type, RustType::Json);
        assert!(matches!(proj.dto.create, DtoFieldType::NestedDto));
        assert!(matches!(proj.dto.update, DtoFieldType::NestedDto));
        assert!(matches!(proj.dto.response, DtoFieldType::NestedDto));
    }

    #[test]
    fn composite_wrapper_produces_multiple_columns() {
        let columns = vec![
            CompositeWrapperColumn {
                suffix: "value".to_string(),
                postgres: "NUMERIC(19,4)".to_string(),
                rust: "Decimal".to_string(),
                sea_orm: "Decimal".to_string(),
                fk_table: String::new(),
                dto_rust_type: None,
            },
            CompositeWrapperColumn {
                suffix: "currency".to_string(),
                postgres: "TEXT".to_string(),
                rust: "String".to_string(),
                sea_orm: "Text".to_string(),
                fk_table: "common.currency_code".to_string(),
                dto_rust_type: None,
            },
        ];

        let proj = ProjectionBuilder::from_classification(
            &RefClassificationKind::CompositeWrapper,
            "amount",
            None,
            None,
            None,
            None,
            None,
            &empty_enum_map(),
            None,
            None,
            Some(&columns),
            false,
        );

        match &proj.entity {
            EntityProjection::CompositeColumns { primary, secondary } => {
                assert_eq!(primary.suffix, "value");
                assert_eq!(
                    primary.column_type.pg(),
                    &PgType::Numeric {
                        precision: 19,
                        scale: 4
                    }
                );
                assert!(primary.fk_target.is_none());

                assert_eq!(secondary.len(), 1);
                assert_eq!(secondary[0].suffix, "currency");
                assert_eq!(secondary[0].column_type.pg(), &PgType::Text);
                let fk = secondary[0].fk_target.as_ref().unwrap();
                assert_eq!(fk.schema, "common");
                assert_eq!(fk.table, "currency_code");
                assert_eq!(fk.column, "id");
            }
            _ => panic!("expected CompositeColumns"),
        }
        assert!(proj.entity.is_fk());
        assert_eq!(proj.domain.rust_type, RustType::Decimal);
        assert!(matches!(
            &proj.dto.create,
            DtoFieldType::Composite { name } if name == "amount"
        ));
    }

    #[test]
    fn structured_wrapper_produces_jsonb_domain_type() {
        let proj = ProjectionBuilder::from_classification(
            &RefClassificationKind::StructuredWrapper,
            "tax_id",
            Some("JSONB"),
            Some("IdentifierType"),
            Some("JsonBinary"),
            None,
            None,
            &empty_enum_map(),
            None,
            None,
            None,
            false,
        );

        match &proj.entity {
            EntityProjection::SingleColumn {
                column_name,
                column_type,
                is_fk,
                fk_target,
            } => {
                assert_eq!(column_name, "tax_id");
                assert_eq!(column_type.pg(), &PgType::Jsonb);
                assert!(!is_fk);
                assert!(fk_target.is_none());
            }
            _ => panic!("expected SingleColumn"),
        }
        assert!(!proj.entity.is_fk());
        assert_eq!(
            proj.domain.rust_type,
            RustType::DomainType("IdentifierType".to_string())
        );
        assert!(matches!(
            &proj.dto.create,
            DtoFieldType::Scalar(RustType::DomainType(name)) if name == "IdentifierType"
        ));
        assert!(matches!(
            &proj.dto.response,
            DtoFieldType::Scalar(RustType::DomainType(name)) if name == "IdentifierType"
        ));
    }

    #[test]
    fn from_scalar_string_produces_text_column() {
        let proj =
            ProjectionBuilder::from_scalar(&codegraph_type_contracts::ScalarKind::String, "given_name");

        match &proj.entity {
            EntityProjection::SingleColumn {
                column_name,
                column_type,
                is_fk,
                fk_target,
            } => {
                assert_eq!(column_name, "given_name");
                assert_eq!(column_type.pg(), &PgType::Text);
                assert_eq!(column_type.rust(), &RustType::String);
                assert!(!is_fk);
                assert!(fk_target.is_none());
            }
            _ => panic!("expected SingleColumn"),
        }
        assert!(!proj.entity.is_fk());
        assert_eq!(proj.domain.rust_type, RustType::String);
        assert!(matches!(
            proj.dto.create,
            DtoFieldType::Scalar(RustType::String)
        ));
        assert!(matches!(
            proj.dto.update,
            DtoFieldType::Scalar(RustType::String)
        ));
        assert!(matches!(
            proj.dto.response,
            DtoFieldType::Scalar(RustType::String)
        ));
    }
}
