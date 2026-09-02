//! Regression tests for include-path machinery defects (issue #82):
//! 1. Junction arrays (array-of-entity-ref) must be skipped — explicit and
//!    auto-discovered — because the data lives in a junction table, not an
//!    FK column the fetch machinery can query.
//! 2. Child-style auto-discovery must not emit reverse fetch helpers for
//!    targets that carry no parent FK column.
//! 3. Space-containing schema titles must resolve imports to the defining
//!    module (no self-referencing dto_included imports).

use std::path::Path;

use codegraph::generate;
use codegraph::generate::api::resolve_include_paths;
use codegraph::generate::ddd::dto::DtoGenerator;
use codegraph::generate::traits::EntityGenerator;
use codegraph::generate::ProjectConfig;
use codegraph_core::mock::MockEngine;
use codegraph_core::types::DetectionSource;
use codegraph_core::types::{ParentCandidate, PropertyNode, SchemaNode};
use codegraph_type_contracts::RefClassificationKind;

fn schema_for(title: &str, table: &str, rust_type_name: &str) -> SchemaNode {
    SchemaNode {
        custom_annotations: Default::default(),
        schema_id: format!("hr/json/{title}.json"),
        title: title.into(),
        description: None,
        schema_type: "object".into(),
        classification: "entity_reference".into(),
        domain: Some("hr".into()),
        rel_path: format!("hr/json/{title}.json"),
        pg_type: "UUID".into(),
        rust_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        rust_type_name: rust_type_name.into(),
        pg_table_name: table.into(),
        api_path_segment: table.replace('_', "-"),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    }
}

