//! `PropertyNode.type_expr` (issue #277): serde round-trip, backward-compat
//! read of legacy payloads, and the `effective_type_expr()` precedence chain
//! (type_expr → projection-derived → None), mirroring `effective_kind()`.

use codegraph_core::types::PropertyNode;
use codegraph_type_contracts::{
    ColumnType, DddFieldProjection, DomainProjection, DtoFieldType, DtoProjections,
    EntityProjection, PgType, RefClassificationKind, RustType, TypeExpr,
};

fn base_property() -> PropertyNode {
    PropertyNode {
        name: "gender_code".into(),
        prop_type: "string".into(),
        description: None,
        format: None,
        is_required: true,
        is_nullable: false,
        is_id: false,
        is_array: false,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "gender_code".into(),
        pg_column_type: "TEXT".into(),
        rust_field_name: "gender".into(),
        rust_field_type: "String".into(),
        sea_orm_type: "Text".into(),
        render_strategy: "codelist".into(),
        ref_target: Some("GenderType".into()),
        classification: None,
        projection: None,
        classification_kind: Some(RefClassificationKind::CodelistReference),
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    }
}

fn codelist_expr() -> TypeExpr {
    TypeExpr::Codelist {
        code_list: "GenderType".to_string(),
    }
}

fn text_projection() -> DddFieldProjection {
    DddFieldProjection {
        entity: EntityProjection::SingleColumn {
            column_name: "gender_code".into(),
            column_type: ColumnType::from_pg(PgType::Text),
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

#[test]
fn type_expr_survives_json_round_trip() {
    let mut prop = base_property();
    prop.type_expr = Some(TypeExpr::List(Box::new(codelist_expr())));
    let json = serde_json::to_string(&prop).unwrap();
    let back: PropertyNode = serde_json::from_str(&json).unwrap();
    assert_eq!(back, prop);
}

#[test]
fn absent_type_expr_serializes_as_before() {
    // Legacy payload WITHOUT the type_expr key must deserialize unchanged
    // (read compat), yielding None.
    let legacy = r#"{
        "name": "gender_code",
        "prop_type": "string",
        "description": null,
        "format": null,
        "is_required": true,
        "is_nullable": false,
        "is_array": false,
        "pattern": null,
        "pg_column_name": "gender_code",
        "pg_column_type": "TEXT",
        "rust_field_name": "gender",
        "rust_field_type": "String",
        "sea_orm_type": "Text",
        "render_strategy": "codelist",
        "ref_target": "GenderType",
        "classification": null
    }"#;
    let prop: PropertyNode = serde_json::from_str(legacy).unwrap();
    assert_eq!(prop.type_expr, None);
    assert_eq!(
        prop.effective_kind(),
        Some(RefClassificationKind::CodelistReference)
    );

    // A node with type_expr: None round-trips through JSON preserving None.
    let prop = base_property();
    let json = serde_json::to_string(&prop).unwrap();
    let back: PropertyNode = serde_json::from_str(&json).unwrap();
    assert_eq!(back.type_expr, None);
    assert_eq!(back, prop);
}

#[test]
fn precedence_accessor_prefers_type_expr_then_projection_then_strings() {
    // Priority 1: populated type_expr wins even when a projection exists.
    let mut prop = base_property();
    prop.projection = Some(text_projection());
    prop.type_expr = Some(codelist_expr());
    assert_eq!(prop.effective_type_expr(), Some(codelist_expr()));

    // Priority 2: projection-derived expression when type_expr is absent.
    let mut prop = base_property();
    prop.projection = Some(text_projection());
    assert_eq!(
        prop.effective_type_expr(),
        Some(TypeExpr::Primitive {
            name: "string".to_string()
        })
    );

    // Priority 3: neither present → None (generators keep legacy strings).
    let prop = base_property();
    assert_eq!(prop.effective_type_expr(), None);
}
