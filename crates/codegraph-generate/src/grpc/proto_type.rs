use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PropertyNode;
use codegraph_type_contracts::{RefClassificationKind, TypeExpr, TypeExprError};

/// Mapped type information for a single protocol buffer field.
pub struct ProtoFieldType {
    /// The proto type as a string (e.g. "string", "int32",
    /// "google.protobuf.Timestamp", or a message name like "CandidateAddress").
    pub proto_type: String,
    /// The corresponding Rust type used by tonic/prost
    /// (e.g. "String", "i32", "prost_types::Timestamp").
    pub rust_type: String,
    /// Whether this type requires a proto import statement.
    pub is_import: bool,
    /// The proto import path, if `is_import` is true.
    pub import_path: Option<String>,
    /// Whether this type is a nested/complex message (not a scalar).
    pub is_message: bool,
}

/// Map a property's classification and type to its proto and tonic representations.
///
/// `db` and `entity_name` are provided for resolving referenced entities,
/// value object properties, and codelist cardinality — currently used only
/// for generating nested message names.
///
/// Read precedence (issue #277): the property's `TypeExpr` when resolvable,
/// else the legacy frozen-string path. Expressions the proto lowerings
/// cannot represent fall back identically — output is byte-identical with
/// the pre-#277 string path either way.
pub fn proto_type_from_field(
    prop: &PropertyNode,
    db: &dyn GraphQuerier,
    entity_name: &str,
) -> ProtoFieldType {
    if let Some(expr) = prop.effective_type_expr()
        && let Ok(field) = proto_type_from_expr(&expr, prop, entity_name)
    {
        return field;
    }
    proto_type_from_field_from_strings(prop, db, entity_name)
}

/// Pure lowering: `TypeExpr` → proto/tonic representation. Message names are
/// composed exactly as the string path does (from the property name and the
/// owning entity), so resolvable expressions are output-identical.
pub fn proto_type_from_expr(
    expr: &TypeExpr,
    prop: &PropertyNode,
    entity_name: &str,
) -> Result<ProtoFieldType, TypeExprError> {
    match expr {
        TypeExpr::Primitive { .. } => {
            let canonical = expr.canonical_rust_str()?;
            Ok(proto_type_from_rust_type(&canonical))
        }
        TypeExpr::Codelist { .. } => Ok(scalar("string", "String")),
        TypeExpr::Ref {
            kind: Some(kind),
            args,
            ..
        } => {
            if !args.is_empty() {
                return Err(TypeExprError::Unsupported {
                    target: "proto",
                    expr: format!("{expr:?}"),
                    reason: "parameterized references carry no wire-level semantics yet"
                        .to_string(),
                });
            }
            Ok(proto_type_from_kind(kind, prop, entity_name))
        }
        TypeExpr::Ref { kind: None, .. } => Err(TypeExprError::Unsupported {
            target: "proto",
            expr: format!("{expr:?}"),
            reason: "untyped reference has no wire representation".to_string(),
        }),
        TypeExpr::List(inner) => {
            let lowered = proto_type_from_expr(inner, prop, entity_name)?;
            if lowered.proto_type.starts_with("repeated ") {
                return Err(TypeExprError::Unsupported {
                    target: "proto",
                    expr: format!("{expr:?}"),
                    reason: "nested lists are not representable".to_string(),
                });
            }
            Ok(ProtoFieldType {
                proto_type: format!("repeated {}", lowered.proto_type),
                rust_type: prop.rust_field_type.clone(),
                is_import: lowered.is_import,
                import_path: lowered.import_path,
                is_message: false,
            })
        }
        TypeExpr::Optional(inner) => proto_type_from_expr(inner, prop, entity_name),
        TypeExpr::Record { .. } => Ok(ProtoFieldType {
            proto_type: "google.protobuf.Struct".to_string(),
            rust_type: "prost_types::Struct".to_string(),
            is_import: true,
            import_path: Some("google/protobuf/struct.proto".to_string()),
            is_message: true,
        }),
    }
}

