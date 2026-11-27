//! Name-convention foreign keys (issue #479/#806): a required scalar column
//! whose `_id`-suffixed name claims an entity but which carries no
//! `$ref`/graph edge (`mox: String workerId` → pg TEXT, no `format: uuid`,
//! or a JSON schema that declares the FK as a plain string/uuid property).
//!
//! The convention is name-derived, so BOTH e2e planes must resolve it the same
//! way — the entity plane's `testData()` dependency seeds
//! (`ui::e2e_test::refs`) and the IFML fixtures' create bodies
//! (`ifml::e2e_test::body`) read this one module. A second copy is how the
//! two planes would drift into different parent seeds for the same column.
//!
//! Deliberate limits (pinned by unit tests):
//! - identity columns (`is_id`) are never convention FKs — a natural-key part
//!   merely NAMED `*_id` (VendorOrder's `vendorOrderId`) is a business value,
//!   not a reference to the entity that carries it;
//! - the subject entity itself is never a convention target (#479);
//! - an ambiguous stem (several candidate entities, none uniquely same-domain)
//!   resolves to `None` — the caller reports the gap instead of guessing.

use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{PropertyNode, SchemaNode};

/// Normalize an identifier for convention matching: keep only lowercase
/// alphanumerics, so `case`, `Case`, and `case_id`'s stem all collapse to
/// `case` (and `CaseType` would become `casetype`, which does not match).
pub(crate) fn normalize_convention_stem(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// The `_id`-stripped stem of a convention-shaped property's field name.
pub(crate) fn convention_stem(prop: &PropertyNode) -> Option<&str> {
    let field = prop
        .rust_field_name
        .strip_prefix("r#")
        .unwrap_or(&prop.rust_field_name);
    field.strip_suffix("_id").filter(|stem| !stem.is_empty())
}

/// The name-convention FK shape: a required, non-array scalar column with no
/// `$ref`/graph edge, whose `_id`-suffixed name claims an entity. The pg type
/// is deliberately NOT checked here — the model may declare the FK as a plain
/// string (`mox: String workerId` → pg TEXT, no `format: uuid`) and the
/// convention is name-derived. Identity columns (`is_id`) are never FKs.
pub(crate) fn is_convention_shape(prop: &PropertyNode) -> bool {
    if prop.is_array || !prop.is_required || prop.ref_target.is_some() || prop.is_id {
        return false;
    }
    convention_stem(prop).is_some()
}

/// Resolve a required scalar FK column with no `$ref`/graph edge (e.g.
/// `case_id`) to its entity by naming convention: strip the `_id` suffix,
/// normalize, and find the unique `is_entity` schema whose normalized
/// `pg_table_name` or `rust_type_name` equals the stem. On multiple matches,
/// prefer a same-domain schema; return `None` if still ambiguous. The
/// subject entity itself is never a convention target (#479): a required
/// business field merely NAMED `*_id` on its own entity (e.g. VendorOrder's
/// `String vendorOrderId`) would otherwise resolve to its carrier, get
/// deferred to dep-setup, never emitted, and 422 every create. True
/// self-FKs are explicit `refers` edges (they arrive via `ref_target`,
/// not this path).
pub(crate) async fn resolve_convention_ref(
    db: &dyn GraphQuerier,
    prop: &PropertyNode,
    current_domain: Option<&str>,
    subject_title: &str,
) -> Option<SchemaNode> {
    if !is_convention_shape(prop) {
        return None;
    }
    let stem = convention_stem(prop)?;
    let stem_norm = normalize_convention_stem(stem);
    let matches: Vec<SchemaNode> = db
        .list_schemas(None)
        .await
        .ok()?
        .into_iter()
        .filter(|s| {
            s.is_entity
                && s.title != subject_title
                && (normalize_convention_stem(&s.pg_table_name) == stem_norm
                    || normalize_convention_stem(&s.rust_type_name) == stem_norm)
        })
        .collect();
    match matches.len() {
        0 => None,
        1 => matches.into_iter().next(),
        _ => {
            let same_domain: Vec<SchemaNode> = matches
                .into_iter()
                .filter(|s| s.domain.as_deref() == current_domain)
                .collect();
            if same_domain.len() == 1 {
                same_domain.into_iter().next()
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_type_contracts::RefClassificationKind;

    fn prop(name: &str, required: bool) -> PropertyNode {
        PropertyNode {
            name: name.to_string(),
            prop_type: "string".into(),
            description: None,
            format: None,
            is_required: required,
            is_nullable: !required,
            is_id: false,
            type_expr: None,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: codegraph_naming::to_snake_case(name),
            pg_column_type: "TEXT".into(),
            rust_field_name: codegraph_naming::to_snake_case(name),
            rust_field_type: "String".into(),
            sea_orm_type: "String".into(),
            render_strategy: "scalar".into(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        }
    }

    fn schema(title: &str, table: &str, domain: &str) -> SchemaNode {
        SchemaNode {
            namespace: None,
            schema_id: format!("id:{title}"),
            title: title.to_string(),
            access: None,
            annotations: None,
            description: None,
            schema_type: "object".to_string(),
            classification: "entity".to_string(),
            domain: Some(domain.to_string()),
            rel_path: format!("{title}.json"),
            pg_type: "UUID".to_string(),
            rust_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            rust_type_name: title.to_string(),
            pg_table_name: table.to_string(),
            api_path_segment: table.to_string(),
            parent_schema: None,
            is_entity: true,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
            custom_annotations: Default::default(),
        }
    }

    #[test]
    fn only_required_scalar_edge_less_non_identity_id_names_are_convention_shaped() {
        assert!(is_convention_shape(&prop("caseId", true)));
        let mut optional = prop("caseId", false);
        optional.is_required = false;
        assert!(!is_convention_shape(&optional));
        let mut identity = prop("workerId", true);
        identity.is_id = true;
        assert!(!is_convention_shape(&identity));
        let mut with_edge = prop("caseId", true);
        with_edge.ref_target = Some("hr/json/case.json".to_string());
        assert!(!is_convention_shape(&with_edge));
        let mut array = prop("caseId", true);
        array.is_array = true;
        assert!(!is_convention_shape(&array));
        assert!(!is_convention_shape(&prop("title", true)));
        let mut classification = prop("caseId", true);
        classification.classification_kind = Some(RefClassificationKind::EntityReference);
        // Classification alone does not disqualify the shape: the resolver is
        // consulted by callers that already know the kind, and the UI plane
        // relies on the shape for refs whose edge is missing.
        assert!(is_convention_shape(&classification));
    }

    #[test]
    fn stems_collapse_case_and_underscores() {
        assert_eq!(
            normalize_convention_stem("case"),
            normalize_convention_stem("Case")
        );
        assert_eq!(
            normalize_convention_stem("worker_type"),
            normalize_convention_stem("WorkerType")
        );
        let case_prop = prop("caseId", true);
        assert_eq!(convention_stem(&case_prop), Some("case"));
        assert_eq!(convention_stem(&prop("case", true)), None);
    }

    #[tokio::test]
    async fn a_unique_same_domain_stem_resolves_and_the_subject_never_does() {
        let engine = codegraph_core::mock::MockEngine::builder()
            .with_schema(schema("CaseType", "case", "hr"))
            .with_schema(schema("WorkerType", "worker", "hr"))
            .with_schema(schema("OrderType", "order", "sales"))
            .build();
        let resolved =
            resolve_convention_ref(&engine, &prop("caseId", true), Some("hr"), "ChildType")
                .await
                .expect("case_id resolves to CaseType");
        assert_eq!(resolved.title, "CaseType");
        assert!(
            resolve_convention_ref(&engine, &prop("childId", true), Some("hr"), "ChildType")
                .await
                .is_none(),
            "the subject entity is never its own convention target (#479)"
        );
    }
}
