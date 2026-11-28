//! Issue #322: an FK targets the schema its referenced table is EMITTED in.
//!
//! A graph FK edge carries the domain the target was DECLARED in (the mox
//! package). When another domain claims the title in the generation order
//! (an explicit `entities` entry re-claims a discovery-only title), the table
//! is emitted in the CLAIMING domain. Pre-#322 the FK named the declared
//! schema, failed the emitted-table filter, and was silently dropped — the
//! reference column shipped with no constraint at all.

use std::fs;
use std::path::{Path, PathBuf};

const MODEL_MOX: &str = r#"
package hr

class EmployeeType {
    String name
    refers PositionType position
    refers PositionType[0..1] backupPosition
}

class PositionType {
    String title
}
"#;

/// `recruiting` explicitly configures `PositionType`, so it claims the title
/// although the class is declared in the `hr` package.
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
entities = ["PositionType"]
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
    let mox = root.path().join("model/hr.mox");
    fs::write(&mox, MODEL_MOX).unwrap();
    let output = root.path().join("generated");
    let mox_files = vec![mox];
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
async fn fk_to_a_reclaimed_entity_targets_the_claiming_schema() {
    let (_root, migs) = generate().await;
    assert!(
        migs.iter()
            .any(|(name, _)| name.ends_with("_recruiting_position.sql")),
        "the reclaimed title is emitted in the claiming domain: {:?}",
        migs.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );
    let ddl = employee_ddl(&migs);
    assert!(
        ddl.contains("REFERENCES recruiting.position(id)"),
        "the FK names the schema the table is emitted in:\n{ddl}"
    );
    assert!(
        !ddl.contains("REFERENCES hr.position"),
        "no FK names the declared-but-unemitted schema:\n{ddl}"
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

/// Issue #57: a required reference is `NOT NULL` + `ON DELETE RESTRICT`; an
/// optional one stays nullable + `ON DELETE SET NULL`. The DDL never pairs a
/// NOT NULL column with SET NULL.
#[tokio::test]
async fn required_reference_restricts_and_optional_sets_null() {
    let (_root, migs) = generate().await;
    let ddl = employee_ddl(&migs);
    let fk_line = |col: &str| {
        ddl.lines()
            .find(|l| l.contains(&format!("FOREIGN KEY ({col})")))
            .unwrap_or_else(|| panic!("no FK on {col}:\n{ddl}"))
            .to_string()
    };
    assert!(
        fk_line("position_id").contains("ON DELETE RESTRICT"),
        "{}",
        fk_line("position_id")
    );
    assert!(
        fk_line("backup_position_id").contains("ON DELETE SET NULL"),
        "{}",
        fk_line("backup_position_id")
    );
    assert!(
        ddl.lines().any(|l| l.contains("position_id UUID NOT NULL")),
        "the required reference is NOT NULL:\n{ddl}"
    );
}
