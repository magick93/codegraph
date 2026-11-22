//! Level 2: Insta snapshot tests for gRPC generator output.
//! Run with: cargo test -p codegraph --test grpc_snapshot_tests

#[path = "test_framework/mod.rs"]
mod test_framework;

mod helpers;

use std::path::Path;

use codegraph::generate::ProjectConfig;
use codegraph::generate::traits::{DomainGenerator, EntityGenerator};

#[test]
fn snapshot_grpc_proto_candidate() {
    let engine = helpers::mock_engine_with_candidate();
    let config = helpers::domain_config();
    let tera = helpers::create_test_tera();
    let project = ProjectConfig::default();

    let generator = codegraph::generate::grpc::proto::GrpcProtoGenerator::new(Path::new(
        "/tmp/grpc-test-proto",
    ));
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(generator.generate(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &project,
        ))
        .expect("GrpcProtoGenerator failed");

    assert!(!files.is_empty(), "Expected at least one generated file");
    for f in &files {
        insta::assert_snapshot!(
            format!("grpc_proto_{}", f.path.to_string_lossy().replace('/', "_")),
            &f.content
        );
    }
}

#[test]
fn snapshot_grpc_service_candidate() {
    let engine = helpers::mock_engine_with_candidate();
    let config = helpers::domain_config();
    let tera = helpers::create_test_tera();
    let project = ProjectConfig::default();

    let generator = codegraph::generate::grpc::service::GrpcServiceGenerator::new(Path::new(
        "/tmp/grpc-test-svc",
    ));
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(generator.generate(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &project,
        ))
        .expect("GrpcServiceGenerator failed");

    assert!(!files.is_empty(), "Expected at least one generated file");
    for f in &files {
        insta::assert_snapshot!(
            format!(
                "grpc_service_{}",
                f.path.to_string_lossy().replace('/', "_")
            ),
            &f.content
        );
    }
}

#[test]
fn snapshot_grpc_router_recruiting() {
    let engine = helpers::mock_engine_with_candidate();
    let config = helpers::domain_config();
    let tera = helpers::create_test_tera();
    let project = ProjectConfig::default();

    let generator = codegraph::generate::grpc::router::GrpcRouterGenerator::new(Path::new(
        "/tmp/grpc-test-router",
    ));
    let entity_titles = vec!["CandidateType".to_string()];
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(generator.generate(
            &engine,
            "recruiting",
            &entity_titles,
            &config,
            &tera,
            &project,
        ))
        .expect("GrpcRouterGenerator failed");

    assert!(!files.is_empty(), "Expected at least one generated file");
    for f in &files {
        insta::assert_snapshot!(
            format!("grpc_router_{}", f.path.to_string_lossy().replace('/', "_")),
            &f.content
        );
    }
}

#[test]
fn snapshot_grpc_proto_candidate_contains_entity_message() {
    let engine = helpers::mock_engine_with_candidate();
    let config = helpers::domain_config();
    let tera = helpers::create_test_tera();
    let project = ProjectConfig::default();

    let generator = codegraph::generate::grpc::proto::GrpcProtoGenerator::new(Path::new(
        "/tmp/grpc-test-check",
    ));
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(generator.generate(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &project,
        ))
        .expect("GrpcProtoGenerator failed");

    let proto_file = files
        .iter()
        .find(|f| f.path.extension().is_some_and(|e| e == "proto"))
        .expect("Expected a .proto file");

    assert!(proto_file.content.contains("message Candidate"));
    assert!(proto_file.content.contains("service CandidateService"));
    assert!(
        proto_file
            .content
            .contains("rpc Create(CreateCandidateRequest) returns (Candidate)")
    );
    assert!(
        proto_file
            .content
            .contains("rpc Get(GetCandidateRequest) returns (Candidate)")
    );
    assert!(
        proto_file
            .content
            .contains("rpc List(ListCandidateRequest) returns (ListCandidateResponse)")
    );

    // Issue #793 — cross-package + well-known-type references must RESOLVE:
    // `FilterClause` lives in `shared.proto`'s `shared` package, and the
    // `Delete` method's `google.protobuf.Empty` needs its own import (a
    // missing import is not an include-path problem).
    assert!(
        proto_file
            .content
            .contains("import \"google/protobuf/empty.proto\";"),
        "the Delete method returns google.protobuf.Empty, so the import must be emitted:\n{}",
        proto_file.content
    );
    assert!(
        proto_file
            .content
            .contains("repeated shared.FilterClause filters = 5;"),
        "FilterClause is declared in the `shared` package and must be qualified:\n{}",
        proto_file.content
    );
    assert!(
        !proto_file.content.contains("repeated FilterClause"),
        "a bare cross-package reference cannot resolve:\n{}",
        proto_file.content
    );
}
