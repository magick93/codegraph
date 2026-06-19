use std::fmt::Write;

use codegraph_core::traits::GraphQuerier;
use codegraph_naming::quote_pg_column;
use codegraph_type_contracts::RefClassificationKind;

use crate::error::Result;
use crate::generate::api::include_path::ResolvedIncludePath;
use crate::generate::filter_fields::{
    resolve_filter_fields, resolve_nested_filter_fields, FilterFieldInfo, NestedFilterFieldInfo,
};
use crate::generate::type_registry;
use codegraph_config::DomainConfig;

/// Quote a SQL identifier (table or column name) if it is a PostgreSQL reserved word.
/// Returns the identifier with escaped double quotes (`\"`) so it can be safely
/// embedded inside Rust string literals written by the code emitter.
fn q(name: &str) -> String {
    let quoted = quote_pg_column(name);
    if quoted.starts_with('"') {
        // Escape the double quotes for embedding in Rust string literals
        format!("\\\"{}\\\"", &quoted[1..quoted.len() - 1])
    } else {
        quoted
    }
}

/// Resolve worker detail JOINs from the parent entity's DDL-defined child tables.
/// Uses the entity's module/table name to derive child table names following
/// the DDL naming convention: {parent_table}_person, {parent_table}_person_name.
async fn resolve_worker_detail_joins(
    db: &dyn GraphQuerier,
    parent_entity_name: &str,
) -> Vec<(String, String, String)> {
    let parent_schema = match db.get_schema(parent_entity_name).await {
        Ok(Some(s)) => s,
        _ => return Vec::new(),
    };
    let parent_table = parent_schema.pg_table_name;
    let schema = parent_schema.domain.as_deref().unwrap_or("public");
    // DDL convention for WorkerType → worker → worker_person → worker_person_name:
    // parent_table = "worker"
    // person_table = "common.worker_person", FK: worker_id
    // name_table = "common.worker_person_name", FK: worker_person_id
    let person_table = format!("{}.{}_{}", schema, parent_table, "person");
    let person_fk = format!("{}_id", parent_table);
    let name_table = format!("{}.{}_{}_{}", schema, parent_table, "person", "name");
    let name_fk = format!("{}_{}_{}", parent_table, "person", "id");
    let mut joins = Vec::new();
    joins.push((name_table, name_fk, "wp".to_string()));
    joins.push((person_table, person_fk, "w".to_string()));
    joins.reverse();
    joins
}

/// Maximum nesting depth for recursive child table building.
/// Prevents stack overflow on deeply-nested or degenerate schemas.
const MAX_CHILD_DEPTH: usize = 10;

/// Resolved tree_include entry with concrete table/column names.
#[derive(Debug)]
struct TreeIncludeResolved {
    /// Response field alias (e.g. "deployed_worker").
    alias: String,
    /// Schema-qualified via table name (e.g. "common.deployment").
    via_table: String,
    /// FK column on via_entity referencing the hierarchy entity (e.g. "position_id").
    via_fk_column: String,
    /// Schema-qualified parent table name (e.g. "common.worker").
    parent_table: String,
    /// FK column on via_entity referencing its parent (e.g. "worker_type_id").
    parent_ref_column: String,
    /// Worker detail JOIN chain: (schema.table, fk_column_on_child, referenced_column_on_parent).
    /// Built from the composition tree of the parent entity.
    worker_detail_joins: Vec<(String, String, String)>,
}

/// Tree representation of an entity's value object structure.
#[allow(dead_code)]
#[derive(Debug)]
struct EntityTree {
    entity_name: String,
    module_name: String,
    schema_name: String,
    table_name: String,
    /// Domain-prefixed entity module name: `{schema_name}_{table_name}`.
    entity_module: String,
    direct_columns: Vec<TreeColumn>,
    child_tables: Vec<ChildTableInfo>,
    has_create: bool,
    has_update: bool,
    has_delete: bool,
    has_workflow: bool,
    has_fts: bool,
    has_embeddings: bool,
    fts_language: String,
    is_auditable: bool,
    filter_fields: Vec<FilterFieldInfo>,
    nested_filter_fields: Vec<NestedFilterFieldInfo>,
    /// FK column for parent-scoped lookups (child entities only).
    parent_ref: Option<String>,
    /// Self-referential FK column for tree/hierarchy queries (e.g. "parent_id").
    hierarchy_field: Option<String>,
    /// Resolved tree_include entries for JOIN-ing related data into tree responses.
    tree_include: Vec<TreeIncludeResolved>,
}

#[allow(dead_code)]
#[derive(Debug)]
struct TreeColumn {
    /// Entity model field name (e.g. `gender_code` for codelist refs).
    field_name: String,
    /// PostgreSQL column name (e.g. `gender_code`). May differ from `field_name`
    /// when the snake_case name is a Rust keyword (field gets `r#` prefix).
    pg_column_name: String,
    /// DTO field name — the API-facing name (e.g. `gender`).
    /// When None, same as `field_name`.
    dto_field_name: Option<String>,
    rust_type: String,
    is_nullable: bool,
    /// Whether this column is an entity reference FK (excluded from Response DTO construction)
    is_entity_ref: bool,
    /// When the DTO uses a different type than the entity column (e.g. codelist enum
    /// `CurrencyCodeList` vs entity `String`), this holds the DTO type name.
    /// The emitter uses `.to_string()` for DTO→entity and `.parse()` for entity→DTO.
    dto_rust_type: Option<String>,
    /// Whether this column is a workflow-managed field (status, approval_status).
    /// These are excluded from create/update but included in responses.
    is_workflow_managed: bool,
    /// Whether this is an array column (Vec<T>). When true AND dto_rust_type is set,
    /// conversion needs .into_iter().map(|v| v.to_string()).collect() instead of .to_string().
    is_array: bool,
    /// When this column is a PostgreSQL range type, holds the lowercased PG cast
    /// (e.g. `"tstzrange"`) so INSERT/UPDATE SQL can include `$N::tstzrange`.
    pg_cast: Option<String>,
    /// True when this column was synthesised by composite-range collapsing
    /// (start/end → TSTZRANGE). These columns exist in the entity model and DDL
    /// but are NOT present on DTOs, so create/update/response must skip them.
    is_composite_range: bool,
    /// True when this column is a StructuredWrapper field stored as JSONB.
    /// The entity model holds `serde_json::Value` but the DTO may use
    /// `Vec<serde_json::Value>` when the property is an array. Emits
    /// serialization/deserialization conversions between the two.
    is_structured_wrapper: bool,
    /// True when this column is a media URL/MIME-type field managed by the
    /// dedicated upload/download handlers. Included in responses but excluded
    /// from create/update commands since media is set via separate endpoints.
    is_media: bool,
}

impl TreeColumn {
    /// Returns the DTO field name (falls back to `field_name`).
    fn dto_name(&self) -> &str {
        self.dto_field_name.as_deref().unwrap_or(&self.field_name)
    }
}

/// Emit the mapping expression for a single entity column → DTO field.
///
/// Handles codelist enum parsing, JSONB deserialization, and plain copy.
/// `pad` is the indentation prefix (e.g. `"            "`).
/// `row_var` is the variable name holding the entity row (e.g. `"row"`).
fn emit_entity_to_dto_field(code: &mut String, col: &TreeColumn, row_var: &str, pad: &str) {
    let dto_field = col.dto_name();
    let entity_field = &col.field_name;
    if col.dto_rust_type.is_some() {
        if col.is_array {
            if col.is_nullable {
                writeln!(
                    code,
                    "{pad}{dto_field}: {row_var}.{entity_field}.map(|v| v.into_iter().filter_map(|x| x.parse().ok()).collect()),",
                ).unwrap();
            } else {
                writeln!(
                    code,
                    "{pad}{dto_field}: {row_var}.{entity_field}.into_iter().filter_map(|v| v.parse().ok()).collect(),",
                ).unwrap();
            }
        } else if col.is_nullable {
            writeln!(
                code,
                "{pad}{dto_field}: {row_var}.{entity_field}.and_then(|v| v.parse().ok()),",
            )
            .unwrap();
        } else {
            writeln!(
                code,
                "{pad}{dto_field}: {row_var}.{entity_field}.parse().unwrap_or_default(),",
            )
            .unwrap();
        }
    } else if col.is_structured_wrapper {
        // Both array and scalar StructuredWrapper use the same serde_json conversion.
        if col.is_nullable {
            writeln!(
                code,
                "{pad}{dto_field}: {row_var}.{entity_field}.and_then(|v| serde_json::from_value(v).ok()),",
            ).unwrap();
        } else {
            writeln!(
                code,
                "{pad}{dto_field}: serde_json::from_value({row_var}.{entity_field}).unwrap_or_default(),",
            ).unwrap();
        }
    } else {
        writeln!(code, "{pad}{dto_field}: {row_var}.{entity_field},").unwrap();
    }
}

/// Emit child table field assignments into a DTO struct literal.
///
/// For array children: `field: field_rows,`
/// For single children: `field: field_rows.into_iter().next(),`
fn emit_child_field_population(code: &mut String, children: &[ChildTableInfo], pad: &str) {
    for child in children {
        if child.is_array {
            writeln!(
                code,
                "{pad}{field}: {field}_rows,",
                field = child.field_name
            )
            .unwrap();
        } else {
            writeln!(
                code,
                "{pad}{field}: {field}_rows.into_iter().next(),",
                field = child.field_name
            )
            .unwrap();
        }
    }
}

/// Emit the response struct construction shared by `find_by_id` and `find_by_id_scoped`.
fn emit_response_construction(code: &mut String, tree: &EntityTree) {
    emit_child_reads(code, &tree.child_tables, "id", 2);
    writeln!(code).unwrap();
    writeln!(code, "        Ok(Some({}Response {{", tree.entity_name).unwrap();
    writeln!(code, "            id: row.id,").unwrap();
    for col in &tree.direct_columns {
        if col.is_composite_range {
            continue;
        }
        emit_entity_to_dto_field(code, col, "row", "            ");
    }
    emit_child_field_population(code, &tree.child_tables, "            ");
    if tree.has_workflow {
        writeln!(code, "            workflow_state: None,").unwrap();
    }
    writeln!(code, "            created_at: row.created_at,").unwrap();
    writeln!(code, "            updated_at: row.updated_at,").unwrap();
    writeln!(code, "        }}))").unwrap();
    writeln!(code, "    }}").unwrap();
}

/// Tracks a child (value object) table that the repository must persist and read.
#[allow(dead_code)]
#[derive(Debug)]
struct ChildTableInfo {
    /// Rust field name on parent DTO (e.g. "person_name")
    field_name: String,
    /// DTO struct name prefix (e.g. "CandidatePersonName")
    struct_name: String,
    /// SQL table name (e.g. "candidate_person_name")
    sql_table_name: String,
    /// SQL schema name (e.g. "recruiting")
    sql_schema_name: String,
    /// Parent FK column (e.g. "candidate_id")
    parent_fk_column: String,
    /// Whether this child is an array (Vec) or single (Option)
    is_array: bool,
    /// Columns in the child table (excluding id and parent FK)
    columns: Vec<ChildColumn>,
    /// Nested child tables (ValueObject properties within this child table)
    child_tables: Vec<ChildTableInfo>,
}

#[allow(dead_code)]
#[derive(Debug)]
struct ChildColumn {
    field_name: String,
    pg_column_name: String,
    rust_type: String,
    /// When the DTO field name differs from the entity field name
    /// (e.g. entity `person_name_code` → DTO `person_name`), holds
    /// the DTO-side name. Falls back to `field_name`.
    dto_field_name: Option<String>,
    is_nullable: bool,
    /// When the DTO uses a different type than the entity column (e.g. codelist enum
    /// `GenderCodeList` vs entity `String`), this holds the DTO type name.
    dto_rust_type: Option<String>,
    /// When this column is a PostgreSQL range type, holds the lowercased PG cast
    /// (e.g. `"tstzrange"`) so INSERT SQL can include `$N::tstzrange`.
    pg_cast: Option<String>,
}

impl ChildColumn {
    fn dto_name(&self) -> &str {
        self.dto_field_name.as_deref().unwrap_or(&self.field_name)
    }
}

