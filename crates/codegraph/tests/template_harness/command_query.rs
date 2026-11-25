use crate::harness::{
    parent_child_mock, setup_mock, test_domain_config, test_generation_order, test_project_config,
    test_tera,
};
use codegraph::generate;
#[allow(unused_imports)]
use codegraph::generate::traits::{DomainGenerator, EntityGenerator, GlobalGenerator};

// === Command Template Tests ===

#[tokio::test]
async fn candidate_command() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-cmd");

    let generator = generate::ddd::command::CommandGenerator::new(&output_dir);
    let files = generator
        .generate(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    let content = &files[0].content;
    assert!(
        content.contains("Candidate"),
        "Should reference Candidate entity"
    );
}

// === RLS session-context single-round-trip bundle (#169 phase 3) ===

#[tokio::test]
async fn ddd_session_context_is_one_round_trip() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();

    for (label, files) in [
        (
            "command.rs",
            generate::ddd::command::CommandGenerator::new(&std::path::PathBuf::from(
                "/tmp/hr-graph-test-harness-rls-cmd",
            ))
            .generate(
                &mock,
                "CandidateType",
                "recruiting",
                &config,
                &tera,
                &test_project_config(),
            )
            .await
            .unwrap(),
        ),
        (
            "query.rs",
            generate::ddd::query::QueryGenerator::new(&std::path::PathBuf::from(
                "/tmp/hr-graph-test-harness-rls-query",
            ))
            .generate(
                &mock,
                "CandidateType",
                "recruiting",
                &config,
                &tera,
                &test_project_config(),
            )
            .await
            .unwrap(),
        ),
    ] {
        assert_eq!(files.len(), 1, "{label}: expected one file");
        let content = &files[0].content;
        assert!(
            content.contains("set_rls_session_vars"),
            "{label}: should keep the set_rls_session_vars helper"
        );
        // The context must be delivered as ONE simple-query payload: set_config
        // calls and the role flip bundled into a single execute_unprepared.
        assert!(
            content.contains("execute_unprepared"),
            "{label}: set_rls_session_vars must use one execute_unprepared round trip"
        );
        assert!(
            content.contains("SET LOCAL ROLE app_user"),
            "{label}: the role flip must ride the same bundle"
        );
        assert!(
            !content.contains(".to_string(),\n        \"SET LOCAL ROLE"),
            "{label}: the role flip must NOT be a standalone Statement::from_string execute"
        );
        assert!(
            !content.contains("from_sql_and_values"),
            "{label}: set_config must not ride a parameterised statement (the bundle is inlined)"
        );
    }
}

// === RLS denial → HTTP 403 mapping (#169 phase 4b) ===

#[tokio::test]
async fn domain_errors_map_rls_denials_to_forbidden() {
    let tera = test_tera();

    let mut ctx = tera::Context::new();
    ctx.insert("domain", "recruiting");
    ctx.insert("errors", &Vec::<serde_json::Value>::new());
    ctx.insert(
        "project",
        &serde_json::json!({
            "generator_name": "codegraph",
            "persistence_provider": "sea_orm",
            "hooks_api_crate": null,
            "database_target": "postgres",
        }),
    );
    let errors = tera
        .render("ddd/errors.tera", &ctx)
        .expect("errors.tera must render");

    // The enum keeps the RLS-forbidden shape and the generic 422 class.
    assert!(
        errors.contains("Forbidden("),
        "errors.tera should define a Forbidden variant"
    );
    assert!(
        errors.contains("StatusCode::FORBIDDEN"),
        "Forbidden must map to HTTP 403"
    );
    assert!(
        errors.contains("Unprocessable("),
        "integrity violations need a 422 class:\n{errors}"
    );
    assert!(
        errors.contains("StatusCode::UNPROCESSABLE_ENTITY"),
        "the Unprocessable variant must map to HTTP 422:\n{errors}"
    );
    assert!(
        errors.contains("crate::error::classify_db_error(&e)"),
        "from_repo_err must delegate to the ONE app-level classifier:\n{errors}"
    );

    // Issue #828: the classifier is in the app error plane, by SQLSTATE.
    let error_rs = tera
        .render("scaffold/error.tera", &ctx)
        .expect("scaffold/error.tera must render");
    for needle in [
        "pub fn classify_db_error(",
        "Some(\"P0403\") | Some(\"42501\") => DbErrorClass::Forbidden",
        "Some(\"23505\") => DbErrorClass::Conflict",
        "Some(\"23503\") =>",
        "Some(\"23502\") | Some(\"23514\") => DbErrorClass::Unprocessable",
    ] {
        assert!(
            error_rs.contains(needle),
            "the app error plane must carry `{needle}`:\n{error_rs}"
        );
    }
    for marker in ["INSUFFICIENT_SCOPE", "violates row-level security policy"] {
        assert!(
            error_rs.contains(marker),
            "the no-SQLSTATE fallback must recognise `{marker}`:\n{error_rs}"
        );
    }
}

