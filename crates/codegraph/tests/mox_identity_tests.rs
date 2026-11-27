//! Issue #522: mox ingest must not force-map composite natural-key `id`
//! features to Uuid.
//!
//! The `id` modifier is the Sculptor identity marker. Two shapes exist:
//!
//! - a single `id` feature keeps its OWN column, with the declared datatype
//!   overridden to Uuid so every layer agrees — the generated surrogate
//!   `id UUID` stays the primary key and every FK's target (issue #821);
//! - multiple `id` features are a composite natural key (the documented
//!   Sculptor convention: "composite natural keys = multiple `id` features")
//!   — every part keeps its declared mapping (String→TEXT, wraps-String→TEXT,
//!   `format "uuid"`→UUID, …) and the generated surrogate `id UUID` column
//!   stays the storage key.
//!
//! Contract pinned here:
//! 1. `composite_id_features_keep_declared_types` — a class with 2+ `id`
//!    features emits TEXT for the string parts and UUID only where declared,
//!    across DDL, DTO, and entity planes;
//! 2. `single_id_feature_stays_forced_uuid` — a single `id` feature is still
//!    forced to Uuid (byte-identical to the pre-#522 behavior);
//! 3. the #821 advisory: a sole `id` whose column equals the class's derived
//!    parent-FK column warns (`sole_identity_fk_collision`); non-colliding
//!    and composite ids never do, and generated output does not move.

use std::fs;
use std::path::{Path, PathBuf};

/// The crewbase#44 discovery shape: a composite natural key whose parts
/// declare their own types. `code`/`^version` are Strings, `jurisdiction` a
/// wraps-String datatype without a format (→ TEXT), `ruleId` a uuid-formatted
/// identifier (→ UUID).
const COMPOSITE_MOX: &str = r#"
package registry

/// A durable identifier.
type Identifier wraps String {
    format "uuid"
}

/// A jurisdiction code.
type JurisdictionCode wraps String

class ComplianceRuleType {
    id Identifier ruleId
    id String code
    id String ^version
    id JurisdictionCode jurisdiction
}
"#;

/// Single-`id` entity: the lone `id` feature keeps its own column and stays
/// forced to Uuid even though it declares String.
const SINGLE_ID_MOX: &str = r#"
package registry

class ProductType {
    id String sku
    String name
}
"#;

const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.registry]
label = "Registry"
schema_dir = "registry"
postgres_schema = "registry"
"#;

struct Fixture {
    root: tempfile::TempDir,
    config: PathBuf,
    mox: PathBuf,
}

fn write_fixture(source: &str) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("domains.toml"), DOMAINS_TOML).unwrap();
    fs::create_dir_all(root.path().join("model")).unwrap();
    fs::write(root.path().join("model/registry.mox"), source).unwrap();
    Fixture {
        config: root.path().join("domains.toml"),
        mox: root.path().join("model/registry.mox"),
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
        schemas: None,
        classifier: None,
        config_path: &fixture.config,
        output,
        extension_points_path: None,
        integration_manifests: &[],
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

/// Locate the main DDL migration for a registry table (files carry a
/// sequence prefix, e.g. `000542_registry_compliance_rule.sql`).
fn migration(table: &str, output: &Path) -> Option<PathBuf> {
    let dir = output.join("migrations");
    fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(&format!("registry_{table}.sql")))
        })
}

async fn run_pipeline(source: &str) -> (Fixture, PathBuf) {
    let fixture = write_fixture(source);
    let output = fixture.out();
    let mox_files = vec![fixture.mox.clone()];
    codegraph::driver::run(run_args(&fixture, &mox_files, &output))
        .await
        .unwrap();
    let moved_output = output.clone();
    (fixture, moved_output)
}