/// Returns the sea_orm `Value::*` expression for a typed NULL, based on the Rust type.
fn null_value_for_type(rust_type: &str) -> &str {
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
fn is_copy_type(rust_type: &str) -> bool {
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
fn is_vec_string(rust_type: &str) -> bool {
    rust_type == "Vec<String>"
}

/// Returns true if the Rust type is a Vec (any element type).
fn is_vec_type(rust_type: &str) -> bool {
    rust_type.starts_with("Vec<")
}

/// Returns the (SeaORM ArrayType, value constructor) for a Vec element type.
fn vec_array_type_and_ctor(rust_type: &str) -> (&'static str, &'static str) {
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
        _ => (
            "sea_orm::sea_query::ArrayType::String",
            "sea_orm::Value::String(Some(Box::new(s.to_string())))",
        ),
    }
}

/// Converts a Rust type like `Vec<String>` to turbofish form `Vec::<String>` for use in
/// expressions like `Vec::<String>::try_get_by(...)`. Types without generics are returned as-is.
fn turbofish(rust_type: &str) -> String {
    if let Some(idx) = rust_type.find('<') {
        format!("{}::{}", &rust_type[..idx], &rust_type[idx..])
    } else {
        rust_type.to_string()
    }
}

use crate::generate::pg_cast_for_type;

/// Returns an explicit typed `sea_orm::Value` constructor expression for the given Rust type
/// and value expression. This avoids ambiguous `.into()` calls, since `sea_orm::Value` has
/// `From` impls for dozens of types and Rust cannot infer which one to use.
fn typed_value_expr(rust_type: &str, value_expr: &str) -> String {
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

/// Emit type-safe value parsing for a nested filter field and return the
/// Rust expression that holds the parsed value (e.g. `"parsed"` or `"val.clone()"`).
///
/// Covers the same type surface as `typed_value_expr()` used for direct-column filters:
/// Uuid, i32, i64, f32, f64, bool, Decimal, NaiveDate, DateTime<Utc>, and String fallback.
fn emit_nested_filter_parse(code: &mut String, nf: &NestedFilterFieldInfo) -> &'static str {
    let key = &nf.filter_key;
    match nf.rust_type.as_str() {
        "Uuid" | "uuid::Uuid" => {
            writeln!(
                code,
                "            let parsed = uuid::Uuid::parse_str(val).map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid UUID for filter '{key}': {{e}}\")))?;",
            )
            .unwrap();
            "parsed"
        }
        "i32" => {
            writeln!(
                code,
                "            let parsed: i32 = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid i32 for filter '{key}': {{e}}\")))?;",
            )
            .unwrap();
            "parsed"
        }
        "i64" => {
            writeln!(
                code,
                "            let parsed: i64 = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid i64 for filter '{key}': {{e}}\")))?;",
            )
            .unwrap();
            "parsed"
        }
        "f32" => {
            writeln!(
                code,
                "            let parsed: f32 = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid f32 for filter '{key}': {{e}}\")))?;",
            )
            .unwrap();
            "parsed"
        }
        "f64" => {
            writeln!(
                code,
                "            let parsed: f64 = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid f64 for filter '{key}': {{e}}\")))?;",
            )
            .unwrap();
            "parsed"
        }
        "bool" => {
            writeln!(
                code,
                "            let parsed: bool = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid bool for filter '{key}': {{e}}\")))?;",
            )
            .unwrap();
            "parsed"
        }
        "Decimal" | "rust_decimal::Decimal" => {
            writeln!(
                code,
                "            let parsed: rust_decimal::Decimal = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid Decimal for filter '{key}': {{e}}\")))?;",
            )
            .unwrap();
            "parsed"
        }
        "NaiveDate" | "chrono::NaiveDate" => {
            writeln!(
                code,
                "            let parsed = chrono::NaiveDate::parse_from_str(val, \"%Y-%m-%d\").map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid date for filter '{key}': {{e}}\")))?;",
            )
            .unwrap();
            "parsed"
        }
        "DateTime<Utc>" | "chrono::DateTime<chrono::Utc>" => {
            writeln!(
                code,
                "            let parsed = val.parse::<chrono::DateTime<chrono::Utc>>().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid datetime for filter '{key}': {{e}}\")))?;",
            )
            .unwrap();
            "parsed"
        }
        _ => {
            // String and everything else — pass through directly.
            "val.clone()"
        }
    }
}

/// Emit a single child column value expression for an INSERT statement.
///
/// When `dto_rust_type` is set, the DTO field is a codelist enum that needs
/// `.to_string()` before being stored as a String column.
fn emit_child_col_write_value(code: &mut String, col: &ChildColumn) {
    let clone_suffix = if is_copy_type(&col.rust_type) {
        ""
    } else {
        ".clone()"
    };
    let has_enum = col.dto_rust_type.is_some();
    let field = col.dto_name();

    if col.is_nullable {
        if is_vec_string(&col.rust_type) || (is_vec_type(&col.rust_type) && has_enum) {
            // Vec<String> or Vec<CodelistType> — store as TEXT[] with element-level conversion
            let map_fn = if is_vec_string(&col.rust_type) {
                "s"
            } else {
                "s.to_string()"
            };
            write!(
                code,
                ", item.{field}.clone().map(|v| sea_orm::Value::Array(sea_orm::sea_query::ArrayType::String, Some(Box::new(v.into_iter().map(|s| sea_orm::Value::String(Some(Box::new({map_fn})))).collect())))).unwrap_or({null})",
                null = null_value_for_type("Vec<String>"),
            )
            .unwrap();
        } else if is_vec_type(&col.rust_type) {
            // Vec<NaiveDate> or other non-string Vec — use typed array
            let (array_type, value_ctor) = vec_array_type_and_ctor(&col.rust_type);
            write!(
                code,
                ", item.{field}.clone().map(|v| sea_orm::Value::Array({array_type}, Some(Box::new(v.into_iter().map(|s| {value_ctor}).collect())))).unwrap_or(sea_orm::Value::Array({array_type}, None))",
            )
            .unwrap();
        } else if has_enum {
            write!(
                code,
                ", item.{field}.as_ref().map(|v| sea_orm::Value::String(Some(Box::new(v.to_string())))).unwrap_or({null})",
                null = null_value_for_type(&col.rust_type),
            )
            .unwrap();
        } else {
            let typed_value = typed_value_expr(&col.rust_type, "v");
            write!(
                code,
                ", item.{field}{clone}.map(|v| {typed_value}).unwrap_or({null})",
                clone = clone_suffix,
                typed_value = typed_value,
                null = null_value_for_type(&col.rust_type),
            )
            .unwrap();
        }
    } else if is_vec_string(&col.rust_type) || (is_vec_type(&col.rust_type) && has_enum) {
        let map_fn = if is_vec_string(&col.rust_type) {
            "s"
        } else {
            "s.to_string()"
        };
        write!(
            code,
            ", sea_orm::Value::Array(sea_orm::sea_query::ArrayType::String, Some(Box::new(item.{field}.clone().into_iter().map(|s| sea_orm::Value::String(Some(Box::new({map_fn})))).collect())))",
        )
        .unwrap();
    } else if is_vec_type(&col.rust_type) {
        let (array_type, value_ctor) = vec_array_type_and_ctor(&col.rust_type);
        write!(
            code,
            ", sea_orm::Value::Array({array_type}, Some(Box::new(item.{field}.clone().into_iter().map(|s| {value_ctor}).collect())))",
        )
        .unwrap();
    } else if has_enum {
        write!(
            code,
            ", sea_orm::Value::String(Some(Box::new(item.{field}.to_string())))",
        )
        .unwrap();
    } else {
        let item_expr = format!("item.{}{}", field, clone_suffix);
        let typed_value = typed_value_expr(&col.rust_type, &item_expr);
        write!(code, ", {}", typed_value).unwrap();
    }
}

/// Recursively build a `ChildTableInfo` for a ValueObject property.
/// Resolves the target schema, classifies its properties, and recurses
/// for any nested ValueObject properties (creating nested child tables).
#[allow(clippy::too_many_arguments)]
async fn build_child_table_info(
    db: &dyn GraphQuerier,
    prop: &codegraph_core::types::PropertyNode,
    parent_schema_title: &str,
    parent_table_name: &str,
    schema_name: &str,
    parent_struct_name: &str,
    visited: &mut std::collections::HashSet<String>,
    depth: usize,
    suffix: &str,
) -> Option<ChildTableInfo> {
    if depth >= MAX_CHILD_DEPTH {
        return None;
    }

    // Resolve the target schema (handles array vs non-array)
    let target = if prop.is_array {
        db.get_array_item_schema(&prop.name, parent_schema_title)
            .await
            .ok()
            .flatten()
    } else {
        db.get_property_ref_target(&prop.name, parent_schema_title)
            .await
            .ok()
            .flatten()
    };

    let target_schema = target?;

    // Cycle guard
    if !visited.insert(target_schema.title.clone()) {
        return None;
    }

    let prop_field_def = codegraph_core::types::resolve_field(prop);

    let raw_child_props = db
        .get_properties(&target_schema.title)
        .await
        .unwrap_or_default();
    let child_props = {
        let mut seen = std::collections::HashSet::new();
        raw_child_props
            .into_iter()
            .filter(|p| p.rust_field_name != "id" && seen.insert(p.rust_field_name.clone()))
            .collect::<Vec<_>>()
    };

    let child_table_name = codegraph_naming::truncate_pg_identifier(&format!(
        "{}_{}",
        parent_table_name, prop_field_def.column_name
    ));
    let child_struct_name = format!(
        "{}{}",
        parent_struct_name,
        codegraph_naming::strip_suffix(&target_schema.rust_type_name, suffix)
    );

    let mut child_columns: Vec<ChildColumn> = Vec::new();
    let mut nested_child_tables: Vec<ChildTableInfo> = Vec::new();

    // Composite range: collapse start/end fields into a single range column (same as DDL generator)
    let consumed_fields: std::collections::HashSet<String> = db
        .get_consumed_fields(&target_schema.title)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(prop, _role)| prop.name)
        .collect();
    let composite_range = db
        .get_composite_range(&target_schema.title)
        .await
        .ok()
        .flatten();
    if let Some(ref range) = composite_range {
        child_columns.push(ChildColumn {
            field_name: range.pg_column_name.clone(),
            pg_column_name: range.pg_column_name.clone(),
            rust_type: "String".to_string(),
            is_nullable: true,
            dto_rust_type: None,
            pg_cast: pg_cast_for_type(&range.pg_type),
        });
    }

    for c in child_props
        .iter()
        .filter(|c| c.pg_column_name != "id" && !consumed_fields.contains(&c.name))
    {
        let field_def = codegraph_core::types::resolve_field(c);
        match c.effective_kind() {
            Some(RefClassificationKind::CodelistReference)
            | Some(RefClassificationKind::CodelistCheck) => {
                let enum_name =
                    crate::generate::ddd::dto::codelist_enum_name_from_ref(&c.ref_target);
                if c.is_array {
                    // Codelist array within a child VO → nested child table
                    let nested_table = codegraph_naming::truncate_pg_identifier(&format!(
                        "{}_{}",
                        child_table_name, c.pg_column_name
                    ));
                    let nested_struct = format!(
                        "{}{}",
                        child_struct_name,
                        codegraph_naming::to_pascal_case(&c.rust_field_name)
                    );
                    nested_child_tables.push(ChildTableInfo {
                        field_name: field_def.rust_field_name.clone(),
                        struct_name: nested_struct,
                        sql_table_name: nested_table,
                        sql_schema_name: schema_name.to_string(),
                        parent_fk_column: codegraph_naming::truncate_pg_identifier(&format!(
                            "{}_id",
                            child_table_name
                        )),
                        is_array: true,
                        columns: vec![ChildColumn {
                            field_name: "code".to_string(),
                            pg_column_name: "code".to_string(),
                            rust_type: "String".to_string(),
                            is_nullable: false,
                            dto_rust_type: enum_name,
                            dto_field_name: None,
                            pg_cast: None,
                        }],
                        child_tables: vec![],
                    });
                } else {
                    let stripped =
                        crate::generate::ddd::dto::strip_code_suffix_safe(
                            &field_def.rust_field_name,
                        );
                    let child_dto_name = if stripped != field_def.rust_field_name {
                        Some(stripped)
                    } else {
                        None
                    };
                    child_columns.push(ChildColumn {
                        field_name: field_def.rust_field_name.clone(),
                        pg_column_name: field_def.column_name.clone(),
                        rust_type: "String".to_string(),
                        is_nullable: !c.is_required,
                        dto_rust_type: enum_name,
                        dto_field_name: child_dto_name,
                        pg_cast: None,
                    });
                }
            }
            Some(RefClassificationKind::PrimitiveWrapper)
            | Some(RefClassificationKind::ArrayWrapper)
            | Some(RefClassificationKind::RangeWrapper)
            | Some(RefClassificationKind::InlineEnum) => {
                let pg_cast = if c.effective_kind() == Some(RefClassificationKind::RangeWrapper) {
                    pg_cast_for_type(&c.pg_column_type)
                } else {
                    None
                };
                child_columns.push(ChildColumn {
                    field_name: field_def.rust_field_name.clone(),
                    pg_column_name: field_def.column_name.clone(),
                    rust_type: c.rust_field_type.clone(),
                    is_nullable: !c.is_required,
                    dto_rust_type: None,
                    pg_cast,
                });
            }
            Some(RefClassificationKind::EntityReference) => {
                child_columns.push(ChildColumn {
                    field_name: field_def.rust_field_name,
                    pg_column_name: field_def.column_name,
                    rust_type: "Uuid".to_string(),
                    is_nullable: true,
                    dto_rust_type: None,
                    pg_cast: None,
                });
            }
            Some(RefClassificationKind::CompositeWrapper)
            | Some(RefClassificationKind::MediaWrapper) => {
                if let Ok(comp_cols) = db
                    .get_composite_columns(&c.name, &target_schema.title)
                    .await
                {
                    for col in &comp_cols {
                        let dto_rust_type = col
                            .dto_rust_type
                            .as_ref()
                            .filter(|dt| *dt != &col.rust_type)
                            .cloned();
                        let pg_cast = pg_cast_for_type(&col.pg_type);
                        child_columns.push(ChildColumn {
                            field_name: format!("{}{}", field_def.rust_field_name, col.suffix),
                            pg_column_name: format!("{}{}", field_def.column_name, col.suffix),
                            rust_type: col.rust_type.clone(),
                            is_nullable: !c.is_required,
                            dto_rust_type,
                            pg_cast,
                        });
                    }
                }
            }
            Some(RefClassificationKind::StructuredWrapper) => {
                // StructuredWrappers are stored as a single JSONB column inline.
                child_columns.push(ChildColumn {
                    field_name: field_def.rust_field_name.clone(),
                    pg_column_name: field_def.column_name.clone(),
                    rust_type: "serde_json::Value".to_string(),
                    is_nullable: !c.is_required,
                    dto_rust_type: None,
                    pg_cast: None,
                });
            }
            Some(RefClassificationKind::ValueObject) => {
                // Recurse: nested ValueObjects become nested child tables
                let nested = Box::pin(build_child_table_info(
                    db,
                    c,
                    &target_schema.title,
                    &child_table_name,
                    schema_name,
                    &child_struct_name,
                    visited,
                    depth + 1,
                    suffix,
                ))
                .await;
                if let Some(nested_info) = nested {
                    nested_child_tables.push(nested_info);
                }
            }
            None => {
                let t = &c.rust_field_type;
                if t.contains("::")
                    || t.starts_with("Vec<")
                    || matches!(
                        t.as_str(),
                        "String" | "bool" | "i16" | "i32" | "i64" | "f32" | "f64" | "u32" | "u64"
                    )
                {
                    child_columns.push(ChildColumn {
                        field_name: field_def.rust_field_name.clone(),
                        pg_column_name: field_def.column_name.clone(),
                        rust_type: t.clone(),
                        is_nullable: !c.is_required,
                        dto_rust_type: None,
                        pg_cast: None,
                    });
                }
            }
        }
    }

    // Deduplicate child_columns by field_name
    {
        let mut seen_fields = std::collections::HashSet::new();
        child_columns.retain(|c| seen_fields.insert(c.field_name.clone()));
    }

    Some(ChildTableInfo {
        field_name: prop_field_def.rust_field_name.clone(),
        struct_name: child_struct_name,
        sql_table_name: child_table_name,
        sql_schema_name: schema_name.to_string(),
        parent_fk_column: codegraph_naming::truncate_pg_identifier(&format!("{}_id", parent_table_name)),
        is_array: prop.is_array,
        columns: child_columns,
        child_tables: nested_child_tables,
    })
}

/// Flatten a nested child table tree into a depth-first ordered list.
/// Each entry retains its correct `parent_fk_column` and `sql_table_name`.
fn flatten_child_tables(children: &[ChildTableInfo]) -> Vec<&ChildTableInfo> {
    let mut result = Vec::new();
    for child in children {
        result.push(child);
        result.extend(flatten_child_tables(&child.child_tables));
    }
    result
}

/// Recursively emit INSERT statements for child tables and their nested children.
///
/// * `parent_id_var` — Rust variable name holding the parent's UUID (e.g. `"id"`, `"child_id"`)
/// * `item_accessor` — DTO access prefix (e.g. `"cmd"`, `"item"`)
/// * `indent` — indentation level (number of 4-space units)
fn emit_child_inserts(
    code: &mut String,
    children: &[ChildTableInfo],
    parent_id_var: &str,
    item_accessor: &str,
    indent: usize,
) {
    let pad = "    ".repeat(indent);
    for child in children {
        // Skip child tables with no data columns — nothing meaningful to insert
        if child.columns.is_empty() && child.child_tables.is_empty() {
            continue;
        }

        writeln!(code).unwrap();
        let col_names: Vec<String> = child.columns.iter().map(|c| q(&c.pg_column_name)).collect();
        let placeholders: Vec<String> = child
            .columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                if let Some(ref cast) = col.pg_cast {
                    if crate::generate::is_geometry_cast(cast) {
                        format!("ST_GeomFromGeoJSON(${})", i + 3)
                    } else {
                        format!("${}::{}", i + 3, cast)
                    }
                } else {
                    format!("${}", i + 3)
                }
            })
            .collect();
        let sql = if col_names.is_empty() {
            format!(
                "INSERT INTO {}.{} (id, {}) VALUES ($1, $2)",
                child.sql_schema_name,
                q(&child.sql_table_name),
                child.parent_fk_column,
            )
        } else {
            format!(
                "INSERT INTO {}.{} (id, {}, {}) VALUES ($1, $2, {})",
                child.sql_schema_name,
                q(&child.sql_table_name),
                child.parent_fk_column,
                col_names.join(", "),
                placeholders.join(", "),
            )
        };

        // Use a unique variable name for this level's row ID to prevent
        // shadowing when grandchild inserts reference the parent FK.
        let row_id_var = format!("child_id_{}", child.sql_table_name.replace('.', "_"));

        if child.is_array {
            writeln!(
                code,
                "{pad}// Insert child rows: {}.{}",
                child.sql_schema_name, child.sql_table_name
            )
            .unwrap();
            let item_var = if child.columns.is_empty() && child.child_tables.is_empty() {
                "_item"
            } else {
                "item"
            };
            writeln!(
                code,
                "{pad}for {item_var} in &{item_accessor}.{field} {{",
                field = child.field_name
            )
            .unwrap();
            writeln!(code, "{pad}    let {row_id_var} = Uuid::new_v4();").unwrap();
            writeln!(code, "{pad}    let stmt = Statement::from_sql_and_values(").unwrap();
            writeln!(code, "{pad}        DatabaseBackend::Postgres,").unwrap();
            writeln!(code, "{pad}        \"{sql}\",").unwrap();
            write!(
                code,
                "{pad}        vec![{row_id_var}.into(), {parent_id_var}.into()"
            )
            .unwrap();
            for col in &child.columns {
                emit_child_col_write_value(code, col);
            }
            writeln!(code, "],").unwrap();
            writeln!(code, "{pad}    );").unwrap();
            writeln!(code, "{pad}    tx.execute(stmt).await?;").unwrap();
            emit_child_inserts(code, &child.child_tables, &row_id_var, "item", indent + 1);
            writeln!(code, "{pad}}}").unwrap();
        } else {
            writeln!(
                code,
                "{pad}// Insert optional child row: {}.{}",
                child.sql_schema_name, child.sql_table_name
            )
            .unwrap();
            let item_var = if child.columns.is_empty() && child.child_tables.is_empty() {
                "_item"
            } else {
                "item"
            };
            writeln!(
                code,
                "{pad}if let Some(ref {item_var}) = {item_accessor}.{field} {{",
                field = child.field_name
            )
            .unwrap();
            writeln!(code, "{pad}    let {row_id_var} = Uuid::new_v4();").unwrap();
            writeln!(code, "{pad}    let stmt = Statement::from_sql_and_values(").unwrap();
            writeln!(code, "{pad}        DatabaseBackend::Postgres,").unwrap();
            writeln!(code, "{pad}        \"{sql}\",").unwrap();
            write!(
                code,
                "{pad}        vec![{row_id_var}.into(), {parent_id_var}.into()"
            )
            .unwrap();
            for col in &child.columns {
                emit_child_col_write_value(code, col);
            }
            writeln!(code, "],").unwrap();
            writeln!(code, "{pad}    );").unwrap();
            writeln!(code, "{pad}    tx.execute(stmt).await?;").unwrap();
            emit_child_inserts(code, &child.child_tables, &row_id_var, "item", indent + 1);
            writeln!(code, "{pad}}}").unwrap();
        }
    }
}

