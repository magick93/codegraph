//! Handler emission pins for the `?include_deleted=` read toggle (issue #792).
//!
//! The soft-delete toggle exists end-to-end below http (both providers'
//! query/repository planes take `include_deleted: bool` when the entity is
//! auditable) but the generated handlers HARDCODE `false`. These pins hold
//! the http surface:
//!
//! - an auditable entity's GET/LIST forward `params.include_deleted
//!   .unwrap_or(false)` where `false` used to be hardcoded;
//! - `ListParams` carries `include_deleted: Option<bool>` (strict bool parse
//!   rides the existing `serde_qs` `QsQuery` machinery — a garbage value
//!   400s at extraction);
//! - the pre-rendered 403 guard renders for auditable entities, naming the
//!   `manager` minimum (the api-key bypass + role-hierarchy check);
//! - the dedicated `/search` endpoint forwards the flag identically;
//! - a NON-auditable entity's handler does NOT reference the param at all
//!   (accept-and-ignore: the query plane has no flag to forward).

use crate::harness::{
    include_domain_config, no_include_domain_config, setup_include_mock,
    setup_include_mock_with_refs, test_project_config, test_tera,
};
use codegraph::generate;
#[allow(unused_imports)]
use codegraph::generate::traits::EntityGenerator;
use codegraph_config::DomainConfig;

