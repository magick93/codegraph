//! Lowering matrix for `TypeExpr` (issue #277): composable abstract type
//! expressions lower to per-target types as pure functions.
//!
//! Targets covered here: `PgType` (DDL column type), `RustType` (canonical
//! Rust type), and the proto scalar names via the canonical Rust string
//! (the proto lowering itself lives in the generate crate).

use codegraph_type_contracts::{
    PgType, RefClassificationKind, RustType, TypeExpr, TypeExprError,
};

fn r#ref(target: &str, kind: Option<RefClassificationKind>) -> TypeExpr {
    TypeExpr::Ref {
        target: target.to_string(),
        kind,
        args: vec![],
    }
}

fn primitive(name: &str) -> TypeExpr {
    TypeExpr::Primitive {
        name: name.to_string(),
    }
}

// ── Ref without args lowers to all targets ─────────────────────────────

#[test]
fn ref_without_args_lowers_to_all_targets() {
    let expr = r#ref("int64", None);
    assert_eq!(expr.lower_pg(), Ok(PgType::BigInt));
    assert_eq!(expr.lower_rust(), Ok(RustType::I64));
}

#[test]
fn ref_to_string_primitive_lowers_across_targets() {
    let expr = r#ref("string", None);
    assert_eq!(expr.lower_pg(), Ok(PgType::Text));
    assert_eq!(expr.lower_rust(), Ok(RustType::String));
}

// ── Nesting: List / Optional compose ───────────────────────────────────

#[test]
fn list_of_optional_nests_correctly() {
    let expr = TypeExpr::List(Box::new(TypeExpr::Optional(Box::new(r#ref(
        "string",
        None,
    )))));
    assert_eq!(expr.lower_pg(), Ok(PgType::TextArray));
    assert_eq!(expr.lower_rust(), Ok(RustType::VecString));
}

#[test]
fn optional_is_type_preserving_for_pg_and_rust() {
    let expr = TypeExpr::Optional(Box::new(primitive("int32")));
    assert_eq!(expr.lower_pg(), Ok(PgType::Integer));
    assert_eq!(expr.lower_rust(), Ok(RustType::I32));
}

#[test]
fn list_of_i64_lowers_to_bigint_array() {
    let expr = TypeExpr::List(Box::new(primitive("int64")));
    assert_eq!(expr.lower_pg(), Ok(PgType::BigIntArray));
    assert_eq!(expr.lower_rust(), Ok(RustType::VecI64));
}

// ── Codelist and entity refs lower with kind ───────────────────────────

#[test]
fn codelist_and_entity_ref_lower_with_kind() {
    let codelist = TypeExpr::Codelist {
        code_list: "GenderType".to_string(),
    };
    assert_eq!(codelist.lower_pg(), Ok(PgType::Text));
    assert_eq!(codelist.lower_rust(), Ok(RustType::String));

    let entity = r#ref("WorkerType", Some(RefClassificationKind::EntityReference));
    assert_eq!(entity.lower_pg(), Ok(PgType::Uuid));
    assert_eq!(entity.lower_rust(), Ok(RustType::Uuid));
}

#[test]
fn codelist_check_kind_lowers_like_codelist_reference() {
    let expr = r#ref(
        "Priority",
        Some(RefClassificationKind::CodelistCheck),
    );
    assert_eq!(expr.lower_pg(), Ok(PgType::Text));
    assert_eq!(expr.lower_rust(), Ok(RustType::String));
}

// ── Unsupported shapes are named errors, not silent fallbacks ──────────

#[test]
fn unsupported_expr_is_a_named_error() {
    let record_in_list = TypeExpr::List(Box::new(TypeExpr::Record { fields: vec![] }));
    let err = record_in_list.lower_pg().expect_err("must not lower");
    assert!(
        err.to_string().contains("Record"),
        "error must name the offending shape: {err}"
    );

    let untyped_unknown = r#ref("MysteryType", None);
    let err = untyped_unknown.lower_pg().expect_err("must not lower");
    assert!(
        err.to_string().contains("MysteryType"),
        "error must name the target: {err}"
    );

    let vo_has_no_scalar_column = r#ref(
        "AddressType",
        Some(RefClassificationKind::ValueObject),
    );
    assert!(vo_has_no_scalar_column.lower_pg().is_err());

    let unknown_primitive = primitive("wibble");
    assert!(matches!(
        unknown_primitive.lower_pg(),
        Err(TypeExprError::UnknownPrimitive { .. })
    ));
}

