//! Level 3: Proto compilation gate — verifies all generated .proto files compile.
//!
//! Run with: `cargo test -p codegraph --test grpc_compile_tests`
//!
//! ## The skip is LOUD and recorded (issue #793)
//!
//! This test used to `return` silently when `protoc` was missing, which counted
//! as `ok`. It therefore "passed" on every machine without protoc — including
//! CI, whose `test` job installs no protobuf-compiler — while carrying two real
//! emission defects:
//!
//! * the List request referenced `FilterClause` bare, but it lives in the
//!   `shared` package (`proto/shared.proto`), so protoc reported
//!   `"FilterClause" is not defined`;
//! * the `Delete` method returns `google.protobuf.Empty` and no
//!   `google/protobuf/empty.proto` import was emitted, so protoc reported
//!   `"google.protobuf.Empty" is not defined` — a `-I` flag cannot fix a
//!   missing import.
//!
//! A silent skip is exactly the masquerade that hid those. Every skip path
//! below prints `SKIPPING grpc_compile_tests: <reason>` on stderr (visible in
//! the CI log and under `--nocapture`), so an absent toolchain is RECORDED in
//! the run output instead of looking like a green gate.
//!
//! Two environment facts are required and each has its own skip line:
//! 1. `protoc` on PATH;
//! 2. a protobuf include dir containing `google/protobuf/empty.proto` —
//!    discovered from `PROTOC_INCLUDE`, then `pkg-config protobuf
//!    --variable=includedir`, then `/usr/include`, `/usr/local/include`.

use std::path::PathBuf;
use std::process::Command;

use codegraph::generate::ProjectConfig;
use codegraph::generate::grpc::scaffold::{GrpcScaffoldContext, GrpcScaffoldDomain};
use codegraph::generate::render_template_with_project;
use codegraph::generate::traits::EntityGenerator;

mod helpers;

/// Print the loud, recorded skip line and bail out of the test.
fn skip(reason: &str) {
    eprintln!("{}", skip_message(reason));
}

/// The exact recorded form of a skip. Pinned by a test so the line cannot
/// quietly lose its `SKIPPING` prefix (the whole point of #793 is that a skip
/// must be visible in the run log).
fn skip_message(reason: &str) -> String {
    format!("SKIPPING grpc_compile_tests: {reason}")
}

/// Locate the protobuf include dir that ships the well-known types.
///
/// Order: `PROTOC_INCLUDE`, `pkg-config protobuf --variable=includedir`,
/// `/usr/include`, `/usr/local/include`; the first candidate that actually
/// contains `google/protobuf/empty.proto` wins.
fn well_known_types_include_dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(dir) = std::env::var("PROTOC_INCLUDE")
        && !dir.is_empty()
    {
        candidates.push(PathBuf::from(dir));
    }
    if let Some(dir) = pkg_config_includedir() {
        candidates.push(dir);
    }
    candidates.push(PathBuf::from("/usr/include"));
    candidates.push(PathBuf::from("/usr/local/include"));

    first_include_with_well_known_types(candidates)
}

/// The candidate scan, split out so the "no WKT include ⇒ loud skip" predicate
/// is pinned without needing a machine that lacks the headers.
fn first_include_with_well_known_types(
    candidates: impl IntoIterator<Item = PathBuf>,
) -> Option<PathBuf> {
    candidates
        .into_iter()
        .find(|dir| dir.join("google/protobuf/empty.proto").is_file())
}

