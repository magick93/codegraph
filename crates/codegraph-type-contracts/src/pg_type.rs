use serde::{Deserialize, Serialize};

use crate::RustType;

/// Represents every Postgres column type used in the generated DDL.
///
/// This is a **closed** enum with no `Custom` escape hatch — every Postgres
/// type that the code generator can emit must have an explicit variant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PgType {
    Text,
    Uuid,
    Boolean,
    SmallInt,
    Integer,
    BigInt,
    Numeric { precision: u8, scale: u8 },
    Real,
    DoublePrecision,
    Date,
    Timestamptz,
    Jsonb,
    TextArray,
    BigIntArray,
    IntegerArray,
    DoubleArray,
    BoolArray,
    Int4Range,
    Int8Range,
    TstzRange,
    DateRange,
    Bytea,
}

impl PgType {
    /// The canonical Rust type for this Postgres type.
    ///
    /// This is THE source of truth for the Pg-to-Rust mapping used across
    /// struct generation, DTO generation, and SeaORM entity generation.
    pub fn canonical_rust_type(&self) -> RustType {
        match self {
            PgType::Text => RustType::String,
            PgType::Uuid => RustType::Uuid,
            PgType::Boolean => RustType::Bool,
            PgType::SmallInt => RustType::I16,
            PgType::Integer => RustType::I32,
            PgType::BigInt => RustType::I64,
            PgType::Numeric { .. } => RustType::Decimal,
            PgType::Real => RustType::F32,
            PgType::DoublePrecision => RustType::F64,
            PgType::Date => RustType::NaiveDate,
            PgType::Timestamptz => RustType::DateTimeUtc,
            PgType::Jsonb => RustType::Json,
            PgType::TextArray => RustType::VecString,
            PgType::BigIntArray => RustType::VecI64,
            PgType::IntegerArray => RustType::VecI32,
            PgType::DoubleArray => RustType::VecF64,
            PgType::BoolArray => RustType::VecBool,
            PgType::Int4Range => RustType::String,
            PgType::Int8Range => RustType::String,
            PgType::TstzRange => RustType::String,
            PgType::DateRange => RustType::String,
            PgType::Bytea => RustType::Bytes,
        }
    }

    /// Returns the DDL string for this type as used in `CREATE TABLE` statements.
    pub fn pg_ddl(&self) -> String {
        match self {
            PgType::Text => "TEXT".to_owned(),
            PgType::Uuid => "UUID".to_owned(),
            PgType::Boolean => "BOOLEAN".to_owned(),
            PgType::SmallInt => "SMALLINT".to_owned(),
            PgType::Integer => "INTEGER".to_owned(),
            PgType::BigInt => "BIGINT".to_owned(),
            PgType::Numeric { precision, scale } => format!("NUMERIC({precision},{scale})"),
            PgType::Real => "REAL".to_owned(),
            PgType::DoublePrecision => "DOUBLE PRECISION".to_owned(),
            PgType::Date => "DATE".to_owned(),
            PgType::Timestamptz => "TIMESTAMPTZ".to_owned(),
            PgType::Jsonb => "JSONB".to_owned(),
            PgType::TextArray => "TEXT[]".to_owned(),
            PgType::BigIntArray => "BIGINT[]".to_owned(),
            PgType::IntegerArray => "INTEGER[]".to_owned(),
            PgType::DoubleArray => "DOUBLE PRECISION[]".to_owned(),
            PgType::BoolArray => "BOOLEAN[]".to_owned(),
            PgType::Int4Range => "INT4RANGE".to_owned(),
            PgType::Int8Range => "INT8RANGE".to_owned(),
            PgType::TstzRange => "TSTZRANGE".to_owned(),
            PgType::DateRange => "DATERANGE".to_owned(),
            PgType::Bytea => "BYTEA".to_owned(),
        }
    }

    /// Returns the SeaORM `ColumnType` variant name for this Postgres type.
    pub fn sea_orm_type(&self) -> &str {
        match self {
            PgType::Text => "Text",
            PgType::Uuid => "Uuid",
            PgType::Boolean => "Boolean",
            PgType::SmallInt => "SmallInteger",
            PgType::Integer => "Integer",
            PgType::BigInt => "BigInteger",
            PgType::Numeric { .. } => "Decimal",
            PgType::Real => "Float",
            PgType::DoublePrecision => "Double",
            PgType::Date => "Date",
            PgType::Timestamptz => "TimestampWithTimeZone",
            PgType::Jsonb => "JsonBinary",
            PgType::TextArray => "Array(RcColumnType::Text)",
            PgType::BigIntArray => "Array(RcColumnType::BigInteger)",
            PgType::IntegerArray => "Array(RcColumnType::Integer)",
            PgType::DoubleArray => "Array(RcColumnType::Double)",
            PgType::BoolArray => "Array(RcColumnType::Boolean)",
            // Range types have no native SeaORM support; store as Text so
            // SeaORM binds as a text parameter that Postgres casts to the range type.
            PgType::Int4Range => "Text",
            PgType::Int8Range => "Text",
            PgType::TstzRange => "Text",
            PgType::DateRange => "Text",
            PgType::Bytea => "Binary",
        }
    }

