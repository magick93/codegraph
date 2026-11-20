//! Issue #702: the domain key is the single Postgres schema source of truth.
//!
//! The DDL keys tables off the domains.toml DOMAIN KEY, while several
//! planes read `DomainEntry.postgres_schema` instead. When the two
//! differed, tables and their policies landed in different schemas —
//! silently broken SQL. The decided direction: every Postgres-qualified
//! plane reads the domain key, `postgres_schema` stays a required field
//! (parsing unchanged), and a divergence between the two is a HARD config
//! error at generation time and in doctor. All committed fixtures keep the
//! two equal, so generated output is byte-identical.
//!
//! Gate layering: the pipeline gate is config-only (no graph access) and
//! runs before any tree is written; doctor carries the same predicate so
//! `codegraph doctor` fails on the same shape without generating.

use std::fs;
use std::path::{Path, PathBuf};

const ALIGNED_MOX: &str = r#"
package payroll

class PayRun {
    String [1] reference
    int sequence
}
"#;

struct Fixture {
    root: tempfile::TempDir,
    config: PathBuf,
    mox: PathBuf,
}

fn write_fixture(domains_toml: &str) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("domains.toml"), domains_toml).unwrap();
    fs::create_dir_all(root.path().join("model")).unwrap();
    fs::write(root.path().join("model/payroll.mox"), ALIGNED_MOX).unwrap();
    Fixture {
        config: root.path().join("domains.toml"),
        mox: root.path().join("model/payroll.mox"),
        root,
    }
}

impl Fixture {
    fn out(&self) -> PathBuf {
        self.root.path().join("generated")
    }
}

fn run_args<'a>(
    fixture: &'a Fixture,
    mox_files: &'a [PathBuf],
    output: &'a Path,
) -> codegraph::driver::RunArgs<'a> {
    codegraph::driver::RunArgs {
        integration_manifests: &[],
        schemas: None,
        classifier: None,
        config_path: &fixture.config,
        output,
        extension_points_path: None,
        profile_name: "default",
        variant: None,
        // Nonexistent path → the no-plan (all generators) path runs.
        profiles_config_path: Some(fixture.root.path().join("profiles.toml")),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files,
        rosetta_files: &[],
        ddd_files: &[],
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    }
}

/// A `postgres_schema` naming a schema other than the domain key is a HARD
/// generation error naming the domain and both spellings — before any tree
/// is written.
#[tokio::test]
async fn divergent_postgres_schema_fails_generation_naming_both_spellings() {
    let fixture = write_fixture(
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.payroll]
label = "Payroll"
schema_dir = "payroll"
postgres_schema = "payroll_legacy"
"#,
    );
    let output = fixture.out();
    let mox_files = vec![fixture.mox.clone()];

    let err = codegraph::driver::run(run_args(&fixture, &mox_files, &output))
        .await
        .expect_err(
            "postgres_schema diverging from the domain key must fail generation \
             (tables and policies would land in different schemas)",
        );
    let message = format!("{err}");
    assert!(
        message.contains("domains.payroll.postgres_schema"),
        "must name the config path: {message}"
    );
    assert!(
        message.contains("payroll_legacy"),
        "must name the declared spelling: {message}"
    );
    assert!(
        message.contains("\"payroll\""),
        "must name the domain key: {message}"
    );
    assert!(message.contains("#702"), "must cite the issue: {message}");
    // Fail BEFORE any tree is written: the config-only gate runs first.
    assert!(
        !output.join("Cargo.toml").exists(),
        "no generated tree may exist when the schema gate fails"
    );
}

/// The standing fixture shape — `postgres_schema` equal to the domain key —
/// still generates (the every-fixture byte-identity contract).
#[tokio::test]
async fn aligned_postgres_schema_still_generates() {
    let fixture = write_fixture(
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.payroll]
label = "Payroll"
schema_dir = "payroll"
postgres_schema = "payroll"
"#,
    );
    let output = fixture.out();
    let mox_files = vec![fixture.mox.clone()];

    codegraph::driver::run(run_args(&fixture, &mox_files, &output))
        .await
        .expect("aligned postgres_schema must generate");

    let ddl = fs::read_to_string(output.join("migrations").join("000100_payroll_schema.sql"))
        .unwrap_or_else(|_| {
            // Migration numbering may shift; fall back to scanning the band.
            let mut all = String::new();
            for entry in fs::read_dir(output.join("migrations")).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().and_then(|e| e.to_str()) == Some("sql") {
                    all.push_str(&fs::read_to_string(&path).unwrap());
                }
            }
            all
        });
    assert!(
        ddl.contains("CREATE TABLE IF NOT EXISTS payroll.pay_run"),
        "the table must land in the DOMAIN-key schema: {}",
        ddl
    );
}
