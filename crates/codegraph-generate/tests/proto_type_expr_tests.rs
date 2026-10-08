//! Proto parity gate (issue #277): feeding `proto_type_from_field` through
//! the `TypeExpr` path must produce IDENTICAL `ProtoFieldType` values to the
//! legacy frozen-string path for every shape the ingest populates. This is
//! the byte-identity contract for the first TypeExpr consumer.

use codegraph_core::mock::MockEngine;
use codegraph_core::types::PropertyNode;
use codegraph_type_contracts::RefClassificationKind;
use codegraph_type_contracts::TypeExpr;

fn prop(
    kind: RefClassificationKind,
    rust_field_type: &str,
    ref_target: Option<&str>,
) -> PropertyNode {
    PropertyNode {
        name: "salary".to_string(),
        prop_type: String::new(),
        description: None,
        format: None,
        is_required: true,
        is_nullable: false,
        is_id: false,
        is_array: rust_field_type.starts_with("Vec<"),
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "salary".to_string(),
        pg_column_type: String::new(),
        rust_field_name: "salary".to_string(),
        rust_field_type: rust_field_type.to_string(),
        sea_orm_type: String::new(),
        render_strategy: String::new(),
        ref_target: ref_target.map(str::to_string),
        classification: None,
        projection: None,
        classification_kind: Some(kind),
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    }
}

fn named(name: &str) -> PropertyNode {
    let mut p = prop(RefClassificationKind::PrimitiveWrapper, "String", None);
    p.name = name.to_string();
    p
}

fn with_expr(mut p: PropertyNode, expr: TypeExpr) -> PropertyNode {
    p.type_expr = Some(expr);
    p
}

fn primitive(name: &str) -> TypeExpr {
    TypeExpr::Primitive {
        name: name.to_string(),
    }
}

fn r#ref(target: &str, kind: RefClassificationKind) -> TypeExpr {
    TypeExpr::Ref {
        target: target.to_string(),
        kind: Some(kind),
        args: vec![],
    }
}

fn assert_parity(legacy: &PropertyNode, expr_fed: &PropertyNode, entity: &str) {
    let db = MockEngine::builder().build();
    let via_string =
        codegraph_generate::grpc::proto_type::proto_type_from_field(legacy, &db, entity);
    let via_expr =
        codegraph_generate::grpc::proto_type::proto_type_from_field(expr_fed, &db, entity);
    assert_eq!(
        via_string.proto_type, via_expr.proto_type,
        "proto_type divergence for entity {entity}"
    );
    assert_eq!(
        via_string.rust_type, via_expr.rust_type,
        "rust_type divergence for entity {entity}"
    );
    assert_eq!(via_string.is_import, via_expr.is_import);
    assert_eq!(via_string.import_path, via_expr.import_path);
    assert_eq!(via_string.is_message, via_expr.is_message);
}

#[test]
fn grpc_message_from_type_expr_matches_string_path() {
    use codegraph_generate::grpc::proto_type::proto_type_from_field;
    let db = MockEngine::builder().build();

    // Expr-fed field resolves through the expr, not the (empty) frozen strings.
    let mut expr_fed = named("amount");
    expr_fed.rust_field_type = String::new();
    expr_fed.type_expr = Some(primitive("decimal"));
    let field = proto_type_from_field(&expr_fed, &db, "Candidate");
    assert_eq!(field.proto_type, "string");
    assert_eq!(field.rust_type, "String");

    let mut expr_fed = named("count");
    expr_fed.rust_field_type = String::new();
    expr_fed.type_expr = Some(primitive("int64"));
    assert_eq!(
        proto_type_from_field(&expr_fed, &db, "Candidate").proto_type,
        "int64"
    );
}

#[test]
fn parity_across_ingest_expressible_shapes() {
    // Primitive scalars: frozen rust string inverts to the same primitive.
    for (rust, name) in [
        ("String", "string"),
        ("i32", "int32"),
        ("i64", "int64"),
        ("f64", "float64"),
        ("bool", "boolean"),
        ("Uuid", "uuid"),
        ("rust_decimal::Decimal", "decimal"),
        ("chrono::NaiveDate", "date"),
        ("chrono::DateTime<chrono::Utc>", "datetime"),
        ("serde_json::Value", "json"),
    ] {
        let legacy = prop(RefClassificationKind::PrimitiveWrapper, rust, None);
        let expr_fed = with_expr(legacy.clone(), primitive(name));
        assert_parity(&legacy, &expr_fed, "Candidate");
    }

    // Primitive arrays (mox/rosetta arrays keep PrimitiveWrapper + Vec<T>).
    let legacy = prop(RefClassificationKind::PrimitiveWrapper, "Vec<String>", None);
    let expr_fed = with_expr(
        legacy.clone(),
        TypeExpr::List(Box::new(primitive("string"))),
    );
    assert_parity(&legacy, &expr_fed, "Candidate");

    // ArrayWrapper arrays.
    let legacy = prop(RefClassificationKind::ArrayWrapper, "Vec<i64>", None);
    let expr_fed = with_expr(legacy.clone(), TypeExpr::List(Box::new(primitive("int64"))));
    assert_parity(&legacy, &expr_fed, "Candidate");

    // Entity reference.
    let legacy = prop(
        RefClassificationKind::EntityReference,
        "WorkerType",
        Some("WorkerType"),
    );
    let expr_fed = with_expr(
        legacy.clone(),
        r#ref("WorkerType", RefClassificationKind::EntityReference),
    );
    assert_parity(&legacy, &expr_fed, "Application");

    // Codelists (reference, check, inline enum).
    for kind in [
        RefClassificationKind::CodelistReference,
        RefClassificationKind::CodelistCheck,
        RefClassificationKind::InlineEnum,
    ] {
        let legacy = prop(kind.clone(), "String", Some("GenderType"));
        let expr_fed = with_expr(
            legacy.clone(),
            TypeExpr::Codelist {
                code_list: "GenderType".to_string(),
            },
        );
        assert_parity(&legacy, &expr_fed, "Candidate");
    }

    // Structured shapes: VO, composite, media, structured, range.
    for (kind, rust) in [
        (RefClassificationKind::ValueObject, "AddressType"),
        (RefClassificationKind::CompositeWrapper, "NameType"),
        (RefClassificationKind::MediaWrapper, "MediaType"),
        (RefClassificationKind::StructuredWrapper, "IdentifierType"),
        (RefClassificationKind::RangeWrapper, "std::ops::Range<i32>"),
    ] {
        let legacy = prop(kind.clone(), rust, Some("Target"));
        let expr_fed = with_expr(legacy.clone(), r#ref("Target", kind));
        assert_parity(&legacy, &expr_fed, "Candidate");
    }
}

#[test]
fn unsupported_expr_falls_back_to_identical_string_path() {
    // A hand-built expr the lowerings reject must NOT change output: the
    // consumer falls back to the frozen strings byte-identically.
    let legacy = prop(RefClassificationKind::PrimitiveWrapper, "Vec<String>", None);
    let unsupported = with_expr(
        legacy.clone(),
        TypeExpr::Ref {
            target: "Mystery".to_string(),
            kind: None,
            args: vec![],
        },
    );
    assert_parity(&legacy, &unsupported, "Candidate");
}
