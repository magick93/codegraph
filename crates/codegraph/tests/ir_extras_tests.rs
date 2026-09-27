//! IR extras (issue #279): per-definition access flags, structured
//! annotations, and the incompleteness taxonomy.
//!
//! Pins:
//! - `SchemaNode.access` (Public/Private) and `SchemaNode.annotations`
//!   (structured, beside the legacy `custom_annotations` map) are
//!   serde-defaulted additions that round-trip through the graph payload.
//! - The public-operations consumer: feature-gated route-auth skip
//!   (`route_auth_is_public`) plus the PUBLIC RLS policy migration
//!   (`PublicOperationsRlsGenerator`, following the #219/#169 patterns).
//! - Incompleteness reasons (`UnresolvedReference` | `TypeMismatch` |
//!   `Draft`) flow into mox ingest stats, the MoxState LSP model +
//!   diagnostics, and doctor output.
//! - Legacy payloads written before the fields existed still load with
//!   `None`/empty (wire compat).

use std::collections::HashMap;
use std::path::Path;

use codegraph_core::mock::MockEngine;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{
    Access, Annotation, AnnotationArg, Incompleteness, IncompletenessReason, SchemaNode,
};

fn schema(title: &str) -> SchemaNode {
    SchemaNode {
        namespace: None,
        schema_id: format!("common/{title}.json"),
        title: title.to_string(),
        description: None,
        schema_type: "object".into(),
        classification: "entity".into(),
        domain: Some("common".into()),
        rel_path: format!("common/{title}.json"),
        pg_type: "UUID".into(),
        rust_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        rust_type_name: title.to_string(),
        pg_table_name: "notice".into(),
        api_path_segment: "notices".into(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: HashMap::new(),
        access: None,
        annotations: None,
    }
}

// ── Access ─────────────────────────────────────────────────────────────

#[test]
fn schema_access_flag_round_trips() {
    let mut s = schema("NoticeType");
    assert_eq!(s.access, None, "default is unset — unchanged behavior");
    s.access = Some(Access::Public);
    let json = serde_json::to_string(&s).expect("serialize");
    let back: SchemaNode = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.access, Some(Access::Public));

    let mut private = schema("SecretType");
    private.access = Some(Access::Private);
    let json = serde_json::to_string(&private).expect("serialize");
    let back: SchemaNode = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.access, Some(Access::Private));
}

#[test]
fn legacy_payloads_without_access_and_annotations_still_load() {
    let legacy = r#"{"schema_id":"a/b.json","title":"T","schema_type":"object",
        "classification":"entity","rel_path":"a/b.json","pg_type":"JSONB",
        "rust_type":"t","sea_orm_type":"t","rust_type_name":"T",
        "pg_table_name":"t","api_path_segment":"t","is_entity":true,
        "is_codelist":false,"is_primitive_wrapper":false,"has_all_of":false,
        "has_one_of":false,"has_any_of":false,"has_definitions":false,
        "custom_annotations":{}}"#;
    let s: SchemaNode = serde_json::from_str(legacy).expect("legacy payload must deserialize");
    assert_eq!(s.access, None);
    assert_eq!(s.annotations, None);
}

// ── Structured annotations ─────────────────────────────────────────────

#[test]
fn structured_annotations_coexist_with_legacy_map() {
    let mut s = schema("NoticeType");
    s.custom_annotations.insert(
        "source".to_string(),
        serde_json::Value::String("mox".to_string()),
    );
    s.annotations = Some(vec![Annotation {
        name: "acme.doc.deprecated".to_string(),
        arguments: vec![
            AnnotationArg::Named {
                name: "since".to_string(),
                value: serde_json::json!("2026-01-01"),
            },
            AnnotationArg::Literal(serde_json::json!(2)),
        ],
    }]);

    let json = serde_json::to_string(&s).expect("serialize");
    let back: SchemaNode = serde_json::from_str(&json).expect("deserialize");
    let annotations = back.annotations.expect("structured annotations survive");
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].name, "acme.doc.deprecated");
    assert_eq!(annotations[0].arguments.len(), 2);
    assert_eq!(
        back.custom_annotations.get("source"),
        Some(&serde_json::Value::String("mox".to_string())),
        "the legacy string map coexists untouched"
    );
}

