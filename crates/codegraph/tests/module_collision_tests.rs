//! WS5: entity-producing title collisions must fail generation LOUDLY.
//!
//! Two DISTINCT schema titles in one domain that BOTH produce entities
//! (tables) and strip to the same module/pg_table_name silently overwrite
//! each other's entity module and migration — the broken app has no hint
//! as to why. The check is entity-aware (the ordering WARN is not): a
//! `PayRunType` entity + a `PayRun` contained value object sharing the
//! `pay_run` module is LEGAL — the VO projects as `{parent}_{feature}`
//! child tables and owns no module of its own.
//!
//! Gate layering: this check keys on titles/modules/tables and complements
//! (never replaces) the duplicate-FK constraint gate in
//! `consistency.rs::check_child_table_consistency`, which keys on FK names.

use std::fs;
use std::path::{Path, PathBuf};

/// Collision fixture: `PayRun` and `PayRunType` are both entity-producing
/// (untouched by `contains`/`refers`, so both bridge as entities) and both
/// strip to `pay_run` — the `Type` suffix stripping collides.
const COLLISION_MOX: &str = r#"
package payroll

/// An entity (default: no contains edges target it).
class PayRun {
    String [1] reference
    int sequence
}

/// Also an entity — and its stripped name lands on the same module/table
/// as PayRun (`Type` suffix stripping).
class PayRunType {
    String [1] period
}
"#;

/// False-positive fixture: `PayRunType` is an entity, `PayRun` is a
/// contained value object of `PayRollSubmission`. Both strip to `pay_run`,
/// but only the entity owns the module/table — the VO projects as the
/// child table `pay_roll_submission_runs`. Generation must SUCCEED.
const VO_MODULE_SHARE_MOX: &str = r#"
package payroll

/// An entity — strips to `pay_run`.
class PayRunType {
    String [1] period
}

/// The entity that contains the value object.
class PayRollSubmission {
    String [1] submissionId
    contains PayRun[] runs
}

/// A contained value object — also strips to `pay_run`, but VOs own no
/// table: this is the #1 false-positive trap and must NOT error.
class PayRun {
    String [1] reference
}
"#;

const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.payroll]
label = "Payroll"
schema_dir = "payroll"
postgres_schema = "payroll"
"#;

struct Fixture {
    root: tempfile::TempDir,
    config: PathBuf,
    mox: PathBuf,
}

fn write_fixture(mox_source: &str) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("domains.toml"), DOMAINS_TOML).unwrap();
    fs::create_dir_all(root.path().join("model")).unwrap();
    fs::write(root.path().join("model/payroll.mox"), mox_source).unwrap();
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

/// (1) Two entity-producing titles in one domain stripping to the same
/// module/pg_table_name hard-error, naming BOTH titles, the domain, and
/// the colliding module/table.
#[tokio::test]
async fn colliding_entity_titles_hard_error_naming_both() {
    let fixture = write_fixture(COLLISION_MOX);
    let output = fixture.out();
    let mox_files = vec![fixture.mox.clone()];

    let err = codegraph::driver::run(run_args(&fixture, &mox_files, &output))
        .await
        .expect_err(
            "two entity-producing titles stripping to the same module/table \
             must fail generation (the second silently overwrites the first)",
        );
    let message = format!("{err}");
    assert!(
        message.contains("PayRun"),
        "must name title PayRun: {message}"
    );
    assert!(
        message.contains("PayRunType"),
        "must name title PayRunType: {message}"
    );
    assert!(
        message.contains("payroll"),
        "must name the domain: {message}"
    );
    assert!(
        message.contains("pay_run"),
        "must name the colliding module/table: {message}"
    );
}

/// (2) The false-positive trap: an entity + a contained value object
/// sharing the stripped module is LEGAL — generation succeeds and the DDL
/// carries the entity's table plus the VO's child table.
#[tokio::test]
async fn entity_plus_contained_vo_sharing_the_module_still_runs() {
    let fixture = write_fixture(VO_MODULE_SHARE_MOX);
    let output = fixture.out();
    let mox_files = vec![fixture.mox.clone()];

    codegraph::driver::run(run_args(&fixture, &mox_files, &output))
        .await
        .expect("entity + contained-VO module sharing must not error");

    // The DDL materialized the entity's table and the VO's child table.
    let migrations = output.join("migrations");
    let mut all_sql = String::new();
    for entry in fs::read_dir(&migrations).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("sql") {
            all_sql.push_str(&fs::read_to_string(&path).unwrap());
        }
    }
    assert!(
        all_sql.contains("CREATE TABLE IF NOT EXISTS payroll.pay_run"),
        "the entity's table must exist: {}",
        all_sql
    );
    assert!(
        all_sql.contains("CREATE TABLE IF NOT EXISTS payroll.pay_roll_submission_runs"),
        "the contained VO must project as a child table: {}",
        all_sql
    );
}