    /// Parses a DDL type string back into a `PgType`.
    ///
    /// Returns `None` for unrecognised strings.
    pub fn from_pg_str(s: &str) -> Option<Self> {
        let upper = s.trim().to_uppercase();

        // Try NUMERIC(p,s) first
        if upper.starts_with("NUMERIC(") && upper.ends_with(')') {
            let inner = &upper["NUMERIC(".len()..upper.len() - 1];
            let parts: Vec<&str> = inner.split(',').collect();
            if parts.len() == 2 {
                let precision = parts[0].trim().parse::<u8>().ok()?;
                let scale = parts[1].trim().parse::<u8>().ok()?;
                return Some(PgType::Numeric { precision, scale });
            }
            return None;
        }

        match upper.as_str() {
            "TEXT" => Some(PgType::Text),
            "UUID" => Some(PgType::Uuid),
            "BOOLEAN" => Some(PgType::Boolean),
            "SMALLINT" => Some(PgType::SmallInt),
            "INTEGER" => Some(PgType::Integer),
            "BIGINT" => Some(PgType::BigInt),
            "REAL" => Some(PgType::Real),
            "DOUBLE PRECISION" => Some(PgType::DoublePrecision),
            "DATE" => Some(PgType::Date),
            "TIMESTAMPTZ" => Some(PgType::Timestamptz),
            "JSONB" => Some(PgType::Jsonb),
            "TEXT[]" => Some(PgType::TextArray),
            "BIGINT[]" => Some(PgType::BigIntArray),
            "INTEGER[]" => Some(PgType::IntegerArray),
            "DOUBLE PRECISION[]" => Some(PgType::DoubleArray),
            "BOOLEAN[]" => Some(PgType::BoolArray),
            "INT4RANGE" => Some(PgType::Int4Range),
            "INT8RANGE" => Some(PgType::Int8Range),
            "TSTZRANGE" => Some(PgType::TstzRange),
            "DATERANGE" => Some(PgType::DateRange),
            "BYTEA" => Some(PgType::Bytea),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_variants() -> Vec<PgType> {
        vec![
            PgType::Text,
            PgType::Uuid,
            PgType::Boolean,
            PgType::SmallInt,
            PgType::Integer,
            PgType::BigInt,
            PgType::Numeric {
                precision: 19,
                scale: 4,
            },
            PgType::Real,
            PgType::DoublePrecision,
            PgType::Date,
            PgType::Timestamptz,
            PgType::Jsonb,
            PgType::TextArray,
            PgType::BigIntArray,
            PgType::IntegerArray,
            PgType::DoubleArray,
            PgType::BoolArray,
            PgType::Int4Range,
            PgType::Int8Range,
            PgType::TstzRange,
            PgType::DateRange,
            PgType::Bytea,
        ]
    }

    #[test]
    fn test_canonical_rust_type_all_variants() {
        let cases: Vec<(PgType, RustType)> = vec![
            (PgType::Text, RustType::String),
            (PgType::Uuid, RustType::Uuid),
            (PgType::Boolean, RustType::Bool),
            (PgType::SmallInt, RustType::I16),
            (PgType::Integer, RustType::I32),
            (PgType::BigInt, RustType::I64),
            (
                PgType::Numeric {
                    precision: 19,
                    scale: 4,
                },
                RustType::Decimal,
            ),
            (PgType::Real, RustType::F32),
            (PgType::DoublePrecision, RustType::F64),
            (PgType::Date, RustType::NaiveDate),
            (PgType::Timestamptz, RustType::DateTimeUtc),
            (PgType::Jsonb, RustType::Json),
            (PgType::TextArray, RustType::VecString),
            (PgType::BigIntArray, RustType::VecI64),
            (PgType::IntegerArray, RustType::VecI32),
            (PgType::DoubleArray, RustType::VecF64),
            (PgType::BoolArray, RustType::VecBool),
            (PgType::Int4Range, RustType::String),
            (PgType::Int8Range, RustType::String),
            (PgType::TstzRange, RustType::String),
            (PgType::DateRange, RustType::String),
            (PgType::Bytea, RustType::Bytes),
        ];

        for (pg, expected_rust) in cases {
            assert_eq!(
                pg.canonical_rust_type(),
                expected_rust,
                "canonical_rust_type mismatch for {:?}",
                pg
            );
        }
    }

    #[test]
    fn test_pg_ddl_all_variants() {
        let cases: Vec<(PgType, &str)> = vec![
            (PgType::Text, "TEXT"),
            (PgType::Uuid, "UUID"),
            (PgType::Boolean, "BOOLEAN"),
            (PgType::SmallInt, "SMALLINT"),
            (PgType::Integer, "INTEGER"),
            (PgType::BigInt, "BIGINT"),
            (
                PgType::Numeric {
                    precision: 19,
                    scale: 4,
                },
                "NUMERIC(19,4)",
            ),
            (PgType::Real, "REAL"),
            (PgType::DoublePrecision, "DOUBLE PRECISION"),
            (PgType::Date, "DATE"),
            (PgType::Timestamptz, "TIMESTAMPTZ"),
            (PgType::Jsonb, "JSONB"),
            (PgType::TextArray, "TEXT[]"),
            (PgType::BigIntArray, "BIGINT[]"),
            (PgType::IntegerArray, "INTEGER[]"),
            (PgType::DoubleArray, "DOUBLE PRECISION[]"),
            (PgType::BoolArray, "BOOLEAN[]"),
            (PgType::Int4Range, "INT4RANGE"),
            (PgType::Int8Range, "INT8RANGE"),
            (PgType::TstzRange, "TSTZRANGE"),
            (PgType::DateRange, "DATERANGE"),
            (PgType::Bytea, "BYTEA"),
        ];

        for (pg, expected_ddl) in cases {
            assert_eq!(pg.pg_ddl(), expected_ddl, "pg_ddl mismatch for {:?}", pg);
        }
    }

    #[test]
    fn test_sea_orm_type_all_variants() {
        let cases: Vec<(PgType, &str)> = vec![
            (PgType::Text, "Text"),
            (PgType::Uuid, "Uuid"),
            (PgType::Boolean, "Boolean"),
            (PgType::SmallInt, "SmallInteger"),
            (PgType::Integer, "Integer"),
            (PgType::BigInt, "BigInteger"),
            (
                PgType::Numeric {
                    precision: 10,
                    scale: 2,
                },
                "Decimal",
            ),
            (PgType::Real, "Float"),
            (PgType::DoublePrecision, "Double"),
            (PgType::Date, "Date"),
            (PgType::Timestamptz, "TimestampWithTimeZone"),
            (PgType::Jsonb, "JsonBinary"),
            (PgType::TextArray, "Array(RcColumnType::Text)"),
            (PgType::BigIntArray, "Array(RcColumnType::BigInteger)"),
            (PgType::IntegerArray, "Array(RcColumnType::Integer)"),
            (PgType::DoubleArray, "Array(RcColumnType::Double)"),
            (PgType::BoolArray, "Array(RcColumnType::Boolean)"),
            (PgType::Int4Range, "Text"),
            (PgType::Int8Range, "Text"),
            (PgType::TstzRange, "Text"),
            (PgType::DateRange, "Text"),
            (PgType::Bytea, "Binary"),
        ];

        for (pg, expected_sea) in cases {
            assert_eq!(
                pg.sea_orm_type(),
                expected_sea,
                "sea_orm_type mismatch for {:?}",
                pg
            );
        }
    }

    #[test]
    fn test_from_pg_str_round_trip() {
        // All variants except Numeric round-trip through pg_ddl -> from_pg_str.
        for pg in all_variants() {
            let ddl = pg.pg_ddl();
            let parsed = PgType::from_pg_str(&ddl);
            assert_eq!(
                parsed,
                Some(pg.clone()),
                "round-trip failed for {:?} (ddl={ddl})",
                pg
            );
        }
    }

    #[test]
    fn test_from_pg_str_numeric_parsing() {
        assert_eq!(
            PgType::from_pg_str("NUMERIC(19,4)"),
            Some(PgType::Numeric {
                precision: 19,
                scale: 4
            })
        );
        assert_eq!(
            PgType::from_pg_str("numeric(10, 2)"),
            Some(PgType::Numeric {
                precision: 10,
                scale: 2
            })
        );
    }

    #[test]
    fn test_from_pg_str_unknown_returns_none() {
        assert_eq!(PgType::from_pg_str("VARCHAR(255)"), None);
        assert_eq!(PgType::from_pg_str("SERIAL"), None);
        assert_eq!(PgType::from_pg_str(""), None);
        assert_eq!(PgType::from_pg_str("NOT_A_TYPE"), None);
    }

    #[test]
    fn test_from_pg_str_case_insensitive() {
        assert_eq!(PgType::from_pg_str("text"), Some(PgType::Text));
        assert_eq!(PgType::from_pg_str("Text"), Some(PgType::Text));
        assert_eq!(
            PgType::from_pg_str("double precision"),
            Some(PgType::DoublePrecision)
        );
        assert_eq!(
            PgType::from_pg_str("double precision[]"),
            Some(PgType::DoubleArray)
        );
        assert_eq!(PgType::from_pg_str("boolean[]"), Some(PgType::BoolArray));
    }
}