#[test]
fn primitive_scalar_lowers_to_pg_and_rust() {
    assert_eq!(primitive("uuid").lower_pg(), Ok(PgType::Uuid));
    assert_eq!(primitive("uuid").lower_rust(), Ok(RustType::Uuid));
    assert_eq!(primitive("datetime").lower_pg(), Ok(PgType::Timestamptz));
    assert_eq!(primitive("datetime").lower_rust(), Ok(RustType::DateTimeUtc));
    assert_eq!(primitive("decimal").lower_rust(), Ok(RustType::Decimal));
}

// ── Serde round trip (externally tagged, nesting-safe) ─────────────────

#[test]
fn type_expr_round_trips_through_json_including_nesting() {
    let expr = TypeExpr::List(Box::new(TypeExpr::Ref {
        target: "WorkerType".to_string(),
        kind: Some(RefClassificationKind::EntityReference),
        args: vec![primitive("string")],
    }));
    let json = serde_json::to_string(&expr).unwrap();
    let back: TypeExpr = serde_json::from_str(&json).unwrap();
    assert_eq!(back, expr);

    let nested = TypeExpr::Optional(Box::new(TypeExpr::List(Box::new(TypeExpr::Record {
        fields: vec![("code".to_string(), primitive("string"))],
    }))));
    let json = serde_json::to_string(&nested).unwrap();
    let back: TypeExpr = serde_json::from_str(&json).unwrap();
    assert_eq!(back, nested);
}

// ── Derivation from frozen ingest data ─────────────────────────────────

#[test]
fn from_frozen_maps_primitive_wrappers_by_rust_string() {
    let expr = TypeExpr::from_frozen(&RefClassificationKind::PrimitiveWrapper, None, "String");
    assert_eq!(expr, Some(primitive("string")));

    let expr = TypeExpr::from_frozen(&RefClassificationKind::PrimitiveWrapper, None, "i32");
    assert_eq!(expr, Some(primitive("int32")));
}

#[test]
fn from_frozen_wraps_primitive_arrays_in_list() {
    let expr = TypeExpr::from_frozen(&RefClassificationKind::PrimitiveWrapper, None, "Vec<String>");
    assert_eq!(expr, Some(TypeExpr::List(Box::new(primitive("string")))));

    let expr = TypeExpr::from_frozen(&RefClassificationKind::ArrayWrapper, None, "Vec<i64>");
    assert_eq!(expr, Some(TypeExpr::List(Box::new(primitive("int64")))));
}