// === DB error detail survives the mapping (#810) + SQLSTATE classification (#828) ===

/// A database error must keep its SOURCE detail through the generated
/// mapping: `tokio-postgres` renders a server error as the bare string
/// `db error` (code/message/detail/hint live in the source), so the
/// domain classifier logs the typed detail AND classifies by SQLSTATE.
/// Without this a scope-denied create answers `500 "db error"` instead of
/// 403 (codegraph#810, diagnosed in codegraph#828), and an FK/unique
/// refusal is an opaque 500 instead of 422/409.
#[tokio::test]
async fn database_errors_keep_their_source_detail() {
    let tera = test_tera();

    let mut ctx = tera::Context::new();
    ctx.insert("domain", "recruiting");
    ctx.insert("errors", &Vec::<serde_json::Value>::new());
    ctx.insert(
        "project",
        &serde_json::json!({
            "generator_name": "codegraph",
            "persistence_provider": "cornucopia",
            "hooks_api_crate": null,
            "database_target": "postgres",
        }),
    );
    let error_rs = tera
        .render("scaffold/error.tera", &ctx)
        .expect("scaffold/error.tera must render");

    let errors = &error_rs;
    assert!(
        errors.contains("pub fn classify_db_error("),
        "the generated error plane must expose ONE SQLSTATE classifier:\n{errors}"
    );
    assert!(
        errors.contains("downcast_ref::<tokio_postgres::Error>()"),
        "the workers slice's typed error must be recovered by downcast:\n{errors}"
    );
    assert!(
        errors.contains("as_db_error()"),
        "the tokio-postgres server detail is only reachable through as_db_error:\n{errors}"
    );
    assert!(
        errors.contains("RuntimeErr::SqlxError"),
        "the monolith's sea-orm/sqlx error must be recovered by downcast too:\n{errors}"
    );
    assert!(
        errors.contains("as_database_error()"),
        "sqlx exposes the SQLSTATE through as_database_error:\n{errors}"
    );
    assert!(
        errors.contains("sqlstate = detail.code"),
        "the log must carry the SQLSTATE (the #810 diagnosis):\n{errors}"
    );
    assert!(
        errors.contains("db_message = detail.message"),
        "the log must carry the server message:\n{errors}"
    );
    assert!(
        errors.contains("error_debug = ?e"),
        "the log must also carry the Debug form:\n{errors}"
    );
    // The old Display-based decision must be structurally gone: it may only
    // survive as the no-SQLSTATE fallback (one `None =>` arm).
    assert!(
        !errors.contains("pub fn from_repo_err(e: impl std::fmt::Display + std::fmt::Debug)"),
        "from_repo_err must take a 'static error so it can downcast:\n{errors}"
    );
    // The domain enum delegates to it (the ONE derivation contract) and the
    // domain-specific log lives there now.
    let domain_errors = tera
        .render("ddd/errors.tera", &ctx)
        .expect("errors.tera must render");
    assert!(
        domain_errors.contains("crate::error::classify_db_error(&e)"),
        "from_repo_err must delegate to the app-level classifier:\n{domain_errors}"
    );

    // The cornucopia repository adapter must preserve the typed error
    // (boxed) rather than flattening it to a String — the String path loses
    // `source()` entirely.
    let mock = setup_mock().await;
    let config = test_domain_config();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-cornucopia-repo");
    let mut project = test_project_config();
    project.database.persistence_provider =
        codegraph::generate::profile::PersistenceProvider::Cornucopia;
    let repo = generate::ddd::cornucopia_repo::CornucopiaRepoGenerator::new(&output_dir)
        .generate(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &project,
        )
        .await
        .unwrap();
    let content = &repo[0].content;
    assert!(
        content.contains("Box::new(e)") || content.contains("map_err(db_err)"),
        "the cornucopia repository adapter must preserve the DB error \
         (source detail), not flatten it to a string:\n{content}"
    );
    assert!(
        !content.contains(".map_err(|e| e.to_string())"),
        "a stringified query error loses the source detail (issue #810):\n{content}"
    );
}