fn prop(
    name: &str,
    column: &str,
    ref_target: &str,
    is_array: bool,
    kind: RefClassificationKind,
) -> PropertyNode {
    PropertyNode {
        name: name.into(),
        prop_type: if is_array {
            "array".into()
        } else {
            "object".into()
        },
        description: None,
        format: None,
        is_required: false,
        is_nullable: true,
        is_array,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: column.into(),
        pg_column_type: "UUID".into(),
        rust_field_name: name.into(),
        rust_field_type: if is_array {
            "Vec<Uuid>".into()
        } else {
            "Option<Uuid>".into()
        },
        sea_orm_type: "Uuid".into(),
        render_strategy: "entity_reference".into(),
        ref_target: Some(ref_target.into()),
        classification: None,
        projection: None,
        classification_kind: Some(kind),
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

fn config_with_allow_include(allow: Option<Vec<String>>) -> codegraph_config::DomainConfig {
    let entities_line = match &allow {
        Some(paths) => format!(
            "allow_include = [{}]",
            paths
                .iter()
                .map(|p| format!("\"{p}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        None => "operations = [\"read\"]".to_string(),
    };
    let toml_str = format!(
        r#"
[defaults]
operations = ["read"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType", "PartyType"]

[domains.hr.entity_config.WorkerType]
{entities_line}
"#
    );
    codegraph_config::config::parse_domain_config_str(&toml_str).unwrap()
}

fn test_tera() -> tera::Tera {
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    generate::template_engine::create_tera(&template_dir).unwrap()
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().unwrap()
}

// ── 1. Explicit include path through a junction array is skipped ────────

#[test]
fn explicit_include_through_junction_array_is_skipped() {
    let worker = schema_for("WorkerType", "worker", "Worker");
    let party = schema_for("PartyType", "party", "Party");
    let engine = MockEngine::builder()
        .with_schema(worker.clone())
        .with_schema(party.clone())
        .with_ref_target("parties", "WorkerType", party.clone())
        .with_properties(
            "WorkerType",
            vec![prop(
                "parties",
                "parties",
                "PartyType",
                true,
                RefClassificationKind::EntityReference,
            )],
        )
        .build();

    let paths = rt()
        .block_on(resolve_include_paths(
            &engine,
            &config_with_allow_include(Some(vec!["parties".into()])),
            "hr",
            "WorkerType",
            Some(&vec!["parties".into()]),
        ))
        .expect("resolution should not fail");

    assert!(
        paths.is_empty(),
        "junction array must not produce an include path, got {:?}",
        paths.iter().map(|p| p.alias.clone()).collect::<Vec<_>>()
    );
}

// ── 2. Auto-discovery skips junction arrays ─────────────────────────────

#[test]
fn auto_discovery_skips_junction_array_relationship() {
    let worker = schema_for("WorkerType", "worker", "Worker");
    let party = schema_for("PartyType", "party", "Party");
    let engine = MockEngine::builder()
        .with_schema(worker.clone())
        .with_schema(party.clone())
        .with_ref_target("parties", "WorkerType", party.clone())
        .with_properties(
            "WorkerType",
            vec![prop(
                "parties",
                "parties",
                "PartyType",
                true,
                RefClassificationKind::EntityReference,
            )],
        )
        .build();

    let paths = rt()
        .block_on(resolve_include_paths(
            &engine,
            &config_with_allow_include(None),
            "hr",
            "WorkerType",
            None,
        ))
        .expect("resolution should not fail");

    assert!(
        !paths
            .iter()
            .any(|p| p.segments.iter().any(|s| s.schema_title == "PartyType")),
        "auto-discovery must not traverse junction arrays, got {:?}",
        paths.iter().map(|p| p.alias.clone()).collect::<Vec<_>>()
    );
}

// ── 3. Child-style discovery skips targets without a parent FK column ───

#[test]
fn auto_discovery_skips_children_without_parent_fk() {
    // Trust holds settlorIds (junction) → Party. Party has NO property
    // referencing trust and no injected parent FK — a child-style include
    // would emit a fetch helper filtering a nonexistent `trust_id` column.
    let trust = schema_for("TrustType", "trust", "Trust");
    let party = schema_for("PartyType", "party", "Party");
    let engine = MockEngine::builder()
        .with_schema(trust.clone())
        .with_schema(party.clone())
        .with_ref_target("settlor_ids", "TrustType", party.clone())
        .with_properties(
            "TrustType",
            vec![prop(
                "settlor_ids",
                "settlor_ids",
                "PartyType",
                true,
                RefClassificationKind::EntityReference,
            )],
        )
        .with_parent_candidate(ParentCandidate {
            child_title: "PartyType".into(),
            parent_title: "TrustType".into(),
            field_name: "settlor_ids".into(),
            source: DetectionSource::ArrayItems,
        })
        .build();

    let paths = rt()
        .block_on(resolve_include_paths(
            &engine,
            &config_with_allow_include(None),
            "hr",
            "TrustType",
            None,
        ))
        .expect("resolution should not fail");

    assert!(
        !paths
            .iter()
            .any(|p| p.segments.iter().any(|s| s.schema_title == "PartyType")),
        "child-style discovery must skip junction targets without a parent FK"
    );
}

// ── 4. Space-titled entities resolve imports to the defining module ─────

#[test]
fn space_titled_include_imports_target_module() {
    // Title with a space; rust_type_name is the ingestion-sanitized form.
    let worker = schema_for("WorkerType", "worker", "Worker");
    let review = schema_for("Review Decision", "review_decision", "ReviewDecision");
    let engine = MockEngine::builder()
        .with_schema(worker.clone())
        .with_schema(review.clone())
        .with_ref_target("review_decision", "WorkerType", review.clone())
        .with_properties(
            "WorkerType",
            vec![prop(
                "review_decision",
                "review_decision_id",
                "Review Decision",
                false,
                RefClassificationKind::EntityReference,
            )],
        )
        .build();

    let config = {
        let toml_str = r#"
[defaults]
operations = ["read"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType"]

[domains.hr.entity_config.WorkerType]
allow_include = ["review_decision"]
"#;
        codegraph_config::config::parse_domain_config_str(toml_str).unwrap()
    };

    let output_dir = tempfile::TempDir::new().unwrap();
    let gen = DtoGenerator::new(output_dir.path());
    let files = rt()
        .block_on(gen.generate(
            &engine,
            "WorkerType",
            "hr",
            &config,
            &test_tera(),
            &ProjectConfig::default(),
        ))
        .expect("DtoGenerator failed");

    let included = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_included"))
        .expect("should emit dto_included.rs");

    assert!(
        included.content.contains(
            "use crate::domain::hr::review_decision::dto_response::ReviewDecisionResponse;"
        ),
        "import must resolve to the target entity's dto_response module:\n{}",
        included.content
    );
    assert!(
        !included
            .content
            .contains("use crate::domain::hr::worker::dto_included::ReviewDecisionResponse;"),
        "must not self-reference the caller's dto_included module:\n{}",
        included.content
    );
}
