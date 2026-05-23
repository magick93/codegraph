use std::collections::{HashMap, HashSet};

use codegraph_type_contracts::{
    ColumnType, DddFieldProjection, DomainProjection, DtoFieldType, DtoProjections,
    EntityProjection, PgType, RefClassificationKind, RustType, ScalarKind,
};

use crate::config::ClassifierConfig;
use crate::projection_builder::ProjectionBuilder;
use crate::ClassificationResult;

/// Build a ClassificationResult for kinds whose pg/rust/sea_orm types come from config.
fn configured_result(
    kind: RefClassificationKind,
    ref_stem: &str,
    pg: &str,
    rust: &str,
    sea_orm: Option<&str>,
    open_end: bool,
) -> ClassificationResult {
    let projection = ProjectionBuilder::from_classification(
        &kind,
        ref_stem,
        Some(pg),
        Some(rust),
        sea_orm,
        None,
        None,
        &HashMap::new(),
        None,
        None,
        None,
        false,
    );
    let column_type = PgType::from_pg_str(pg).map(ColumnType::from_pg);
    ClassificationResult {
        kind,
        column_type,
        projection,
        open_end,
    }
}

/// Build a ClassificationResult for text-typed kinds (codelists, inline enums).
fn text_result(kind: RefClassificationKind, ref_stem: &str) -> ClassificationResult {
    configured_result(kind, ref_stem, "TEXT", "String", Some("Text"), false)
}

/// Build a ClassificationResult for kinds with no type info (entity refs, value objects).
fn bare_result(kind: RefClassificationKind, ref_stem: &str) -> ClassificationResult {
    let projection = ProjectionBuilder::from_classification(
        &kind,
        ref_stem,
        None,
        None,
        None,
        None,
        None,
        &HashMap::new(),
        None,
        None,
        None,
        false,
    );
    ClassificationResult {
        kind,
        column_type: None,
        projection,
        open_end: false,
    }
}

pub fn classify_ref(
    ref_stem: &str,
    ref_path: &str,
    config: &ClassifierConfig,
    is_entity: &dyn Fn(&str) -> bool,
) -> ClassificationResult {
    // 1. Primitive wrappers
    if let Some(pw) = config.primitive_wrappers.get(ref_stem) {
        return configured_result(
            RefClassificationKind::PrimitiveWrapper,
            ref_stem,
            &pw.postgres,
            &pw.rust,
            Some(&pw.sea_orm),
            false,
        );
    }

    // 2. Structured wrappers (JSONB with typed struct)
    if let Some(sw) = config.structured_wrappers.get(ref_stem) {
        let kind = RefClassificationKind::StructuredWrapper;
        let projection = ProjectionBuilder::from_classification(
            &kind,
            ref_stem,
            Some("JSONB"),
            Some(&sw.rust),
            Some("JsonBinary"),
            None,
            None,
            &HashMap::new(),
            None,
            None,
            None,
            false,
        );
        return ClassificationResult {
            kind,
            column_type: Some(ColumnType::from_pg(PgType::Jsonb)),
            projection,
            open_end: false,
        };
    }

    // 3. Array wrappers
    if let Some(aw) = config.array_wrappers.get(ref_stem) {
        return configured_result(
            RefClassificationKind::ArrayWrapper,
            ref_stem,
            &aw.postgres,
            &aw.rust,
            Some(&aw.sea_orm),
            false,
        );
    }

    // 3. Range wrappers
    if let Some(rw) = config.range_wrappers.get(ref_stem) {
        return configured_result(
            RefClassificationKind::RangeWrapper,
            ref_stem,
            &rw.postgres,
            &rw.rust,
            None,
            rw.open_end,
        );
    }

    // 4. Composite wrappers
    if let Some(cw) = config
        .composite_wrappers
        .iter()
        .find(|c| c.schema == ref_stem)
    {
        let kind = RefClassificationKind::CompositeWrapper;
        let projection = ProjectionBuilder::from_classification(
            &kind,
            ref_stem,
            None,
            None,
            None,
            None,
            None,
            &HashMap::new(),
            None,
            None,
            Some(&cw.columns),
            false,
        );
        return ClassificationResult {
            kind,
            column_type: None,
            projection,
            open_end: false,
        };
    }

    // 4b. Media wrappers
    if let Some(mw) = config.media_wrappers.get(ref_stem) {
        let kind = RefClassificationKind::MediaWrapper;
        let projection = ProjectionBuilder::from_classification(
            &kind,
            ref_stem,
            None,
            None,
            None,
            None,
            None,
            &HashMap::new(),
            None,
            None,
            Some(&mw.columns),
            false,
        );
        return ClassificationResult {
            kind,
            column_type: None,
            projection,
            open_end: false,
        };
    }

    // 4a. (structured_wrappers handled above at step 2)

    // 5. Codelists (path contains "codelist/")
    if ref_path.contains("codelist/") || ref_path.contains("codelist\\") {
        let check_schemas: HashSet<&str> = config
            .codelist_as_check
            .schemas
            .iter()
            .map(|s| s.as_str())
            .collect();
        let kind = if check_schemas.contains(ref_stem) {
            RefClassificationKind::CodelistCheck
        } else {
            RefClassificationKind::CodelistReference
        };
        return text_result(kind, ref_stem);
    }

    // 6. Entity or value object
    let kind = if is_entity(ref_stem) {
        RefClassificationKind::EntityReference
    } else {
        RefClassificationKind::ValueObject
    };
    bare_result(kind, ref_stem)
}

