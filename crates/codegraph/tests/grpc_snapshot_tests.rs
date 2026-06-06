//! Level 2: Insta snapshot tests for gRPC generator output.
//! Run with: cargo test -p codegraph --test grpc_snapshot_tests

#[path = "test_framework/mod.rs"]
mod test_framework;

mod helpers;

use std::path::Path;

use codegraph::generate::traits::{EntityGenerator, DomainGenerator};
use codegraph::generate::ProjectConfig;

#[test]
fn snapshot_grpc_proto_candidate() {
    let engine = helpers::mock_engine_with_candidate();
    let config = helpers::domain_config();
    let tera = helpers::create_test_tera();
    let project = ProjectConfig::default();

    let gen = codegraph::generate::grpc::proto::GrpcProtoGenerator::new(Path::new("/tmp/grpc-test-proto"));
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(gen.generate(&engine, "CandidateType", "recruiting", &config, &tera, &project))
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

    let gen = codegraph::generate::grpc::service::GrpcServiceGenerator::new(Path::new("/tmp/grpc-test-svc"));
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(gen.generate(&engine, "CandidateType", "recruiting", &config, &tera, &project))
        .expect("GrpcServiceGenerator failed");

    assert!(!files.is_empty(), "Expected at least one generated file");
    for f in &files {
        insta::assert_snapshot!(
            format!("grpc_service_{}", f.path.to_string_lossy().replace('/', "_")),
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

    let gen = codegraph::generate::grpc::router::GrpcRouterGenerator::new(Path::new("/tmp/grpc-test-router"));
    let entity_titles = vec!["CandidateType".to_string()];
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(gen.generate(&engine, "recruiting", &entity_titles, &config, &tera, &project))
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

    let gen = codegraph::generate::grpc::proto::GrpcProtoGenerator::new(Path::new("/tmp/grpc-test-check"));
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(gen.generate(&engine, "CandidateType", "recruiting", &config, &tera, &project))
        .expect("GrpcProtoGenerator failed");

    let proto_file = files.iter().find(|f| f.path.extension().map_or(false, |e| e == "proto"))
        .expect("Expected a .proto file");

    assert!(proto_file.content.contains("message Candidate"));
    assert!(proto_file.content.contains("service CandidateService"));
    assert!(proto_file.content.contains("rpc Create(CreateCandidateRequest) returns (Candidate)"));
    assert!(proto_file.content.contains("rpc Get(GetCandidateRequest) returns (Candidate)"));
    assert!(proto_file.content.contains("rpc List(ListCandidateRequest) returns (ListCandidateResponse)"));
}
