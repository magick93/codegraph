use crate::harness::{
    include_domain_config, parent_child_mock, setup_include_mock, setup_include_mock_with_refs,
    setup_mock, test_domain_config, test_project_config, test_tera,
};
use codegraph::generate;
use codegraph::generate::ProjectConfig;
#[allow(unused_imports)]
use codegraph::generate::traits::{DomainGenerator, EntityGenerator, GlobalGenerator};

// === Repository Template Tests ===

#[tokio::test]
async fn candidate_repository() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-repo");

    let generator = generate::ddd::repository::RepositoryTraitGenerator::new(&output_dir);
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

    assert!(files.len() >= 2, "Should have trait + impl files");

    let trait_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("repository.rs"))
        .expect("Should have repository trait file");
    assert!(trait_file.content.contains("CandidateRepository"));

    let impl_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("repository_impl.rs"))
        .expect("Should have repository impl file");
    assert!(impl_file.content.contains("CandidateRepositoryImpl"));
    assert!(impl_file.content.contains("async fn create"));
    assert!(impl_file.content.contains("async fn find_by_id"));
    assert!(impl_file.content.contains("async fn update"));
    // CandidateType operations = ["create", "read", "update", "list"] — no delete
    assert!(!impl_file.content.contains("async fn delete"));
    assert!(impl_file.content.contains("async fn list"));
}

/// Child entity repository trait must include parent_id in create signature.
#[tokio::test]
async fn child_repository_trait_create_has_parent_id() {
    let (mock, candidates) = parent_child_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-child-repo-trait");

    let generator = generate::ddd::repository::RepositoryTraitGenerator::new(&output_dir)
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

    assert!(
        !files.is_empty(),
        "Repository trait generator should produce a file"
    );
    let content = &files[0].content;

    // create() trait method must include parent_id
    assert!(
        content.contains("parent_id: Uuid"),
        "Child repository create must include parent_id parameter. Got:\n{content}"
    );

    // find_by_id_scoped must be present
    assert!(
        content.contains("find_by_id_scoped"),
        "Child repository must include find_by_id_scoped method. Got:\n{content}"
    );
}

// ── E5: Repository emitter with ?include= ───────────────────────────────

#[tokio::test]
async fn repository_emitter_produces_fetch_methods_with_include() {
    use codegraph::generate::api::include_path::{IncludeSegment, ResolvedIncludePath};
    use codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;

    let mock = setup_include_mock();
    let config = include_domain_config();

    let include_paths = vec![ResolvedIncludePath {
        alias: "person".to_string(),
        segments: vec![IncludeSegment {
            entity_name: "PersonType".to_string(),
            schema_title: "PersonType".to_string(),
            module_name: "person".to_string(),
            domain: "hr".to_string(),
            table: "\"hr\".\"person\"".to_string(),
            fk_column: "person_id".to_string(),
            reverse_fk_column: "worker_id".to_string(),
            fk_is_required: false,
            reverse_fk_is_required: false,
            is_array: false,
            child_table_override: None,
        }],
        response_rust_type: "PersonResponse".to_string(),
        fetch_method: "fetch_person_for_worker".to_string(),
        batch_fetch_method: "fetch_person_batch_for_worker".to_string(),
    }];

    let emitter = RepositoryImplEmitter;
    let code = emitter
        .emit(
            &mock,
            "WorkerType",
            "hr",
            &config,
            None,
            &include_paths,
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    // 1. Single fetch method exists
    assert!(
        code.contains("pub(crate) async fn fetch_person_for_worker"),
        "Repository should emit fetch_person_for_worker. Got:\n{code}"
    );

    // 2. Single fetch uses .filter(crate::entity::hr_person::Column::Id.eq(fk_value))
    assert!(
        code.contains(r#"crate::entity::hr_person::Column::Id.eq(fk_value)"#),
        "Single fetch should filter by Id.eq(fk_value). Got:\n{code}"
    );

    // 3. Batch fetch method exists
    assert!(
        code.contains("pub(crate) async fn fetch_person_batch_for_worker"),
        "Repository should emit fetch_person_batch_for_worker. Got:\n{code}"
    );

    // 4. Batch fetch accepts source_ids: &[Uuid]
    assert!(
        code.contains("source_ids: &[Uuid]"),
        "Batch fetch should accept source_ids: &[Uuid]. Got:\n{code}"
    );

    // 5. Batch fetch returns HashMap<Uuid, Option<PersonResponse>>
    assert!(
        code.contains("HashMap<Uuid, Option<PersonResponse>>"),
        "Batch fetch should return HashMap<Uuid, Option<PersonResponse>>. Got:\n{code}"
    );
}

#[tokio::test]
async fn repository_emitter_resolves_include_paths_when_none_provided() {
    use codegraph::generate::api::include_path::ResolvedIncludePath;
    use codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;

    // Cross-generator contract: the handler's hydration block calls
    // `repo.fetch_{alias}_for_{module}(...)` for every include path it
    // resolves. When `emit` is invoked without pre-resolved paths (e.g.
    // from test harnesses or tooling outside the pipeline), it must derive
    // the same include surface itself — an empty caller-supplied list must
    // not silently strip the hydration methods the handler emits.
    let mock = setup_include_mock_with_refs();
    let config = include_domain_config();

    let paths: &[ResolvedIncludePath] = &[];
    let emitter = RepositoryImplEmitter;
    let code = emitter
        .emit(
            &mock,
            "WorkerType",
            "hr",
            &config,
            None,
            paths,
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    assert!(
        code.contains("pub(crate) async fn fetch_person_for_worker"),
        "Should self-resolve and emit fetch_person_for_worker when the caller \
         supplies no paths (handler hydration expects it). Got:\n{code}"
    );
    assert!(
        code.contains("pub(crate) async fn fetch_person_batch_for_worker"),
        "Should self-resolve and emit the batch variant too. Got:\n{code}"
    );
}

#[tokio::test]
async fn repository_emitter_self_resolution_respects_entity_gate() {
    use codegraph::generate::api::include_path::ResolvedIncludePath;
    use codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;

    // The internal resolution must apply the same gate as the handler/DTO/
    // repository generators: a non-root entity without explicit
    // `allow_include` gets no include paths, so no fetch methods are emitted.
    let mock = setup_include_mock_with_refs();
    let toml_str = r#"
[defaults]
operations = ["create", "read", "update", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType", "PersonType"]

[domains.hr.entity_config.WorkerType]
role = "child"
operations = ["create", "read", "update", "list"]
"#;
    let config = codegraph_config::config::parse_domain_config_str(toml_str).unwrap();

    let paths: &[ResolvedIncludePath] = &[];
    let emitter = RepositoryImplEmitter;
    let code = emitter
        .emit(
            &mock,
            "WorkerType",
            "hr",
            &config,
            None,
            paths,
            &codegraph::generate::ProjectConfig::default(),
        )
        .await
        .unwrap();

    assert!(
        !code.contains("fetch_person_for_worker"),
        "Child entity without allow_include must NOT gain fetch methods from \
         self-resolution. Got:\n{code}"
    );
}
