//! Integration tests for `codegraph migrate` (JSON Schema → rexlang .mox).

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use codegraph::migrate::{migrate, MigrateArgs, STDLIB_FILE_NAME};

const EMPLOYEE_JSON: &str = r#"{
  "$id": "https://example.com/hr/json/EmployeeType.json",
  "title": "EmployeeType",
  "description": "An employee record",
  "type": "object",
  "properties": {
    "id": { "type": "string", "format": "uuid" },
    "name": { "type": "string", "minLength": 2, "maxLength": 100, "description": "Full name" },
    "email": { "type": "string", "format": "email" },
    "hireDate": { "type": "string", "format": "date" },
    "createdAt": { "type": "string", "format": "date-time" },
    "website": { "type": "string", "format": "uri" },
    "nickname": { "type": "string", "default": "unknown" },
    "age": { "type": "integer", "minimum": 16, "maximum": 90 },
    "salary": { "type": "number" },
    "active": { "type": "boolean", "default": true },
    "badgeCode": { "type": "string", "pattern": "^[A-Z]{6}$" },
    "status": { "type": "string", "enum": ["active", "on-leave", "type"] },
    "skills": { "type": "array", "items": { "type": "string" } },
    "dependents": { "type": "array", "items": { "type": "string" } }
  },
  "required": ["id", "name", "status", "skills"]
}"#;

const NOTE_JSON: &str = r#"{
  "title": "NoteType",
  "properties": {
    "body": { "type": "object", "properties": { "text": { "type": "string" } } },
    "author": { "type": "string" }
  }
}"#;

const CUSTOMER_JSON: &str = r#"{
  "title": "CustomerType",
  "type": "object",
  "properties": {
    "id": { "type": "string", "format": "uuid" },
    "name": { "type": "string" },
    "manager": { "$ref": "../hr/json/EmployeeType.json" },
    "representative": { "$ref": "AccountRepType.json" },
    "orders": { "type": "array", "items": { "$ref": "OrderType.json" } },
    "creditLimit": { "type": "number" }
  },
  "required": ["id", "name", "representative"]
}"#;

const ACCOUNT_REP_JSON: &str = r#"{
  "title": "AccountRepType",
  "properties": {
    "id": { "type": "string", "format": "uuid" },
    "name": { "type": "string" },
    "region": { "type": "string" }
  },
  "required": ["id"]
}"#;

const ORDER_JSON: &str = r#"{
  "title": "OrderType",
  "properties": {
    "id": { "type": "string", "format": "uuid" },
    "total": { "type": "number" },
    "placedAt": { "type": "string", "format": "date-time" }
  },
  "required": ["id", "total"]
}"#;

const PREMIUM_CUSTOMER_JSON: &str = r#"{
  "title": "PremiumCustomerType",
  "allOf": [
    { "$ref": "CustomerType.json" },
    {
      "properties": {
        "benefits": { "type": "string" },
        "discountRate": { "type": "number", "default": 0.1 }
      }
    }
  ]
}"#;

/// Two domains: `hr` (primitives, formats, constraints, inline enum, an
/// inline-object property) and `sales` ($refs, an allOf extension).
fn write_fixture(dir: &Path) {
    let hr = dir.join("hr/json");
    let sales = dir.join("sales/json");
    fs::create_dir_all(&hr).expect("create hr dir");
    fs::create_dir_all(&sales).expect("create sales dir");
    fs::write(hr.join("EmployeeType.json"), EMPLOYEE_JSON).expect("write employee");
    fs::write(hr.join("NoteType.json"), NOTE_JSON).expect("write note");
    fs::write(sales.join("CustomerType.json"), CUSTOMER_JSON).expect("write customer");
    fs::write(sales.join("AccountRepType.json"), ACCOUNT_REP_JSON).expect("write rep");
    fs::write(sales.join("OrderType.json"), ORDER_JSON).expect("write order");
    fs::write(
        sales.join("PremiumCustomerType.json"),
        PREMIUM_CUSTOMER_JSON,
    )
    .expect("write premium");
}

fn run_args<'a>(
    schemas: &'a Path,
    output: &'a Path,
    dry_run: bool,
    force: bool,
) -> MigrateArgs<'a> {
    MigrateArgs {
        schemas,
        output,
        dry_run,
        force,
    }
}

