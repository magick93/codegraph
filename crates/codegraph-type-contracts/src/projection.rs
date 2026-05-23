use serde::{Deserialize, Serialize};

use crate::{ColumnType, RustType};

/// Cross-layer projection for a single DDD field.
/// Computed once during classification, consumed by all generators.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DddFieldProjection {
    pub entity: EntityProjection,
    pub domain: DomainProjection,
    pub dto: DtoProjections,
}

/// How this field maps to Postgres columns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EntityProjection {
    SingleColumn {
        column_name: String,
        column_type: ColumnType,
        is_fk: bool,
        fk_target: Option<FkTarget>,
    },
    CompositeColumns {
        primary: CompositeColumn,
        secondary: Vec<CompositeColumn>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositeColumn {
    pub suffix: String,
    pub column_type: ColumnType,
    pub fk_target: Option<FkTarget>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FkTarget {
    pub schema: String,
    pub table: String,
    pub column: String,
}

/// Domain layer projection (intentionally minimal).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainProjection {
    pub rust_type: RustType,
}

/// DTO projections for create/update/response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DtoProjections {
    pub create: DtoFieldType,
    pub update: DtoFieldType,
    pub response: DtoFieldType,
}

/// How this field appears in DTOs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DtoFieldType {
    Scalar(RustType),
    Codelist { enum_name: String },
    NestedDto,
    EntityRef { entity_name: String },
    Composite { name: String },
}

impl EntityProjection {
    pub fn is_fk(&self) -> bool {
        match self {
            Self::SingleColumn { is_fk, .. } => *is_fk,
            Self::CompositeColumns {
                primary, secondary, ..
            } => primary.fk_target.is_some() || secondary.iter().any(|c| c.fk_target.is_some()),
        }
    }

    pub fn rust_type_str(&self) -> String {
        match self {
            Self::SingleColumn { column_type, .. } => column_type.rust_type_str(),
            Self::CompositeColumns { primary, .. } => primary.column_type.rust_type_str(),
        }
    }

    pub fn sea_orm_type_str(&self) -> String {
        match self {
            Self::SingleColumn { column_type, .. } => column_type.sea_orm_type().to_string(),
            Self::CompositeColumns { primary, .. } => {
                primary.column_type.sea_orm_type().to_string()
            }
        }
    }
}

impl DddFieldProjection {
    pub fn format_rust_type(&self, is_required: bool) -> String {
        let inner = self.domain.rust_type.as_rust_str();
        if is_required {
            inner
        } else {
            format!("Option<{inner}>")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PgType, RustType};

    fn make_column_type() -> ColumnType {
        ColumnType::from_pg(PgType::Uuid)
    }

    fn make_dto_projections() -> DtoProjections {
        DtoProjections {
            create: DtoFieldType::Scalar(RustType::Uuid),
            update: DtoFieldType::Scalar(RustType::Uuid),
            response: DtoFieldType::Scalar(RustType::Uuid),
        }
    }

    #[test]
    fn test_entity_projection_single_column_is_fk_true() {
        let proj = EntityProjection::SingleColumn {
            column_name: "person_id".to_owned(),
            column_type: make_column_type(),
            is_fk: true,
            fk_target: Some(FkTarget {
                schema: "common".to_owned(),
                table: "person".to_owned(),
                column: "id".to_owned(),
            }),
        };
        assert!(proj.is_fk());
    }

    #[test]
    fn test_entity_projection_single_column_is_fk_false() {
        let proj = EntityProjection::SingleColumn {
            column_name: "name".to_owned(),
            column_type: make_column_type(),
            is_fk: false,
            fk_target: None,
        };
        assert!(!proj.is_fk());
    }

    #[test]
    fn test_entity_projection_composite_columns_is_fk_via_secondary() {
        let primary = CompositeColumn {
            suffix: "value".to_owned(),
            column_type: make_column_type(),
            fk_target: None,
        };
        let secondary = vec![CompositeColumn {
            suffix: "ref_id".to_owned(),
            column_type: make_column_type(),
            fk_target: Some(FkTarget {
                schema: "payroll".to_owned(),
                table: "pay_run".to_owned(),
                column: "id".to_owned(),
            }),
        }];
        let proj = EntityProjection::CompositeColumns { primary, secondary };
        assert!(proj.is_fk());
    }

    #[test]
    fn test_ddd_field_projection_format_rust_type_required() {
        let projection = DddFieldProjection {
            entity: EntityProjection::SingleColumn {
                column_name: "id".to_owned(),
                column_type: make_column_type(),
                is_fk: false,
                fk_target: None,
            },
            domain: DomainProjection {
                rust_type: RustType::Uuid,
            },
            dto: make_dto_projections(),
        };
        assert_eq!(projection.format_rust_type(true), "Uuid");
    }

    #[test]
    fn test_ddd_field_projection_format_rust_type_optional() {
        let projection = DddFieldProjection {
            entity: EntityProjection::SingleColumn {
                column_name: "id".to_owned(),
                column_type: make_column_type(),
                is_fk: false,
                fk_target: None,
            },
            domain: DomainProjection {
                rust_type: RustType::Uuid,
            },
            dto: make_dto_projections(),
        };
        assert_eq!(projection.format_rust_type(false), "Option<Uuid>");
    }
}