#[tokio::test]
async fn command_and_query_route_repo_errors_through_the_classifier() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();

    let cmd = generate::ddd::command::CommandGenerator::new(&std::path::PathBuf::from(
        "/tmp/hr-graph-test-harness-rls-403-cmd",
    ))
    .generate(
        &mock,
        "CandidateType",
        "recruiting",
        &config,
        &tera,
        &test_project_config(),
    )
    .await
    .unwrap();
    let query = generate::ddd::query::QueryGenerator::new(&std::path::PathBuf::from(
        "/tmp/hr-graph-test-harness-rls-403-query",
    ))
    .generate(
        &mock,
        "CandidateType",
        "recruiting",
        &config,
        &tera,
        &test_project_config(),
    )
    .await
    .unwrap();

    for (label, files) in [("command.rs", cmd), ("query.rs", query)] {
        let content = &files[0].content;
        assert!(
            content.contains("from_repo_err"),
            "{label}: repo errors must be classified via from_repo_err"
        );
        assert!(
            !content.contains("contains(\"duplicate key\")")
                && !content.contains("UNIQUE constraint"),
            "{label}: a unique violation must be classified by SQLSTATE, not by \
             Display text (tokio-postgres renders it as bare `db error`):\n{content}"
        );
    }
}

// === Single-trip JWT auth (#169 phase 6) ===

#[tokio::test]
async fn jwt_auth_resolves_context_in_one_round_trip() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-jwt-single-trip");

    let generator = generate::scaffold::generator::ScaffoldGenerator::new(
        &output_dir,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        "sea-orm",
    );
    let files = generator
        .generate(
            &mock,
            &config,
            &test_generation_order(),
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let middleware = &files
        .iter()
        .find(|f| f.path.ends_with("middleware/mod.rs"))
        .expect("Should generate middleware/mod.rs")
        .content;

    // The JWT path must spend exactly ONE DB round trip: the merged
    // resolve_jwt_context RPC (org + role in one payload).
    assert!(
        middleware.contains("resolve_jwt_context"),
        "verify_jwt must use the merged resolve_jwt_context RPC"
    );
    assert!(
        !middleware.contains("resolve_user_org("),
        "verify_jwt must not do a separate org lookup round trip"
    );
    assert!(
        !middleware.contains("get_current_user_role("),
        "verify_jwt must not do a separate role lookup round trip"
    );

    // The generated migration exposes the merged SECURITY DEFINER function.
    let migration = &files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("api_key_management"))
        .expect("Should generate the api key migration")
        .content;
    assert!(
        migration.contains("CREATE OR REPLACE FUNCTION public.resolve_jwt_context"),
        "the migration must define public.resolve_jwt_context"
    );
    assert!(
        migration.contains("GRANT EXECUTE ON FUNCTION public.resolve_jwt_context TO app_user"),
        "app_user must be able to call resolve_jwt_context"
    );
}

// === Query Template Tests ===
#[tokio::test]
async fn candidate_query() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-query");

    let generator = generate::ddd::query::QueryGenerator::new(&output_dir);
    let files = generator
        .generate(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    let content = &files[0].content;
    assert!(
        content.contains("Candidate"),
        "Should reference Candidate entity"
    );
}

// === Event Template Tests ===

#[tokio::test]
async fn candidate_event() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-event");

    let generator = generate::ddd::event::EventGenerator::new(&output_dir);
    let files = generator
        .generate(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    let content = &files[0].content;
    assert!(
        content.contains("Candidate"),
        "Should reference Candidate entity"
    );
}

// === Enriched Event Tests ===

#[tokio::test]
async fn candidate_enriched_event() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-enriched-event");

    let generator = generate::ddd::event::EventGenerator::new(&output_dir);
    let files = generator
        .generate(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    let content = &files[0].content;
    assert!(
        content.contains("CandidateEventPayload"),
        "Should contain enriched event payload struct. Got:\n{}",
        content
    );
    assert!(
        content.contains("correlation_id: Uuid"),
        "Event payload should include correlation_id. Got:\n{}",
        content
    );
    assert!(
        content.contains("occurred_at: DateTime<Utc>"),
        "Event payload should include occurred_at. Got:\n{}",
        content
    );
    assert!(
        content.contains("platform_organization_id: Uuid"),
        "Event payload should include platform_organization_id. Got:\n{}",
        content
    );
    assert!(
        content.contains("changed_fields: Vec<String>"),
        "Updated variant should include changed_fields. Got:\n{}",
        content
    );
    assert!(
        content.contains("DelegationChanged"),
        "Should contain DelegationChanged variant for workflow entities. Got:\n{}",
        content
    );
}