#[test]
fn migrate_emits_compiling_mox_per_domain_and_stdlib() {
    let fixture = TempDir::new().expect("tempdir");
    write_fixture(fixture.path());
    let output = TempDir::new().expect("tempdir");

    let report = migrate(run_args(fixture.path(), output.path(), false, false))
        .expect("migration must succeed");

    // One file per domain + the stdlib.
    let file_names: Vec<String> = report
        .files
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        file_names,
        vec![
            STDLIB_FILE_NAME.to_string(),
            "hr.mox".to_string(),
            "sales.mox".to_string()
        ]
    );
    for path in &report.files {
        assert!(path.exists(), "{} must exist", path.display());
    }

    // Six top-level schemas converted (Employee, Note, Customer, AccountRep,
    // Order, PremiumCustomer).
    assert_eq!(report.converted, 6);
    assert!(
        report.compile_errors.is_empty(),
        "compile errors: {:?}",
        report.compile_errors
    );

    // The inline-object property is needs-review, not fatal.
    assert!(
        report
            .needs_review
            .iter()
            .any(|note| note.contains("NoteType.body")),
        "needs_review should mention NoteType.body: {:?}",
        report.needs_review
    );
    // The float default has no rex literal form.
    assert!(
        report
            .needs_review
            .iter()
            .any(|note| note.contains("discountRate")),
        "needs_review should mention discountRate: {:?}",
        report.needs_review
    );

    let hr = fs::read_to_string(output.path().join("hr.mox")).expect("read hr.mox");
    assert!(hr.starts_with("package hr\n"), "header: {hr}");
    assert!(hr.contains("/// An employee record"), "doc comment: {hr}");
    assert!(hr.contains("class EmployeeType {"), "class line: {hr}");
    assert!(hr.contains("codegraph_stdlib.Uuid id"), "uuid: {hr}");
    assert!(
        hr.contains("codegraph_stdlib.Date[0..1] hireDate"),
        "date: {hr}"
    );
    assert!(
        hr.contains("codegraph_stdlib.DateTime[0..1] createdAt"),
        "datetime: {hr}"
    );
    assert!(
        hr.contains("codegraph_stdlib.Email[0..1] email"),
        "email: {hr}"
    );
    assert!(
        hr.contains("codegraph_stdlib.Uri[0..1] website"),
        "uri: {hr}"
    );
    assert!(
        hr.contains("String[0..1] nickname = \"unknown\""),
        "optional + default: {hr}"
    );
    assert!(
        hr.contains("Int[0..1] age { minimum 16 maximum 90 }"),
        "numeric constraints: {hr}"
    );
    assert!(hr.contains("Double[0..1] salary"), "double: {hr}");
    assert!(
        hr.contains("Boolean[0..1] active = true"),
        "bool default: {hr}"
    );
    assert!(
        hr.contains("String[0..1] badgeCode { pattern \"^[A-Z]{6}$\" }"),
        "pattern constraint: {hr}"
    );
    assert!(hr.contains("String[] skills"), "required array: {hr}");
    assert!(
        hr.contains("String[0..*] dependents"),
        "optional array: {hr}"
    );
    assert!(
        hr.contains("/// Full name\n    String name { minLength 2 maxLength 100 }"),
        "doc + string constraints: {hr}"
    );
    // Inline string enum: identifier-safe values verbatim, sanitized with a
    // label otherwise, keywords escaped.
    assert!(hr.contains("enum EmployeeTypeStatus {"), "enum: {hr}");
    assert!(hr.contains("    active = 0"), "literal: {hr}");
    assert!(
        hr.contains("    on_leave as \"on-leave\" = 1"),
        "labeled literal: {hr}"
    );
    assert!(
        hr.contains("    ^type as \"type\" = 2"),
        "keyword literal: {hr}"
    );
    assert!(
        hr.contains("EmployeeTypeStatus status\n"),
        "required enum attr: {hr}"
    );
    // The inline object property is skipped.
    assert!(
        !hr.contains("body"),
        "object property must be skipped: {hr}"
    );
    assert!(hr.contains("String[0..1] author"), "sibling survives: {hr}");

    let sales = fs::read_to_string(output.path().join("sales.mox")).expect("read sales.mox");
    assert!(sales.starts_with("package sales\n"), "header: {sales}");
    assert!(
        sales.contains("class PremiumCustomerType extends CustomerType {"),
        "allOf extends: {sales}"
    );
    assert!(
        sales.contains("String[0..1] benefits"),
        "merged allOf props: {sales}"
    );
    assert!(
        sales.contains("Double[0..1] discountRate"),
        "merged optional prop: {sales}"
    );
    assert!(
        sales.contains("refers EmployeeType[0..1] manager"),
        "cross-domain scalar ref: {sales}"
    );
    assert!(
        sales.contains("refers AccountRepType representative"),
        "required scalar ref: {sales}"
    );
    assert!(
        sales.contains("refers OrderType[0..*] orders"),
        "array ref: {sales}"
    );

    // Parse-verify the written output with the rex compiler, independently
    // of the migration's own verification.
    let sources = vec![
        (
            STDLIB_FILE_NAME.to_string(),
            fs::read_to_string(output.path().join(STDLIB_FILE_NAME)).expect("read stdlib"),
        ),
        ("hr.mox".to_string(), hr.clone()),
        ("sales.mox".to_string(), sales.clone()),
    ];
    let compilation = rex_driver::compile_files(&sources);
    let errors: Vec<String> = compilation
        .diagnostics
        .iter()
        .filter(|(_, diagnostic)| diagnostic.is_error())
        .map(|(_, diagnostic)| diagnostic.message.clone())
        .collect();
    assert!(errors.is_empty(), "emitted mox must compile: {errors:?}");
    assert!(compilation.model.is_some());
    let packages = compilation.model.expect("model").packages;
    assert_eq!(packages.len(), 3, "one package per file");
    let hr_package = packages
        .iter()
        .find(|p| p.name == "hr")
        .expect("hr package");
    let employee = hr_package
        .classes
        .iter()
        .find(|c| c.name == "EmployeeType")
        .expect("EmployeeType class");
    assert_eq!(employee.features.len(), 14, "all mappable features present");
}

