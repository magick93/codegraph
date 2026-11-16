//! Issue #668 (a): a mox `class X table "…"` declaration (upstream
//! pherolang#177 `ClassDef::table_name`) is the effective storage table on
//! EVERY generated plane.
//!
//! The bridge writes the declared spelling straight into
//! `SchemaNode.pg_table_name` — the one field the DDL, the SeaORM entity, the
//! repository SQL, the filter/search SQL, the hurl/openapi naming and the
//! composition child-table derivation all read. Nothing is re-derived, so the
//! declaration cannot drift plane by plane. HTTP URLs are unaffected: they key
//! on `api_path_segment`, which still derives from the TITLE.
//!
//! Issue #668 (b): a `register` class is the reference-data plane. It gets a
//! `common.{table}` seed migration (the codelist generators own it) and NO
//! entity surface at all — no entity module, no repository, no handler, no
//! route, no `{domain}.{table}` table. Generation ordering skips it.

use std::fs;
use std::path::{Path, PathBuf};

/// `PayRunType` declares the storage table `pay_run_rows`; it also contains a
/// value object, so the child table must derive from the DECLARED parent table
/// (`pay_run_rows_lines`, not `pay_run_lines`).
const TABLE_NAME_MOX: &str = r#"
package payroll

class PayRunType table "pay_run_rows" {
    id String payRunId
    String reference
    contains PayRunLine[] lines
    refers Employee[0..1] approvedBy
}

class PayRunLine {
    String label
    int amount
}

class Employee {
    String name
}

register Role {
    String display_name
    String[0..1] badge = "none"
}

class Assignment {
    id String assignmentId
    refers Role[0..1] role
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

fn write_fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("domains.toml"), DOMAINS_TOML).unwrap();
    fs::create_dir_all(root.path().join("model")).unwrap();
    fs::write(root.path().join("model/payroll.mox"), TABLE_NAME_MOX).unwrap();
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

/// Every emitted file path, relative to the output root (sorted).
fn emitted_paths(output: &Path) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                out.push(
                    path.strip_prefix(root)
                        .unwrap()
                        .display()
                        .to_string()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(output, output, &mut out);
    out.sort();
    out
}

fn read_all(output: &Path, subdir: &str, ext: &str) -> String {
    let mut text = String::new();
    let dir = output.join(subdir);
    let Ok(entries) = fs::read_dir(&dir) else {
        return text;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            text.push_str(&fs::read_to_string(&path).unwrap_or_default());
            text.push('\n');
        }
    }
    text
}

/// Run the pipeline; the fixture (and its tempdir) is returned so the output
/// tree lives as long as the assertions need it.
async fn generate() -> (Fixture, PathBuf) {
    let fixture = write_fixture();
    let output = fixture.out();
    let mox_files = vec![fixture.mox.clone()];
    codegraph::driver::run(run_args(&fixture, &mox_files, &output))
        .await
        .expect("generation must succeed");
    (fixture, output)
}