fn pkg_config_includedir() -> Option<PathBuf> {
    let output = Command::new("pkg-config")
        .args(["protobuf", "--variable=includedir"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!dir.is_empty()).then(|| PathBuf::from(dir))
}

/// Render the REAL `shared.proto` (the `proto_shared.tera` template the
/// scaffold generator emits) instead of hand-writing a stub.
///
/// The previous stub was the same source of truth drift the emission bug was:
/// it declared the types the test wanted to see, not the ones consumers get.
fn render_shared_proto(tera: &tera::Tera, project: &ProjectConfig) -> String {
    let ctx = GrpcScaffoldContext {
        domains: vec![GrpcScaffoldDomain {
            name: "recruiting".to_string(),
        }],
    };
    render_template_with_project(tera, "grpc/proto_shared.tera", &ctx, project)
        .expect("failed to render grpc/proto_shared.tera")
}

/// Check that the generated Candidate proto file compiles with protoc.
#[test]
fn test_grpc_proto_files_compile() {
    let has_protoc = Command::new("protoc")
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success());
    if !has_protoc {
        skip("protoc not found on PATH (install protobuf-compiler)");
        return;
    }

    let Some(wkt_include) = well_known_types_include_dir() else {
        skip(
            "no protobuf include dir containing google/protobuf/empty.proto \
             (set PROTOC_INCLUDE, or install protobuf development headers)",
        );
        return;
    };

    let engine = helpers::mock_engine_with_candidate();
    let config = helpers::domain_config();
    let tera = helpers::create_test_tera();
    let project = ProjectConfig::default();

    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let out_dir = tmp.path().join("output");

    let generator = codegraph::generate::grpc::proto::GrpcProtoGenerator::new(&out_dir);
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

    // Write the REAL shared.proto (the template the scaffold generator emits).
    let proto_root = out_dir.join("proto");
    std::fs::create_dir_all(&proto_root).unwrap();
    let shared = render_shared_proto(&tera, &project);
    assert!(
        shared.contains("package shared;"),
        "rendered shared.proto must declare `package shared;`:\n{shared}"
    );
    assert!(
        shared.contains("message FilterClause"),
        "rendered shared.proto must declare FilterClause:\n{shared}"
    );
    std::fs::write(proto_root.join("shared.proto"), shared).unwrap();

    // Collect all .proto files
    let proto_files: Vec<_> = files
        .iter()
        .filter(|f| f.path.extension().is_some_and(|e| e == "proto"))
        .map(|f| proto_root.join(&f.path))
        .collect();

    // The gate exists to compile what the generator emits; an empty emission is
    // a failure, never a skip (the #793 masquerade class).
    assert!(
        !proto_files.is_empty(),
        "GrpcProtoGenerator emitted no .proto files for CandidateType"
    );

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

    // Run protoc. `-I proto_root` resolves `shared.proto`; the well-known-types
    // include dir resolves `google/protobuf/{timestamp,struct,empty}.proto`.
    let output = Command::new("protoc")
        .arg("-I")
        .arg(&proto_root)
        .arg("-I")
        .arg(&wkt_include)
        .arg("-o")
        .arg("/dev/null")
        .args(&proto_files)
        .output()
        .expect("failed to run protoc");

    assert!(
        output.status.success(),
        "protoc compilation failed (wkt include: {}):\n{}",
        wkt_include.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The skip line is the record of a non-run — it must name the test and the
/// reason (issue #793).
#[test]
fn the_skip_line_is_loud_and_names_the_reason() {
    let line = skip_message("protoc not found on PATH (install protobuf-compiler)");
    assert!(line.starts_with("SKIPPING grpc_compile_tests: "), "{line}");
    assert!(line.contains("protoc not found"), "{line}");
}

/// Missing `google/protobuf/empty.proto` under every candidate means the WKT
/// include was not found — the caller turns that into the loud skip, never a
/// silent pass.
#[test]
fn wkt_include_scan_skips_dirs_without_empty_proto() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let without = tmp.path().join("without-wkt");
    std::fs::create_dir_all(&without).unwrap();
    let with_root = tmp.path().join("with-wkt");
    let with_proto = with_root.join("google/protobuf");
    std::fs::create_dir_all(&with_proto).unwrap();
    std::fs::write(with_proto.join("empty.proto"), "").unwrap();

    assert_eq!(
        first_include_with_well_known_types(vec![without.clone()]),
        None,
        "a candidate without google/protobuf/empty.proto is not an include"
    );
    assert_eq!(
        first_include_with_well_known_types(vec![without, with_root.clone()]),
        Some(with_root)
    );
}