/// Recursively emit SELECT + Response-building code for child tables.
///
/// For each child table, emits code that:
/// 1. Queries child rows by parent FK
/// 2. For each child row, recursively queries nested grandchild tables
/// 3. Builds the child Response struct including nested children
///
/// * `parent_id_expr` — Rust expression for the parent row's ID (e.g. `"id"`, `"row.id"`)
/// * `indent` — indentation level (number of 4-space units)
fn emit_child_reads(
    code: &mut String,
    children: &[ChildTableInfo],
    parent_id_expr: &str,
    indent: usize,
) {
    let pad = "    ".repeat(indent);
    for child in children {
        writeln!(code).unwrap();

        // Child tables with no data columns and no nested children — emit empty vec.
        if child.columns.is_empty() && child.child_tables.is_empty() {
            writeln!(
                code,
                "{pad}let {field}_rows: Vec<{struct_name}Response> = Vec::new();",
                field = child.field_name,
                struct_name = child.struct_name,
            )
            .unwrap();
            continue;
        }

        // Child tables with no data columns but with nested children —
        // still need to query for ids to resolve grandchildren.
        let col_names: Vec<String> = child
            .columns
            .iter()
            .map(|c| {
                if let Some(ref cast) = c.pg_cast {
                    if crate::generate::is_geometry_cast(cast) {
                        format!(
                            "ST_AsGeoJSON({})::text AS {}",
                            q(&c.pg_column_name),
                            q(&c.pg_column_name)
                        )
                    } else {
                        format!(
                            "{}::{} AS {}",
                            q(&c.pg_column_name),
                            cast,
                            q(&c.pg_column_name)
                        )
                    }
                } else {
                    q(&c.pg_column_name)
                }
            })
            .collect();
        let select_cols = if col_names.is_empty() {
            "id".to_string()
        } else {
            format!("id, {}", col_names.join(", "))
        };
        let select_sql = format!(
            "SELECT {} FROM {}.{} WHERE {} = $1 ORDER BY created_at",
            select_cols,
            child.sql_schema_name,
            q(&child.sql_table_name),
            child.parent_fk_column,
        );

        writeln!(code, "{pad}let {field}_rows = {{", field = child.field_name).unwrap();
        writeln!(code, "{pad}    let stmt = Statement::from_sql_and_values(").unwrap();
        writeln!(code, "{pad}        DatabaseBackend::Postgres,").unwrap();
        writeln!(code, "{pad}        \"{}\",", select_sql).unwrap();
        writeln!(code, "{pad}        vec![{parent_id_expr}.into()],").unwrap();
        writeln!(code, "{pad}    );").unwrap();
        writeln!(code, "{pad}    let rows = db.query_all(stmt).await?;").unwrap();
        writeln!(
            code,
            "{pad}    let mut items = Vec::with_capacity(rows.len());"
        )
        .unwrap();
        let child_row_var = if child.columns.is_empty() && child.child_tables.is_empty() {
            "_child_row"
        } else {
            "child_row"
        };
        writeln!(code, "{pad}    for {} in &rows {{", child_row_var).unwrap();
        if !child.columns.is_empty() || !child.child_tables.is_empty() {
            writeln!(code, "{pad}        use sea_orm::TryGetable;").unwrap();
        }

        // If there are nested children, extract the child row's id for sub-queries.
        // Only emit child_row_id when at least one grandchild actually needs
        // a parent-id query (has columns or its own children). Grandchildren
        // with neither are emitted as `Vec::new()` and never consume the id.
        if !child.child_tables.is_empty() {
            let needs_id = child
                .child_tables
                .iter()
                .any(|gc| !gc.columns.is_empty() || !gc.child_tables.is_empty());
            if needs_id {
                writeln!(
                    code,
                    "{pad}        let child_row_id: Uuid = Uuid::try_get_by(child_row, \"id\").map_err(|e| format!(\"{{e:?}}\"))?;"
                )
                .unwrap();
            }
            // Recursively query nested grandchild tables using child_row_id.
            emit_child_reads(code, &child.child_tables, "child_row_id", indent + 2);
        }

        writeln!(
            code,
            "{pad}        items.push({}Response {{",
            child.struct_name
        )
        .unwrap();
        for col in &child.columns {
            emit_child_col_read_value(code, col, &format!("{pad}            "));
        }
        // Wire nested children into the response struct.
        emit_child_field_population(code, &child.child_tables, &format!("{pad}            "));
        writeln!(code, "{pad}        }});").unwrap();
        writeln!(code, "{pad}    }}").unwrap();
        writeln!(code, "{pad}    items").unwrap();
        writeln!(code, "{pad}}};").unwrap();
    }
}

/// Emit a single child column read expression for response struct construction.
fn emit_child_col_read_value(code: &mut String, col: &ChildColumn, pad: &str) {
    if col.dto_rust_type.is_some() {
        if col.is_nullable {
            writeln!(
                code,
                "{pad}{field}: Option::<String>::try_get_by(child_row, \"{pg}\").ok().flatten().and_then(|v| v.parse().ok()),",
                field = col.field_name,
                pg = col.pg_column_name,
            )
            .unwrap();
        } else {
            writeln!(
                code,
                "{pad}{field}: String::try_get_by(child_row, \"{pg}\").map_err(|e| format!(\"{{e:?}}\"))?.parse().unwrap_or_default(),",
                field = col.field_name,
                pg = col.pg_column_name,
            )
            .unwrap();
        }
    } else if col.is_nullable {
        writeln!(
            code,
            "{pad}{field}: Option::<{typ}>::try_get_by(child_row, \"{pg}\").ok().flatten(),",
            field = col.field_name,
            typ = col.rust_type,
            pg = col.pg_column_name,
        )
        .unwrap();
    } else {
        writeln!(
            code,
            "{pad}{field}: {typ}::try_get_by(child_row, \"{pg}\").map_err(|e| format!(\"{{e:?}}\"))?,",
            field = col.field_name,
            typ = turbofish(&col.rust_type),
            pg = col.pg_column_name,
        )
        .unwrap();
    }
}

/// Classify properties into direct columns and child tables.
/// Bundles the context needed by `build_columns_and_children` to classify properties.
struct ClassificationContext<'a> {
    schema_title: &'a str,
    module_name: &'a str,
    schema_name: &'a str,
    entity_name: &'a str,
    composite_range: &'a Option<codegraph_core::types::CompositeRange>,
    consumed_fields: &'a std::collections::HashSet<String>,
    all_field_names: &'a std::collections::HashSet<String>,
    entity_titles: &'a std::collections::HashSet<String>,
    workflow_managed: &'a std::collections::HashSet<String>,
    suffix: &'a str,
}

///
/// This is the core property-classification loop extracted from `query_entity_tree`.
/// It walks each property and, based on its `RefClassificationKind`, decides whether
/// to emit a direct column (TreeColumn) or a child table (ChildTableInfo).
async fn build_columns_and_children(
    db: &dyn GraphQuerier,
    props: &[codegraph_core::types::PropertyNode],
    ctx: &ClassificationContext<'_>,
) -> Result<(Vec<TreeColumn>, Vec<ChildTableInfo>)> {
    use crate::generate::pg_cast_for_type;

    let schema_title = ctx.schema_title;
    let module_name = ctx.module_name;
    let schema_name = ctx.schema_name;
    let entity_name = ctx.entity_name;
    let consumed_fields = ctx.consumed_fields;
    let all_field_names = ctx.all_field_names;
    let entity_titles = ctx.entity_titles;
    let workflow_managed = ctx.workflow_managed;

    let mut direct_columns = Vec::new();

    // Add composite range column (if present) so DDL has it, but mark as
    // composite so create/update/response code skips DTO references.
    if let Some(ref range) = ctx.composite_range {
        direct_columns.push(TreeColumn {
            field_name: range.pg_column_name.clone(),
            pg_column_name: range.pg_column_name.clone(),
            dto_field_name: None,
            rust_type: "String".to_string(),
            is_nullable: true,
            is_entity_ref: false,
            dto_rust_type: None,
            is_workflow_managed: false,
            is_array: false,
            pg_cast: pg_cast_for_type(&range.pg_type),
            is_composite_range: true,
            is_structured_wrapper: false,
            is_media: false,
        });
    }
    let mut child_tables = Vec::new();
    let mut seen_child_structs = std::collections::HashSet::new();

    for prop in props {
        if prop.rust_field_name == "id" {
            continue;
        }
        if consumed_fields.contains(&prop.name) {
            continue;
        }
        let is_workflow_field = workflow_managed.contains(&prop.rust_field_name);
        let field_def = codegraph_core::types::resolve_field(prop);
        if matches!(
            prop.effective_kind(),
            Some(RefClassificationKind::CompositeWrapper)
                | Some(RefClassificationKind::MediaWrapper)
        ) {
            let is_media = prop.effective_kind() == Some(RefClassificationKind::MediaWrapper);
            if let Ok(comp_cols) = db.get_composite_columns(&prop.name, schema_title).await {
                for col in &comp_cols {
                    let dto_rust_type = col
                        .dto_rust_type
                        .as_ref()
                        .filter(|dt| *dt != &col.rust_type)
                        .cloned();
                    let suffix_name = format!("{}{}", field_def.rust_field_name, col.suffix);
                    let suffix_pg = format!("{}{}", field_def.column_name, col.suffix);
                    direct_columns.push(TreeColumn {
                        field_name: suffix_name,
                        pg_column_name: suffix_pg,
                        dto_field_name: None,
                        rust_type: col.rust_type.clone(),
                        is_nullable: !prop.is_required,
                        is_entity_ref: false,
                        dto_rust_type,
                        is_workflow_managed: is_workflow_field,
                        is_array: false,
                        pg_cast: None,
                        is_composite_range: false,
                        is_structured_wrapper: false,
                        is_media,
                    });
                }
            }
        } else if matches!(
            prop.effective_kind(),
            Some(RefClassificationKind::PrimitiveWrapper)
                | Some(RefClassificationKind::ArrayWrapper)
                | Some(RefClassificationKind::RangeWrapper)
                | Some(RefClassificationKind::InlineEnum)
        ) {
            let pg_cast = if prop.effective_kind() == Some(RefClassificationKind::RangeWrapper) {
                pg_cast_for_type(&prop.pg_column_type)
            } else {
                None
            };
            direct_columns.push(TreeColumn {
                field_name: field_def.rust_field_name.clone(),
                pg_column_name: field_def.column_name.clone(),
                dto_field_name: None,
                rust_type: prop.rust_field_type.clone(),
                is_nullable: !prop.is_required,
                is_entity_ref: false,
                dto_rust_type: None,
                is_workflow_managed: is_workflow_field,
                is_array: prop.is_array,
                pg_cast,
                is_composite_range: false,
                is_structured_wrapper: false,
                is_media: false,
            });
        } else if matches!(
            prop.effective_kind(),
            Some(RefClassificationKind::CodelistCheck)
                | Some(RefClassificationKind::CodelistReference)
        ) {
            if prop.is_array {
                let enum_name =
                    crate::generate::ddd::dto::codelist_enum_name_from_ref(&prop.ref_target);
                let child_table_name = codegraph_naming::truncate_pg_identifier(&format!(
                    "{}_{}",
                    module_name, prop.pg_column_name
                ));
                let child_struct = format!(
                    "{}{}",
                    entity_name,
                    codegraph_naming::to_pascal_case(&prop.rust_field_name)
                );
                if seen_child_structs.insert(child_struct.clone()) {
                    child_tables.push(ChildTableInfo {
                        field_name: field_def.rust_field_name.clone(),
                        struct_name: child_struct,
                        sql_table_name: child_table_name,
                        sql_schema_name: schema_name.to_string(),
                        parent_fk_column: codegraph_naming::truncate_pg_identifier(&format!(
                            "{}_id",
                            module_name
                        )),
                        is_array: true,
                        columns: vec![ChildColumn {
                            field_name: "code".to_string(),
                            pg_column_name: "code".to_string(),
                            rust_type: "String".to_string(),
                            is_nullable: false,
                            dto_rust_type: enum_name,
                            pg_cast: None,
                        }],
                        child_tables: vec![],
                    });
                }
            } else {
                let codelist_type =
                    crate::generate::ddd::dto::codelist_enum_name_from_ref(&prop.ref_target);
                // rust_field_name at ingestion may retain _code suffix for some
                // classifier paths. The DTO field name strips _code — set
                // dto_field_name so col.dto_name() returns the correct DTO name.
                let stripped = crate::generate::ddd::dto::strip_code_suffix_safe(&field_def.rust_field_name);
                let dto_name = if stripped != field_def.rust_field_name {
                    Some(stripped)
                } else {
                    None // same as field_name, no override needed
                };
                direct_columns.push(TreeColumn {
                    field_name: field_def.rust_field_name.clone(),
                    pg_column_name: field_def.column_name.clone(),
                    dto_field_name: dto_name,
                    rust_type: "String".to_string(),
                    is_nullable: !prop.is_required,
                    is_entity_ref: false,
                    dto_rust_type: codelist_type,
                    is_workflow_managed: is_workflow_field,
                    is_array: false,
                    pg_cast: None,
                    is_composite_range: false,
                    is_structured_wrapper: false,
                    is_media: false,
                });
            }
        } else if prop.effective_kind() == Some(RefClassificationKind::StructuredWrapper) {
            direct_columns.push(TreeColumn {
                field_name: field_def.rust_field_name.clone(),
                pg_column_name: field_def.column_name.clone(),
                dto_field_name: None,
                rust_type: "serde_json::Value".to_string(),
                is_nullable: !prop.is_required,
                is_entity_ref: false,
                dto_rust_type: None,
                is_workflow_managed: is_workflow_field,
                is_array: prop.is_array,
                pg_cast: None,
                is_composite_range: false,
                is_structured_wrapper: true,
                is_media: false,
            });
        } else if prop.effective_kind() == Some(RefClassificationKind::EntityReference) {
            direct_columns.push(TreeColumn {
                field_name: field_def.rust_field_name,
                pg_column_name: field_def.column_name,
                dto_field_name: None,
                rust_type: "Uuid".to_string(),
                is_nullable: true,
                is_entity_ref: true,
                dto_rust_type: None,
                is_workflow_managed: is_workflow_field,
                is_array: false,
                pg_cast: None,
                is_composite_range: false,
                is_structured_wrapper: false,
                is_media: false,
            });
        } else if prop.effective_kind() == Some(RefClassificationKind::ValueObject) {
            let is_entity_fk = if !prop.is_array {
                db.get_property_ref_target(&prop.name, schema_title)
                    .await
                    .ok()
                    .flatten()
                    .map(|t| entity_titles.contains(&t.title))
                    .unwrap_or(false)
            } else {
                false
            };
            if is_entity_fk {
                direct_columns.push(TreeColumn {
                    field_name: format!("{}_id", field_def.rust_field_name),
                    pg_column_name: format!("{}_id", field_def.column_name),
                    dto_field_name: None,
                    rust_type: "Uuid".to_string(),
                    is_nullable: true,
                    is_entity_ref: true,
                    dto_rust_type: None,
                    is_workflow_managed: is_workflow_field,
                    is_array: false,
                    pg_cast: None,
                    is_composite_range: false,
                    is_structured_wrapper: false,
                    is_media: false,
                });
            } else {
                let mut visited = std::collections::HashSet::new();
                visited.insert(schema_title.to_string());
                if let Some(child_info) = Box::pin(build_child_table_info(
                    db,
                    prop,
                    schema_title,
                    module_name,
                    schema_name,
                    entity_name,
                    &mut visited,
                    0,
                    ctx.suffix,
                ))
                .await
                {
                    if seen_child_structs.insert(child_info.struct_name.clone()) {
                        child_tables.push(child_info);
                    }
                }
            }
        }
    }

    // Deduplicate direct_columns by field_name — composite wrappers and allOf
    // composition can produce duplicate column names. Keep the first occurrence.
    {
        let mut seen_fields = std::collections::HashSet::new();
        direct_columns.retain(|c| seen_fields.insert(c.field_name.clone()));
    }

    Ok((direct_columns, child_tables))
}

/// Which CRUD operation is being emitted.
/// Centralises column filtering and DTO field name resolution
/// so all paths stay consistent — adding a new variant forces
/// defining its filter and field name at compile time.
#[derive(Clone, Copy)]
enum CrudOp {
    CreateActiveModel,
    CreateRawSql,
    Update,
}

impl CrudOp {
    fn columns<'a>(&self, tree: &'a EntityTree) -> Vec<&'a TreeColumn> {
        match self {
            CrudOp::CreateActiveModel => tree.direct_columns.iter()
                .filter(|c| !c.is_workflow_managed && !c.is_composite_range && !c.is_media)
                .filter(|c| !Self::is_parent_fk(c, tree))
                .collect(),
            CrudOp::CreateRawSql => tree.direct_columns.iter()
                .filter(|c| !c.is_workflow_managed && !c.is_composite_range && !c.is_media)
                .filter(|c| !Self::is_parent_fk(c, tree))
                .collect(),
            CrudOp::Update => tree.direct_columns.iter()
                .filter(|c| !c.is_workflow_managed && !c.is_media && !c.is_composite_range)
                .filter(|c| c.pg_cast.is_none())
                .collect(),
        }
    }

    fn is_parent_fk(c: &TreeColumn, tree: &EntityTree) -> bool {
        tree.parent_ref.as_ref().is_some_and(|pr| {
            c.field_name.eq_ignore_ascii_case(pr)
                || c.pg_column_name.eq_ignore_ascii_case(pr)
                || c.pg_column_name == format!("{}_id", codegraph_naming::to_snake_case(pr))
        })
    }
}

/// Emits repository implementation Rust code by walking the entity's graph subtree.
pub struct RepositoryImplEmitter;

