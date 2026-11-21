//! Shared fixture for the issue #781 P1 cornucopia `?include=` gates.
//!
//! Two integration tests consume this module:
//!
//! * `cornucopia_include_tests` — the wiring pin (string/syn assertions over
//!   the emitted plane, flag ON and flag OFF);
//! * `cornucopia_include_compile_tests` — the #551 compile pin (the include
//!   plane assembled into a scratch crate and `cargo check`ed against a
//!   stubbed `cornucopia-queries`).
//!
//! Factored out (the `tests/test_framework` precedent) so the fixture cannot
//! drift between the two gates: the compile pin must assemble the SAME model,
//! config and profiles the wiring pin asserts.

use std::fs;
use std::path::{Path, PathBuf};

/// `refers X[0..1]` is a scalar FK (the implemented scalar include shape). A
/// bare `refers X` is many-valued in mox, so the optional form is load-bearing.
/// `refers X[1]` is a REQUIRED scalar FK — the reverse side of it is a
/// collection whose reverse-FK column is non-nullable, exercising the
/// required group-key arm.
///
/// The ONE-TO-MANY relationship is implicit: `WorkOrderType` and `InvoiceType`
/// both point at `CustomerType`, so auto-discovery (CustomerType is `root`
/// with no explicit `allow_include`) resolves `customer?include=work_order`
/// and `customer?include=invoice` as reverse-FK collections. An explicit
/// `allow_include` cannot express that direction — `resolve_explicit_paths`
/// derives its array flag from a SOURCE-side array property, which a scalar
/// child reference never has.
///
/// `InvoiceType`'s explicit surface carries the dot notation: `customer.org`
/// is the implemented one-hop shape, and `customer.org.currency` (three
/// segments) is the deliberately unsupported shape, so the per-path
/// fail-closed arm is exercised by the SAME handler that hydrates
/// `customer.org`. `InvoiceType` deliberately does NOT also list `customer`
/// — a single-segment path would claim the `customer` include field the dot
/// path's combined response owns.
pub const MODEL_MOX: &str = r#"
package shop

class CurrencyType {
    String code
}

class OrganizationType {
    String name
    refers CurrencyType[0..1] currency
}

class CustomerType {
    String name
    refers OrganizationType[0..1] org
}

class WorkOrderType {
    String reference
    refers CustomerType[0..1] customer
    int total
}

class InvoiceType {
    String number
    refers CustomerType[1] customer
    int total
}
"#;

pub const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.shop]
label = "Shop"
schema_dir = "shop"
postgres_schema = "shop"

# `root` + NO explicit `allow_include` ⇒ auto-discovery, which is the only
# resolver path that turns a child's scalar reference into the reverse-FK
# (array) shape. `WorkOrderType.customer` is optional ⇒ optional group key;
# `InvoiceType.customer` is required ⇒ plain group key.
[domains.shop.entity_config.CustomerType]
role = "root"

[domains.shop.entity_config.WorkOrderType]
role = "root"
allow_include = ["customer"]

# The dot-notation source: `customer.org` hydrates (one hop); the
# three-segment path is the named 501 shape the fail-closed arm asserts.
[domains.shop.entity_config.InvoiceType]
allow_include = ["customer.org", "customer.org.currency"]
"#;

/// Two profiles over one model: the preview feature ON and OFF. Everything
/// else is identical, so the flag is the only variable.
///
/// `domain_types_base` is deliberately NOT set: the profile's generator list
/// omits the monolith `scaffold` generator, so the run emits the
/// domain-types crate (`Cargo.toml` + `src/{lib.rs,shop,codelist,context.rs,
/// query.rs}`) at the output ROOT beside the app tree. The compile pin
/// assembles both halves from that layout (see
/// `cornucopia_include_compile_tests`), so the fixture stays exactly what the
/// wiring pin asserts.
pub const PROFILES_TOML: &str = r#"
[profiles.cornucopia-includes.meta]
name = "cornucopia-includes"
version = "1.0.0"
description = "cornucopia include hydration preview"

[profiles.cornucopia-includes.features]
persistence_provider = "cornucopia"
deployment_topology = "monolith"
cornucopia_include_hydration = true

[profiles.cornucopia-includes.api]
generators = [
    "ddl", "cornucopia_queries", "cornucopia_repo", "dto", "repository",
    "command", "query", "event", "handler", "lifecycle_trait",
    "domain_types_dto", "domain_types_query_service",
    "errors", "router", "links",
    "cornucopia_config", "basejump_setup", "pgmq_setup", "platform_schema",
    "workflow_seed", "hook_registry", "domain_types_scaffold", "codelist",
    "openapi",
]
output = "generated/"

[profiles.cornucopia-off.meta]
name = "cornucopia-off"
version = "1.0.0"
description = "cornucopia without include hydration"

[profiles.cornucopia-off.features]
persistence_provider = "cornucopia"
deployment_topology = "monolith"

[profiles.cornucopia-off.api]
generators = [
    "ddl", "cornucopia_queries", "cornucopia_repo", "dto", "repository",
    "command", "query", "event", "handler", "lifecycle_trait",
    "domain_types_dto", "domain_types_query_service",
    "errors", "router", "links",
    "cornucopia_config", "basejump_setup", "pgmq_setup", "platform_schema",
    "workflow_seed", "hook_registry", "domain_types_scaffold", "codelist",
    "openapi",
]
output = "generated/"
"#;

pub const FLAG_ON_PROFILE: &str = "cornucopia-includes";
pub const FLAG_OFF_PROFILE: &str = "cornucopia-off";

pub struct Fixture {
    pub root: tempfile::TempDir,
    pub config: PathBuf,
    pub mox: PathBuf,
    pub profiles: PathBuf,
}

pub fn write_fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("domains.toml"), DOMAINS_TOML).unwrap();
    fs::create_dir_all(root.path().join("model")).unwrap();
    fs::write(root.path().join("model/shop.mox"), MODEL_MOX).unwrap();
    fs::write(root.path().join("profiles.toml"), PROFILES_TOML).unwrap();
    Fixture {
        config: root.path().join("domains.toml"),
        mox: root.path().join("model/shop.mox"),
        profiles: root.path().join("profiles.toml"),
        root,
    }
}

pub fn run_args<'a>(
    fixture: &'a Fixture,
    profile: &'a str,
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
        profile_name: profile,
        variant: None,
        profiles_config_path: Some(fixture.profiles.clone()),
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

/// Run the driver over the fixture and return the fixture (keeps the tempdir
/// alive) plus the output directory.
pub async fn generate(profile: &str) -> (Fixture, PathBuf) {
    let fixture = write_fixture();
    let output = fixture.root.path().join(format!("generated-{profile}"));
    let mox_files = vec![fixture.mox.clone()];
    codegraph::driver::run(run_args(&fixture, profile, &mox_files, &output))
        .await
        .expect("generation must succeed");
    (fixture, output)
}

pub fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}