#[test]
fn from_frozen_maps_refs_and_codelists_with_target() {
    let expr = TypeExpr::from_frozen(
        &RefClassificationKind::EntityReference,
        Some("WorkerType"),
        "WorkerType",
    );
    assert_eq!(
        expr,
        Some(r#ref("WorkerType", Some(RefClassificationKind::EntityReference)))
    );

    let expr = TypeExpr::from_frozen(&RefClassificationKind::CodelistReference, Some("Gender"), "");
    assert_eq!(
        expr,
        Some(TypeExpr::Codelist {
            code_list: "Gender".to_string()
        })
    );

    let expr = TypeExpr::from_frozen(&RefClassificationKind::ValueObject, Some("AddressType"), "");
    assert_eq!(
        expr,
        Some(r#ref("AddressType", Some(RefClassificationKind::ValueObject)))
    );
}

#[test]
fn from_frozen_returns_none_when_unrepresentable() {
    // Non-invertible frozen rust string on a primitive wrapper.
    assert_eq!(
        TypeExpr::from_frozen(&RefClassificationKind::PrimitiveWrapper, None, "MonetaryAmount"),
        None
    );
    // Ref-shaped kinds need a target.
    assert_eq!(
        TypeExpr::from_frozen(&RefClassificationKind::EntityReference, None, "WorkerType"),
        None
    );
}

// ── Derivation from projections (legacy graphs) ────────────────────────

fn projection_of(
    entity: codegraph_type_contracts::EntityProjection,
    dto_response: codegraph_type_contracts::DtoFieldType,
) -> codegraph_type_contracts::DddFieldProjection {
    use codegraph_type_contracts::{
        DddFieldProjection, DomainProjection, DtoFieldType, DtoProjections, RustType,
    };
    DddFieldProjection {
        entity,
        domain: DomainProjection {
            rust_type: RustType::String,
        },
        dto: DtoProjections {
            create: dto_response.clone(),
            update: dto_response.clone(),
            response: dto_response,
        },
    }
}

#[test]
fn from_projection_derives_codelist_and_entity_refs() {
    use codegraph_type_contracts::{
        ColumnType, DtoFieldType, EntityProjection, FkTarget, PgType,
    };

    let codelist = projection_of(
        EntityProjection::SingleColumn {
            column_name: "gender_code".into(),
            column_type: ColumnType::from_pg(PgType::Text),
            is_fk: true,
            fk_target: Some(FkTarget {
                schema: "common".into(),
                table: "gender_code".into(),
                column: "code".into(),
            }),
        },
        DtoFieldType::Codelist {
            enum_name: "Gender".into(),
        },
    );
    assert_eq!(
        TypeExpr::from_projection(&codelist),
        Some(TypeExpr::Codelist {
            code_list: "Gender".to_string()
        })
    );

    let entity_ref = projection_of(
        EntityProjection::SingleColumn {
            column_name: "worker_id".into(),
            column_type: ColumnType::from_pg(PgType::Uuid),
            is_fk: true,
            fk_target: Some(FkTarget {
                schema: "hr".into(),
                table: "worker".into(),
                column: "id".into(),
            }),
        },
        DtoFieldType::EntityRef {
            entity_name: "WorkerType".into(),
        },
    );
    assert_eq!(
        TypeExpr::from_projection(&entity_ref),
        Some(r#ref("WorkerType", Some(RefClassificationKind::EntityReference)))
    );
}

#[test]
fn from_projection_derives_primitives_and_lists() {
    use codegraph_type_contracts::{ColumnType, EntityProjection, PgType, RustType, DtoFieldType};

    let text = projection_of(
        EntityProjection::SingleColumn {
            column_name: "name".into(),
            column_type: ColumnType::from_pg(PgType::Text),
            is_fk: false,
            fk_target: None,
        },
        DtoFieldType::Scalar(RustType::String),
    );
    assert_eq!(TypeExpr::from_projection(&text), Some(primitive("string")));

    let text_array = projection_of(
        EntityProjection::SingleColumn {
            column_name: "tags".into(),
            column_type: ColumnType::from_pg(PgType::TextArray),
            is_fk: false,
            fk_target: None,
        },
        DtoFieldType::Scalar(RustType::VecString),
    );
    assert_eq!(
        TypeExpr::from_projection(&text_array),
        Some(TypeExpr::List(Box::new(primitive("string"))))
    );
}

#[test]
fn from_projection_returns_none_for_ambiguous_shapes() {
    use codegraph_type_contracts::{
        ColumnType, CompositeColumn, DtoFieldType, EntityProjection, PgType, RustType,
    };

    // Composite (and media) wrappers are indistinguishable in a projection.
    let composite = projection_of(
        EntityProjection::CompositeColumns {
            primary: CompositeColumn {
                suffix: "value".into(),
                column_type: ColumnType::from_pg(PgType::Numeric {
                    precision: 19,
                    scale: 4,
                }),
                fk_target: None,
            },
            secondary: vec![],
        },
        DtoFieldType::Composite {
            name: "amount".into(),
        },
    );
    assert_eq!(TypeExpr::from_projection(&composite), None);

    // Range wrappers are indistinguishable from primitives in a projection.
    let range = projection_of(
        EntityProjection::SingleColumn {
            column_name: "salary".into(),
            column_type: ColumnType::from_pg(PgType::Int4Range),
            is_fk: false,
            fk_target: None,
        },
        DtoFieldType::Scalar(RustType::String),
    );
    assert_eq!(TypeExpr::from_projection(&range), None);
}