#[tokio::test]
async fn annotations_are_queryable() {
    let mut s = schema("NoticeType");
    s.annotations = Some(vec![Annotation {
        name: "acme.doc.tag".to_string(),
        arguments: vec![AnnotationArg::Literal(serde_json::json!("public-facing"))],
    }]);
    let engine = MockEngine::new();
    engine.ingest_schema(&s).await.expect("ingest schema");
    let stored = engine
        .get_schema("NoticeType")
        .await
        .expect("query")
        .expect("schema present");
    let annotations = stored
        .annotations
        .expect("structured annotations round-trip through the graph payload");
    assert_eq!(annotations[0].name, "acme.doc.tag");
}

// ── Public-operations consumer (flag-gated) ────────────────────────────

#[test]
fn route_auth_is_public_only_when_fully_public_and_flagged() {
    use codegraph::generate::api::router::route_auth_is_public;
    let ops = vec!["list".to_string(), "read".to_string()];

    assert!(
        !route_auth_is_public(
            false,
            Some(Access::Public),
            Some(&["list".into(), "read".into()]),
            &ops
        ),
        "flag OFF keeps auth layers (byte-identity)"
    );
    assert!(
        !route_auth_is_public(true, None, Some(&["list".into(), "read".into()]), &ops),
        "no access flag = unchanged behavior"
    );
    assert!(
        !route_auth_is_public(
            true,
            Some(Access::Private),
            Some(&["list".into(), "read".into()]),
            &ops
        ),
        "Private stays authed"
    );
    assert!(
        !route_auth_is_public(true, Some(Access::Public), Some(&["list".into()]), &ops),
        "partial public_operations keeps auth on the remainder"
    );
    assert!(route_auth_is_public(
        true,
        Some(Access::Public),
        Some(&["list".into(), "read".into()]),
        &ops
    ));
}

fn public_schema() -> SchemaNode {
    let mut s = schema("NoticeType");
    s.access = Some(Access::Public);
    s
}

fn domain_config_with_public_operations() -> codegraph_config::config::DomainConfig {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("domains.toml");
    std::fs::write(
        &path,
        r#"
[defaults]
operations = ["list", "read"]
type_suffix = "Type"

[domains.common]
label = "Common"
schema_dir = "schemas/common"
postgres_schema = "common"
entities = ["NoticeType"]

[domains.common.entity_config.NoticeType]
public_operations = ["list", "read"]

[domains.common.entity_config.NoticeType.permissions]
scope = "common:notice"
"#,
    )
    .expect("write domains.toml");
    codegraph_config::config::parse_domain_config(&path).expect("parse domains.toml")
}

async fn generate_public_rls(flag: bool) -> Vec<codegraph::generate::traits::GeneratedFile> {
    use codegraph::generate::traits::GlobalGenerator;
    let engine = MockEngine::new();
    engine
        .ingest_schema(&public_schema())
        .await
        .expect("ingest");
    let config = domain_config_with_public_operations();
    let tera = codegraph::generate::template_engine::create_tera(Path::new("")).unwrap();
    let project = codegraph::generate::ProjectConfig {
        public_operations_rls: flag,
        ..codegraph::generate::ProjectConfig::default()
    };
    let gen = codegraph::generate::db::public_operations_rls::PublicOperationsRlsGenerator::new(
        Path::new("/tmp/public-rls-test"),
    );
    let empty: Vec<codegraph::generate::GenerationEntry> = vec![];
    gen.generate(&engine, &config, &empty, &tera, &project)
        .await
        .expect("generation failed")
}

#[tokio::test]
async fn public_operation_mounts_public_route() {
    let files = generate_public_rls(true).await;
    assert_eq!(files.len(), 1, "exactly one PUBLIC RLS migration");
    assert!(
        files[0]
            .path
            .to_string_lossy()
            .ends_with("021000_public_operations_rls.sql"),
        "unexpected migration path: {}",
        files[0].path.display()
    );
    let content = &files[0].content;
    assert!(
        content.contains("TO PUBLIC"),
        "policy must be granted TO PUBLIC: {content}"
    );
    assert!(
        content.contains("common.notice"),
        "policy targets the public entity's table: {content}"
    );
}