async fn render_handler(mock: &codegraph_core::mock::MockEngine, config: &DomainConfig) -> String {
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/codegraph-test-handler-include-deleted");

    let generator = generate::api::handler::HandlerGenerator::new(&output_dir);
    let files = generator
        .generate(
            mock,
            "WorkerType",
            "hr",
            config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();
    assert_eq!(files.len(), 1, "Should produce exactly one handler file");
    files[0].content.clone()
}

/// The auditable GET forwards the resolved toggle into `find_by_id` (both the
/// root and the child-scoped arm read the same param).
#[tokio::test]
async fn auditable_get_forwards_the_include_deleted_param() {
    let mock = setup_include_mock_with_refs();
    let content = render_handler(&mock, &include_domain_config()).await;

    assert!(
        content.contains("find_by_id(id, params.include_deleted.unwrap_or(false)"),
        "auditable GET must forward the resolved toggle into find_by_id. Got:\n{content}"
    );
}

/// `ListParams` gains the strict-bool toggle field (serde_qs rejects a
/// garbage value with a 400 at extraction).
#[tokio::test]
async fn list_params_carries_the_include_deleted_toggle() {
    let mock = setup_include_mock_with_refs();
    let content = render_handler(&mock, &include_domain_config()).await;

    assert!(
        content.contains("#[serde(default)]\n    pub include_deleted: Option<bool>,"),
        "ListParams must carry the toggle as a strict Option<bool>. Got:\n{content}"
    );
}

/// LIST forwards the toggle into both read calls: the plain `list_filtered`
/// and the `?q=` full-text delegation to `search`.
#[tokio::test]
async fn auditable_list_forwards_the_include_deleted_param() {
    let mock = setup_include_mock_with_refs();
    let config: DomainConfig = codegraph_config::config::parse_domain_config_str(
        r#"
[defaults]
operations = ["create", "read", "update", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType", "PersonType"]

[domains.hr.entity_config.WorkerType]
allow_include = ["person"]
operations = ["create", "read", "update", "list"]

[domains.hr.entity_config.WorkerType.search]
fts_columns = ["name"]
fts_rest_mode = "both"
"#,
    )
    .unwrap();
    let content = render_handler(&mock, &config).await;

    assert!(
        content.contains(
            "list_filtered(params.page, params.page_size, &filters, params.include_deleted.unwrap_or(false)"
        ),
        "auditable LIST must forward the resolved toggle into list_filtered. Got:\n{content}"
    );
    assert!(
        content.contains(
            "search(q, params.page, params.page_size, params.include_deleted.unwrap_or(false)"
        ),
        "the ?q= FTS delegation must forward the resolved toggle. Got:\n{content}"
    );
    // The dedicated /search endpoint forwards the same param (SearchParams
    // carries the toggle too).
    assert!(
        content.contains("search(&params.q, params.page, params.page_size, params.include_deleted.unwrap_or(false)"),
        "the dedicated /search endpoint must forward the resolved toggle. Got:\n{content}"
    );
    assert!(
        content.contains("/// Include soft-deleted rows in the results (`?include_deleted=true`)."),
        "SearchParams carries the toggle as well. Got:\n{content}"
    );
}

/// The under-privileged 403 guard renders for auditable entities, naming the
/// `manager` minimum and the fail-closed api-key/role-hierarchy semantics.
#[tokio::test]
async fn auditable_handlers_render_the_role_gate() {
    let mock = setup_include_mock_with_refs();
    let content = render_handler(&mock, &include_domain_config()).await;

    assert!(
        content.contains("params.include_deleted == Some(true)"),
        "the guard must fire only on an explicit true. Got:\n{content}"
    );
    assert!(
        content.contains("include_deleted_role_allows(&api_key_info)"),
        "the guard must consult the emitted role helper. Got:\n{content}"
    );
    assert!(
        content.contains("requires a manager-or-better role"),
        "the 403 message must name the `manager` minimum. Got:\n{content}"
    );
    assert!(
        content.contains("AppError::forbidden("),
        "denial is 403, never a silent ignore. Got:\n{content}"
    );
    // The api-key bypass + hierarchy rank check ride the emitted helper.
    assert!(
        content.contains("crate::middleware::AuthMode::ApiKey"),
        "API-key sessions bypass the role gate (scopes enforce at the DB tier). Got:\n{content}"
    );
    assert!(
        content.contains("const HIERARCHY: &[&str]"),
        "the domain's role hierarchy rides the emitted helper. Got:\n{content}"
    );
}

/// An auditable entity WITHOUT an include surface still extracts `ListParams`
/// on GET (the extractor gate widens from `has_include` to
/// `has_include or is_auditable`).
#[tokio::test]
async fn auditable_get_without_include_still_extracts_list_params() {
    let mock = setup_include_mock();
    let content = render_handler(&mock, &no_include_domain_config()).await;

    assert!(
        !content.contains("pub include: Option<String>"),
        "no include surface: the include field must be absent. Got:\n{content}"
    );
    assert!(
        content.contains("crate::qs_query::QsQuery(params): crate::qs_query::QsQuery<ListParams>,"),
        "the auditable GET still extracts ListParams for the toggle. Got:\n{content}"
    );
    assert!(
        content.contains("find_by_id(id, params.include_deleted.unwrap_or(false)"),
        "the toggle is forwarded on the no-include GET too. Got:\n{content}"
    );
    assert!(
        content.contains("pub include_deleted: Option<bool>,"),
        "ListParams is defined (widened gating) and carries the toggle. Got:\n{content}"
    );
}

/// A non-auditable entity's handler does NOT reference the param anywhere:
/// the query plane has no flag to forward, so the request is
/// accepted-and-ignored (`ListParams` has no such field; serde_qs drops the
/// unknown key).
#[tokio::test]
async fn non_auditable_handler_does_not_reference_the_param() {
    let mock = setup_include_mock_with_refs();
    let config: DomainConfig = codegraph_config::config::parse_domain_config_str(
        r#"
[defaults]
operations = ["create", "read", "update", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
auditable = false
entities = ["WorkerType", "PersonType"]

[domains.hr.entity_config.WorkerType]
allow_include = ["person"]
operations = ["create", "read", "update", "list"]
"#,
    )
    .unwrap();
    let content = render_handler(&mock, &config).await;

    assert!(
        !content.contains("include_deleted"),
        "non-auditable handlers must not reference the toggle. Got:\n{content}"
    );
    assert!(
        content.contains("find_by_id(id, api_key_info.api_key_id"),
        "the query call keeps today's shape (no include-deleted arg). Got:\n{content}"
    );
}