/// (1) Composite natural key: every `id` part keeps its declared mapping —
/// TEXT for the string parts, UUID only where declared (the uuid-formatted
/// `ruleId`) — across DDL, DTO, and entity planes. The synthetic `id UUID`
/// surrogate stays the storage key.
#[tokio::test]
async fn composite_id_features_keep_declared_types() {
    let (_fixture, output) = run_pipeline(COMPOSITE_MOX).await;

    // ── DDL plane ────────────────────────────────────────────────────────
    let ddl = fs::read_to_string(
        migration("compliance_rule", &output).expect("no migration for registry.compliance_rule"),
    )
    .unwrap();
    assert!(ddl.contains("rule_id UUID"), "{ddl}");
    assert!(ddl.contains("code TEXT"), "{ddl}");
    assert!(
        !ddl.contains("code UUID"),
        "a String natural-key part must not be force-mapped to Uuid: {ddl}"
    );
    // `version` is a render-time quoted identifier (PG_RESERVED_WORDS).
    assert!(ddl.contains("\"version\" TEXT"), "{ddl}");
    assert!(
        !ddl.contains("\"version\" UUID"),
        "a String natural-key part must not be force-mapped to Uuid: {ddl}"
    );
    assert!(ddl.contains("jurisdiction TEXT"), "{ddl}");
    assert!(
        !ddl.contains("jurisdiction UUID"),
        "a wraps-String natural-key part must not be force-mapped to Uuid: {ddl}"
    );
    // The surrogate storage key is untouched.
    assert!(
        ddl.contains("id UUID NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY"),
        "{ddl}"
    );

    // ── DTO plane (the domain-types crate carries the field structs; the
    // app-side `src/domain/.../dto_response.rs` is the linked wrapper) ────
    let dto_dir = output.join("crates/domain-types/src/registry/compliance_rule");
    let response =
        fs::read_to_string(dto_dir.join("dto_response.rs")).expect("dto_response.rs must generate");
    for (field, ty) in [
        ("rule_id", "uuid::Uuid"),
        ("code", "String"),
        ("version", "String"),
        ("jurisdiction", "String"),
    ] {
        assert!(
            response.contains(&format!("pub {field}: {ty}")),
            "dto_response must type {field} as {ty}:\n{response}"
        );
    }
    let create =
        fs::read_to_string(dto_dir.join("dto_create.rs")).expect("dto_create.rs must generate");
    assert!(
        create.contains("pub code: String"),
        "create DTO must type code as String:\n{create}"
    );
    assert!(
        create.contains("pub rule_id: uuid::Uuid"),
        "create DTO must type rule_id as uuid::Uuid:\n{create}"
    );

    // ── Entity plane ─────────────────────────────────────────────────────
    let entity = fs::read_to_string(output.join("src/entity/registry_compliance_rule.rs"))
        .expect("entity module must generate");
    for (field, ty) in [
        ("rule_id", "uuid::Uuid"),
        ("code", "String"),
        ("version", "String"),
        ("jurisdiction", "String"),
    ] {
        assert!(
            entity.contains(&format!("pub {field}: {ty}")),
            "entity must type {field} as {ty}:\n{entity}"
        );
    }
}

/// (2) Single-`id` entity: the lone `id` feature keeps its own column —
/// still forced to Uuid regardless of its declared type (byte-identical to
/// the pre-#522 behavior).
#[tokio::test]
async fn single_id_feature_stays_forced_uuid() {
    let (_fixture, output) = run_pipeline(SINGLE_ID_MOX).await;

    let ddl = fs::read_to_string(
        migration("product", &output).expect("no migration for registry.product"),
    )
    .unwrap();
    assert!(ddl.contains("sku UUID"), "{ddl}");
    assert!(!ddl.contains("sku TEXT"), "{ddl}");
    // Non-identity siblings keep their declared mapping (`name` is a
    // render-time quoted identifier).
    assert!(ddl.contains("\"name\" TEXT"), "{ddl}");

    let response =
        fs::read_to_string(output.join("crates/domain-types/src/registry/product/dto_response.rs"))
            .expect("dto_response.rs must generate");
    assert!(
        response.contains("pub sku: uuid::Uuid"),
        "single-id entity stays Uuid-typed:\n{response}"
    );
}