#[tokio::test]
async fn public_rls_generator_is_inert_when_flag_off() {
    let files = generate_public_rls(false).await;
    assert!(
        files.is_empty(),
        "flag OFF must emit nothing (byte-identity): {files:?}"
    );
}

// ── Incompleteness ─────────────────────────────────────────────────────

#[tokio::test]
async fn unresolved_import_reports_hole_reason() {
    use codegraph::ingest::mox_ingest::{wire_alias_refs, MoxIngestStats, PendingAliasRef};
    let engine = MockEngine::new();
    // Seed the declaring schema so only the alias is unresolved.
    engine
        .ingest_schema(&schema("TodoListType"))
        .await
        .expect("ingest");
    let mut stats = MoxIngestStats::default();
    wire_alias_refs(
        &engine,
        &engine,
        &[PendingAliasRef {
            schema_title: "TodoListType".into(),
            prop_name: "items".into(),
            alias: "Widget".into(),
            is_array: false,
        }],
        "Type",
        &mut stats,
    )
    .await
    .expect("wiring");
    assert_eq!(stats.unresolved_aliases, 1);
    assert!(
        stats.incompleteness.iter().any(|i| matches!(
            &i.reason,
            IncompletenessReason::UnresolvedReference { target } if target == "Widget"
        )),
        "stats must carry the structured UnresolvedReference reason: {:?}",
        stats.incompleteness
    );
}

#[tokio::test]
async fn mox_lsp_uses_incompleteness_vocabulary() {
    use auto_lsp::lsp_types::DiagnosticSeverity;
    use codegraph::lsp::mox::{build_mox_state, import_missing_diagnostic};

    let dir = tempfile::tempdir().expect("tempdir");
    let model = dir.path().join("model.mox");
    std::fs::write(
        &model,
        "package todo\n\nimport schema \"missing.json\" as Widget\n\nclass C {}\n",
    )
    .expect("write model");

    let state = build_mox_state(
        std::slice::from_ref(&model),
        &std::collections::HashSet::new(),
        "Type",
    );
    assert!(
        state.incompleteness.iter().any(|i| matches!(
            &i.reason,
            IncompletenessReason::UnresolvedReference { target } if target == "missing.json"
        )),
        "MoxState must carry the structured hole reason: {:?}",
        state.incompleteness
    );

    let diagnostic = import_missing_diagnostic(
        &Incompleteness::unresolved_reference("missing.json"),
        "missing.json",
        dir.path().join("missing.json").as_path(),
        auto_lsp::lsp_types::Range::default(),
    );
    assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::ERROR));
    let data = diagnostic.data.expect("structured diagnostic payload");
    assert_eq!(data["kind"], "unresolved_reference", "{data}");
    assert_eq!(data["target"], "missing.json", "{data}");
}

#[test]
fn doctor_reports_incompleteness_reasons() {
    let dir = tempfile::tempdir().expect("tempdir");
    let model = dir.path().join("model.mox");
    std::fs::write(
        &model,
        concat!(
            "package todo\n\n",
            "import schema \"missing.json\" as Widget\n\n",
            "class C {}\n",
        ),
    )
    .expect("write model");
    let draft = dir.path().join("draft.mox");
    std::fs::write(
        &draft,
        "package draft\n\nclass D {\n    derived int total { expr { } }\n}\n",
    )
    .expect("write draft");

    let findings = codegraph::doctor_extras::scan_mox_incompleteness(&[model, draft]);
    assert!(
        findings.iter().any(|f| matches!(
            &f.incompleteness.reason,
            IncompletenessReason::UnresolvedReference { target } if target == "missing.json"
        )),
        "doctor must report the unresolved import target: {findings:?}"
    );
    assert!(
        findings
            .iter()
            .any(|f| matches!(&f.incompleteness.reason, IncompletenessReason::Draft)),
        "a derived feature with no expr body is a Draft: {findings:?}"
    );
    let line = codegraph::doctor_extras::report_line(&findings[0]);
    assert!(
        line.contains("UnresolvedReference") || line.contains("incomplete"),
        "doctor output carries the reason: {line}"
    );
}