pub fn classify_inline_enum(values: &[String], config: &ClassifierConfig) -> ClassificationResult {
    if values.len() <= config.inline_enum_threshold {
        text_result(RefClassificationKind::InlineEnum, "inline_enum")
    } else {
        text_result(RefClassificationKind::CodelistReference, "codelist")
    }
}

pub fn classify_plain_type(schema: &serde_json::Value) -> ClassificationResult {
    let schema_type = match schema.get("type") {
        Some(serde_json::Value::String(s)) => s.as_str(),
        Some(serde_json::Value::Array(arr)) => {
            // ["string", "null"] → pick the non-null type
            arr.iter()
                .filter_map(|v| v.as_str())
                .find(|t| *t != "null")
                .unwrap_or("object")
        }
        _ => "object",
    };

    let format = schema.get("format").and_then(|v| v.as_str()).unwrap_or("");

    let scalar_kind = match schema_type {
        "string" if format == "date" => Some(ScalarKind::Date),
        "string" if format == "date-time" => Some(ScalarKind::DateTime),
        "string" => Some(ScalarKind::String),
        "integer" => Some(ScalarKind::Integer),
        "number" => Some(ScalarKind::Number),
        "boolean" => Some(ScalarKind::Boolean),
        _ => None,
    };

    if let Some(sk) = scalar_kind {
        let kind = RefClassificationKind::PrimitiveWrapper;
        let projection = ProjectionBuilder::from_scalar(&sk, "");
        let pg = match sk {
            ScalarKind::String => PgType::Text,
            ScalarKind::Integer => PgType::BigInt,
            ScalarKind::Number => PgType::DoublePrecision,
            ScalarKind::Boolean => PgType::Boolean,
            ScalarKind::Date => PgType::Date,
            ScalarKind::DateTime => PgType::Timestamptz,
            ScalarKind::Json => PgType::Jsonb,
        };
        let column_type = Some(ColumnType::from_pg(pg));
        ClassificationResult {
            kind,
            column_type,
            projection,
            open_end: false,
        }
    } else {
        // Catch-all: unknown/object types → ValueObject with JSONB
        let kind = RefClassificationKind::ValueObject;
        let column_type = Some(ColumnType::from_pg(PgType::Jsonb));
        let projection = DddFieldProjection {
            entity: EntityProjection::SingleColumn {
                column_name: String::new(),
                column_type: ColumnType::from_pg(PgType::Jsonb),
                is_fk: false,
                fk_target: None,
            },
            domain: DomainProjection {
                rust_type: RustType::Json,
            },
            dto: DtoProjections {
                create: DtoFieldType::Scalar(RustType::Json),
                update: DtoFieldType::Scalar(RustType::Json),
                response: DtoFieldType::Scalar(RustType::Json),
            },
        };
        ClassificationResult {
            kind,
            column_type,
            projection,
            open_end: false,
        }
    }
}