// === Command with correlation_id Tests ===

#[tokio::test]
async fn candidate_command_correlation_id() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-cmd-corr");

    let generator = generate::ddd::command::CommandGenerator::new(&output_dir);
    let files = generator
        .generate(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    let content = &files[0].content;
    assert!(
        content.contains("correlation_id: Uuid"),
        "Command handler should accept correlation_id. Got:\n{}",
        content
    );
}

// === Security: session-context inlining tests (#169) ===

#[tokio::test]
async fn command_inlines_typed_uuid_context_bundle() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-cmd-security");

    let generator = generate::ddd::command::CommandGenerator::new(&output_dir);
    let files = generator
        .generate(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let cmd_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("command"))
        .expect("Should have a command file");

    // The context bundle inlines values (simple query protocol takes no bind
    // parameters) — but ONLY typed Uuid fields, so the interpolation surface
    // is injection-proof by construction.
    assert!(
        cmd_file
            .content
            .contains("set_config('app.current_api_key', '{}', true)"),
        "Command should inline the api_key context value. Got:\n{}",
        cmd_file.content
    );
    assert!(
        cmd_file
            .content
            .contains("set_config('app.organization_id', '{}', true)"),
        "Command should inline the org_id context value"
    );
    assert!(
        cmd_file
            .content
            .contains("set_config('app.correlation_id', '{}', true)"),
        "Command should inline the correlation_id context value"
    );
    // The format! args must be the typed Uuid locals, never user strings.
    assert!(
        cmd_file
            .content
            .contains("        api_key_id,\n        organization_id,"),
        "Command must interpolate typed Uuid locals, not arbitrary strings"
    );
    // One round trip: the role flip rides the same payload.
    assert!(
        cmd_file.content.contains("SET LOCAL ROLE app_user"),
        "Command should keep the app_user role flip in the bundle"
    );
    assert!(
        !cmd_file.content.contains("$1"),
        "Command must not use parameter placeholders ($1) for the context bundle"
    );
}

#[tokio::test]
async fn query_inlines_typed_uuid_context_bundle() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-query-security");

    let generator = generate::ddd::query::QueryGenerator::new(&output_dir);
    let files = generator
        .generate(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let query_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("query"))
        .expect("Should have a query file");

    // Query sets 2 vars inlined (no correlation_id on reads).
    assert!(
        query_file
            .content
            .contains("set_config('app.current_api_key', '{}', true)"),
        "Query should inline the api_key context value. Got:\n{}",
        query_file.content
    );
    assert!(
        query_file
            .content
            .contains("set_config('app.organization_id', '{}', true)"),
        "Query should inline the org_id context value"
    );
    assert!(
        query_file
            .content
            .contains("set_config('app.user_id', '{}', true)"),
        "Query should inline the user_id context value"
    );
    // One round trip: the role flip rides the same payload.
    assert!(
        query_file.content.contains("SET LOCAL ROLE app_user"),
        "Query should keep the app_user role flip in the bundle"
    );
    // Must NOT use parameter placeholders (the bundle rides the simple protocol).
    assert!(
        !query_file
            .content
            .contains("set_config('app.current_api_key', $1"),
        "Query must not use parameter placeholders for the context bundle"
    );
}

