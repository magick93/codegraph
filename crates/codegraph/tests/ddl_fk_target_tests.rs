//! Issue #322 / #57: FK constraints in emitted DDL.
//!
//! #322 — an FK targets the schema its referenced table is EMITTED in. The
//! graph FK edge carries the domain the target was DECLARED in, while the
//! table is emitted in the domain that claims the title in the generation
//! order; `retain_generated_fks` re-points a disagreeing FK (the in-crate
//! `db::ddl::tests` KATs pin the remap). End to end, this file pins the
//! invariant over a cross-domain model: every `REFERENCES <schema>.<table>`
//! names a table some emitted migration creates, and the cross-domain FK
//! survives into the DDL.
//!
//! #57 — a required reference is `NOT NULL` + `ON DELETE RESTRICT`; an
//! optional one is nullable + `ON DELETE SET NULL`.

use std::fs;
use std::path::{Path, PathBuf};

const HR_MOX: &str = r#"
package hr

class EmployeeType {
    String name
    refers PositionType[0..1] backupPosition
}
"#;

const RECRUITING_MOX: &str = r#"
package recruiting

class PositionType {
    String title
}
"#;

const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"
"#;

fn run_args<'a>(
    root: &'a Path,
    config: &'a Path,
    mox_files: &'a [PathBuf],
    output: &'a Path,
) -> codegraph::driver::RunArgs<'a> {
    let _ = root;
    codegraph::driver::RunArgs {
        integration_manifests: &[],
        schemas: None,
        classifier: None,
        config_path: config,
        output,
        extension_points_path: None,
        profile_name: "default",
        variant: None,
        // Nonexistent path → the no-plan (all generators) path runs.
        profiles_config_path: Some(root.join("profiles.toml")),
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

/// `(file name, contents)` for every emitted migration.
fn migrations(output: &Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = fs::read_dir(output.join("migrations"))
        .expect("migrations dir")
        .filter_map(|e| e.ok())
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                fs::read_to_string(e.path()).unwrap_or_default(),
            )
        })
        .collect();
    out.sort();
    out
}

async fn generate() -> (tempfile::TempDir, Vec<(String, String)>) {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("domains.toml");
    fs::write(&config, DOMAINS_TOML).unwrap();
    fs::create_dir_all(root.path().join("model")).unwrap();
    let hr = root.path().join("model/hr.mox");
    fs::write(&hr, HR_MOX).unwrap();
    let recruiting = root.path().join("model/recruiting.mox");
    fs::write(&recruiting, RECRUITING_MOX).unwrap();
    let output = root.path().join("generated");
    let mox_files = vec![hr, recruiting];
    codegraph::driver::run(run_args(root.path(), &config, &mox_files, &output))
        .await
        .expect("generation succeeds");
    let migs = migrations(&output);
    (root, migs)
}

fn employee_ddl(migs: &[(String, String)]) -> &str {
    migs.iter()
        .find(|(name, _)| name.ends_with("_hr_employee.sql"))
        .map(|(_, sql)| sql.as_str())
        .unwrap_or_else(|| {
            panic!(
                "no hr employee migration in {:?}",
                migs.iter().map(|(n, _)| n).collect::<Vec<_>>()
            )
        })
}

#[tokio::test]
async fn cross_domain_fk_targets_the_emitting_schema() {
    let (_root, migs) = generate().await;
    assert!(
        migs.iter()
            .any(|(name, _)| name.ends_with("_recruiting_position.sql")),
        "the target is emitted in its own domain: {:?}",
        migs.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );
    let ddl = employee_ddl(&migs);
    assert!(
        ddl.contains("REFERENCES recruiting.position(id)"),
        "the cross-domain FK names the schema the table is emitted in:\n{ddl}"
    );
}

/// Every `REFERENCES <schema>.<table>` an emitted migration names is a table
/// some emitted migration creates — no FK points at a phantom table.
#[tokio::test]
async fn every_fk_target_is_an_emitted_table() {
    let (_root, migs) = generate().await;
    let all_sql: String = migs.iter().map(|(_, s)| s.as_str()).collect();
    let created = |schema: &str, table: &str| {
        all_sql.contains(&format!("CREATE TABLE IF NOT EXISTS {schema}.{table} "))
            || all_sql.contains(&format!("CREATE TABLE IF NOT EXISTS {schema}.{table}("))
            || all_sql.contains(&format!("CREATE TABLE IF NOT EXISTS {schema}.{table}\n"))
            || all_sql.contains(&format!("CREATE TABLE {schema}.{table} "))
            || all_sql.contains(&format!("CREATE TABLE {schema}.{table}("))
    };
    let mut phantoms = Vec::new();
    for (name, sql) in &migs {
        for rest in sql.split("REFERENCES ").skip(1) {
            let target: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
                .collect();
            if let Some((schema, table)) = target.split_once('.')
                && !created(schema, table)
            {
                phantoms.push(format!("{name}: {target}"));
            }
        }
    }
    assert!(
        phantoms.is_empty(),
        "FKs to tables no migration creates: {phantoms:?}"
    );
}

/// Issue #57: an optional reference stays nullable + `ON DELETE SET NULL`.
/// (A required mox `refers` lowers to a junction child table, so the required
/// RESTRICT leg is pinned over a scalar JSON reference in
/// `generate_tests::required_fk`.)
#[tokio::test]
async fn optional_reference_sets_null() {
    let (_root, migs) = generate().await;
    let ddl = employee_ddl(&migs);
    let fk = ddl
        .lines()
        .find(|l| l.contains("FOREIGN KEY (backup_position_id)"))
        .unwrap_or_else(|| panic!("no FK on backup_position_id:\n{ddl}"));
    assert!(fk.contains("ON DELETE SET NULL"), "{fk}");
    assert!(
        ddl.lines().any(|l| l.trim() == "backup_position_id UUID,"),
        "the optional reference is nullable:\n{ddl}"
    );
}