pub fn get_consumed_fields(schema_stem: &str, config: &ClassifierConfig) -> HashSet<String> {
    let mut consumed = HashSet::new();
    for cr in &config.composite_ranges {
        if cr.schema == schema_stem {
            consumed.insert(cr.start.clone());
            consumed.insert(cr.end.clone());
        }
    }
    consumed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse_classifier_config_str;

    fn test_config() -> ClassifierConfig {
        parse_classifier_config_str(
            r#"
inline_enum_threshold = 20
[primitive_wrappers.CodeType]
postgres = "TEXT"
rust = "String"
sea_orm = "Text"

[[composite_wrappers]]
schema = "AmountType"
columns = [
  { suffix = "_value", postgres = "NUMERIC(19,4)", rust = "rust_decimal::Decimal", sea_orm = "Decimal", fk_table = "" },
  { suffix = "_currency", postgres = "TEXT", rust = "String", sea_orm = "Text", fk_table = "" },
]

[[composite_ranges]]
schema = "EffectiveDateType"
start = "validFrom"
end = "validTo"
column = "effective_period"
postgres = "DATERANGE"
rust = "DateRange"

[codelist_as_check]
schemas = ["GenderCodeList"]
"#,
        )
        .unwrap()
    }

    #[test]
    fn classify_primitive_wrapper() {
        let config = test_config();
        let result = classify_ref("CodeType", "common/json/CodeType.json", &config, &|_| false);
        assert_eq!(result.kind, RefClassificationKind::PrimitiveWrapper);
        assert_eq!(result.column_type.as_ref().unwrap().pg(), &PgType::Text);
    }

    #[test]
    fn classify_composite_wrapper() {
        let config = test_config();
        let result = classify_ref(
            "AmountType",
            "common/json/AmountType.json",
            &config,
            &|_| false,
        );
        assert_eq!(result.kind, RefClassificationKind::CompositeWrapper);
        assert!(result.column_type.is_none());
        // Composite column data lives in projection.entity as CompositeColumns
        assert!(matches!(
            result.projection.entity,
            EntityProjection::CompositeColumns { .. }
        ));
    }

    #[test]
    fn classify_codelist() {
        let config = test_config();
        let result = classify_ref(
            "CurrencyCodeList",
            "common/json/codelist/CurrencyCodeList.json",
            &config,
            &|_| false,
        );
        assert_eq!(result.kind, RefClassificationKind::CodelistReference);
    }

    #[test]
    fn classify_codelist_as_check() {
        let config = test_config();
        let result = classify_ref(
            "GenderCodeList",
            "common/json/codelist/GenderCodeList.json",
            &config,
            &|_| false,
        );
        assert_eq!(result.kind, RefClassificationKind::CodelistCheck);
    }

    #[test]
    fn classify_entity_reference() {
        let config = test_config();
        let result = classify_ref(
            "CandidateType",
            "recruiting/json/CandidateType.json",
            &config,
            &|name| name == "CandidateType",
        );
        assert_eq!(result.kind, RefClassificationKind::EntityReference);
    }

    #[test]
    fn classify_value_object() {
        let config = test_config();
        let result = classify_ref("NameType", "common/json/NameType.json", &config, &|_| false);
        assert_eq!(result.kind, RefClassificationKind::ValueObject);
    }

    #[test]
    fn classify_plain_string() {
        let schema = serde_json::json!({"type": "string"});
        let result = classify_plain_type(&schema);
        assert_eq!(result.kind, RefClassificationKind::PrimitiveWrapper);
        assert_eq!(result.column_type.as_ref().unwrap().pg(), &PgType::Text);
    }

    #[test]
    fn classify_plain_date() {
        let schema = serde_json::json!({"type": "string", "format": "date"});
        let result = classify_plain_type(&schema);
        assert_eq!(result.column_type.as_ref().unwrap().pg(), &PgType::Date);
    }

    #[test]
    fn consumed_fields_for_composite_range() {
        let config = test_config();
        let consumed = get_consumed_fields("EffectiveDateType", &config);
        assert!(consumed.contains("validFrom"));
        assert!(consumed.contains("validTo"));
    }

    #[test]
    fn classify_structured_wrapper() {
        let config = parse_classifier_config_str(
            r#"
inline_enum_threshold = 20
[structured_wrappers]
"IdentifierType" = { postgres = "JSONB", rust = "IdentifierType", sea_orm = "Json" }

[codelist_as_check]
schemas = []
"#,
        )
        .unwrap();
        let result = classify_ref(
            "IdentifierType",
            "common/json/base/IdentifierType.json",
            &config,
            &|_| false,
        );
        assert_eq!(result.kind, RefClassificationKind::StructuredWrapper);
        assert_eq!(result.column_type.as_ref().unwrap().pg(), &PgType::Jsonb);
        // Domain type should be the configured Rust type name
        assert_eq!(
            result.projection.domain.rust_type,
            RustType::DomainType("IdentifierType".to_string())
        );
    }

    #[test]
    fn inline_enum_within_threshold() {
        let config = test_config();
        let vals = vec!["A".into(), "B".into(), "C".into()];
        let result = classify_inline_enum(&vals, &config);
        assert_eq!(result.kind, RefClassificationKind::InlineEnum);
    }

    #[test]
    fn classifies_structured_wrapper_as_jsonb() {
        use crate::config::TypeMapping;
        let mut config = test_config();
        config.structured_wrappers = HashMap::from([(
            "IdentifierType".to_string(),
            TypeMapping {
                postgres: "JSONB".to_string(),
                rust: "IdentifierType".to_string(),
                sea_orm: "Json".to_string(),
            },
        )]);
        let result = classify_ref(
            "IdentifierType",
            "base/IdentifierType.json",
            &config,
            &|_| false,
        );
        assert_eq!(result.kind, RefClassificationKind::StructuredWrapper);
        assert_eq!(result.column_type.as_ref().unwrap().pg(), &PgType::Jsonb);
    }

    #[test]
    fn classifies_media_reference_type() {
        let toml_str = r#"
            [media_wrappers.MediaReferenceType]
            accept = ["image/*"]

            [[media_wrappers.MediaReferenceType.columns]]
            suffix = "_url"
            postgres = "TEXT"
            rust = "Option<String>"
            sea_orm = "Text"

            [[media_wrappers.MediaReferenceType.columns]]
            suffix = "_mime_type"
            postgres = "TEXT"
            rust = "Option<String>"
            sea_orm = "Text"
        "#;
        let config = crate::config::parse_classifier_config_str(toml_str).unwrap();
        let result = classify_ref(
            "MediaReferenceType",
            "base/MediaReferenceType.json",
            &config,
            &|_| false,
        );
        assert_eq!(result.kind, RefClassificationKind::MediaWrapper);
    }
}