/// Child entity command template must accept parent_id and pass it to repository.
#[tokio::test]
async fn child_command_accepts_parent_id() {
    let (mock, candidates) = parent_child_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-child-command");

    let generator = generate::ddd::command::CommandGenerator::new(&output_dir)
        .with_parent_candidates(candidates);
    let files = generator
        .generate(
            &mock,
            "RewardType",
            "compensation",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    assert!(!files.is_empty(), "Command generator should produce a file");
    let content = &files[0].content;

    // create() must accept parent_id parameter
    assert!(
        content.contains("parent_id: Uuid"),
        "Child command create must accept parent_id parameter. Got:\n{content}"
    );

    // Must pass parent_id to repo.create
    assert!(
        content.contains("repo.create(&tx, cmd, parent_id"),
        "Child command must pass parent_id to repo.create. Got:\n{content}"
    );
}

/// Child entity query handler must include find_by_id_scoped.
#[tokio::test]
async fn child_query_has_find_by_id_scoped() {
    let (mock, candidates) = parent_child_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-child-query");

    let generator =
        generate::ddd::query::QueryGenerator::new(&output_dir).with_parent_candidates(candidates);
    let files = generator
        .generate(
            &mock,
            "RewardType",
            "compensation",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    assert!(!files.is_empty(), "Query generator should produce a file");
    let content = &files[0].content;

    assert!(
        content.contains("find_by_id_scoped"),
        "Child query must include find_by_id_scoped method. Got:\n{content}"
    );
}

// === Lifecycle hooks × persistence provider (issue #502) ===

fn hooks_project_config(
    provider: codegraph::generate::profile::PersistenceProvider,
) -> codegraph::generate::ProjectConfig {
    codegraph::generate::ProjectConfig {
        identity: codegraph::generate::IdentityConfig {
            hooks_api_crate: "my_hooks".into(),
            ..Default::default()
        },
        database: codegraph::generate::DatabaseConfig {
            persistence_provider: provider,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Issue #509 (the #502 refusal is RETIRED): under cornucopia the command
/// layer carries ALL hook call sites — transactional hooks run on the
/// open cornucopia transaction via the erased `HookTx` adapter (Abort ⇒
/// rollback preserved) and post-commit hooks ride the `ClientSource`
/// adapter, un-gated: they compile and fire in the wasm32 worker slice.
#[tokio::test]
async fn cornucopia_command_keeps_all_hooks_without_wasm_gates() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-cmd-cornucopia-hooks");

    let generator = generate::ddd::command::CommandGenerator::new(&output_dir);
    let files = generator
        .generate(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &hooks_project_config(codegraph::generate::profile::PersistenceProvider::Cornucopia),
        )
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    let content = &files[0].content;

    // Transactional hooks: present on every provider since #509.
    for tx_site in [
        "hooks.before_create(",
        "after_create_tx",
        "hooks.before_update(",
        "after_update_tx",
        "generated::types::TxContext::new(",
    ] {
        assert!(
            content.contains(tx_site),
            "cornucopia command must carry the transactional hook site `{tx_site}`:\n{content}"
        );
    }

    // The tx handle is the erased SPI adapter over the cornucopia DbTx.
    // The adapter is bound to a local BEFORE TxContext::new borrows it —
    // `&HookTxAdapter(&tx)` inline drops the temporary at the statement
    // end while the context still borrows it (E0716; the #518 runtime pin
    // executes this path against real Postgres).
    assert!(
        content.contains("let tx_adapter = crate::hook_adapters::HookTxAdapter(&tx);"),
        "tx hooks must bind the adapter to a local before the context borrows it:\n{content}"
    );
    assert!(
        content.contains("TxContext::new(\n                &tx_adapter,"),
        "tx hooks must adapt the open cornucopia transaction:\n{content}"
    );

    // Post-commit hooks: present, un-gated, on the ClientSource adapter —
    // no wasm32 cfg (they fire in the worker slice).
    assert!(
        content.contains("after_create_commit(&post_ctx)"),
        "post-commit hook call site must remain:\n{content}"
    );
    assert!(
        content.contains("generated::types::PostCommitContext::new("),
        "post-commit context construction must remain:\n{content}"
    );
    assert!(
        content.contains("crate::hook_adapters::HookConnSourceAdapter("),
        "post-commit hooks must adapt the ClientSource handle:\n{content}"
    );
    assert!(
        content.contains("self.client_source.clone()"),
        "post-commit connection source must be the cornucopia ClientSource:\n{content}"
    );
    assert!(
        !content.contains("Post-commit hooks carry a SeaORM handle"),
        "the #502-era SeaORM-handle comment must be gone:\n{content}"
    );
}

/// Contrast: the SeaORM monolith keeps its transactional hooks — the
/// refusal (#502) and the sea_orm gate must never disable them.
#[tokio::test]
async fn sea_orm_command_keeps_transactional_hooks() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-cmd-seaorm-hooks");

    let generator = generate::ddd::command::CommandGenerator::new(&output_dir);
    let files = generator
        .generate(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &hooks_project_config(codegraph::generate::profile::PersistenceProvider::SeaOrm),
        )
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    let content = &files[0].content;
    for tx_site in [
        "hooks.before_create(",
        "after_create_tx",
        "hooks.before_update(",
        "after_update_tx",
        "TxContext::new(",
    ] {
        assert!(
            content.contains(tx_site),
            "sea_orm command must keep the transactional hook site `{tx_site}`:\n{content}"
        );
    }
}