/// Kind-driven proto mapping shared by the string and expression paths.
fn proto_type_from_kind(
    kind: &RefClassificationKind,
    prop: &PropertyNode,
    entity_name: &str,
) -> ProtoFieldType {
    match kind {
        RefClassificationKind::EntityReference => ProtoFieldType {
            proto_type: "string".to_string(),
            rust_type: "String".to_string(),
            is_import: false,
            import_path: None,
            is_message: false,
        },
        RefClassificationKind::CodelistReference
        | RefClassificationKind::CodelistCheck
        | RefClassificationKind::InlineEnum => ProtoFieldType {
            // Base type is string; the context builder may upgrade to a proto enum name
            // when the codelist cardinality is ≤ 20.
            proto_type: "string".to_string(),
            rust_type: "String".to_string(),
            is_import: false,
            import_path: None,
            is_message: false,
        },
        RefClassificationKind::ValueObject => {
            let msg_name = format!(
                "{}{}",
                entity_name,
                codegraph_naming::to_pascal_case(&prop.name)
            );
            ProtoFieldType {
                proto_type: msg_name.clone(),
                rust_type: msg_name,
                is_import: false,
                import_path: None,
                is_message: true,
            }
        }
        RefClassificationKind::CompositeWrapper => {
            let msg_name = format!(
                "{}{}",
                entity_name,
                codegraph_naming::to_pascal_case(&prop.name)
            );
            ProtoFieldType {
                proto_type: msg_name.clone(),
                rust_type: msg_name,
                is_import: false,
                import_path: None,
                is_message: true,
            }
        }
        RefClassificationKind::MediaWrapper => ProtoFieldType {
            proto_type: "MediaContent".to_string(),
            rust_type: "MediaContent".to_string(),
            is_import: false,
            import_path: None,
            is_message: true,
        },
        RefClassificationKind::StructuredWrapper => ProtoFieldType {
            proto_type: "google.protobuf.Struct".to_string(),
            rust_type: "prost_types::Struct".to_string(),
            is_import: true,
            import_path: Some("google/protobuf/struct.proto".to_string()),
            is_message: true,
        },
        RefClassificationKind::RangeWrapper => {
            let msg_name = format!("{}Range", codegraph_naming::to_pascal_case(&prop.name));
            ProtoFieldType {
                proto_type: msg_name.clone(),
                rust_type: msg_name,
                is_import: false,
                import_path: None,
                is_message: true,
            }
        }
        // PrimitiveWrapper resolves through the frozen Rust string, which the
        // expression path does not carry — callers fall back to the strings.
        RefClassificationKind::PrimitiveWrapper | RefClassificationKind::ArrayWrapper => {
            proto_type_from_rust_type(&prop.rust_field_type)
        }
    }
}

/// The legacy frozen-string path, unchanged from pre-#277 behavior.
fn proto_type_from_field_from_strings(
    prop: &PropertyNode,
    _db: &dyn GraphQuerier,
    entity_name: &str,
) -> ProtoFieldType {
    let kind = prop.effective_kind();
    match kind {
        None => proto_type_from_rust_type(&prop.rust_field_type),
        Some(RefClassificationKind::PrimitiveWrapper) => {
            proto_type_from_rust_type(&prop.rust_field_type)
        }
        Some(kind) => proto_type_from_kind(&kind, prop, entity_name),
    }
}