impl RepositoryImplEmitter {
    pub async fn emit(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        parent_ref: Option<&str>,
        include_paths: &[ResolvedIncludePath],
    ) -> Result<String> {
        let tree = self
            .query_entity_tree(db, schema_title, domain, config, parent_ref)
            .await?;
        let mut code = String::with_capacity(4096);

        // Deduplicate include paths by alias to prevent duplicate struct field
        // emissions (auto-discover can produce the same child entity through
        // multiple FK relationships).
        let include_paths = {
            let mut seen = std::collections::HashSet::new();
            include_paths
                .iter()
                .filter(|path| seen.insert(path.alias.clone()))
                .cloned()
                .collect::<Vec<_>>()
        };

        self.emit_header(&tree, &mut code);
        if tree.has_create {
            self.emit_create_fn(&tree, &mut code);
        }
        self.emit_find_by_id_fn(&tree, &mut code);
        if tree.parent_ref.is_some() {
            self.emit_find_by_id_scoped_fn(&tree, &mut code);
        }
        if tree.has_update {
            self.emit_update_fn(&tree, &mut code);
        }
        if tree.has_delete {
            self.emit_delete_fn(&tree, &mut code);
        }
        self.emit_list_fn(&tree, &mut code);
        if tree.has_fts {
            self.emit_search_fn(&tree, &mut code);
        }
        if tree.has_embeddings {
            self.emit_semantic_search_fn(&tree, &mut code);
        }
        if tree.hierarchy_field.is_some() {
            self.emit_find_tree_fn(&tree, &mut code);
            self.emit_find_ancestors_fn(&tree, &mut code);
        }
        self.emit_footer(&mut code);

        // Resolve scalar fields for each include path segment using resolve_field()
        // so that both the DTO side and entity Model side use rust_field_name.
        // EntityReference fields get _id appended; CodelistReference fields get _code stripped.
        let all_props = db.list_all_properties().await?;
        let mut include_segment_dto_fields: Vec<Vec<Vec<String>>> = Vec::new();
        let mut include_segment_col_fields: Vec<Vec<Vec<String>>> = Vec::new();
        for path in &include_paths {
            let mut per_seg_dto: Vec<Vec<String>> = Vec::new();
            let mut per_seg_col: Vec<Vec<String>> = Vec::new();
            for seg in &path.segments {
                let mut dto_fields: Vec<String> = Vec::new();
                let mut col_fields: Vec<String> = Vec::new();
                // Use schema_title directly — include_path.rs already resolves
                // it to the canonical title for each segment. The fallback graph
                // query could return the wrong properties from a shared parent
                // when schema inheritance is involved.
                if let Some(props) = all_props.get(&seg.schema_title) {
                    let mut seen = std::collections::HashSet::new();
                    for prop in props {
                        // Skip properties that don't map to database columns —
                        // the entity Model won't have them as Rust fields.
                        if prop.pg_column_name.is_empty() {
                            continue;
                        }
                        // Skip array properties — the entity generator expands
                        // these into separate child tables, not direct Model fields.
                        if prop.is_array {
                            continue;
                        }
                        if prop.rust_field_name == "id"
                            || prop.rust_field_name == "created_at"
                            || prop.rust_field_name == "updated_at"
                        {
                            continue;
                        }
                        if matches!(prop.effective_kind(), Some(RefClassificationKind::ValueObject)) {
                            continue;
                        }
                        let fd = codegraph_core::types::resolve_field(prop);
                        // Deduplicate by rust_field_name — list_all_properties()
                        // can return duplicate entries from interface inheritance.
                        if seen.insert(fd.rust_field_name.clone()) {
                            dto_fields.push(fd.rust_field_name.clone());
                            col_fields.push(fd.rust_field_name.clone());
                        }
                    }
                }
                per_seg_dto.push(dto_fields);
                per_seg_col.push(col_fields);
            }
            include_segment_dto_fields.push(per_seg_dto);
            include_segment_col_fields.push(per_seg_col);
        }

        if !include_paths.is_empty() {
            // Add import statements for cross-entity types referenced by include paths.
            // These types (e.g. PersonResponse) live in other entity modules and need
            // use crate::domain::{domain}::{module}::dto_response::TypeName imports.
            let caller_base: Vec<String> = vec![
                "crate".into(), "domain".into(), domain.into(),
                tree.module_name.clone(), "repository_impl".into(),
            ];
            let mut include_type_names: Vec<String> = Vec::new();
            for path in &include_paths {
                include_type_names.push(path.response_rust_type.clone());
                if path.segments.len() > 1 {
                    if let Some(last_seg) = path.segments.last() {
                        include_type_names.push(format!("{}Response", last_seg.entity_name));
                    }
                }
            }
            // Deduplicate while preserving order.
            let mut seen = std::collections::HashSet::new();
            include_type_names.retain(|n| seen.insert(n.clone()));
            let imports = type_registry::resolve_imports(&include_type_names, &caller_base);
            for import in &imports {
                writeln!(code, "{}", import).unwrap();
            }
            // Also add direct imports for enriched types from dto_included module.
            // These types (e.g. DeploymentWithPositionResponse) are generated in the
            // current entity's dto_included.rs but may not yet be registered in the
            // type registry when the repository emitter runs (DTO generator runs later).
            for path in &include_paths {
                if path.segments.len() > 1 {
                    writeln!(
                        code,
                        "use super::dto_included::{};",
                        path.response_rust_type
                    )
                    .unwrap();
                }
            }

            writeln!(code).unwrap();
            writeln!(
                code,
                "impl {}RepositoryImpl {{",
                tree.entity_name
            )
            .unwrap();
            self.emit_include_fetch_methods(&tree, &mut code, &include_paths, &include_segment_dto_fields, &include_segment_col_fields);
            writeln!(code, "}}").unwrap();
        }

        Ok(code)
    }

    async fn query_entity_tree(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        parent_ref: Option<&str>,
    ) -> Result<EntityTree> {
        let schema = db
            .get_schema(schema_title)
            .await?
            .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

        let entity_name = schema.rust_type_name.clone();
        let module_name = schema.pg_table_name.clone();
        let schema_name = domain.to_string();

        // Determine enabled operations
        let operations = config
            .domains
            .get(domain)
            .and_then(|d| d.get_entity_config(schema_title))
            .and_then(|ec| ec.operations.clone())
            .unwrap_or_else(|| config.defaults.operations.clone());
        let has_create = operations.contains(&"create".to_string());
        let has_update = operations.contains(&"update".to_string());
        let has_delete = operations.contains(&"delete".to_string());
        let entity_cfg = config
            .domains
            .get(domain)
            .and_then(|d| d.get_entity_config(schema_title));
        let has_workflow = entity_cfg
            .and_then(|ec| ec.workflow.as_ref())
            .map(|wf| wf.generate_action_endpoints)
            .unwrap_or(false);

        let is_auditable = config
            .domains
            .get(domain)
            .and_then(|d| d.auditable)
            .unwrap_or(true);

        // Workflow-managed fields are excluded from create/update DTOs but
        // included in response DTOs. Mark them so the repository can include
        // them in reads but skip them in create/update writes.
        let mut workflow_managed = std::collections::HashSet::new();
        if let Some(wf) = entity_cfg.and_then(|ec| ec.workflow.as_ref()) {
            workflow_managed.insert(codegraph_naming::to_snake_case(&wf.status_field));
            if let Some(ref af) = wf.approval_status_field {
                workflow_managed.insert(codegraph_naming::to_snake_case(af));
            }
        }

        let all_props = db.get_properties(schema_title).await?;
        let props = {
            let mut seen = std::collections::HashSet::new();
            all_props
                .into_iter()
                .filter(|p| seen.insert(p.rust_field_name.clone()))
                .collect::<Vec<_>>()
        };

        // Consumed fields from composite range collapsing — skip these in all operations
        let consumed_fields: std::collections::HashSet<String> = db
            .get_consumed_fields(schema_title)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(prop, _role)| prop.name)
            .collect();

        // Composite range: collapsed start/end → single range column
        let composite_range = db.get_composite_range(schema_title).await.ok().flatten();

        // Collect all raw field names to detect collisions when stripping _code suffix.
        let all_field_names: std::collections::HashSet<String> =
            props.iter().map(|p| p.rust_field_name.clone()).collect();

        // Query the graph for all entity titles so we can detect when a
        // ValueObject property actually targets an entity (FK column, not child table).
        let entity_titles: std::collections::HashSet<String> =
            db.get_entity_names().await?.into_iter().collect();

        let cls_ctx = ClassificationContext {
            schema_title,
            module_name: &module_name,
            schema_name: &schema_name,
            entity_name: &entity_name,
            composite_range: &composite_range,
            consumed_fields: &consumed_fields,
            all_field_names: &all_field_names,
            entity_titles: &entity_titles,
            workflow_managed: &workflow_managed,
            suffix: &config.defaults.type_suffix,
        };
        let (mut direct_columns, child_tables) =
            build_columns_and_children(db, &props, &cls_ctx).await?;

        // Add synthetic hierarchy column (self-referential FK) when configured.
        if let Some(ref hf) = entity_cfg.and_then(|ec| ec.hierarchy_field.as_ref()) {
            if !direct_columns.iter().any(|c| c.field_name == **hf) {
                direct_columns.push(TreeColumn {
                    field_name: hf.to_string(),
                    pg_column_name: hf.to_string(),
                    dto_field_name: None,
                    rust_type: "Uuid".to_string(),
                    is_nullable: true,
                    is_entity_ref: false,
                    dto_rust_type: None,
                    is_workflow_managed: false,
                    is_array: false,
                    pg_cast: None,
                    is_composite_range: false,
                    is_structured_wrapper: false,
                    is_media: false,
                });
            }
        }

        let entity_module = format!("{}_{}", schema_name, module_name);

        let search = entity_cfg.map(|ec| &ec.search);
        let has_fts = search
            .and_then(|s| s.fts_columns.as_ref())
            .map(|cols| !cols.is_empty())
            .unwrap_or(false);
        let has_embeddings = search
            .map(|s| !s.embedding_columns.is_empty())
            .unwrap_or(false);
        let fts_language = search
            .map(|s| s.fts_language.clone())
            .unwrap_or_else(|| "english".to_string());

        let filter_fields = resolve_filter_fields(
            db,
            schema_title,
            entity_cfg
                .and_then(|ec| ec.filter_fields.as_ref())
                .map(|v| v.as_slice()),
        )
        .await?;

        let nested_filter_fields =
            resolve_nested_filter_fields(db, schema_title, &module_name, &schema_name, config)
                .await?;

        let hierarchy_field = entity_cfg
            .and_then(|ec| ec.hierarchy_field.as_ref())
            .cloned();

        // Resolve tree_include entries: find FK columns and parent refs
        let tree_include = {
            let mut resolved = Vec::new();
            if let Some(entries) = entity_cfg.and_then(|ec| ec.tree_include.as_ref()) {
                for entry in entries {
                    // Find the via entity's domain entry (search all domains)
                    let via_domain_entry = config.domains.values().find(|d| {
                        d.entity_config.contains_key(&entry.via_entity)
                    });
                    let via_entity_cfg = via_domain_entry
                        .and_then(|d| d.entity_config.get(&entry.via_entity));

                    // Get via entity's parent_ref column name
                    let parent_ref_col = via_entity_cfg
                        .and_then(|ec| ec.parent_ref.as_ref())
                        .cloned();

                    // Get via entity's parent entity name from role/parent config
                    let parent_entity_name = via_entity_cfg
                        .and_then(|ec| ec.parent.as_ref())
                        .cloned();

                    // Find the FK column on via_entity that references the current entity.
                    // Use the naming convention: snake_case(prop.name) + "_id" matches the DDL.
                    let via_props = db.get_properties(&entry.via_entity).await?;
                    let mut via_fk = None;
                    for prop in &via_props {
                        if let Ok(Some(target)) = db.get_property_ref_target(&prop.name, &entry.via_entity).await {
                            if target.title == schema_title {
                                let col = codegraph_core::types::resolve_field(prop).column_name;
                                via_fk = Some(col);
                                break;
                            }
                        }
                    }

                    // Get via entity's schema for table name
                    let via_schema = db.get_schema(&entry.via_entity).await?
                        .ok_or_else(|| crate::error::Error::SchemaNotFound(entry.via_entity.clone()))?;

                    // Get parent entity's schema for table name
                    let parent_schema = if let Some(ref name) = parent_entity_name {
                        db.get_schema(name).await.ok().flatten()
                    } else {
                        None
                    };

                    // Resolve worker detail JOINs from the parent entity's composition tree.
                    // Walks child tables to find person name columns (given, family) and avatar_url.
                    let worker_detail_joins = if let Some(ref parent_name) = parent_entity_name {
                        resolve_worker_detail_joins(db, parent_name).await
                    } else {
                        Vec::new()
                    };

                    if let (Some(p_ref), Some(fk), Some(parent_schema)) = (parent_ref_col, via_fk, parent_schema) {
                        resolved.push(TreeIncludeResolved {
                            alias: entry.alias.clone(),
                            via_table: format!("{}.{}", via_schema.domain.as_deref().unwrap_or("public"), via_schema.pg_table_name),
                            via_fk_column: fk,
                            parent_table: format!("{}.{}", parent_schema.domain.as_deref().unwrap_or("public"), parent_schema.pg_table_name),
                            parent_ref_column: p_ref,
                            worker_detail_joins,
                        });
                    }
                }
            }
            resolved
        };