// ── Issue #821: the sole `id` feature is a SECOND column, not the PK ───────

/// The #821 shape: the sole `id` feature is spelled like the parent-FK
/// column the class's child tables derive (`worker` → `worker_id`). The
/// feature stays its own UUID column; every child's `worker_id` references
/// the generated surrogate `id`.
const COLLIDING_ID_MOX: &str = r#"
package registry

/// A durable identifier.
type Identifier wraps String {
    format "uuid"
}

class Address {
    String line1
}

class Worker {
    id readonly Identifier workerId
    String name
    contains Address[] addresses
}
"#;

/// The non-colliding twin: a sole `id` feature whose column (`sku`) is not
/// the derived parent-FK column (`product_id`).
const NON_COLLIDING_ID_MOX: &str = r#"
package registry

class Product {
    id String sku
    String name
}
"#;

/// A composite key whose part happens to share the derived FK spelling:
/// composite natural keys keep their declared mapping and never warn.
const COMPOSITE_COLLIDING_MOX: &str = r#"
package registry

class Worker {
    id String workerId
    id String region
    String name
}
"#;

async fn identity_findings(source: &str) -> Vec<String> {
    let fixture = write_fixture(source);
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let engine = codegraph_grafeo::GrafeoEngine::in_memory().unwrap();
    codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        std::slice::from_ref(&fixture.mox),
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap();
    codegraph::validate::ValidationPass::run(&engine, &config)
        .await
        .into_iter()
        .filter(|issue| issue.check == "sole_identity_fk_collision")
        .map(|issue| {
            assert!(
                matches!(issue.severity, codegraph::validate::Severity::Warning),
                "the collision is advisory, never an error: {issue:?}"
            );
            issue.message
        })
        .collect()
}

/// (3) Issue #821: a sole `id` feature spelled like the class's derived
/// parent-FK column warns, naming the class, the feature, the colliding
/// column, what the child tables' same-named column references, and the
/// rename fix.
#[tokio::test]
async fn sole_id_feature_colliding_with_the_parent_fk_warns() {
    let findings = identity_findings(COLLIDING_ID_MOX).await;
    assert_eq!(findings.len(), 1, "exactly one finding: {findings:?}");
    let message = &findings[0];
    for needle in [
        "`Worker`",
        "`workerId`",
        "`worker_id`",
        "surrogate `id`",
        "child table",
        "Rename the feature",
        "`workerNumber`",
    ] {
        assert!(message.contains(needle), "missing {needle:?} in: {message}");
    }
}

/// (4) The contract stays visible in the emitted DDL: the business id is its
/// own UUID column on the root table and the child's `worker_id` references
/// the surrogate `id` — exactly what the Warning describes. Generated output
/// is unchanged by #821 (advisory only).
#[tokio::test]
async fn colliding_sole_id_keeps_its_own_column_and_children_reference_the_surrogate() {
    let (_fixture, output) = run_pipeline(COLLIDING_ID_MOX).await;
    let ddl =
        fs::read_to_string(migration("worker", &output).expect("no migration for registry.worker"))
            .unwrap();
    assert!(
        ddl.contains("id UUID PRIMARY KEY"),
        "the surrogate stays the storage key:\n{ddl}"
    );
    assert!(
        ddl.contains("worker_id UUID"),
        "the business id keeps its own forced-UUID column:\n{ddl}"
    );
    assert!(
        ddl.contains("REFERENCES registry.worker(id)"),
        "the child's FK targets the surrogate `id`:\n{ddl}"
    );
}

/// (5) A non-colliding sole `id` (`sku`) and a composite key never warn.
#[tokio::test]
async fn non_colliding_and_composite_ids_do_not_warn() {
    assert!(identity_findings(NON_COLLIDING_ID_MOX).await.is_empty());
    assert!(identity_findings(COMPOSITE_COLLIDING_MOX).await.is_empty());
    assert!(identity_findings(COMPOSITE_MOX).await.is_empty());
}