/// Map a Rust type string to its proto/tonic equivalent.
///
/// This is the fallback for `PrimitiveWrapper` and unclassified fields,
/// and serves as the inner-type resolver for `ArrayWrapper`.
pub fn proto_type_from_rust_type(rust_type: &str) -> ProtoFieldType {
    match rust_type {
        "String" => scalar("string", "String"),
        "i32" => scalar("int32", "i32"),
        "i16" => scalar("int32", "i32"),
        "i64" => scalar("int64", "i64"),
        "f32" => scalar("float", "f32"),
        "f64" => scalar("double", "f64"),
        "bool" => scalar("bool", "bool"),
        "uuid::Uuid" | "Uuid" => scalar("string", "String"),
        "rust_decimal::Decimal" | "Decimal" => scalar("string", "String"),
        "chrono::NaiveDate" | "NaiveDate" => ProtoFieldType {
            proto_type: "google.protobuf.Timestamp".to_string(),
            rust_type: "prost_types::Timestamp".to_string(),
            is_import: true,
            import_path: Some("google/protobuf/timestamp.proto".to_string()),
            is_message: true,
        },
        "chrono::DateTime<chrono::Utc>" => ProtoFieldType {
            proto_type: "google.protobuf.Timestamp".to_string(),
            rust_type: "prost_types::Timestamp".to_string(),
            is_import: true,
            import_path: Some("google/protobuf/timestamp.proto".to_string()),
            is_message: true,
        },
        "serde_json::Value" => ProtoFieldType {
            proto_type: "google.protobuf.Struct".to_string(),
            rust_type: "prost_types::Struct".to_string(),
            is_import: true,
            import_path: Some("google/protobuf/struct.proto".to_string()),
            is_message: true,
        },
        s if s.starts_with("Vec<") => {
            let inner_type = s
                .strip_prefix("Vec<")
                .and_then(|s| s.strip_suffix('>'))
                .unwrap_or(s);
            let inner = proto_type_from_rust_type(inner_type);
            ProtoFieldType {
                proto_type: format!("repeated {}", inner.proto_type),
                rust_type: s.to_string(),
                is_import: inner.is_import,
                import_path: inner.import_path,
                is_message: false,
            }
        }
        _ => scalar("string", "String"),
    }
}

