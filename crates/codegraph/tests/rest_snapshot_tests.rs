//! Level 2: Insta snapshot tests for REST API generator output.
//! Run with: cargo test -p codegraph --test rest_snapshot_tests

#[path = "test_framework/mod.rs"]
mod test_framework;

mod helpers;

use std::path::Path;

use codegraph::generate::traits::{DomainGenerator, EntityGenerator, GlobalGenerator};
use codegraph::generate::{GenerationEntry, ProjectConfig};

#[test]
fn snapshot_rest_handler_candidate() {
    codegraph::generate::type_registry::register_framework_types();
    let engine = helpers::mock_engine_with_candidate();
    let config = helpers::domain_config();
    let tera = helpers::create_test_tera();
    let project = ProjectConfig::default();

    let gen = codegraph::generate::api::handler::HandlerGenerator::new(Path::new(
        "/tmp/rest-test-handler",
    ));
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(gen.generate(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &project,
        ))
        .expect("HandlerGenerator failed");

    assert!(!files.is_empty(), "Expected at least one generated file");
    for f in &files {
        insta::assert_snapshot!(
            format!("rest_handler_{}", f.path.to_string_lossy().replace('/', "_")),
            &f.content
        );
    }
}

#[test]
fn snapshot_rest_router_recruiting() {
    let engine = helpers::mock_engine_with_candidate();
    let config = helpers::domain_config();
    let tera = helpers::create_test_tera();
    let project = ProjectConfig::default();

    let gen = codegraph::generate::api::router::RouterGenerator::new(Path::new(
        "/tmp/rest-test-router",
    ));
    let entity_titles = vec!["CandidateType".to_string()];
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(gen.generate(
            &engine,
            "recruiting",
            &entity_titles,
            &config,
            &tera,
            &project,
        ))
        .expect("RouterGenerator failed");

    assert!(!files.is_empty(), "Expected at least one generated file");
    for f in &files {
        insta::assert_snapshot!(
            format!(
                "rest_router_{}",
                f.path.to_string_lossy().replace('/', "_")
            ),
            &f.content
        );
    }
}

#[test]
fn snapshot_rest_openapi_domain_recruiting() {
    let engine = helpers::mock_engine_with_candidate();
    let config = helpers::domain_config();
    let tera = helpers::create_test_tera();
    let project = ProjectConfig::default();

    let generation_order = vec![GenerationEntry {
        schema_title: "CandidateType".to_string(),
        domain: "recruiting".to_string(),
        pg_schema: "recruiting".to_string(),
        is_cyclic: false,
    }];

    let gen = codegraph::generate::api::openapi::OpenApiGenerator::new(Path::new(
        "/tmp/rest-test-openapi-domain",
    ));
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(gen.generate(
            &engine,
            &config,
            &generation_order,
            &tera,
            &project,
        ))
        .expect("OpenApiGenerator failed");

    let recruiting_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("recruiting.rs"))
        .expect("Expected recruiting.rs in OpenAPI output");

    insta::assert_snapshot!(
        format!(
            "rest_openapi_domain_{}",
            recruiting_file
                .path
                .to_string_lossy()
                .replace('/', "_")
        ),
        &recruiting_file.content
    );
}

#[test]
fn snapshot_rest_openapi_all() {
    let engine = helpers::mock_engine_with_candidate();
    let config = helpers::domain_config();
    let tera = helpers::create_test_tera();
    let project = ProjectConfig::default();

    let generation_order = vec![GenerationEntry {
        schema_title: "CandidateType".to_string(),
        domain: "recruiting".to_string(),
        pg_schema: "recruiting".to_string(),
        is_cyclic: false,
    }];

    let gen = codegraph::generate::api::openapi::OpenApiGenerator::new(Path::new(
        "/tmp/rest-test-openapi-all",
    ));
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(gen.generate(
            &engine,
            &config,
            &generation_order,
            &tera,
            &project,
        ))
        .expect("OpenApiGenerator failed");

    let all_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("all.rs"))
        .expect("Expected all.rs in OpenAPI output");

    insta::assert_snapshot!(
        format!(
            "rest_openapi_all_{}",
            all_file.path.to_string_lossy().replace('/', "_")
        ),
        &all_file.content
    );
}

#[test]
fn snapshot_rest_links_recruiting() {
    let engine = helpers::mock_engine_with_candidate();
    let config = helpers::domain_config();
    let tera = helpers::create_test_tera();
    let project = ProjectConfig::default();

    let gen = codegraph::generate::api::links::LinksGenerator::new(Path::new(
        "/tmp/rest-test-links",
    ));
    let entity_titles = vec!["CandidateType".to_string()];
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(gen.generate(
            &engine,
            "recruiting",
            &entity_titles,
            &config,
            &tera,
            &project,
        ))
        .expect("LinksGenerator failed");

    assert!(!files.is_empty(), "Expected at least one generated file");
    for f in &files {
        insta::assert_snapshot!(
            format!("rest_links_{}", f.path.to_string_lossy().replace('/', "_")),
            &f.content
        );
    }
}