#[test]
fn dry_run_reports_but_writes_nothing() {
    let fixture = TempDir::new().expect("tempdir");
    write_fixture(fixture.path());
    let output = TempDir::new().expect("tempdir");

    let report = migrate(run_args(fixture.path(), output.path(), true, false))
        .expect("dry run must succeed");
    assert_eq!(report.files.len(), 3);
    for path in &report.files {
        assert!(!path.exists(), "dry run must not write {}", path.display());
    }
    assert_eq!(fs::read_dir(output.path()).expect("dir").count(), 0);
}

#[test]
fn rerun_without_force_is_rejected() {
    let fixture = TempDir::new().expect("tempdir");
    write_fixture(fixture.path());
    let output = TempDir::new().expect("tempdir");

    migrate(run_args(fixture.path(), output.path(), false, false)).expect("first run");
    let second = migrate(run_args(fixture.path(), output.path(), false, false));
    let message = second.expect_err("second run must fail").to_string();
    assert!(
        message.contains("--force") && message.contains("hr.mox"),
        "error must list the file and the flag: {message}"
    );

    migrate(run_args(fixture.path(), output.path(), false, true))
        .expect("force rerun must succeed");
}

#[test]
fn empty_schemas_dir_is_an_error() {
    let fixture = TempDir::new().expect("tempdir");
    let output = TempDir::new().expect("tempdir");
    let error = migrate(run_args(fixture.path(), output.path(), false, false))
        .expect_err("no schemas must error");
    assert!(
        error.to_string().contains("no JSON schemas"),
        "message: {error}"
    );
}

#[test]
fn unsupported_constructs_are_reported_not_fatal() {
    let fixture = TempDir::new().expect("tempdir");
    let domain = fixture.path().join("things/json");
    fs::create_dir_all(&domain).expect("mkdir");
    fs::write(
        domain.join("WidgetType.json"),
        r#"{
  "title": "WidgetType",
  "properties": {
    "size": { "type": "string", "format": "hostname" },
    "flags": { "type": "string", "enum": [1, 2, 3] },
    "weird": { "type": "string", "minLength": 10, "maxLength": 2 }
  }
}"#,
    )
    .expect("write widget");
    let output = TempDir::new().expect("tempdir");

    let report = migrate(run_args(fixture.path(), output.path(), false, false))
        .expect("migration with drops must still succeed");
    assert_eq!(report.converted, 1);
    let mox = fs::read_to_string(output.path().join("things.mox")).expect("read");
    assert!(
        mox.contains("String[0..1] size"),
        "unknown format falls back: {mox}"
    );
    assert!(
        mox.contains("String[0..1] flags"),
        "non-string enum falls back: {mox}"
    );
    assert!(
        !mox.contains("minLength"),
        "inconsistent bounds must be dropped: {mox}"
    );
    assert!(
        report
            .needs_review
            .iter()
            .any(|note| note.contains("hostname")),
        "unknown format noted: {:?}",
        report.needs_review
    );
    assert!(
        report
            .needs_review
            .iter()
            .any(|note| note.contains("minLength exceeds maxLength")),
        "inconsistent bounds noted: {:?}",
        report.needs_review
    );
}
