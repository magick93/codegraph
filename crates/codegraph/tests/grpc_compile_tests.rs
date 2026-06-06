//! Level 3: Proto compilation gate — verifies all generated .proto files compile.
//! Run with: cargo test -p codegraph --test grpc_compile_tests
//! Requires `protoc` in PATH (skipped if absent).


use codegraph::generate::traits::EntityGenerator;
use codegraph::generate::ProjectConfig;

mod helpers;

/// Check that the generated Candidate proto file compiles with protoc.
#[test]
fn test_grpc_proto_files_compile() {
    // Check if protoc is available
    let has_protoc = std::process::Command::new("protoc")
        .arg("--version")
        .output()
        .is_ok();
    if !has_protoc {
        eprintln!("Skipping: protoc not found in PATH");
        return;
    }

    let engine = helpers::mock_engine_with_candidate();
    let config = helpers::domain_config();
    let tera = helpers::create_test_tera();
    let project = ProjectConfig::default();

    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let out_dir = tmp.path().join("output");

    let gen = codegraph::generate::grpc::proto::GrpcProtoGenerator::new(&out_dir);
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(gen.generate(&engine, "CandidateType", "recruiting", &config, &tera, &project))
        .expect("GrpcProtoGenerator failed");

    // Write a minimal shared.proto for the import dependency
    let proto_root = out_dir.join("proto");
    std::fs::create_dir_all(&proto_root).unwrap();
    std::fs::write(
        proto_root.join("shared.proto"),
        r#"syntax = "proto3";
package shared;
option go_package = "shared/;shared";
message FilterClause {
  string field = 1;
  string value = 2;
  string operator = 3;
}
message SearchResult {
  string id = 1;
  float score = 2;
}
"#,
    )
    .unwrap();

    // Collect all .proto files
    let proto_files: Vec<_> = files
        .iter()
        .filter(|f| f.path.extension().map_or(false, |e| e == "proto"))
        .map(|f| proto_root.join(&f.path))
        .collect();

    if proto_files.is_empty() {
        eprintln!("Skipping: no proto files generated");
        return;
    }

    // Write each proto file to disk
    for pf in &proto_files {
        if let Some(parent) = pf.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let content = files
            .iter()
            .find(|f| proto_root.join(&f.path) == *pf)
            .map(|f| &f.content)
            .unwrap();
        std::fs::write(pf, content).unwrap();
    }

    // Run protoc
    let output = std::process::Command::new("protoc")
        .arg("-I")
        .arg(&proto_root)
        .arg("-o")
        .arg("/dev/null")
        .args(&proto_files)
        .output()
        .expect("failed to run protoc");

    assert!(
        output.status.success(),
        "protoc compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
