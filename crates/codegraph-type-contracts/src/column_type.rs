use serde::{Deserialize, Serialize};

use crate::{PgType, RustType};

/// A validated Postgres ↔ Rust type pair.
/// Construction restricted to known-good combinations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnType {
    pg: PgType,
    rust: RustType,
}

impl ColumnType {
    /// Construct from PgType using canonical Rust mapping.
    pub fn from_pg(pg: PgType) -> Self {
        let rust = pg.canonical_rust_type();
        Self { pg, rust }
    }

    /// Construct with a domain type override (for codelist enums, composite structs).
    pub fn with_domain_type(pg: PgType, domain_type: String) -> Self {
        Self {
            pg,
            rust: RustType::DomainType(domain_type),
        }
    }

    pub fn pg(&self) -> &PgType {
        &self.pg
    }

    pub fn rust(&self) -> &RustType {
        &self.rust
    }

    pub fn pg_ddl(&self) -> String {
        self.pg.pg_ddl()
    }

    pub fn sea_orm_type(&self) -> &str {
        self.pg.sea_orm_type()
    }

    pub fn rust_type_str(&self) -> String {
        self.rust.as_rust_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_pg_text_maps_to_rust_string() {
        let col = ColumnType::from_pg(PgType::Text);
        assert_eq!(col.rust(), &RustType::String);
        assert_eq!(col.pg(), &PgType::Text);
    }

    #[test]
    fn test_with_domain_type_sets_domain_rust_type() {
        let col = ColumnType::with_domain_type(PgType::Text, "CountryCode".into());
        assert_eq!(col.rust(), &RustType::DomainType("CountryCode".to_owned()));
        assert_eq!(col.pg(), &PgType::Text);
    }

    #[test]
    fn test_pg_ddl_delegates_to_pg_type() {
        let col = ColumnType::from_pg(PgType::Uuid);
        assert_eq!(col.pg_ddl(), "UUID");
    }

    #[test]
    fn test_sea_orm_type_delegates_to_pg_type() {
        let col = ColumnType::from_pg(PgType::Boolean);
        assert_eq!(col.sea_orm_type(), "Boolean");
    }

    #[test]
    fn test_rust_type_str_delegates_to_rust_type() {
        let col = ColumnType::from_pg(PgType::BigInt);
        assert_eq!(col.rust_type_str(), "i64");
    }

    #[test]
    fn test_rust_type_str_domain_type() {
        let col = ColumnType::with_domain_type(PgType::Jsonb, "PayRun".into());
        assert_eq!(col.rust_type_str(), "PayRun");
        assert_eq!(col.pg_ddl(), "JSONB");
        assert_eq!(col.sea_orm_type(), "JsonBinary");
    }
}