fn scalar(proto_type: &str, rust_type: &str) -> ProtoFieldType {
    ProtoFieldType {
        proto_type: proto_type.to_string(),
        rust_type: rust_type.to_string(),
        is_import: false,
        import_path: None,
        is_message: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_core::mock::MockEngine;

    fn prop(
        name: &str,
        rust_field_type: &str,
        classification_kind: Option<RefClassificationKind>,
    ) -> PropertyNode {
        PropertyNode {
            name: name.to_string(),
            prop_type: String::new(),
            description: None,
            format: None,
            is_required: true,
            is_nullable: false,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: name.to_string(),
            pg_column_type: String::new(),
            rust_field_name: name.to_string(),
            rust_field_type: rust_field_type.to_string(),
            sea_orm_type: String::new(),
            render_strategy: String::new(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
            type_expr: None,
        }
    }

    fn empty_mock() -> MockEngine {
        MockEngine::builder().build()
    }

    // ── PrimitiveWrapper variants ────────────────────────────────────

    #[test]
    fn proto_type_for_string() {
        let p = prop(
            "name",
            "String",
            Some(RefClassificationKind::PrimitiveWrapper),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "string");
        assert_eq!(result.rust_type, "String");
        assert!(!result.is_import);
        assert!(!result.is_message);
    }

    #[test]
    fn proto_type_for_i32() {
        let p = prop("age", "i32", Some(RefClassificationKind::PrimitiveWrapper));
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "int32");
        assert_eq!(result.rust_type, "i32");
    }

    #[test]
    fn proto_type_for_i16() {
        let p = prop(
            "score",
            "i16",
            Some(RefClassificationKind::PrimitiveWrapper),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "int32");
        assert_eq!(result.rust_type, "i32");
    }

    #[test]
    fn proto_type_for_i64() {
        let p = prop(
            "count",
            "i64",
            Some(RefClassificationKind::PrimitiveWrapper),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "int64");
        assert_eq!(result.rust_type, "i64");
    }

    #[test]
    fn proto_type_for_f32() {
        let p = prop(
            "rating",
            "f32",
            Some(RefClassificationKind::PrimitiveWrapper),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "float");
        assert_eq!(result.rust_type, "f32");
    }

    #[test]
    fn proto_type_for_f64() {
        let p = prop(
            "score",
            "f64",
            Some(RefClassificationKind::PrimitiveWrapper),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "double");
        assert_eq!(result.rust_type, "f64");
    }

    #[test]
    fn proto_type_for_bool() {
        let p = prop(
            "active",
            "bool",
            Some(RefClassificationKind::PrimitiveWrapper),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "bool");
        assert_eq!(result.rust_type, "bool");
    }

    #[test]
    fn proto_type_for_uuid() {
        let p = prop(
            "id",
            "uuid::Uuid",
            Some(RefClassificationKind::PrimitiveWrapper),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "string");
        assert_eq!(result.rust_type, "String");
    }

    #[test]
    fn proto_type_for_decimal() {
        let p = prop(
            "amount",
            "rust_decimal::Decimal",
            Some(RefClassificationKind::PrimitiveWrapper),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "string");
        assert_eq!(result.rust_type, "String");
    }

    #[test]
    fn proto_type_for_naive_date() {
        let p = prop(
            "birth_date",
            "chrono::NaiveDate",
            Some(RefClassificationKind::PrimitiveWrapper),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "google.protobuf.Timestamp");
        assert_eq!(result.rust_type, "prost_types::Timestamp");
        assert!(result.is_import);
        assert_eq!(
            result.import_path.as_deref(),
            Some("google/protobuf/timestamp.proto")
        );
        assert!(result.is_message);
    }

    #[test]
    fn proto_type_for_datetime_utc() {
        let p = prop(
            "created_at",
            "chrono::DateTime<chrono::Utc>",
            Some(RefClassificationKind::PrimitiveWrapper),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "google.protobuf.Timestamp");
        assert_eq!(result.rust_type, "prost_types::Timestamp");
        assert!(result.is_import);
        assert!(result.is_message);
    }

    // ── EntityReference ──────────────────────────────────────────────

    #[test]
    fn proto_type_for_entity_reference() {
        let p = prop(
            "candidate_id",
            "Uuid",
            Some(RefClassificationKind::EntityReference),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Application");
        assert_eq!(result.proto_type, "string");
        assert_eq!(result.rust_type, "String");
        assert!(!result.is_message);
    }

    // ── CodelistReference ────────────────────────────────────────────

    #[test]
    fn proto_type_for_codelist_reference() {
        let p = prop(
            "gender_code",
            "String",
            Some(RefClassificationKind::CodelistReference),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "string");
        assert_eq!(result.rust_type, "String");
        assert!(!result.is_message);
    }

    #[test]
    fn proto_type_for_inline_enum() {
        let p = prop("status", "String", Some(RefClassificationKind::InlineEnum));
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "string");
    }

    #[test]
    fn proto_type_for_codelist_check() {
        let p = prop(
            "category",
            "String",
            Some(RefClassificationKind::CodelistCheck),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "string");
    }

    // ── ValueObject ──────────────────────────────────────────────────

    #[test]
    fn proto_type_for_value_object() {
        let p = prop(
            "address",
            "AddressType",
            Some(RefClassificationKind::ValueObject),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "CandidateAddress");
        assert_eq!(result.rust_type, "CandidateAddress");
        assert!(result.is_message);
    }

    #[test]
    fn proto_type_for_value_object_snake_field() {
        let p = prop(
            "home_address",
            "AddressType",
            Some(RefClassificationKind::ValueObject),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "CandidateHomeAddress");
        assert!(result.is_message);
    }

    // ── CompositeWrapper ─────────────────────────────────────────────

    #[test]
    fn proto_type_for_composite_wrapper() {
        let p = prop(
            "full_name",
            "NameType",
            Some(RefClassificationKind::CompositeWrapper),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "CandidateFullName");
        assert!(result.is_message);
    }

    // ── MediaWrapper ─────────────────────────────────────────────────

    #[test]
    fn proto_type_for_media_wrapper() {
        let p = prop(
            "photo",
            "MediaType",
            Some(RefClassificationKind::MediaWrapper),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "MediaContent");
        assert_eq!(result.rust_type, "MediaContent");
        assert!(result.is_message);
    }

    // ── StructuredWrapper ────────────────────────────────────────────

    #[test]
    fn proto_type_for_structured_wrapper() {
        let p = prop(
            "metadata",
            "serde_json::Value",
            Some(RefClassificationKind::StructuredWrapper),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "google.protobuf.Struct");
        assert_eq!(result.rust_type, "prost_types::Struct");
        assert!(result.is_import);
        assert!(result.is_message);
    }

    // ── ArrayWrapper ─────────────────────────────────────────────────

    #[test]
    fn proto_type_for_array_of_strings() {
        let p = prop(
            "tags",
            "Vec<String>",
            Some(RefClassificationKind::ArrayWrapper),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "repeated string");
        assert_eq!(result.rust_type, "Vec<String>");
    }

    #[test]
    fn proto_type_for_array_of_i64() {
        let p = prop(
            "scores",
            "Vec<i64>",
            Some(RefClassificationKind::ArrayWrapper),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "repeated int64");
    }

    // ── RangeWrapper ─────────────────────────────────────────────────

    #[test]
    fn proto_type_for_range_wrapper() {
        let p = prop(
            "salary",
            "std::ops::Range<i32>",
            Some(RefClassificationKind::RangeWrapper),
        );
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "SalaryRange");
        assert!(result.is_message);
    }

    // ── No classification (fallback to from_rust_type) ───────────────

    #[test]
    fn proto_type_unclassified_string() {
        let p = prop("name", "String", None);
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "string");
    }

    #[test]
    fn proto_type_unclassified_i32() {
        let p = prop("age", "i32", None);
        let result = proto_type_from_field(&p, &empty_mock(), "Candidate");
        assert_eq!(result.proto_type, "int32");
    }

    // ── from_rust_type direct usage ──────────────────────────────────

    #[test]
    fn from_rust_type_string() {
        let result = proto_type_from_rust_type("String");
        assert_eq!(result.proto_type, "string");
        assert_eq!(result.rust_type, "String");
    }

    #[test]
    fn from_rust_type_uuid() {
        let result = proto_type_from_rust_type("uuid::Uuid");
        assert_eq!(result.proto_type, "string");
    }

    #[test]
    fn from_rust_type_datetime() {
        let result = proto_type_from_rust_type("chrono::DateTime<chrono::Utc>");
        assert_eq!(result.proto_type, "google.protobuf.Timestamp");
    }

    #[test]
    fn from_rust_type_serde_json_value() {
        let result = proto_type_from_rust_type("serde_json::Value");
        assert_eq!(result.proto_type, "google.protobuf.Struct");
    }

    #[test]
    fn from_rust_type_vec_string() {
        let result = proto_type_from_rust_type("Vec<String>");
        assert_eq!(result.proto_type, "repeated string");
    }

    #[test]
    fn from_rust_type_unknown_falls_back_to_string() {
        let result = proto_type_from_rust_type("SomeUnknownType");
        assert_eq!(result.proto_type, "string");
        assert_eq!(result.rust_type, "String");
    }

    #[test]
    fn from_rust_type_naive_date() {
        let result = proto_type_from_rust_type("chrono::NaiveDate");
        assert_eq!(result.proto_type, "google.protobuf.Timestamp");
        assert!(result.is_import);
    }

    #[test]
    fn from_rust_type_f32() {
        let result = proto_type_from_rust_type("f32");
        assert_eq!(result.proto_type, "float");
    }

    #[test]
    fn from_rust_type_bool() {
        let result = proto_type_from_rust_type("bool");
        assert_eq!(result.proto_type, "bool");
    }
}
