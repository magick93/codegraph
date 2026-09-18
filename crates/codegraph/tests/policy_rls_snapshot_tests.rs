//! Level 2: Insta snapshots for the policy-driven RLS generator
//! (issue #219). Postgres emission, the sqlite no-op, and the hard
//! generation errors for `when` expressions outside the mappable subset.
//! Run with: cargo test -p codegraph --test policy_rls_snapshot_tests

use std::collections::HashMap;
use std::path::Path;

use codegraph::generate::db::dialect::{dialect_for_target, DatabaseTarget};
use codegraph::generate::db::policy_rls::PolicyRlsGenerator;
use codegraph::generate::traits::GlobalGenerator;
use codegraph::generate::ProjectConfig;
use codegraph_config::config::{DefaultsConfig, DomainConfig, DomainEntry};
use codegraph_core::mock::MockEngine;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{
    ActorNode, ActorPolicyModel, ActorPolicyNode, CapabilityNode, GrantEdge,
};
use codegraph_type_contracts::RefClassificationKind;

fn property(name: &str, pg_type: &str) -> codegraph_core::types::PropertyNode {
    codegraph_core::types::PropertyNode {
        name: name.to_string(),
        prop_type: "string".to_string(),
        description: None,
        format: None,
        is_required: true,
        is_nullable: false,
        is_array: false,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: name.to_string(),
        pg_column_type: pg_type.to_string(),
        rust_field_name: name.to_string(),
        rust_field_type: "String".to_string(),
        sea_orm_type: "String".to_string(),
        render_strategy: "flat".to_string(),
        ref_target: None,
        classification: None,
        projection: None,
        classification_kind: Some(RefClassificationKind::PrimitiveWrapper),
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

fn engine_with_candidate() -> MockEngine {
    MockEngine::builder()
        .with_schema(codegraph_core::types::SchemaNode {
            schema_id: "candidate".into(),
            title: "CandidateType".into(),
            description: Some("A job candidate".into()),
            schema_type: "object".into(),
            classification: "entity".into(),
            domain: Some("recruiting".into()),
            rel_path: "recruiting/json/CandidateType.json".into(),
            pg_type: "entity".into(),
            rust_type: "CandidateType".into(),
            sea_orm_type: "Entity".into(),
            rust_type_name: "Candidate".into(),
            pg_table_name: "candidate".into(),
            api_path_segment: "candidates".into(),
            parent_schema: None,
            is_entity: true,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
            custom_annotations: Default::default(),
        })
        .with_properties(
            "CandidateType",
            vec![
                property("status", "TEXT"),
                property("amount", "INTEGER"),
                property("internal", "BOOLEAN"),
            ],
        )
        .build()
}

fn actor(name: &str, kind: Option<&str>, extends: Option<&str>) -> ActorNode {
    ActorNode {
        name: name.to_string(),
        kind: kind.map(|k| k.to_string()),
        extends: extends.map(|e| e.to_string()),
        block: Some("core".to_string()),
    }
}

fn capability(name: &str, class: &str) -> CapabilityNode {
    CapabilityNode {
        name: name.to_string(),
        class: class.to_string(),
        block: Some("core".to_string()),
    }
}

fn grant(actor: &str, capability: &str, effect: &str, when: Option<&str>) -> GrantEdge {
    GrantEdge {
        actor: actor.to_string(),
        capability: capability.to_string(),
        effect: effect.to_string(),
        when: when.map(|w| w.to_string()),
        obligations: vec![],
    }
}

fn policy(
    actors: Vec<ActorNode>,
    capabilities: Vec<CapabilityNode>,
    grants: Vec<GrantEdge>,
) -> ActorPolicyModel {
    ActorPolicyModel {
        actors,
        capabilities,
        grants,
        policy: ActorPolicyNode {
            blocks: vec!["core".to_string()],
            never_both: vec![],
            purposes: vec![],
            delegations: vec![],
        },
    }
}

fn domain_config() -> DomainConfig {
    let mut domains = HashMap::new();
    domains.insert(
        "recruiting".to_string(),
        DomainEntry {
            label: "Recruiting".into(),
            schema_dir: "schemas/recruiting".into(),
            postgres_schema: "recruiting".into(),
            depends_on: vec!["common".into()],
            entities: vec!["CandidateType".into()],
            entity_config: HashMap::new(),
            auto_discover: None,
            exclude_entities: vec![],
            force_entities: vec![],
            force_value_objects: vec![],
            exclude: vec![],
            auditable: None,
            tier: "extended".into(),
            worker_name: None,
            custom_domain: None,
            service_bindings: None,
            hyperdrive_binding: None,
            cron_triggers: None,
            remote_include_mode: None,
            webhooks: None,
            queue_name: None,
            queue_binding: None,
            queue_max_retries: None,
            queue_max_concurrency: None,
            observability: None,
            custom_routes: false,
        },
    );
    DomainConfig {
        defaults: DefaultsConfig {
            operations: vec!["create".into(), "read".into()],
            auto_discover: false,
            split_openapi_by_domain: false,
            app_name: "test-app".into(),
            max_bulk_size: 100,
            type_suffix: "Type".into(),
            types_import_prefix: "codegraph_type_contracts".into(),
            generation_mode: "full".into(),
            api_version: "v1".into(),
        },
        rbac: None,
        domains,
    }
}

async fn generate(
    engine: &MockEngine,
    dialect: DatabaseTarget,
) -> Result<Vec<codegraph::generate::traits::GeneratedFile>, codegraph_core::error::Error> {
    let config = domain_config();
    // create_tera uses the templates embedded by codegraph-generate's
    // build.rs; the path argument is ignored (kept for signature compat).
    let tera = codegraph::generate::template_engine::create_tera(Path::new("")).unwrap();
    let project = ProjectConfig::default();
    let gen = PolicyRlsGenerator::new(Path::new("/tmp/policy-rls-test"))
        .with_dialect(dialect_for_target(dialect));
    let empty: Vec<codegraph::generate::GenerationEntry> = vec![];
    gen.generate(engine, &config, &empty, &tera, &project).await
}

/// The core snapshot: human + agent + inherited actors, permit with a `when`
/// expression, unconditional permit, forbid → REVOKE, and the qualified
/// class form — all in one migration.
#[tokio::test]
async fn snapshot_policy_rls_migration_postgres() {
    let engine = engine_with_candidate();
    engine
        .ingest_actor_policy(&policy(
            vec![
                actor("Recruiter", Some("human"), None),
                actor("Auditor", Some("human"), Some("Recruiter")),
                actor("Screener", Some("agent"), None),
            ],
            vec![
                capability("ReviewCandidate", "recruiting::CandidateType"),
                capability("ArchiveCandidate", "CandidateType"),
            ],
            vec![
                grant(
                    "Recruiter",
                    "ReviewCandidate",
                    "permit",
                    Some("status == \"active\" && amount > 0"),
                ),
                grant("Auditor", "ReviewCandidate", "permit", None),
                grant(
                    "Recruiter",
                    "ArchiveCandidate",
                    "permit",
                    Some("internal != null"),
                ),
                grant("Screener", "ReviewCandidate", "forbid", None),
            ],
        ))
        .await
        .unwrap();

    let files = generate(&engine, DatabaseTarget::Postgres)
        .await
        .expect("generation failed");
    assert_eq!(files.len(), 1, "expected exactly one policy RLS migration");
    let file = &files[0];
    assert!(
        file.path
            .to_string_lossy()
            .ends_with("020000_policy_rls.sql"),
        "unexpected migration path: {}",
        file.path.display()
    );
    insta::assert_snapshot!("policy_rls_postgres_migration", &file.content);
}

/// Pinned decision #5: sqlite is a documented no-op (no RLS).
#[tokio::test]
async fn sqlite_emits_nothing() {
    let engine = engine_with_candidate();
    engine
        .ingest_actor_policy(&policy(
            vec![actor("Recruiter", Some("human"), None)],
            vec![capability("ReviewCandidate", "CandidateType")],
            vec![grant("Recruiter", "ReviewCandidate", "permit", None)],
        ))
        .await
        .unwrap();

    let files = generate(&engine, DatabaseTarget::Sqlite)
        .await
        .expect("generation failed");
    insta::assert_debug_snapshot!("policy_rls_sqlite_noop", &files);
}

/// No policy in the graph → no migration (flag on, nothing to add).
#[tokio::test]
async fn no_policy_graph_emits_nothing() {
    let engine = engine_with_candidate();
    let files = generate(&engine, DatabaseTarget::Postgres)
        .await
        .expect("generation failed");
    assert!(files.is_empty());
}

/// A `when` expression outside the closed subset is a HARD generation error
/// naming the capability (rexlang Cedar-backend philosophy).
#[tokio::test]
async fn unmappable_when_expr_is_a_hard_error_naming_the_capability() {
    let engine = engine_with_candidate();
    engine
        .ingest_actor_policy(&policy(
            vec![actor("Recruiter", Some("human"), None)],
            vec![capability("ReviewCandidate", "CandidateType")],
            vec![grant(
                "Recruiter",
                "ReviewCandidate",
                "permit",
                Some("amount + 1 > 0"),
            )],
        ))
        .await
        .unwrap();

    let err = generate(&engine, DatabaseTarget::Postgres)
        .await
        .expect_err("arithmetic in when_expr must fail generation");
    let message = err.to_string();
    assert!(
        message.contains("ReviewCandidate"),
        "error must name the capability, got: {message}"
    );
    assert!(
        message.contains("arithmetic"),
        "error must name the construct, got: {message}"
    );
}

/// A field reference that is not a column of the capability's class cannot
/// be mapped faithfully → hard error naming the capability.
#[tokio::test]
async fn unknown_field_in_when_expr_is_a_hard_error_naming_the_capability() {
    let engine = engine_with_candidate();
    engine
        .ingest_actor_policy(&policy(
            vec![actor("Recruiter", Some("human"), None)],
            vec![capability("ReviewCandidate", "CandidateType")],
            vec![grant(
                "Recruiter",
                "ReviewCandidate",
                "permit",
                Some("ghost == 1"),
            )],
        ))
        .await
        .unwrap();

    let err = generate(&engine, DatabaseTarget::Postgres)
        .await
        .expect_err("unknown field must fail generation");
    let message = err.to_string();
    assert!(
        message.contains("ReviewCandidate") && message.contains("ghost"),
        "error must name the capability and field, got: {message}"
    );
}

/// A conditional forbid cannot become a REVOKE (row predicates do not apply
/// to privileges) → hard error naming the capability.
#[tokio::test]
async fn conditional_forbid_is_a_hard_error_naming_the_capability() {
    let engine = engine_with_candidate();
    engine
        .ingest_actor_policy(&policy(
            vec![actor("Recruiter", Some("human"), None)],
            vec![capability("ReviewCandidate", "CandidateType")],
            vec![grant(
                "Recruiter",
                "ReviewCandidate",
                "forbid",
                Some("internal"),
            )],
        ))
        .await
        .unwrap();

    let err = generate(&engine, DatabaseTarget::Postgres)
        .await
        .expect_err("conditional forbid must fail generation");
    let message = err.to_string();
    assert!(
        message.contains("ReviewCandidate"),
        "error must name the capability, got: {message}"
    );
    assert!(
        message.contains("forbid"),
        "error must mention forbid, got: {message}"
    );
}

/// A capability whose class does not resolve to an ingested schema cannot
/// be mapped to a table → hard error naming the capability.
#[tokio::test]
async fn unresolvable_class_is_a_hard_error_naming_the_capability() {
    let engine = engine_with_candidate();
    engine
        .ingest_actor_policy(&policy(
            vec![actor("Recruiter", Some("human"), None)],
            vec![capability("ApproveRefund", "refunds::RefundRequest")],
            vec![grant("Recruiter", "ApproveRefund", "permit", None)],
        ))
        .await
        .unwrap();

    let err = generate(&engine, DatabaseTarget::Postgres)
        .await
        .expect_err("unresolvable class must fail generation");
    let message = err.to_string();
    assert!(
        message.contains("ApproveRefund"),
        "error must name the capability, got: {message}"
    );
    assert!(
        message.contains("RefundRequest"),
        "error must name the class, got: {message}"
    );
}
