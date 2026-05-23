use serde::{Deserialize, Serialize};

/// Represents every Rust type that the code generator can emit.
///
/// This is a closed enum — every type used in the generated code must have
/// an explicit variant. The `DomainType` variant handles entity/value-object
/// references whose names are only known at generation time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RustType {
    String,
    Uuid,
    Bool,
    I16,
    I32,
    I64,
    Decimal,
    F32,
    F64,
    NaiveDate,
    DateTimeUtc,
    Json,
    VecString,
    VecI64,
    VecI32,
    VecF64,
    VecBool,
    RangeI32,
    RangeI64,
    RangeDateTimeUtc,
    RangeNaiveDate,
    Bytes,
    /// A domain-specific type whose name is resolved at generation time.
    DomainType(std::string::String),
}

impl RustType {
    /// Returns the Rust source-code representation of this type.
    pub fn as_rust_str(&self) -> std::string::String {
        match self {
            RustType::String => "String".to_owned(),
            RustType::Uuid => "Uuid".to_owned(),
            RustType::Bool => "bool".to_owned(),
            RustType::I16 => "i16".to_owned(),
            RustType::I32 => "i32".to_owned(),
            RustType::I64 => "i64".to_owned(),
            RustType::Decimal => "rust_decimal::Decimal".to_owned(),
            RustType::F32 => "f32".to_owned(),
            RustType::F64 => "f64".to_owned(),
            RustType::NaiveDate => "chrono::NaiveDate".to_owned(),
            RustType::DateTimeUtc => "chrono::DateTime<chrono::Utc>".to_owned(),
            RustType::Json => "serde_json::Value".to_owned(),
            RustType::VecString => "Vec<String>".to_owned(),
            RustType::VecI64 => "Vec<i64>".to_owned(),
            RustType::VecI32 => "Vec<i32>".to_owned(),
            RustType::VecF64 => "Vec<f64>".to_owned(),
            RustType::VecBool => "Vec<bool>".to_owned(),
            RustType::RangeI32 => "std::ops::Range<i32>".to_owned(),
            RustType::RangeI64 => "std::ops::Range<i64>".to_owned(),
            RustType::RangeDateTimeUtc => {
                "std::ops::Range<chrono::DateTime<chrono::Utc>>".to_owned()
            }
            RustType::RangeNaiveDate => "std::ops::Range<chrono::NaiveDate>".to_owned(),
            RustType::Bytes => "Vec<u8>".to_owned(),
            RustType::DomainType(name) => name.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_as_rust_str_primitives() {
        assert_eq!(RustType::String.as_rust_str(), "String");
        assert_eq!(RustType::Uuid.as_rust_str(), "Uuid");
        assert_eq!(RustType::Bool.as_rust_str(), "bool");
        assert_eq!(RustType::I16.as_rust_str(), "i16");
        assert_eq!(RustType::I32.as_rust_str(), "i32");
        assert_eq!(RustType::I64.as_rust_str(), "i64");
        assert_eq!(RustType::F32.as_rust_str(), "f32");
        assert_eq!(RustType::F64.as_rust_str(), "f64");
    }

    #[test]
    fn test_as_rust_str_qualified_types() {
        assert_eq!(RustType::Decimal.as_rust_str(), "rust_decimal::Decimal");
        assert_eq!(RustType::NaiveDate.as_rust_str(), "chrono::NaiveDate");
        assert_eq!(
            RustType::DateTimeUtc.as_rust_str(),
            "chrono::DateTime<chrono::Utc>"
        );
        assert_eq!(RustType::Json.as_rust_str(), "serde_json::Value");
    }

    #[test]
    fn test_as_rust_str_collections() {
        assert_eq!(RustType::VecString.as_rust_str(), "Vec<String>");
        assert_eq!(RustType::VecI64.as_rust_str(), "Vec<i64>");
        assert_eq!(RustType::VecI32.as_rust_str(), "Vec<i32>");
        assert_eq!(RustType::VecF64.as_rust_str(), "Vec<f64>");
        assert_eq!(RustType::VecBool.as_rust_str(), "Vec<bool>");
        assert_eq!(RustType::Bytes.as_rust_str(), "Vec<u8>");
    }

    #[test]
    fn test_as_rust_str_ranges() {
        assert_eq!(RustType::RangeI32.as_rust_str(), "std::ops::Range<i32>");
        assert_eq!(RustType::RangeI64.as_rust_str(), "std::ops::Range<i64>");
        assert_eq!(
            RustType::RangeDateTimeUtc.as_rust_str(),
            "std::ops::Range<chrono::DateTime<chrono::Utc>>"
        );
        assert_eq!(
            RustType::RangeNaiveDate.as_rust_str(),
            "std::ops::Range<chrono::NaiveDate>"
        );
    }

    #[test]
    fn test_as_rust_str_domain_type() {
        let dt = RustType::DomainType("PersonType".to_owned());
        assert_eq!(dt.as_rust_str(), "PersonType");
    }
}