#[tokio::test]
async fn declared_table_wins_on_every_generated_plane() {
    let (_fixture, output) = generate().await;
    let paths = emitted_paths(&output);

    // ── DDL: the entity's table and the composition child table both derive
    // from the DECLARED parent table.
    let migrations = read_all(&output, "migrations", "sql");
    assert!(
        migrations.contains("CREATE TABLE IF NOT EXISTS payroll.pay_run_rows"),
        "the declared table must be created: {paths:#?}"
    );
    assert!(
        !migrations.contains("CREATE TABLE IF NOT EXISTS payroll.pay_run ("),
        "the derived table must not exist:\n{}",
        migrations
            .lines()
            .filter(|l| l.contains("CREATE TABLE"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        migrations.contains("CREATE TABLE IF NOT EXISTS payroll.pay_run_rows_lines"),
        "the contained VO's child table must derive from the DECLARED parent table:\n{migrations}"
    );

    // ── SeaORM entity: module + `table_name` attribute follow the table.
    let entity_path = "src/entity/payroll_pay_run_rows.rs";
    assert!(
        paths.iter().any(|p| p == entity_path),
        "the SeaORM entity module must be named after the table: {paths:#?}"
    );
    let entity = fs::read_to_string(output.join(entity_path)).unwrap();
    assert!(
        entity.contains("table_name = \"pay_run_rows\""),
        "the entity must declare the table: {entity}"
    );

    // ── Repository SQL: the emitted statements name the declared table and
    // reach the SeaORM entity module that owns it.
    let repository = read_all(&output, "src/domain/payroll/pay_run_rows", "rs");
    assert!(
        repository.contains("db.table = \"payroll.pay_run_rows\""),
        "the repository's tracing/table names must carry the declared table: {repository}"
    );
    assert!(
        repository.contains("crate::entity::payroll_pay_run_rows::"),
        "the repository must use the declared table's entity module: {repository}"
    );
    assert!(
        !repository.contains("payroll.pay_run ")
            && !repository.contains("entity::payroll_pay_run::"),
        "repository must not reference the derived table/module: {repository}"
    );

    // ── hurl naming follows the table; the HTTP PATH does not.
    assert!(
        paths
            .iter()
            .any(|p| p.starts_with("hurl/") && p.ends_with("_pay_run_rows.hurl")),
        "the hurl contract file must be named after the table: {paths:#?}"
    );
    let hurl = read_all(&output, "hurl", "hurl");
    assert!(
        hurl.contains("/api/v1/payroll/pay-run"),
        "the HTTP path keeps the title-derived segment: {hurl}"
    );

    // ── The router/openapi PATH is keyed on the api path segment (derived
    // from the TITLE), so a table override never moves an endpoint.
    let api_sources = read_all(&output, "src/api/payroll", "rs");
    assert!(
        api_sources.contains("/pay-run"),
        "the route path keeps the title-derived segment: {api_sources}"
    );
    assert!(
        !api_sources.contains("/pay-run-rows"),
        "the table spelling must never leak into a URL: {api_sources}"
    );
}

#[tokio::test]
async fn register_class_gets_a_seed_table_and_no_entity_surface() {
    let (_fixture, output) = generate().await;
    let paths = emitted_paths(&output);

    // ── The reference-data plane: a `common.role` seed migration.
    let migrations = read_all(&output, "migrations", "sql");
    assert!(
        migrations.contains("CREATE TABLE IF NOT EXISTS common.role"),
        "the register must get a common-schema seed table:\n{migrations}"
    );
    assert!(
        migrations.contains("-- Codelist: common.role"),
        "the register's table is the codelist/seed plane:\n{migrations}"
    );
    assert!(
        migrations.contains("Mox `register` class"),
        "the seed migration must SAY what a register is (no silent mapping):\n{migrations}"
    );

    // ── NO entity surface: no module, no repository, no table in the payroll
    // schema, no route.
    assert!(
        !migrations.contains("CREATE TABLE IF NOT EXISTS payroll.role"),
        "a register must never get an entity table:\n{migrations}"
    );
    for forbidden in [
        "src/entity/payroll_role.rs",
        "src/domain/payroll/role/mod.rs",
        "src/domain/payroll/role/repository.rs",
        "src/domain/payroll/role/command.rs",
    ] {
        assert!(
            !paths.iter().any(|p| p == forbidden),
            "a register must not generate `{forbidden}`: {paths:#?}"
        );
    }
    let routers = read_all(&output, "src/api", "rs") + &read_all(&output, "src", "rs");
    assert!(
        !routers.contains("/role\""),
        "a register must not be routed"
    );

    // ── The codelist Rust enum rides along (the zero-variant stub the
    // codelist generator emits for a value-less codelist), so `Role` exists
    // as a type without inventing any variants.
    let role_enum = paths
        .iter()
        .find(|p| p.ends_with("codelist/role.rs"))
        .unwrap_or_else(|| panic!("the register's codelist enum stub must be emitted: {paths:#?}"));
    let role_enum = fs::read_to_string(output.join(role_enum)).unwrap();
    assert!(
        role_enum.contains("pub enum Role {}"),
        "a register declares no literals: {role_enum}"
    );

    // ── A referring entity still stores the register row's id.
    let migrations_all = migrations;
    assert!(
        migrations_all.contains("role_id"),
        "the referring entity keeps its role reference column:\n{migrations_all}"
    );
    assert!(
        migrations_all.contains("CREATE TABLE IF NOT EXISTS payroll.assignment"),
        "the referring entity itself is a normal entity"
    );
}
