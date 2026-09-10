use codegraph_naming::quote_pg_column;

/// Quote a SQL identifier (table or column name) if it is a PostgreSQL reserved word.
/// Returns the identifier with escaped double quotes (`\"`) so it can be safely
/// embedded inside Rust string literals written by the code emitter.
pub(crate) fn q(name: &str) -> String {
    let quoted = quote_pg_column(name);
    if quoted.starts_with('"') {
        // Escape the double quotes for embedding in Rust string literals
        format!("\\\"{}\\\"", &quoted[1..quoted.len() - 1])
    } else {
        quoted
    }
}

/// Returns the sea_orm `Value::*` expression for a typed NULL, based on the Rust type.
pub(crate) fn null_value_for_type(rust_type: &str) -> &str {
    match rust_type {
        "bool" => "sea_orm::Value::Bool(None)",
        "NaiveDate" | "chrono::NaiveDate" => "sea_orm::Value::ChronoDate(None)",
        "DateTime<Utc>" | "chrono::DateTime<chrono::Utc>" => {
            "sea_orm::Value::ChronoDateTimeUtc(None)"
        }
        "Decimal" | "rust_decimal::Decimal" => "sea_orm::Value::Decimal(None)",
        "Uuid" | "uuid::Uuid" => "sea_orm::Value::Uuid(None)",
        "i32" => "sea_orm::Value::Int(None)",
        "i64" => "sea_orm::Value::BigInt(None)",
        "f32" => "sea_orm::Value::Float(None)",
        "f64" => "sea_orm::Value::Double(None)",
        "Vec<String>" => "sea_orm::Value::Array(sea_orm::sea_query::ArrayType::String, None)",
        "serde_json::Value" | "Vec<serde_json::Value>" => "sea_orm::Value::Json(None)",
        _ => "sea_orm::Value::String(None)",
    }
}

/// Returns true if the Rust type implements Copy (no `.clone()` needed).
pub(crate) fn is_copy_type(rust_type: &str) -> bool {
    matches!(
        rust_type,
        "bool"
            | "NaiveDate"
            | "chrono::NaiveDate"
            | "DateTime<Utc>"
            | "chrono::DateTime<chrono::Utc>"
            | "Uuid"
            | "uuid::Uuid"
            | "i32"
            | "i64"
            | "f64"
            | "Decimal"
            | "rust_decimal::Decimal"
    )
}

/// Returns true if the Rust type is `Vec<String>`, requiring special array value conversion.
pub(crate) fn is_vec_string(rust_type: &str) -> bool {
    rust_type == "Vec<String>"
}

/// Returns true if the Rust type is a Vec (any element type).
pub(crate) fn is_vec_type(rust_type: &str) -> bool {
    rust_type.starts_with("Vec<")
}

/// Returns the (SeaORM ArrayType, value constructor) for a Vec element type.
pub(crate) fn vec_array_type_and_ctor(rust_type: &str) -> (&'static str, &'static str) {
    let inner = rust_type
        .strip_prefix("Vec<")
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or("String");
    match inner {
        "NaiveDate" | "chrono::NaiveDate" => (
            "sea_orm::sea_query::ArrayType::ChronoDate",
            "sea_orm::Value::ChronoDate(Some(Box::new(s)))",
        ),
        "DateTime<Utc>" | "chrono::DateTime<chrono::Utc>" => (
            "sea_orm::sea_query::ArrayType::ChronoDateTimeUtc",
            "sea_orm::Value::ChronoDateTimeUtc(Some(Box::new(s)))",
        ),
        "i32" => (
            "sea_orm::sea_query::ArrayType::Int",
            "sea_orm::Value::Int(Some(s))",
        ),
        "i64" => (
            "sea_orm::sea_query::ArrayType::BigInt",
            "sea_orm::Value::BigInt(Some(s))",
        ),
        "f64" => (
            "sea_orm::sea_query::ArrayType::Double",
            "sea_orm::Value::Double(Some(s))",
        ),
        "bool" => (
            "sea_orm::sea_query::ArrayType::Bool",
            "sea_orm::Value::Bool(Some(s))",
        ),
        "serde_json::Value" => (
            "sea_orm::sea_query::ArrayType::Json",
            "sea_orm::Value::Json(Some(Box::new(s)))",
        ),
        _ => (
            "sea_orm::sea_query::ArrayType::String",
            "sea_orm::Value::String(Some(Box::new(s.to_string())))",
        ),
    }
}

/// Converts a Rust type like `Vec<String>` to turbofish form `Vec::<String>` for use in
/// expressions like `Vec::<String>::try_get_by(...)`. Types without generics are returned as-is.
pub(crate) fn turbofish(rust_type: &str) -> String {
    if let Some(idx) = rust_type.find('<') {
        format!("{}::{}", &rust_type[..idx], &rust_type[idx..])
    } else {
        rust_type.to_string()
    }
}

/// Returns an explicit typed `sea_orm::Value` constructor expression for the given Rust type
/// and value expression. This avoids ambiguous `.into()` calls, since `sea_orm::Value` has
/// `From` impls for dozens of types and Rust cannot infer which one to use.
pub(crate) fn typed_value_expr(rust_type: &str, value_expr: &str) -> String {
    match rust_type {
        "bool" => format!("sea_orm::Value::Bool(Some({}))", value_expr),
        "i32" => format!("sea_orm::Value::Int(Some({}))", value_expr),
        "i64" => format!("sea_orm::Value::BigInt(Some({}))", value_expr),
        "f32" => format!("sea_orm::Value::Float(Some({}))", value_expr),
        "f64" => format!("sea_orm::Value::Double(Some({}))", value_expr),
        "String" => format!("sea_orm::Value::String(Some(Box::new({})))", value_expr),
        "NaiveDate" | "chrono::NaiveDate" => {
            format!("sea_orm::Value::ChronoDate(Some(Box::new({})))", value_expr)
        }
        "DateTime<Utc>" | "chrono::DateTime<chrono::Utc>" => {
            format!(
                "sea_orm::Value::ChronoDateTimeUtc(Some(Box::new({})))",
                value_expr
            )
        }
        "Decimal" | "rust_decimal::Decimal" => {
            format!("sea_orm::Value::Decimal(Some(Box::new({})))", value_expr)
        }
        "Uuid" | "uuid::Uuid" => {
            format!("sea_orm::Value::Uuid(Some(Box::new({})))", value_expr)
        }
        "serde_json::Value" => {
            format!("sea_orm::Value::Json(Some(Box::new({})))", value_expr)
        }
        "Vec<serde_json::Value>" => {
            format!(
                "sea_orm::Value::Json(Some(Box::new(serde_json::Value::Array({}))))",
                value_expr
            )
        }
        _ => format!(
            "sea_orm::Value::String(Some(Box::new({}.to_string())))",
            value_expr
        ),
    }
}
