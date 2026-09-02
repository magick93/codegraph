use std::path::PathBuf;

use codegraph::generate::traits::GeneratedFile;

#[path = "test_framework/mod.rs"]
mod test_framework;

use test_framework::validators::file_presence::FilePresenceValidator;
use test_framework::validators::snapshot::SnapshotCollector;
use test_framework::validators::string_pattern::StringPatternValidator;
use test_framework::validators::OutputValidator;

fn make_file(rel_path: &str, content: &str) -> GeneratedFile {
    GeneratedFile {
        path: PathBuf::from(rel_path),
        content: content.to_string(),
    }
}

fn write_temp_file(dir: &std::path::Path, rel_path: &str, content: &str) {
    let full = dir.join(rel_path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(full, content).unwrap();
}

// ── FilePresenceValidator ────────────────────────────────────────────

#[test]
fn test_file_presence_validator_pass() {
    let dir = tempfile::tempdir().unwrap();
    write_temp_file(dir.path(), "foo.txt", "hello");
    write_temp_file(dir.path(), "bar.txt", "world");

    let files = vec![make_file("foo.txt", "hello")];

    let v = FilePresenceValidator {
        label: "test".into(),
        required_paths: vec!["foo.txt".into(), "bar.txt".into()],
    };

    let result = v.validate(&files, dir.path());
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
}

#[test]
fn test_file_presence_validator_fail() {
    let dir = tempfile::tempdir().unwrap();
    write_temp_file(dir.path(), "foo.txt", "hello");

    let files = vec![make_file("foo.txt", "hello")];

    let v = FilePresenceValidator {
        label: "test".into(),
        required_paths: vec!["foo.txt".into(), "missing.txt".into()],
    };

    let result = v.validate(&files, dir.path());
    assert!(result.is_err(), "expected Err, got {:?}", result);
    let errors = result.unwrap_err();
    assert!(
        errors.iter().any(|e| e.contains("missing.txt")),
        "expected error mentioning missing.txt, got {:?}",
        errors
    );
}

// ── StringPatternValidator ───────────────────────────────────────────

#[test]
fn test_string_pattern_validator_contains() {
    let dir = tempfile::tempdir().unwrap();
    let files = vec![make_file("a.txt", "hello world")];

    let v = StringPatternValidator {
        label: "test".into(),
        required_patterns: vec!["hello".into()],
        forbidden_patterns: vec![],
    };

    let result = v.validate(&files, dir.path());
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
}

#[test]
fn test_string_pattern_validator_missing_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let files = vec![make_file("a.txt", "hello world")];

    let v = StringPatternValidator {
        label: "test".into(),
        required_patterns: vec!["missing".into()],
        forbidden_patterns: vec![],
    };

    let result = v.validate(&files, dir.path());
    assert!(result.is_err(), "expected Err, got {:?}", result);
    let errors = result.unwrap_err();
    assert!(
        errors.iter().any(|e| e.contains("missing")),
        "expected error mentioning 'missing', got {:?}",
        errors
    );
}

#[test]
fn test_string_pattern_validator_forbidden_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let files = vec![make_file("a.txt", "hello badword world")];

    let v = StringPatternValidator {
        label: "test".into(),
        required_patterns: vec![],
        forbidden_patterns: vec!["badword".into()],
    };

    let result = v.validate(&files, dir.path());
    assert!(result.is_err(), "expected Err, got {:?}", result);
    let errors = result.unwrap_err();
    assert!(
        errors.iter().any(|e| e.contains("badword")),
        "expected error mentioning 'badword', got {:?}",
        errors
    );
}

#[test]
fn test_string_pattern_validator_no_files() {
    let dir = tempfile::tempdir().unwrap();

    let v = StringPatternValidator {
        label: "test".into(),
        required_patterns: vec!["anything".into()],
        forbidden_patterns: vec![],
    };

    let result = v.validate(&[], dir.path());
    assert!(result.is_err(), "expected Err, got {:?}", result);
}

// ── SnapshotCollector ────────────────────────────────────────────────

#[test]
fn test_snapshot_collector_collects_files() {
    let dir = tempfile::tempdir().unwrap();
    let files = vec![
        make_file("foo.rs", "fn main() {}"),
        make_file("bar.rs", "mod foo;"),
    ];

    let c = SnapshotCollector::new("test");
    let result = c.validate(&files, dir.path());
    assert!(result.is_ok());

    let map = c.files.lock().unwrap();
    assert_eq!(map.len(), 2);
    assert!(map.contains_key("foo.rs"));
    assert!(map.contains_key("bar.rs"));
    assert_eq!(map.get("foo.rs").unwrap().content, "fn main() {}");
}

#[test]
fn test_snapshot_collector_empty_dir() {
    let dir = tempfile::tempdir().unwrap();

    let c = SnapshotCollector::new("test");
    let result = c.validate(&[], dir.path());
    assert!(result.is_ok());

    let map = c.files.lock().unwrap();
    assert!(map.is_empty());
}

#[test]
fn test_snapshot_collector_recursive() {
    let dir = tempfile::tempdir().unwrap();
    let files = vec![
        make_file("a/b/c.txt", "deep"),
        make_file("a/d.txt", "mid"),
        make_file("root.txt", "top"),
    ];

    let c = SnapshotCollector::new("test");
    let result = c.validate(&files, dir.path());
    assert!(result.is_ok());

    let map = c.files.lock().unwrap();
    assert_eq!(map.len(), 3);
    assert!(map.contains_key("a/b/c.txt"));
    assert!(map.contains_key("a/d.txt"));
    assert!(map.contains_key("root.txt"));
}