        Ok(EntityTree {
            entity_name,
            module_name: module_name.clone(),
            schema_name,
            table_name: module_name,
            entity_module,
            direct_columns,
            child_tables,
            has_create,
            has_update,
            has_delete,
            has_workflow,
            has_fts,
            has_embeddings,
            fts_language,
            filter_fields,
            nested_filter_fields,
            parent_ref: parent_ref.map(|s| s.to_string()),
            hierarchy_field,
            tree_include,
            is_auditable,
        })
    }

    fn emit_header(&self, tree: &EntityTree, code: &mut String) {
        writeln!(
            code,
            "//! Generated repository implementation for {}.",
            tree.entity_name
        )
        .unwrap();
        writeln!(code, "//! DO NOT EDIT — generated by {}.", crate::generate::get_project_config().generator_name).unwrap();
        writeln!(code).unwrap();
        writeln!(code, "use async_trait::async_trait;").unwrap();
        writeln!(code, "use sea_orm::{{").unwrap();
        let has_range_cols = tree
            .direct_columns
            .iter()
            .any(|c| c.pg_cast.is_some() && !c.is_composite_range);
        if tree.child_tables.is_empty()
            && !tree.has_fts
            && !tree.has_embeddings
            && !has_range_cols
            && tree.hierarchy_field.is_none()
        {
            writeln!(
                code,
                "    ActiveModelTrait, ColumnTrait, DatabaseTransaction,"
            )
            .unwrap();
            writeln!(
                code,
                "    EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,"
            )
            .unwrap();
        } else {
            writeln!(code, "    ActiveModelTrait, ColumnTrait, ConnectionTrait,").unwrap();
            writeln!(
                code,
                "    DatabaseTransaction, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,"
            )
            .unwrap();
            writeln!(code, "    DatabaseBackend, Statement,").unwrap();
        }
        writeln!(code, "}};").unwrap();
        writeln!(code, "use uuid::Uuid;").unwrap();
        writeln!(code).unwrap();
        writeln!(
            code,
            "use super::repository::{}Repository;",
            tree.entity_name
        )
        .unwrap();
        if tree.has_create {
            writeln!(
                code,
                "use super::dto_create::Create{}Request;",
                tree.entity_name
            )
            .unwrap();
        }
        if tree.has_update {
            writeln!(
                code,
                "use super::dto_update::Update{}Request;",
                tree.entity_name
            )
            .unwrap();
        }
        writeln!(
            code,
            "use super::dto_response::{}Response;",
            tree.entity_name
        )
        .unwrap();
        // Import child DTO response types (including nested children)
        let all_children = flatten_child_tables(&tree.child_tables);
        let mut imported = std::collections::HashSet::new();
        for child in &all_children {
            if imported.insert(child.struct_name.clone()) {
                writeln!(
                    code,
                    "use super::dto_response::{}Response;",
                    child.struct_name
                )
                .unwrap();
            }
        }
        writeln!(code).unwrap();
        writeln!(code, "pub struct {}RepositoryImpl;", tree.entity_name).unwrap();
        writeln!(code).unwrap();
        writeln!(code, "#[async_trait]").unwrap();
        writeln!(
            code,
            "impl {}Repository for {}RepositoryImpl {{",
            tree.entity_name, tree.entity_name
        )
        .unwrap();
    }

    fn emit_create_fn(&self, tree: &EntityTree, code: &mut String) {
        writeln!(
            code,
            "    #[tracing::instrument(skip(self, tx), fields(db.operation = \"insert\", db.table = \"{}.{}\"))]",
            tree.schema_name, tree.table_name
        )
        .unwrap();
        writeln!(code, "    async fn create(").unwrap();
        writeln!(code, "        &self,").unwrap();
        writeln!(code, "        tx: &DatabaseTransaction,").unwrap();
        writeln!(code, "        cmd: Create{}Request,", tree.entity_name).unwrap();
        if tree.parent_ref.is_some() {
            writeln!(code, "        parent_id: Uuid,").unwrap();
        }
        writeln!(code, "    ) -> Result<Uuid, Box<dyn std::error::Error>> {{").unwrap();
        writeln!(code, "        let id = Uuid::new_v4();").unwrap();
        writeln!(code).unwrap();
        let has_range_cols = tree
            .direct_columns
            .iter()
            .any(|c| c.pg_cast.is_some() && !c.is_composite_range);

        // Dispatch to CrudOp::CreateActiveModel or CrudOp::CreateRawSql
        if has_range_cols {
            // Use raw SQL INSERT so range parameters get explicit casts
            self.emit_create_raw_sql(tree, code);
        } else {
            // Use SeaORM ActiveModel insert (no range columns)
            self.emit_create_active_model(tree, code);
        }

        // Insert child table rows (recursively handles nested children).
        emit_child_inserts(code, &tree.child_tables, "id", "cmd", 2);

        writeln!(code).unwrap();
        writeln!(code, "        Ok(id)").unwrap();
        writeln!(code, "    }}").unwrap();
    }

    /// Emit parent entity INSERT using SeaORM ActiveModel (no range columns).
    fn emit_create_active_model(&self, tree: &EntityTree, code: &mut String) {
        writeln!(
            code,
            "        // Insert into {}.{} (direct columns)",
            tree.schema_name, tree.table_name
        )
        .unwrap();
        writeln!(
            code,
            "        let model = crate::entity::{}::ActiveModel {{",
            tree.entity_module
        )
        .unwrap();
        writeln!(code, "            id: Set(id),").unwrap();
        if let Some(ref parent_ref) = tree.parent_ref {
            let fk_field = codegraph_naming::to_snake_case(parent_ref);
            writeln!(code, "            {fk_field}: Set(Some(parent_id)),").unwrap();
        }
        let op = CrudOp::CreateActiveModel;
        for col in op.columns(tree) {
            let entity_field = &col.field_name;
            let dto_field = col.dto_name();
            if col.dto_rust_type.is_some() {
                if col.is_array {
                    // Vec<CodelistEnum> → Vec<String>
                    if col.is_nullable {
                        writeln!(
                            code,
                            "            {entity_field}: Set(cmd.{dto_field}.map(|v| v.into_iter().map(|x| x.to_string()).collect())),",
                        )
                        .unwrap();
                    } else {
                        writeln!(
                            code,
                            "            {entity_field}: Set(cmd.{dto_field}.into_iter().map(|v| v.to_string()).collect()),",
                        )
                        .unwrap();
                    }
                } else if col.is_nullable {
                    writeln!(
                        code,
                        "            {entity_field}: Set(cmd.{dto_field}.map(|v| v.to_string())),",
                    )
                    .unwrap();
                } else {
                    writeln!(
                        code,
                        "            {entity_field}: Set(cmd.{dto_field}.to_string()),",
                    )
                    .unwrap();
                }
            } else if col.is_structured_wrapper {
                // StructuredWrapper (scalar or array): DTO has typed struct/Vec, entity needs JSONB.
                if col.is_nullable {
                    if col.is_array {
                        writeln!(
                            code,
                            "            {entity_field}: Set(cmd.{dto_field}.map(|v| serde_json::to_value(v).unwrap_or(serde_json::Value::Null))),",
                        ).unwrap();
                    } else {
                        writeln!(
                            code,
                            "            {entity_field}: Set(cmd.{dto_field}.as_ref().and_then(|v| serde_json::to_value(v).ok())),",
                        ).unwrap();
                    }
                } else {
                    writeln!(
                        code,
                        "            {entity_field}: Set(serde_json::to_value(cmd.{dto_field}).unwrap_or(serde_json::Value::Null)),",
                    ).unwrap();
                }
            } else {
                writeln!(
                    code,
                    "            {}: Set(cmd.{}),",
                    entity_field, dto_field
                )
                .unwrap();
            }
        }
        writeln!(code, "            ..Default::default()").unwrap();
        writeln!(code, "        }};").unwrap();
        writeln!(code, "        model.insert(tx).await?;").unwrap();
    }

    /// Emit parent entity INSERT using raw SQL with explicit range casts.
    fn emit_create_raw_sql(&self, tree: &EntityTree, code: &mut String) {
        writeln!(
            code,
            "        // Insert into {}.{} via raw SQL (range columns need explicit casts)",
            tree.schema_name, tree.table_name
        )
        .unwrap();

        // Collect non-workflow columns for the INSERT (exclude composite range
        // columns which exist in DDL but not on DTOs, and exclude parent FK
        // for child entities since it's already set from the route)
        let op = CrudOp::CreateRawSql;
        let insert_cols = op.columns(tree);

        // Build column names: id + optional FK + direct columns (use PG column names for SQL, quoted)
        let mut col_names = vec!["id".to_string()];
        let mut placeholders = vec!["$1".to_string()];
        let mut param_offset = 2usize;
        if let Some(ref parent_ref) = tree.parent_ref {
            col_names.push(q(parent_ref));
            placeholders.push(format!("${param_offset}"));
            param_offset += 1;
        }
        for col in &insert_cols {
            col_names.push(q(&col.pg_column_name));
        }

        // Build placeholders for direct columns with optional casts
        for (i, col) in insert_cols.iter().enumerate() {
            let idx = i + param_offset;
            if let Some(ref cast) = col.pg_cast {
                if crate::generate::is_geometry_cast(cast) {
                    placeholders.push(format!("ST_GeomFromGeoJSON(${idx})"));
                } else {
                    placeholders.push(format!("${idx}::{cast}"));
                }
            } else {
                placeholders.push(format!("${idx}"));
            }
        }

        let sql = format!(
            "INSERT INTO {}.{} ({}) VALUES ({})",
            tree.schema_name,
            q(&tree.table_name),
            col_names.join(", "),
            placeholders.join(", "),
        );

        writeln!(code, "        let stmt = Statement::from_sql_and_values(").unwrap();
        writeln!(code, "            DatabaseBackend::Postgres,").unwrap();
        writeln!(code, "            \"{}\",", sql).unwrap();
        write!(code, "            vec![id.into()").unwrap();
        if tree.parent_ref.is_some() {
            write!(code, ", parent_id.into()").unwrap();
        }

        for col in &insert_cols {
            let dto_field = col.dto_name();
            let has_enum = col.dto_rust_type.is_some();
            let clone_suffix = if is_copy_type(&col.rust_type) {
                ""
            } else {
                ".clone()"
            };

            if col.is_structured_wrapper {
                // StructuredWrapper (scalar or array): serialize to JSONB Value.
                if col.is_nullable {
                    write!(
                        code,
                        ", cmd.{dto_field}.as_ref().and_then(|v| serde_json::to_value(v).ok().map(|j| sea_orm::Value::Json(Some(Box::new(j))))).unwrap_or(sea_orm::Value::Json(None))",
                    )
                    .unwrap();
                } else {
                    write!(
                        code,
                        ", sea_orm::Value::Json(Some(Box::new(serde_json::to_value(&cmd.{dto_field}).unwrap_or_default())))",
                    )
                    .unwrap();
                }
            } else if col.is_nullable {
                if is_vec_string(&col.rust_type) || (is_vec_type(&col.rust_type) && has_enum) {
                    let map_fn = if is_vec_string(&col.rust_type) {
                        "s"
                    } else {
                        "s.to_string()"
                    };
                    write!(
                        code,
                        ", cmd.{dto_field}.clone().map(|v| sea_orm::Value::Array(sea_orm::sea_query::ArrayType::String, Some(Box::new(v.into_iter().map(|s| sea_orm::Value::String(Some(Box::new({map_fn})))).collect())))).unwrap_or({null})",
                        null = null_value_for_type("Vec<String>"),
                    )
                    .unwrap();
                } else if is_vec_type(&col.rust_type) {
                    let (array_type, value_ctor) = vec_array_type_and_ctor(&col.rust_type);
                    write!(
                        code,
                        ", cmd.{dto_field}.clone().map(|v| sea_orm::Value::Array({array_type}, Some(Box::new(v.into_iter().map(|s| {value_ctor}).collect())))).unwrap_or(sea_orm::Value::Array({array_type}, None))",
                    )
                    .unwrap();
                } else if has_enum {
                    write!(
                        code,
                        ", cmd.{dto_field}.as_ref().map(|v| sea_orm::Value::String(Some(Box::new(v.to_string())))).unwrap_or({null})",
                        null = null_value_for_type(&col.rust_type),
                    )
                    .unwrap();
                } else {
                    let typed_value = typed_value_expr(&col.rust_type, "v");
                    write!(
                        code,
                        ", cmd.{dto_field}{clone_suffix}.map(|v| {typed_value}).unwrap_or({null})",
                        null = null_value_for_type(&col.rust_type),
                    )
                    .unwrap();
                }
            } else if is_vec_string(&col.rust_type) || (is_vec_type(&col.rust_type) && has_enum) {
                let map_fn = if is_vec_string(&col.rust_type) {
                    "s"
                } else {
                    "s.to_string()"
                };
                write!(
                    code,
                    ", sea_orm::Value::Array(sea_orm::sea_query::ArrayType::String, Some(Box::new(cmd.{dto_field}.clone().into_iter().map(|s| sea_orm::Value::String(Some(Box::new({map_fn})))).collect())))",
                )
                .unwrap();
            } else if is_vec_type(&col.rust_type) {
                let (array_type, value_ctor) = vec_array_type_and_ctor(&col.rust_type);
                write!(
                    code,
                    ", sea_orm::Value::Array({array_type}, Some(Box::new(cmd.{dto_field}.clone().into_iter().map(|s| {value_ctor}).collect())))",
                )
                .unwrap();
            } else if has_enum {
                write!(
                    code,
                    ", sea_orm::Value::String(Some(Box::new(cmd.{dto_field}.to_string())))",
                )
                .unwrap();
            } else {
                let item_expr = format!("cmd.{dto_field}{clone_suffix}");
                let typed_value = typed_value_expr(&col.rust_type, &item_expr);
                write!(code, ", {typed_value}").unwrap();
            }
        }

        writeln!(code, "],").unwrap();
        writeln!(code, "        );").unwrap();
        writeln!(code, "        tx.execute(stmt).await?;").unwrap();
    }

    fn emit_find_by_id_fn(&self, tree: &EntityTree, code: &mut String) {
        writeln!(code).unwrap();
        writeln!(
            code,
            "    #[tracing::instrument(skip(self, db), fields(db.operation = \"select\", db.table = \"{}.{}\"))]",
            tree.schema_name, tree.table_name
        )
        .unwrap();
        writeln!(code, "    async fn find_by_id(").unwrap();
        writeln!(code, "        &self,").unwrap();
        writeln!(code, "        db: &DatabaseTransaction,").unwrap();
        writeln!(code, "        id: Uuid,").unwrap();
        writeln!(
            code,
            "    ) -> Result<Option<{}Response>, Box<dyn std::error::Error>> {{",
            tree.entity_name
        )
        .unwrap();
        writeln!(
            code,
            // Use find().filter() instead of find_by_id() because SeaORM's
            // find_by_id() requires the primary key type to impl Into<Value>,
            // which fails for composite keys and custom ID wrappers.
            "        let row = crate::entity::{}::Entity::find()",
            tree.entity_module
        )
        .unwrap();
        writeln!(
            code,
            "            .filter(crate::entity::{}::Column::Id.eq(id))",
            tree.entity_module
        )
        .unwrap();
        if tree.is_auditable {
            writeln!(
                code,
                "            .filter(crate::entity::{}::Column::DeletedAt.is_null())",
                tree.entity_module
            )
            .unwrap();
        }
        writeln!(code, "            .one(db)").unwrap();
        writeln!(code, "            .await?;").unwrap();
        writeln!(code).unwrap();
        writeln!(code, "        let row = match row {{").unwrap();
        writeln!(code, "            Some(r) => r,").unwrap();
        writeln!(code, "            None => return Ok(None),").unwrap();
        writeln!(code, "        }};").unwrap();
        emit_response_construction(code, tree);
    }

    /// Emit `find_by_id_scoped` — same as `find_by_id` but adds a parent FK filter.
    fn emit_find_by_id_scoped_fn(&self, tree: &EntityTree, code: &mut String) {
        let parent_ref = tree.parent_ref.as_deref().unwrap();
        let pascal_col = codegraph_naming::to_pascal_case(parent_ref);
        writeln!(code).unwrap();
        writeln!(
            code,
            "    #[tracing::instrument(skip(self, db), fields(db.operation = \"select_scoped\", db.table = \"{}.{}\"))]",
            tree.schema_name, tree.table_name
        )
        .unwrap();
        writeln!(code, "    async fn find_by_id_scoped(").unwrap();
        writeln!(code, "        &self,").unwrap();
        writeln!(code, "        db: &DatabaseTransaction,").unwrap();
        writeln!(code, "        id: Uuid,").unwrap();
        writeln!(code, "        parent_id: Uuid,").unwrap();
        writeln!(
            code,
            "    ) -> Result<Option<{}Response>, Box<dyn std::error::Error>> {{",
            tree.entity_name
        )
        .unwrap();
        writeln!(
            code,
            "        let row = crate::entity::{}::Entity::find()",
            tree.entity_module
        )
        .unwrap();
        writeln!(
            code,
            "            .filter(crate::entity::{}::Column::Id.eq(id))",
            tree.entity_module
        )
        .unwrap();
        writeln!(
            code,
            "            .filter(crate::entity::{}::Column::{}.eq(parent_id))",
            tree.entity_module, pascal_col
        )
        .unwrap();
        if tree.is_auditable {
            writeln!(
                code,
                "            .filter(crate::entity::{}::Column::DeletedAt.is_null())",
                tree.entity_module
            )
            .unwrap();
        }
        writeln!(code, "            .one(db)").unwrap();
        writeln!(code, "            .await?;").unwrap();
        writeln!(code).unwrap();
        writeln!(code, "        let row = match row {{").unwrap();
        writeln!(code, "            Some(r) => r,").unwrap();
        writeln!(code, "            None => return Ok(None),").unwrap();
        writeln!(code, "        }};").unwrap();

        emit_response_construction(code, tree);
    }

    fn emit_update_fn(&self, tree: &EntityTree, code: &mut String) {
        writeln!(code).unwrap();
        writeln!(
            code,
            "    #[tracing::instrument(skip(self, tx), fields(db.operation = \"update\", db.table = \"{}.{}\"))]",
            tree.schema_name, tree.table_name
        )
        .unwrap();
        writeln!(code, "    async fn update(").unwrap();
        writeln!(code, "        &self,").unwrap();
        writeln!(code, "        tx: &DatabaseTransaction,").unwrap();
        writeln!(code, "        id: Uuid,").unwrap();
        writeln!(code, "        cmd: Update{}Request,", tree.entity_name).unwrap();
        writeln!(code, "    ) -> Result<(), Box<dyn std::error::Error>> {{").unwrap();
        writeln!(
            code,
            "        // Update {}.{} — only set fields present in the update request",
            tree.schema_name, tree.table_name
        )
        .unwrap();
        let has_updatable_cols = tree.direct_columns.iter().any(|c| {
            !c.is_workflow_managed && !c.is_composite_range && !c.is_media && c.pg_cast.is_none()
        });
        let mut_kw = if has_updatable_cols { "mut " } else { "" };
        writeln!(
            code,
            "        let {}model = crate::entity::{}::ActiveModel {{",
            mut_kw, tree.entity_module
        )
        .unwrap();
        writeln!(code, "            id: Set(id),").unwrap();
        writeln!(code, "            ..Default::default()").unwrap();
        writeln!(code, "        }};").unwrap();
        let op = CrudOp::Update;
        for col in op.columns(tree) {
            let entity_field = &col.field_name;
            let dto_field = col.dto_name();
            if col.is_structured_wrapper {
                // StructuredWrapper (scalar or array): serialize to JSONB.
                if col.is_nullable {
                    if col.is_array {
                        writeln!(
                            code,
                            "        if let Some(v) = cmd.{dto_field} {{ model.{entity_field} = Set(Some(serde_json::to_value(v).unwrap_or(serde_json::Value::Null))); }}",
                        ).unwrap();
                    } else {
                        writeln!(
                            code,
                            "        if let Some(v) = cmd.{dto_field} {{ model.{entity_field} = Set(serde_json::to_value(v).ok()); }}",
                        ).unwrap();
                    }
                } else {
                    writeln!(
                        code,
                        "        if let Some(v) = cmd.{dto_field} {{ model.{entity_field} = Set(serde_json::to_value(v).unwrap_or(serde_json::Value::Null)); }}",
                    ).unwrap();
                }
            } else {
                let value_expr = if col.dto_rust_type.is_some() && col.is_array {
                    "v.into_iter().map(|x| x.to_string()).collect()"
                } else if col.dto_rust_type.is_some() {
                    "v.to_string()"
                } else {
                    "v"
                };
                if col.is_nullable {
                    writeln!(
                        code,
                        "        if let Some(v) = cmd.{dto_field} {{ model.{entity_field} = Set(Some({value_expr})); }}",
                    )
                    .unwrap();
                } else {
                    writeln!(
                        code,
                        "        if let Some(v) = cmd.{dto_field} {{ model.{entity_field} = Set({value_expr}); }}",
                    )
                    .unwrap();
                }
            }
        }
        writeln!(code, "        match model.update(tx).await {{").unwrap();
        writeln!(code, "            Ok(_) => {{}}").unwrap();
        writeln!(code, "            Err(sea_orm::DbErr::RecordNotUpdated) => {{ /* RLS hid the row — find_by_id will return 404 */ }}").unwrap();
        writeln!(code, "            Err(e) => return Err(e.into()),").unwrap();
        writeln!(code, "        }}").unwrap();

        // Update range columns via a single raw SQL UPDATE with explicit casts.
        // All range columns are collected into one statement to avoid per-column round-trips.
        let range_cols: Vec<&TreeColumn> = tree
            .direct_columns
            .iter()
            .filter(|c| {
                !c.is_workflow_managed
                    && !c.is_composite_range
                    && !c.is_media
                    && c.pg_cast.is_some()
            })
            .collect();
        if !range_cols.is_empty() {
            // All update DTO fields are Option<T>, so we only emit the UPDATE when at
            // least one range field is present. Build the SET clause and values dynamically.
            writeln!(code).unwrap();
            writeln!(
                code,
                "        // Range columns need explicit casts — build a single UPDATE"
            )
            .unwrap();
            writeln!(code, "        {{").unwrap();
            writeln!(
                code,
                "            let mut set_clauses: Vec<String> = Vec::new();"
            )
            .unwrap();
            writeln!(
                code,
                "            let mut values: Vec<sea_orm::Value> = Vec::new();"
            )
            .unwrap();
            for col in &range_cols {
                let cast = col.pg_cast.as_deref().unwrap();
            let dto_field = col.dto_name();
            let pg_col = q(&col.pg_column_name);
                let typed_value = typed_value_expr(&col.rust_type, "v");
                writeln!(code, "            if let Some(v) = cmd.{dto_field} {{").unwrap();
                let set_expr = if crate::generate::is_geometry_cast(cast) {
                    format!("                set_clauses.push(format!(\"{pg_col} = ST_GeomFromGeoJSON(${{}})\", values.len() + 1));")
                } else {
                    format!("                set_clauses.push(format!(\"{pg_col} = ${{}}::{cast}\", values.len() + 1));")
                };
                writeln!(code, "{set_expr}").unwrap();
                writeln!(code, "                values.push({typed_value});").unwrap();
                writeln!(code, "            }}").unwrap();
            }
            writeln!(code, "            if !set_clauses.is_empty() {{").unwrap();
            writeln!(
                code,
                "                let id_placeholder = format!(\"${{}}\", values.len() + 1);"
            )
            .unwrap();
            writeln!(
                code,
                "                let sql = format!(\"UPDATE {schema}.{table} SET {{}} WHERE id = {{}}\", set_clauses.join(\", \"), id_placeholder);",
                schema = tree.schema_name,
                table = q(&tree.table_name),
            )
            .unwrap();
            writeln!(
                code,
                "                values.push(sea_orm::Value::Uuid(Some(Box::new(id))));"
            )
            .unwrap();
            writeln!(
                code,
                "                let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, &sql, values);"
            )
            .unwrap();
            writeln!(code, "                tx.execute(stmt).await?;").unwrap();
            writeln!(code, "            }}").unwrap();
            writeln!(code, "        }}").unwrap();
        }

        // Update child tables: delete existing + re-insert when field is present
        for child in &tree.child_tables {
            // Skip child tables with no data columns and no nested children.
            if child.columns.is_empty() && child.child_tables.is_empty() {
                continue;
            }
            let col_names: Vec<String> =
                child.columns.iter().map(|c| q(&c.pg_column_name)).collect();
            let placeholders: Vec<String> = child
                .columns
                .iter()
                .enumerate()
                .map(|(i, col)| {
                    let base = format!("${}", i + 3);
                    if let Some(ref cast) = col.pg_cast {
                        format!("{base}::{cast}")
                    } else {
                        base
                    }
                })
                .collect();
            let insert_sql = if col_names.is_empty() {
                format!(
                    "INSERT INTO {}.{} (id, {}) VALUES ($1, $2)",
                    child.sql_schema_name,
                    q(&child.sql_table_name),
                    child.parent_fk_column,
                )
            } else {
                format!(
                    "INSERT INTO {}.{} (id, {}, {}) VALUES ($1, $2, {})",
                    child.sql_schema_name,
                    q(&child.sql_table_name),
                    child.parent_fk_column,
                    col_names.join(", "),
                    placeholders.join(", "),
                )
            };
            let delete_sql = format!(
                "DELETE FROM {}.{} WHERE {} = $1",
                child.sql_schema_name,
                q(&child.sql_table_name),
                child.parent_fk_column,
            );

            writeln!(code).unwrap();
            if child.is_array {
                writeln!(
                    code,
                    "        // Replace child rows: {}.{}",
                    child.sql_schema_name, child.sql_table_name
                )
                .unwrap();
                writeln!(
                    code,
                    "        if let Some(ref items) = cmd.{} {{",
                    child.field_name
                )
                .unwrap();
                writeln!(
                    code,
                    "            let del = Statement::from_sql_and_values(DatabaseBackend::Postgres, \"{}\", vec![id.into()]);",
                    delete_sql
                )
                .unwrap();
                writeln!(code, "            tx.execute(del).await?;").unwrap();
                let item_var = if child.columns.is_empty() && child.child_tables.is_empty() {
                    "_item"
                } else {
                    "item"
                };
                writeln!(code, "            for {} in items {{", item_var).unwrap();
                writeln!(code, "                let child_id = Uuid::new_v4();").unwrap();
                writeln!(
                    code,
                    "                let stmt = Statement::from_sql_and_values("
                )
                .unwrap();
                writeln!(code, "                    DatabaseBackend::Postgres,").unwrap();
                writeln!(code, "                    \"{}\",", insert_sql).unwrap();
                write!(code, "                    vec![child_id.into(), id.into()").unwrap();
                for col in &child.columns {
                    emit_child_col_write_value(code, col);
                }
                writeln!(code, "],").unwrap();
                writeln!(code, "                );").unwrap();
                writeln!(code, "                tx.execute(stmt).await?;").unwrap();
                emit_child_inserts(code, &child.child_tables, "child_id", "item", 4);
                writeln!(code, "            }}").unwrap();
                writeln!(code, "        }}").unwrap();
            } else {
                writeln!(
                    code,
                    "        // Replace optional child row: {}.{}",
                    child.sql_schema_name, child.sql_table_name
                )
                .unwrap();
                let item_var = if child.columns.is_empty() && child.child_tables.is_empty() {
                    "_item"
                } else {
                    "item"
                };
                writeln!(
                    code,
                    "        if let Some(ref {}) = cmd.{} {{",
                    item_var, child.field_name
                )
                .unwrap();
                writeln!(
                    code,
                    "            let del = Statement::from_sql_and_values(DatabaseBackend::Postgres, \"{}\", vec![id.into()]);",
                    delete_sql
                )
                .unwrap();
                writeln!(code, "            tx.execute(del).await?;").unwrap();
                writeln!(code, "            let child_id = Uuid::new_v4();").unwrap();
                writeln!(
                    code,
                    "            let stmt = Statement::from_sql_and_values("
                )
                .unwrap();
                writeln!(code, "                DatabaseBackend::Postgres,").unwrap();
                writeln!(code, "                \"{}\",", insert_sql).unwrap();
                write!(code, "                vec![child_id.into(), id.into()").unwrap();
                for col in &child.columns {
                    emit_child_col_write_value(code, col);
                }
                writeln!(code, "],").unwrap();
                writeln!(code, "            );").unwrap();
                writeln!(code, "            tx.execute(stmt).await?;").unwrap();
                emit_child_inserts(code, &child.child_tables, "child_id", "item", 3);
                writeln!(code, "        }}").unwrap();
            }
        }

        writeln!(code).unwrap();
        writeln!(code, "        Ok(())").unwrap();
        writeln!(code, "    }}").unwrap();
    }

    fn emit_delete_fn(&self, tree: &EntityTree, code: &mut String) {
        writeln!(code).unwrap();
        writeln!(
            code,
            "    #[tracing::instrument(skip(self, tx), fields(db.operation = \"delete\", db.table = \"{}.{}\"))]",
            tree.schema_name, tree.table_name
        )
        .unwrap();
        writeln!(code, "    async fn delete(").unwrap();
        writeln!(code, "        &self,").unwrap();
        writeln!(code, "        tx: &DatabaseTransaction,").unwrap();
        writeln!(code, "        id: Uuid,").unwrap();
        writeln!(code, "    ) -> Result<(), Box<dyn std::error::Error>> {{").unwrap();
        if tree.is_auditable {
            writeln!(
                code,
                "        let model = crate::entity::{}::Entity::find()",
                tree.entity_module
            )
            .unwrap();
            writeln!(
                code,
                "            .filter(crate::entity::{}::Column::Id.eq(id))",
                tree.entity_module
            )
            .unwrap();
            writeln!(
                code,
                "            .filter(crate::entity::{}::Column::DeletedAt.is_null())",
                tree.entity_module
            )
            .unwrap();
            writeln!(code, "            .one(tx)").unwrap();
            writeln!(code, "            .await?").unwrap();
            writeln!(
                code,
                "            .ok_or_else(|| Box::<dyn std::error::Error>::from(\"Entity not found or already deleted\"))?;"
            )
            .unwrap();
            writeln!(
                code,
                "        let mut active: crate::entity::{}::ActiveModel = model.into();",
                tree.entity_module
            )
            .unwrap();
            writeln!(
                code,
                "        active.deleted_at = sea_orm::ActiveValue::Set(Some(chrono::Utc::now().into()));"
            )
            .unwrap();
            writeln!(code, "        match active.update(tx).await {{").unwrap();
            writeln!(code, "            Ok(_) => {{}}").unwrap();
            writeln!(code, "            Err(sea_orm::DbErr::RecordNotUpdated) => {{ /* RLS hid the row — find_by_id will return 404 */ }}").unwrap();
            writeln!(code, "            Err(e) => return Err(e.into()),").unwrap();
            writeln!(code, "        }}").unwrap();
        } else {
            writeln!(
                code,
                "        // CASCADE handles child cleanup for {}.{}",
                tree.schema_name, tree.table_name
            )
            .unwrap();
            writeln!(
                code,
                "        crate::entity::{}::Entity::delete_by_id(id)",
                tree.entity_module
            )
            .unwrap();
            writeln!(code, "            .exec(tx)").unwrap();
            writeln!(code, "            .await?;").unwrap();
        }
        writeln!(code, "        Ok(())").unwrap();
        writeln!(code, "    }}").unwrap();
    }

    fn emit_list_fn(&self, tree: &EntityTree, code: &mut String) {
        writeln!(code).unwrap();
        writeln!(
            code,
            "    #[tracing::instrument(skip(self, db), fields(db.operation = \"select_list\", db.table = \"{}.{}\"))]",
            tree.schema_name, tree.table_name
        )
        .unwrap();
        writeln!(code, "    async fn list(").unwrap();
        writeln!(code, "        &self,").unwrap();
        writeln!(code, "        db: &DatabaseTransaction,").unwrap();
        writeln!(code, "        page: u64,").unwrap();
        writeln!(code, "        page_size: u64,").unwrap();
        writeln!(
            code,
            "        filters: &std::collections::HashMap<String, String>,"
        )
        .unwrap();
        writeln!(
            code,
            "    ) -> Result<(Vec<{}Response>, u64), Box<dyn std::error::Error>> {{",
            tree.entity_name
        )
        .unwrap();

        // Build filter condition from JSON:API filter params.
        let has_any_filters =
            !tree.filter_fields.is_empty() || !tree.nested_filter_fields.is_empty() || tree.parent_ref.is_some();
        if has_any_filters {
            writeln!(
                code,
                "        let mut condition = sea_orm::Condition::all();"
            )
            .unwrap();

            // --- Direct column filters ---
            for ff in &tree.filter_fields {
                // Strip r# raw identifier prefix before PascalCase conversion —
                // SeaORM Column variants use the bare name (e.g. `Type`, not `RType`).
                let bare_name = ff.field_name.strip_prefix("r#").unwrap_or(&ff.field_name);
                let pascal_col = codegraph_naming::to_pascal_case(bare_name);
                writeln!(
                    code,
                    "        if let Some(val) = filters.get(\"{}\") {{",
                    ff.field_name
                )
                .unwrap();
                // Generate type-appropriate parsing.
                match ff.rust_type.as_str() {
                    "Uuid" | "uuid::Uuid" => {
                        writeln!(code, "            let parsed = uuid::Uuid::parse_str(val).map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid UUID for filter '{}': {{e}}\", )))?;", ff.field_name).unwrap();
                        writeln!(code, "            condition = condition.add(crate::entity::{}::Column::{}.eq(parsed));", tree.entity_module, pascal_col).unwrap();
                    }
                    "i32" => {
                        writeln!(code, "            let parsed: i32 = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid i32 for filter '{}': {{e}}\")))?;", ff.field_name).unwrap();
                        writeln!(code, "            condition = condition.add(crate::entity::{}::Column::{}.eq(parsed));", tree.entity_module, pascal_col).unwrap();
                    }
                    "i64" => {
                        writeln!(code, "            let parsed: i64 = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid i64 for filter '{}': {{e}}\")))?;", ff.field_name).unwrap();
                        writeln!(code, "            condition = condition.add(crate::entity::{}::Column::{}.eq(parsed));", tree.entity_module, pascal_col).unwrap();
                    }
                    "bool" => {
                        writeln!(code, "            let parsed: bool = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid bool for filter '{}': {{e}}\")))?;", ff.field_name).unwrap();
                        writeln!(code, "            condition = condition.add(crate::entity::{}::Column::{}.eq(parsed));", tree.entity_module, pascal_col).unwrap();
                    }
                    _ => {
                        // String and everything else — exact match.
                        writeln!(code, "            condition = condition.add(crate::entity::{}::Column::{}.eq(val.clone()));", tree.entity_module, pascal_col).unwrap();
                    }
                }
                writeln!(code, "        }}").unwrap();
            }

            // --- Parent ref filter (child entity scoped to a parent) ---
            if let Some(ref parent_ref) = tree.parent_ref {
                let pascal_col = codegraph_naming::to_pascal_case(parent_ref);
                writeln!(code, "        if let Some(val) = filters.get(\"{parent_ref}\") {{").unwrap();
                writeln!(code, "            let parsed = uuid::Uuid::parse_str(val).map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid UUID for filter '{parent_ref}': {{e}}\")))?;").unwrap();
                writeln!(code, "            condition = condition.add(crate::entity::{}::Column::{}.eq(parsed));", tree.entity_module, pascal_col).unwrap();
                writeln!(code, "        }}").unwrap();
            }

            // --- Nested (child / grandchild) filters via EXISTS subqueries ---
            // All identifiers (schema, table, column) are always double-quoted in the
            // generated SQL to guard against future names that match PG reserved words.
            for nf in &tree.nested_filter_fields {
                writeln!(
                    code,
                    "        if let Some(val) = filters.get(\"{}\") {{",
                    nf.filter_key
                )
                .unwrap();

                // Type-safe value parsing — same patterns as direct filters.
                let val_expr = emit_nested_filter_parse(code, nf);

                if let Some(ref ij) = nf.intermediate_join {
                    // Grandchild: nested EXISTS through an intermediate child table.
                    //   EXISTS (SELECT 1 FROM intermediate WHERE intermediate.parent_fk = parent.id
                    //     AND EXISTS (SELECT 1 FROM grandchild WHERE grandchild.child_fk = intermediate.id
                    //       AND grandchild.column = $value))
                    writeln!(
                        code,
                        "            condition = condition.add(sea_orm::Condition::any().add(sea_orm::sea_query::Expr::cust_with_values("
                    ).unwrap();
                    writeln!(
                        code,
                        "                \"EXISTS (SELECT 1 FROM \\\"{}\\\".\\\"{}\\\" _intermediate WHERE _intermediate.\\\"{}\\\" = \\\"{}\\\".\\\"{}\\\".\\\"id\\\" AND EXISTS (SELECT 1 FROM \\\"{}\\\".\\\"{}\\\" _gc WHERE _gc.\\\"{}\\\" = _intermediate.\\\"id\\\" AND _gc.\\\"{}\\\" = $1))\",",
                        ij.sql_schema,
                        ij.sql_table_name,
                        ij.parent_fk_column,
                        tree.schema_name,
                        tree.table_name,
                        nf.sql_schema,
                        nf.sql_table_name,
                        nf.parent_fk_column,
                        nf.pg_column_name,
                    ).unwrap();
                    writeln!(
                        code,
                        "                vec![sea_orm::Value::from({val_expr})],"
                    )
                    .unwrap();
                    writeln!(code, "            )));").unwrap();
                } else {
                    // Direct child: single EXISTS subquery.
                    //   EXISTS (SELECT 1 FROM child WHERE child.parent_fk = parent.id AND child.column = $value)
                    writeln!(
                        code,
                        "            condition = condition.add(sea_orm::Condition::any().add(sea_orm::sea_query::Expr::cust_with_values("
                    ).unwrap();
                    writeln!(
                        code,
                        "                \"EXISTS (SELECT 1 FROM \\\"{}\\\".\\\"{}\\\" _child WHERE _child.\\\"{}\\\" = \\\"{}\\\".\\\"{}\\\".\\\"id\\\" AND _child.\\\"{}\\\" = $1)\",",
                        nf.sql_schema,
                        nf.sql_table_name,
                        nf.parent_fk_column,
                        tree.schema_name,
                        tree.table_name,
                        nf.pg_column_name,
                    ).unwrap();
                    writeln!(
                        code,
                        "                vec![sea_orm::Value::from({val_expr})],"
                    )
                    .unwrap();
                    writeln!(code, "            )));").unwrap();
                }

                writeln!(code, "        }}").unwrap();
            }
        }

        writeln!(
            code,
            "        let query = crate::entity::{}::Entity::find()",
            tree.entity_module
        )
        .unwrap();
        if has_any_filters {
            writeln!(code, "            .filter(condition)").unwrap();
        }
        if tree.is_auditable {
            writeln!(
                code,
                "            .filter(crate::entity::{}::Column::DeletedAt.is_null())",
                tree.entity_module
            )
            .unwrap();
        }
        writeln!(
            code,
            "            .order_by_desc(crate::entity::{}::Column::CreatedAt);",
            tree.entity_module
        )
        .unwrap();
        writeln!(
            code,
            "        let paginator = query.paginate(db, page_size);"
        )
        .unwrap();
        writeln!(code).unwrap();
        writeln!(code, "        let total = paginator.num_items().await?;").unwrap();
        writeln!(
            code,
            "        let rows = paginator.fetch_page(page).await?;"
        )
        .unwrap();
        writeln!(code).unwrap();
        writeln!(
            code,
            "        let mut results = Vec::with_capacity(rows.len());"
        )
        .unwrap();
        writeln!(code, "        for row in rows {{").unwrap();

        // Query child tables for each parent row (recursively handles nested children).
        emit_child_reads(code, &tree.child_tables, "row.id", 3);

        writeln!(
            code,
            "            results.push({}Response {{",
            tree.entity_name
        )
        .unwrap();
        writeln!(code, "                id: row.id,").unwrap();
        for col in &tree.direct_columns {
            if col.is_composite_range {
                continue;
            }
            emit_entity_to_dto_field(code, col, "row", "                ");
        }
        emit_child_field_population(code, &tree.child_tables, "                ");
        if tree.has_workflow {
            writeln!(code, "                workflow_state: None,").unwrap();
        }
        writeln!(code, "                created_at: row.created_at,").unwrap();
        writeln!(code, "                updated_at: row.updated_at,").unwrap();
        writeln!(code, "            }});").unwrap();
        writeln!(code, "        }}").unwrap();
        writeln!(code).unwrap();
        writeln!(code, "        Ok((results, total))").unwrap();
        writeln!(code, "    }}").unwrap();
    }

    /// Emit full-text search that returns ranked IDs and total count.
    fn emit_search_fn(&self, tree: &EntityTree, code: &mut String) {
        writeln!(code).unwrap();
        writeln!(
            code,
            "    #[tracing::instrument(skip(self, db), fields(db.operation = \"search_ids\", db.table = \"{}.{}\"  ))]",
            tree.schema_name, tree.table_name
        )
        .unwrap();
        writeln!(code, "    async fn search_ids(").unwrap();
        writeln!(code, "        &self,").unwrap();
        writeln!(code, "        db: &DatabaseTransaction,").unwrap();
        writeln!(code, "        query: &str,").unwrap();
        writeln!(code, "        page: u64,").unwrap();
        writeln!(code, "        page_size: u64,").unwrap();
        writeln!(
            code,
            "    ) -> Result<(Vec<uuid::Uuid>, u64), Box<dyn std::error::Error>> {{"
        )
        .unwrap();
        writeln!(
            code,
            "        let count_stmt = Statement::from_sql_and_values("
        )
        .unwrap();
        writeln!(code, "            DatabaseBackend::Postgres,").unwrap();
        writeln!(
            code,
            "            \"SELECT COUNT(*) AS count FROM {}.{} WHERE search_tsv @@ websearch_to_tsquery('{}', $1)\",",
            tree.schema_name, q(&tree.table_name), tree.fts_language
        )
        .unwrap();
        writeln!(code, "            vec![query.into()],").unwrap();
        writeln!(code, "        );").unwrap();
        writeln!(
            code,
            "        let count_row = db.query_one(count_stmt).await?"
        )
        .unwrap();
        writeln!(
            code,
            "            .ok_or(\"count query returned no rows\")?;"
        )
        .unwrap();
        writeln!(
            code,
            "        let total: i64 = count_row.try_get(\"\", \"count\")?;"
        )
        .unwrap();
        writeln!(code, "        let total = total as u64;").unwrap();
        writeln!(code).unwrap();
        writeln!(code, "        let offset = page * page_size;").unwrap();
        writeln!(code, "        let stmt = Statement::from_sql_and_values(").unwrap();
        writeln!(code, "            DatabaseBackend::Postgres,").unwrap();
        writeln!(
            code,
            "            \"SELECT id FROM {}.{} WHERE search_tsv @@ websearch_to_tsquery('{}', $1) ORDER BY ts_rank(search_tsv, websearch_to_tsquery('{}', $1)) DESC LIMIT $2 OFFSET $3\",",
            tree.schema_name, q(&tree.table_name), tree.fts_language, tree.fts_language
        )
        .unwrap();
        writeln!(
            code,
            "            vec![query.into(), (page_size as i64).into(), (offset as i64).into()],"
        )
        .unwrap();
        writeln!(code, "        );").unwrap();
        writeln!(code, "        let rows = db.query_all(stmt).await?;").unwrap();
        writeln!(code, "        let ids: Vec<uuid::Uuid> = rows.iter()").unwrap();
        writeln!(
            code,
            "            .filter_map(|r| r.try_get::<uuid::Uuid>(\"\", \"id\").ok())"
        )
        .unwrap();
        writeln!(code, "            .collect();").unwrap();
        writeln!(code).unwrap();
        writeln!(code, "        Ok((ids, total))").unwrap();
        writeln!(code, "    }}").unwrap();
    }

    /// Emit semantic similarity search that returns IDs ordered by cosine similarity.
    fn emit_semantic_search_fn(&self, tree: &EntityTree, code: &mut String) {
        writeln!(code).unwrap();
        writeln!(
            code,
            "    #[tracing::instrument(skip(self, db, embedding), fields(db.operation = \"semantic_search_ids\", db.table = \"{}.{}\"  ))]",
            tree.schema_name, tree.table_name
        )
        .unwrap();
        writeln!(code, "    async fn semantic_search_ids(").unwrap();
        writeln!(code, "        &self,").unwrap();
        writeln!(code, "        db: &DatabaseTransaction,").unwrap();
        writeln!(code, "        embedding: &[f32],").unwrap();
        writeln!(code, "        limit: u64,").unwrap();
        writeln!(
            code,
            "    ) -> Result<Vec<uuid::Uuid>, Box<dyn std::error::Error>> {{"
        )
        .unwrap();
        writeln!(code, "        let vec_str = format!(\"[{{}}]\", embedding.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(\",\"));").unwrap();
        writeln!(code, "        let stmt = Statement::from_sql_and_values(").unwrap();
        writeln!(code, "            DatabaseBackend::Postgres,").unwrap();
        let emb_col = format!("{}_embedding", tree.table_name);
        writeln!(
            code,
            "            \"SELECT id FROM {schema}.{table} ORDER BY {col} <=> $1::vector LIMIT $2\",",
            schema = tree.schema_name,
            table = q(&tree.table_name),
            col = emb_col
        )
        .unwrap();
        writeln!(
            code,
            "            vec![vec_str.into(), (limit as i64).into()],"
        )
        .unwrap();
        writeln!(code, "        );").unwrap();
        writeln!(code, "        let rows = db.query_all(stmt).await?;").unwrap();
        writeln!(code, "        let ids: Vec<uuid::Uuid> = rows.iter()").unwrap();
        writeln!(
            code,
            "            .filter_map(|r| r.try_get::<uuid::Uuid>(\"\", \"id\").ok())"
        )
        .unwrap();
        writeln!(code, "            .collect();").unwrap();
        writeln!(code).unwrap();
        writeln!(code, "        Ok(ids)").unwrap();
        writeln!(code, "    }}").unwrap();
    }

    /// Emit `find_tree` — recursive CTE fetching the subtree rooted at `root_id`.
    fn emit_find_tree_fn(&self, tree: &EntityTree, code: &mut String) {
        let hf = tree.hierarchy_field.as_deref().unwrap();
        let has_tree_include = !tree.tree_include.is_empty();
        writeln!(code).unwrap();
        writeln!(
            code,
            "    #[tracing::instrument(skip(self, db), fields(db.operation = \"find_tree\", db.table = \"{}.{}\"))]",
            tree.schema_name, tree.table_name
        )
        .unwrap();
        writeln!(code, "    async fn find_tree(").unwrap();
        writeln!(code, "        &self,").unwrap();
        writeln!(code, "        db: &DatabaseTransaction,").unwrap();
        writeln!(code, "        root_id: Uuid,").unwrap();
        writeln!(code, "        max_depth: Option<i32>,").unwrap();
        if has_tree_include {
            writeln!(
                code,
                "    ) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {{"
            )
            .unwrap();
        } else {
            writeln!(
                code,
                "    ) -> Result<Vec<{}Response>, Box<dyn std::error::Error>> {{",
                tree.entity_name
            )
            .unwrap();
        }
        writeln!(code, "        let sql = if let Some(depth) = max_depth {{").unwrap();
        writeln!(
            code,
            "            format!(\"WITH RECURSIVE tree AS (SELECT *, 0 AS _tree_depth FROM {schema}.{table} WHERE id = $1 UNION ALL SELECT c.*, t._tree_depth + 1 AS _tree_depth FROM {schema}.{table} c JOIN tree t ON c.{hf} = t.id WHERE t._tree_depth < $2) SELECT * FROM tree ORDER BY _tree_depth, created_at\",)",
            schema = tree.schema_name,
            table = q(&tree.table_name),
            hf = hf
        )
        .unwrap();
        writeln!(code, "        }} else {{").unwrap();
        writeln!(
            code,
            "            format!(\"WITH RECURSIVE tree AS (SELECT *, 0 AS _tree_depth FROM {schema}.{table} WHERE id = $1 UNION ALL SELECT c.*, t._tree_depth + 1 AS _tree_depth FROM {schema}.{table} c JOIN tree t ON c.{hf} = t.id) SELECT * FROM tree ORDER BY _tree_depth, created_at\",)",
            schema = tree.schema_name,
            table = q(&tree.table_name),
            hf = hf
        )
        .unwrap();
        writeln!(code, "        }};").unwrap();
        writeln!(
            code,
            "        let values = if let Some(depth) = max_depth {{"
        )
        .unwrap();
        writeln!(code, "            vec![root_id.into(), depth.into()]").unwrap();
        writeln!(code, "        }} else {{").unwrap();
        writeln!(code, "            vec![root_id.into()]").unwrap();
        writeln!(code, "        }};").unwrap();
        writeln!(
            code,
            "        let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, sql, values);"
        )
        .unwrap();
        writeln!(
            code,
            "        let rows = crate::entity::{}::Entity::find()",
            tree.entity_module
        )
        .unwrap();
        writeln!(code, "            .from_raw_sql(stmt)").unwrap();
        writeln!(code, "            .all(db)").unwrap();
        writeln!(code, "            .await?;").unwrap();
        writeln!(code).unwrap();

        if has_tree_include {
            // Emit worker map query
            self.emit_tree_include_worker_fetch(tree, code);
        }

        // Emit result construction (Vec<Response> or Vec<Value>)
        writeln!(
            code,
            "        let mut results = Vec::with_capacity(rows.len());"
        )
        .unwrap();
        writeln!(code, "        for row in rows {{").unwrap();
        emit_child_reads(code, &tree.child_tables, "row.id", 3);
        if has_tree_include {
            writeln!(
                code,
                "            let mut val = serde_json::to_value({}Response {{",
                tree.entity_name
            )
            .unwrap();
        } else {
            writeln!(
                code,
                "            results.push({}Response {{",
                tree.entity_name
            )
            .unwrap();
        }
        writeln!(code, "                id: row.id,").unwrap();
        for col in &tree.direct_columns {
            if col.is_composite_range {
                continue;
            }
            emit_entity_to_dto_field(code, col, "row", "                ");
        }
        emit_child_field_population(code, &tree.child_tables, "                ");
        if tree.has_workflow {
            writeln!(code, "                workflow_state: None,").unwrap();
        }
        writeln!(code, "                created_at: row.created_at,").unwrap();
        writeln!(code, "                updated_at: row.updated_at,").unwrap();
        if has_tree_include {
            writeln!(code, "            }}).map_err(|e| -> Box<dyn std::error::Error> {{ format!(\"Serialization error: {{e}}\").into() }})?;").unwrap();
            // Emit worker merge block
            for inc in &tree.tree_include {
                writeln!(
                    code,
                    "            if let Some(worker) = worker_map.get(&row.id) {{"
                )
                .unwrap();
                writeln!(
                    code,
                    "                val.as_object_mut().unwrap().insert(\"{}\".to_string(), worker.clone());",
                    inc.alias
                )
                .unwrap();
                writeln!(code, "            }}").unwrap();
            }
            writeln!(code, "            results.push(val);").unwrap();
        } else {
            writeln!(code, "            }});").unwrap();
        }
        writeln!(code, "        }}").unwrap();
        writeln!(code).unwrap();
        writeln!(code, "        Ok(results)").unwrap();
        writeln!(code, "    }}").unwrap();
    }

    /// Emit code that fetches tree_include worker data into a position_id→Value map.
    fn emit_tree_include_worker_fetch(&self, tree: &EntityTree, code: &mut String) {
        writeln!(
            code,
            "        let mut worker_map: std::collections::HashMap<Uuid, serde_json::Value> = std::collections::HashMap::new();"
        )
        .unwrap();
        writeln!(code, "        let pos_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();").unwrap();
        writeln!(code).unwrap();
        for inc in &tree.tree_include {
            // Build JOIN chain from parent table through worker detail tables.
            // The composition tree tells us: WorkerType → WorkerPersonType → WorkerPersonNameType.
            // We assign fixed aliases: w (worker), wp (worker_person), wpn (worker_person_name).
            let mut from_clause = format!(
                "{} d JOIN {} w ON w.id = d.\"{}\" AND w.deleted_at IS NULL",
                inc.via_table, inc.parent_table, inc.parent_ref_column
            );
            let mut has_person = false;
            // worker_detail_joins is [person→worker, name→person] after reverse.
            // Index 0 (person): alias "wp", parent alias "w" (worker)
            // Index 1 (name): alias "wpn", parent alias "wp" (person)
            for (i, (table, fk_col, parent_alias)) in inc.worker_detail_joins.iter().enumerate() {
                let alias = if i == 0 { "wp" } else { "wpn" };
                from_clause.push_str(&format!(" JOIN {} {} ON {}.{} = {}.id", table, alias, alias, fk_col, parent_alias));
                has_person = true;
            }
            if has_person {
                // Escape double-quotes in from_clause for embedding in Rust string literals
                let escaped_from = from_clause.replace('"', "\\\"");
                writeln!(
                    code,
                    "        let worker_sql = format!(\"SELECT d.\\\"{fk}\\\" AS position_id, jsonb_build_object('id', w.id, 'given_name', wpn.given, 'family_name', wpn.family, 'avatar_url', wp.avatar_url) AS deployed_worker FROM {from} WHERE d.\\\"{fk}\\\" = ANY($1) AND d.deleted_at IS NULL\");",
                    fk = inc.via_fk_column,
                    from = escaped_from,
                )
                .unwrap();
            } else {
                // Fallback: no person chain, just return worker ID
                writeln!(
                    code,
                    "        let worker_sql = format!(\"SELECT d.\\\"{fk}\\\" AS position_id, jsonb_build_object('id', w.id) AS deployed_worker FROM {from} WHERE d.\\\"{fk}\\\" = ANY($1) AND d.deleted_at IS NULL\");",
                    fk = inc.via_fk_column,
                    from = from_clause,
                )
                .unwrap();
            }
            writeln!(
                code,
                "        let worker_stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, worker_sql, vec![pos_ids.clone().into()]);"
            )
            .unwrap();
            writeln!(code, "        let worker_rows = db.query_all(worker_stmt).await?;").unwrap();
            writeln!(code, "        for wr in &worker_rows {{").unwrap();
            writeln!(code, "            let pos_id: Uuid = wr.try_get_by_index(0).map_err(|e| -> Box<dyn std::error::Error> {{ format!(\"Missing position_id: {{e}}\").into() }})?;").unwrap();
            writeln!(code, "            let worker_json: serde_json::Value = wr.try_get_by_index(1).map_err(|e| -> Box<dyn std::error::Error> {{ format!(\"Missing deployed_worker: {{e}}\").into() }})?;").unwrap();
            writeln!(code, "            worker_map.insert(pos_id, worker_json);").unwrap();
            writeln!(code, "        }}").unwrap();
            writeln!(code).unwrap();
        }
    }

    /// Emit `find_ancestors` — recursive CTE fetching ancestors from a node to the root.
    fn emit_find_ancestors_fn(&self, tree: &EntityTree, code: &mut String) {
        let hf = tree.hierarchy_field.as_deref().unwrap();
        writeln!(code).unwrap();
        writeln!(
            code,
            "    #[tracing::instrument(skip(self, db), fields(db.operation = \"find_ancestors\", db.table = \"{}.{}\"))]",
            tree.schema_name, tree.table_name
        )
        .unwrap();
        writeln!(code, "    async fn find_ancestors(").unwrap();
        writeln!(code, "        &self,").unwrap();
        writeln!(code, "        db: &DatabaseTransaction,").unwrap();
        writeln!(code, "        node_id: Uuid,").unwrap();
        writeln!(
            code,
            "    ) -> Result<Vec<{}Response>, Box<dyn std::error::Error>> {{",
            tree.entity_name
        )
        .unwrap();
        writeln!(code, "        let stmt = Statement::from_sql_and_values(",).unwrap();
        writeln!(code, "            DatabaseBackend::Postgres,").unwrap();
        writeln!(
            code,
            "            \"WITH RECURSIVE ancestors AS (SELECT * FROM {schema}.{table} WHERE id = $1 UNION ALL SELECT p.* FROM {schema}.{table} p JOIN ancestors a ON a.{hf} = p.id) SELECT * FROM ancestors\",",
            schema = tree.schema_name,
            table = q(&tree.table_name),
            hf = hf
        )
        .unwrap();
        writeln!(code, "            vec![node_id.into()],").unwrap();
        writeln!(code, "        );").unwrap();
        writeln!(
            code,
            "        let rows = crate::entity::{}::Entity::find()",
            tree.entity_module
        )
        .unwrap();
        writeln!(code, "            .from_raw_sql(stmt)").unwrap();
        writeln!(code, "            .all(db)").unwrap();
        writeln!(code, "            .await?;").unwrap();
        writeln!(code).unwrap();
        writeln!(
            code,
            "        let mut results = Vec::with_capacity(rows.len());"
        )
        .unwrap();
        writeln!(code, "        for row in rows {{").unwrap();
        emit_child_reads(code, &tree.child_tables, "row.id", 3);
        writeln!(
            code,
            "            results.push({}Response {{",
            tree.entity_name
        )
        .unwrap();
        writeln!(code, "                id: row.id,").unwrap();
        for col in &tree.direct_columns {
            if col.is_composite_range {
                continue;
            }
            emit_entity_to_dto_field(code, col, "row", "                ");
        }
        emit_child_field_population(code, &tree.child_tables, "                ");
        if tree.has_workflow {
            writeln!(code, "                workflow_state: None,").unwrap();
        }
        writeln!(code, "                created_at: row.created_at,").unwrap();
        writeln!(code, "                updated_at: row.updated_at,").unwrap();
        writeln!(code, "            }});").unwrap();
        writeln!(code, "        }}").unwrap();
        writeln!(code).unwrap();
        writeln!(code, "        Ok(results)").unwrap();
        writeln!(code, "    }}").unwrap();
    }

    fn emit_include_fetch_methods(
        &self,
        tree: &EntityTree,
        code: &mut String,
        include_paths: &[ResolvedIncludePath],
        include_segment_dto_fields: &[Vec<Vec<String>>],
        include_segment_col_fields: &[Vec<Vec<String>>],
    ) {
        for (idx, path) in include_paths.iter().enumerate() {
            let per_seg_dto = include_segment_dto_fields.get(idx).map(|v| v.as_slice()).unwrap_or(&[]);
            let per_seg_col = include_segment_col_fields.get(idx).map(|v| v.as_slice()).unwrap_or(&[]);
            if path.segments.len() == 1 {
                let dto_fields = per_seg_dto.first().map(|v| v.as_slice()).unwrap_or(&[]);
                let col_fields = per_seg_col.first().map(|v| v.as_slice()).unwrap_or(&[]);
                self.emit_single_fetch_method(tree, code, path, dto_fields, col_fields);
                self.emit_batch_fetch_method(tree, code, path, dto_fields, col_fields);
            } else {
                let intermediate_dto = per_seg_dto.first().map(|v| v.as_slice()).unwrap_or(&[]);
                let leaf_dto = per_seg_dto.get(1).map(|v| v.as_slice()).unwrap_or(&[]);
                let intermediate_col = per_seg_col.first().map(|v| v.as_slice()).unwrap_or(&[]);
                let leaf_col = per_seg_col.get(1).map(|v| v.as_slice()).unwrap_or(&[]);
                self.emit_dot_fetch_method(tree, code, path, intermediate_dto, intermediate_col, leaf_dto, leaf_col);
            }
        }
    }

    /// Emit field assignments for a SeaORM entity row into a response struct.
    /// Uses `dto_fields` for the left side (response struct field names) and
    /// `col_fields` for the right side (entity Model field names from pg_column_name).
    /// These differ for codelist fields: DTO uses "worker_type", entity uses "worker_type_code".
    fn emit_field_assignments(code: &mut String, row_var: &str, dto_fields: &[String], col_fields: &[String]) {
        for (dto_name, col_name) in dto_fields.iter().zip(col_fields.iter()) {
            writeln!(code, "                {}: {}.{},", dto_name, row_var, col_name).unwrap();
        }
    }

    fn emit_single_fetch_method(
        &self,
        tree: &EntityTree,
        code: &mut String,
        path: &ResolvedIncludePath,
        dto_fields: &[String],
        col_fields: &[String],
    ) {
        let seg = &path.segments[0];
        let src_module = &tree.entity_module;
        let resp_type = &path.response_rust_type;
        let target_module = format!("{}_{}", seg.domain, seg.module_name);

        writeln!(code).unwrap();
        writeln!(
            code,
            "    pub(crate) async fn {}(",
            path.fetch_method
        )
        .unwrap();
        writeln!(code, "        &self,").unwrap();
        writeln!(code, "        db: &DatabaseTransaction,").unwrap();
        writeln!(code, "        source_id: Uuid,").unwrap();
        if seg.is_array {
            writeln!(
                code,
                "    ) -> Result<Vec<{}>, Box<dyn std::error::Error>> {{",
                resp_type
            )
            .unwrap();
        } else {
            writeln!(
                code,
                "    ) -> Result<Option<{}>, Box<dyn std::error::Error>> {{",
                resp_type
            )
            .unwrap();
        }

        if seg.is_array {
            let reverse_fk_pascal = codegraph_naming::to_pascal_case(&seg.reverse_fk_column);
            writeln!(
                code,
                "        let rows = crate::entity::{}::Entity::find()",
                target_module
            )
            .unwrap();
            writeln!(
                code,
                "            .filter(crate::entity::{}::Column::{}.eq(source_id))",
                target_module, reverse_fk_pascal
            )
            .unwrap();
            writeln!(code, "            .all(db)").unwrap();
            writeln!(code, "            .await?;").unwrap();
            writeln!(
                code,
                "        let mut results = Vec::with_capacity(rows.len());"
            )
            .unwrap();
            writeln!(code, "        for row in rows {{").unwrap();
            writeln!(code, "            results.push({} {{", resp_type).unwrap();
            writeln!(code, "                id: row.id,").unwrap();
            Self::emit_field_assignments(code, "row", dto_fields, col_fields);
            writeln!(code, "                created_at: row.created_at,").unwrap();
            writeln!(code, "                updated_at: row.updated_at,").unwrap();
            writeln!(code, "            }});").unwrap();
            writeln!(code, "        }}").unwrap();
            writeln!(code, "        Ok(results)").unwrap();
        } else {
            writeln!(
                code,
                "        let source = crate::entity::{}::Entity::find()",
                src_module
            )
            .unwrap();
            writeln!(
                code,
                "            .filter(crate::entity::{}::Column::Id.eq(source_id))",
                src_module
            )
            .unwrap();
            writeln!(code, "            .one(db)").unwrap();
            writeln!(code, "            .await?;").unwrap();
            writeln!(code, "        let source = match source {{").unwrap();
            writeln!(code, "            Some(s) => s,").unwrap();
            writeln!(code, "            None => return Ok(None),").unwrap();
            writeln!(code, "        }};").unwrap();
            writeln!(
                code,
                "        let fk_value = match source.{} {{",
                seg.fk_column
            )
            .unwrap();
            writeln!(code, "            Some(v) => v,").unwrap();
            writeln!(code, "            None => return Ok(None),").unwrap();
            writeln!(code, "        }};").unwrap();
            writeln!(
                code,
                "        let target = crate::entity::{}::Entity::find()",
                target_module
            )
            .unwrap();
            writeln!(
                code,
                "            .filter(crate::entity::{}::Column::Id.eq(fk_value))",
                target_module
            )
            .unwrap();
            writeln!(code, "            .one(db)").unwrap();
            writeln!(code, "            .await?;").unwrap();
            writeln!(code, "        let target = match target {{").unwrap();
            writeln!(code, "            Some(t) => t,").unwrap();
            writeln!(code, "            None => return Ok(None),").unwrap();
            writeln!(code, "        }};").unwrap();
            writeln!(code, "        Ok(Some({} {{", resp_type).unwrap();
            writeln!(code, "            id: target.id,").unwrap();
            Self::emit_field_assignments(code, "target", dto_fields, col_fields);
            writeln!(code, "            created_at: target.created_at,").unwrap();
            writeln!(code, "            updated_at: target.updated_at,").unwrap();
            writeln!(code, "        }}))").unwrap();
        }

        writeln!(code, "    }}").unwrap();
    }

    fn emit_batch_fetch_method(
        &self,
        tree: &EntityTree,
        code: &mut String,
        path: &ResolvedIncludePath,
        dto_fields: &[String],
        col_fields: &[String],
    ) {
        let seg = &path.segments[0];
        let src_module = &tree.entity_module;
        let resp_type = &path.response_rust_type;
        let target_module = format!("{}_{}", seg.domain, seg.module_name);

        writeln!(code).unwrap();
        writeln!(
            code,
            "    pub(crate) async fn {}(",
            path.batch_fetch_method
        )
        .unwrap();
        writeln!(code, "        &self,").unwrap();
        writeln!(code, "        db: &DatabaseTransaction,").unwrap();
        writeln!(code, "        source_ids: &[Uuid],").unwrap();
        if seg.is_array {
            writeln!(
                code,
                "    ) -> Result<std::collections::HashMap<Uuid, Vec<{}>>, Box<dyn std::error::Error>> {{",
                resp_type
            )
            .unwrap();
        } else {
            writeln!(
                code,
                "    ) -> Result<std::collections::HashMap<Uuid, Option<{}>>, Box<dyn std::error::Error>> {{",
                resp_type
            )
            .unwrap();
        }

        if seg.is_array {
            let reverse_fk_pascal = codegraph_naming::to_pascal_case(&seg.reverse_fk_column);
            writeln!(
                code,
                "        let rows = crate::entity::{}::Entity::find()",
                target_module
            )
            .unwrap();
            writeln!(
                code,
                "            .filter(crate::entity::{}::Column::{}.is_in(source_ids.to_vec()))",
                target_module, reverse_fk_pascal
            )
            .unwrap();
            writeln!(code, "            .all(db)").unwrap();
            writeln!(code, "            .await?;").unwrap();
            writeln!(
                code,
                "        let mut result: std::collections::HashMap<Uuid, Vec<{}>> = std::collections::HashMap::new();",
                resp_type
            )
            .unwrap();
            writeln!(code, "        for id in source_ids {{").unwrap();
            writeln!(
                code,
                "            result.entry(*id).or_insert_with(Vec::new);"
            )
            .unwrap();
            writeln!(code, "        }}").unwrap();
            writeln!(code, "        for row in rows {{").unwrap();
            writeln!(
                code,
                "            let key = row.{};",
                seg.reverse_fk_column
            )
            .unwrap();
            writeln!(
                code,
                "            result.entry(key).or_default().push({} {{",
                resp_type
            )
            .unwrap();
            writeln!(code, "                id: row.id,").unwrap();
            Self::emit_field_assignments(code, "row", dto_fields, col_fields);
            writeln!(code, "                created_at: row.created_at,").unwrap();
            writeln!(code, "                updated_at: row.updated_at,").unwrap();
            writeln!(code, "            }});").unwrap();
            writeln!(code, "        }}").unwrap();
        } else {
            writeln!(
                code,
                "        let sources = crate::entity::{}::Entity::find()",
                src_module
            )
            .unwrap();
            writeln!(
                code,
                "            .filter(crate::entity::{}::Column::Id.is_in(source_ids.to_vec()))",
                src_module
            )
            .unwrap();
            writeln!(code, "            .all(db)").unwrap();
            writeln!(code, "            .await?;").unwrap();
            writeln!(
                code,
                "        let mut fk_values: Vec<Uuid> = Vec::new();"
            )
            .unwrap();
            writeln!(code, "        for source in &sources {{").unwrap();
            writeln!(
                code,
                "            if let Some(fk) = source.{} {{",
                seg.fk_column
            )
            .unwrap();
            writeln!(code, "                fk_values.push(fk);").unwrap();
            writeln!(code, "            }}").unwrap();
            writeln!(code, "        }}").unwrap();
            writeln!(
                code,
                "        let targets = crate::entity::{}::Entity::find()",
                target_module
            )
            .unwrap();
            writeln!(
                code,
                "            .filter(crate::entity::{}::Column::Id.is_in(fk_values))",
                target_module
            )
            .unwrap();
            writeln!(code, "            .all(db)").unwrap();
            writeln!(code, "            .await?;").unwrap();
            writeln!(
                code,
                "        let target_by_id: std::collections::HashMap<Uuid, {}> = targets.into_iter().map(|t| (t.id, {} {{",
                resp_type, resp_type
            )
            .unwrap();
            writeln!(code, "            id: t.id,").unwrap();
            Self::emit_field_assignments(code, "t", dto_fields, col_fields);
            writeln!(code, "            created_at: t.created_at,").unwrap();
            writeln!(code, "            updated_at: t.updated_at,").unwrap();
            writeln!(
                code,
                "        }})).collect();"
            )
            .unwrap();
            writeln!(
                code,
                "        let mut result: std::collections::HashMap<Uuid, Option<{}>> = std::collections::HashMap::new();",
                resp_type
            )
            .unwrap();
            writeln!(code, "        for id in source_ids {{").unwrap();
            writeln!(
                code,
                "            let found = sources.iter().find(|s| s.id == *id).and_then(|s| s.{}.and_then(|fk| target_by_id.get(&fk).cloned()));",
                seg.fk_column
            )
            .unwrap();
            writeln!(
                code,
                "            result.insert(*id, found);"
            )
            .unwrap();
            writeln!(code, "        }}").unwrap();
        }

        writeln!(code, "        Ok(result)").unwrap();
        writeln!(code, "    }}").unwrap();
    }

    fn emit_dot_fetch_method(
        &self,
        _tree: &EntityTree,
        code: &mut String,
        path: &ResolvedIncludePath,
        intermediate_dto: &[String],
        intermediate_col: &[String],
        leaf_dto: &[String],
        leaf_col: &[String],
    ) {
        let seg0 = &path.segments[0];
        let seg1 = &path.segments[1];
        let resp_type = &path.response_rust_type;
        let leaf_resp_type = path.segments.last().map(|s| format!("{}Response", s.entity_name)).unwrap_or_default();
        let intermediate_module = format!("{}_{}", seg0.domain, seg0.module_name);
        let leaf_module = format!("{}_{}", seg1.domain, seg1.module_name);

        writeln!(code).unwrap();
        writeln!(
            code,
            "    pub(crate) async fn {}(",
            path.fetch_method
        )
        .unwrap();
        writeln!(code, "        &self,").unwrap();
        writeln!(code, "        db: &DatabaseTransaction,").unwrap();
        writeln!(code, "        source_id: Uuid,").unwrap();
        writeln!(
            code,
            "    ) -> Result<Option<{}>, Box<dyn std::error::Error>> {{",
            resp_type
        )
        .unwrap();

        writeln!(
            code,
            "        let intermediate = crate::entity::{}::Entity::find()",
            intermediate_module
        )
        .unwrap();
        writeln!(
            code,
            "            .filter(crate::entity::{}::Column::Id.eq(source_id))",
            intermediate_module
        )
        .unwrap();
        writeln!(code, "            .one(db)").unwrap();
        writeln!(code, "            .await?;").unwrap();
        writeln!(code, "        let intermediate = match intermediate {{").unwrap();
        writeln!(code, "            Some(s) => s,").unwrap();
        writeln!(code, "            None => return Ok(None),").unwrap();
        writeln!(code, "        }};").unwrap();
        writeln!(
            code,
            "        let fk_value = match intermediate.{} {{",
            seg1.fk_column
        )
        .unwrap();
        writeln!(code, "            Some(v) => v,").unwrap();
        writeln!(code, "            None => return Ok(None),").unwrap();
        writeln!(code, "        }};").unwrap();
        writeln!(
            code,
            "        let leaf = crate::entity::{}::Entity::find()",
            leaf_module
        )
        .unwrap();
        writeln!(
            code,
            "            .filter(crate::entity::{}::Column::Id.eq(fk_value))",
            leaf_module
        )
        .unwrap();
        writeln!(code, "            .one(db)").unwrap();
        writeln!(code, "            .await?;").unwrap();

        // Build enriched response: base fields from intermediate, nested leaf from leaf
        writeln!(code, "        let leaf_dto = leaf.map(|l| {} {{", leaf_resp_type).unwrap();
        writeln!(code, "            id: l.id,").unwrap();
        Self::emit_field_assignments(code, "l", leaf_dto, leaf_col);
        writeln!(code, "            created_at: l.created_at,").unwrap();
        writeln!(code, "            updated_at: l.updated_at,").unwrap();
        writeln!(code, "        }});").unwrap();

        writeln!(code, "        Ok(Some({} {{", resp_type).unwrap();
        writeln!(code, "            id: intermediate.id,").unwrap();
        // Intermediate entity fields go into the enriched struct base
        Self::emit_field_assignments(code, "intermediate", intermediate_dto, intermediate_col);
        writeln!(code, "            created_at: intermediate.created_at,").unwrap();
        writeln!(code, "            updated_at: intermediate.updated_at,").unwrap();
        writeln!(code, "            {}: leaf_dto,", seg1.module_name).unwrap();
        writeln!(code, "        }}))").unwrap();

        writeln!(code, "    }}").unwrap();
    }

    fn emit_footer(&self, code: &mut String) {
        writeln!(code, "}}").unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_column(
        field: &str,
        rust_type: &str,
        nullable: bool,
        dto_rust_type: Option<&str>,
        is_array: bool,
        is_structured_wrapper: bool,
    ) -> TreeColumn {
        TreeColumn {
            field_name: field.to_string(),
            pg_column_name: field.to_string(),
            dto_field_name: None,
            rust_type: rust_type.to_string(),
            is_nullable: nullable,
            is_entity_ref: false,
            dto_rust_type: dto_rust_type.map(|s| s.to_string()),
            is_workflow_managed: false,
            is_array,
            pg_cast: None,
            is_composite_range: false,
            is_structured_wrapper,
            is_media: false,
        }
    }

    // --- emit_entity_to_dto_field tests ---

    #[test]
    fn entity_to_dto_plain_required() {
        let col = make_column("given_name", "String", false, None, false, false);
        let mut code = String::new();
        emit_entity_to_dto_field(&mut code, &col, "row", "    ");
        assert_eq!(code, "    given_name: row.given_name,\n");
    }

    #[test]
    fn entity_to_dto_codelist_required() {
        let col = make_column(
            "gender_code",
            "String",
            false,
            Some("GenderCodeList"),
            false,
            false,
        );
        let mut code = String::new();
        emit_entity_to_dto_field(&mut code, &col, "row", "    ");
        assert!(code.contains(".parse().unwrap_or_default()"));
    }

    #[test]
    fn entity_to_dto_codelist_nullable() {
        let col = make_column(
            "currency_code",
            "String",
            true,
            Some("CurrencyCodeList"),
            false,
            false,
        );
        let mut code = String::new();
        emit_entity_to_dto_field(&mut code, &col, "row", "    ");
        assert!(code.contains(".and_then(|v| v.parse().ok())"));
    }

    #[test]
    fn entity_to_dto_codelist_array_required() {
        let col = make_column(
            "codes",
            "Vec<String>",
            false,
            Some("StatusEnum"),
            true,
            false,
        );
        let mut code = String::new();
        emit_entity_to_dto_field(&mut code, &col, "row", "    ");
        assert!(code.contains("filter_map"));
        assert!(!code.contains(".map(|v|"));
    }

    #[test]
    fn entity_to_dto_codelist_array_nullable() {
        let col = make_column(
            "codes",
            "Vec<String>",
            true,
            Some("StatusEnum"),
            true,
            false,
        );
        let mut code = String::new();
        emit_entity_to_dto_field(&mut code, &col, "row", "    ");
        assert!(code.contains(".map(|v| v.into_iter()"));
    }

    #[test]
    fn entity_to_dto_jsonb_required() {
        let col = make_column("address", "serde_json::Value", false, None, false, true);
        let mut code = String::new();
        emit_entity_to_dto_field(&mut code, &col, "row", "    ");
        assert!(code.contains("serde_json::from_value(row.address).unwrap_or_default()"));
    }

    #[test]
    fn entity_to_dto_jsonb_nullable() {
        let col = make_column("metadata", "serde_json::Value", true, None, false, true);
        let mut code = String::new();
        emit_entity_to_dto_field(&mut code, &col, "row", "    ");
        assert!(code.contains(".and_then(|v| serde_json::from_value(v).ok())"));
    }

    #[test]
    fn entity_to_dto_jsonb_array_required() {
        let col = make_column("tags", "serde_json::Value", false, None, true, true);
        let mut code = String::new();
        emit_entity_to_dto_field(&mut code, &col, "row", "    ");
        assert!(code.contains("serde_json::from_value(row.tags).unwrap_or_default()"));
    }

    #[test]
    fn entity_to_dto_jsonb_array_nullable() {
        let col = make_column("prefs", "serde_json::Value", true, None, true, true);
        let mut code = String::new();
        emit_entity_to_dto_field(&mut code, &col, "row", "    ");
        assert!(code.contains(".and_then(|v| serde_json::from_value(v).ok())"));
    }

    // --- emit_child_field_population tests ---

    #[test]
    fn child_population_array() {
        let children = vec![ChildTableInfo {
            field_name: "addresses".to_string(),
            struct_name: "CandidateAddress".to_string(),
            sql_table_name: "candidate_address".to_string(),
            sql_schema_name: "recruiting".to_string(),
            parent_fk_column: "candidate_id".to_string(),
            is_array: true,
            columns: vec![],
            child_tables: vec![],
        }];
        let mut code = String::new();
        emit_child_field_population(&mut code, &children, "    ");
        assert_eq!(code, "    addresses: addresses_rows,\n");
    }

    #[test]
    fn child_population_single() {
        let children = vec![ChildTableInfo {
            field_name: "profile".to_string(),
            struct_name: "CandidateProfile".to_string(),
            sql_table_name: "candidate_profile".to_string(),
            sql_schema_name: "recruiting".to_string(),
            parent_fk_column: "candidate_id".to_string(),
            is_array: false,
            columns: vec![],
            child_tables: vec![],
        }];
        let mut code = String::new();
        emit_child_field_population(&mut code, &children, "    ");
        assert_eq!(code, "    profile: profile_rows.into_iter().next(),\n");
    }

    #[test]
    fn child_population_empty() {
        let mut code = String::new();
        emit_child_field_population(&mut code, &[], "    ");
        assert_eq!(code, "");
    }
}
