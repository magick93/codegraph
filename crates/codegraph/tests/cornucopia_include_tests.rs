//! Issue #781 P1 — cornucopia `?include=` hydration (preview feature
//! `cornucopia_include_hydration`).
//!
//! End-to-end proof that ONE switch flips the whole plane together:
//!
//! * `queries/{domain}/{table}.sql` gains the annotated include query pair
//!   (single + batch),
//! * `cornucopia_repository_impl.rs` gains the inherent fetch methods,
//! * `query.rs` gains the session-owning query-handler wrappers,
//! * the handler hydrates the implemented shape and answers a guarded 501 for
//!   every modelled shape the plane does not implement.
//!
//! Flag OFF is the issue #781 P0 behaviour: no include SQL, no adapter methods,
//! no wrappers, and the handler's guarded 501 — byte-identical to today.
//!
//! The unit KATs for each emitted fragment live beside the emitters
//! (`db::cornucopia_queries`, `ddd::cornucopia_repo`, `api::include_plan`); this
//! test pins that they are WIRED together by the driver.

use std::fs;
use std::path::{Path, PathBuf};

/// `refers X[0..1]` is a scalar FK (the implemented include shape). A bare
/// `refers X` is many-valued in mox, so the optional form is load-bearing.
///
/// `customer.org` is a dot-notation include — the deliberately unsupported
/// shape, so the per-path fail-closed arm is exercised by the SAME handler
/// that hydrates `customer`.
const MODEL_MOX: &str = r#"
package shop

class OrganizationType {
    String name
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
"#;

const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.shop]
label = "Shop"
schema_dir = "shop"
postgres_schema = "shop"

[domains.shop.entity_config.CustomerType]
role = "root"

[domains.shop.entity_config.WorkOrderType]
role = "root"
allow_include = ["customer", "customer.org"]
"#;

/// Two profiles over one model: the preview feature ON and OFF. Everything
/// else is identical, so the flag is the only variable.
const PROFILES_TOML: &str = r#"
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

struct Fixture {
    root: tempfile::TempDir,
    config: PathBuf,
    mox: PathBuf,
    profiles: PathBuf,
}

fn write_fixture() -> Fixture {
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

fn run_args<'a>(
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

async fn generate(profile: &str) -> (Fixture, PathBuf) {
    let fixture = write_fixture();
    let output = fixture.root.path().join(format!("generated-{profile}"));
    let mox_files = vec![fixture.mox.clone()];
    codegraph::driver::run(run_args(&fixture, profile, &mox_files, &output))
        .await
        .expect("generation must succeed");
    (fixture, output)
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Parse-only gate (#551 posture, the cheap half): the emitted Rust must be
/// syntactically valid. Types are NOT checked here — the cornucopia query
/// crate that the adapter calls is produced by `cornucopia` codegen, so the
/// full compile pin needs the stubbed-crate harness (see the design record's
/// "compiled-app/assembly pin" follow-up).
fn assert_parses(path: &Path) {
    let src = read(path);
    if let Err(e) = syn::parse_file(&src) {
        panic!(
            "emitted file does not parse: {}: {e}\n{src}",
            path.display()
        );
    }
}

#[tokio::test]
async fn cornucopia_include_hydration_plane_is_wired_end_to_end() {
    let (_fixture, output) = generate("cornucopia-includes").await;

    // ── 1. SQL: the annotated include query pair ────────────────────────
    let sql = read(&output.join("queries/shop/work_order.sql"));
    assert!(
        sql.contains("--! include_customer_for_work_order (source_id) : ("),
        "single include query missing:\n{sql}"
    );
    assert!(
        sql.contains("--! include_customer_batch_for_work_order (source_ids) : (source_id,"),
        "batch include query missing:\n{sql}"
    );
    assert!(
        sql.contains(
            "WHERE \"id\" = (SELECT \"customer_id\" FROM \"shop\".\"work_order\" WHERE \"id\" = :source_id)"
        ),
        "the FK must be read through a correlated subquery on the source:\n{sql}"
    );
    assert!(
        sql.contains("WHERE \"id\" = ANY(:source_ids::uuid[])"),
        "the batch query must key on the requested source ids:\n{sql}"
    );
    // The soft-delete marker is filtered only when the target's tree carries
    // one (a SoftDelete policy). This fixture has none — the same condition
    // `get_{entity}` / `list_{entity}` filter on — so the include must NOT
    // invent a marker the table may not have. The KAT in
    // `db::cornucopia_queries::include_query_tests` covers the marker arm.
    assert!(
        !sql.contains("deleted_at"),
        "no soft-delete marker without a policy:\n{sql}"
    );

    // ── 2. Adapter: inherent fetch methods on the source repository ─────
    let adapter = read(&output.join("src/domain/shop/work_order/cornucopia_repository_impl.rs"));
    assert!(
        adapter.contains("impl WorkOrderRepositoryImpl {"),
        "include helpers must be inherent methods (the trait stays CRUD-only):\n{adapter}"
    );
    assert!(
        adapter.contains("pub async fn fetch_customer_for_work_order<C: GenericClient + Sync>("),
        "GET adapter method missing:\n{adapter}"
    );
    assert!(
        adapter
            .contains("pub async fn fetch_customer_batch_for_work_order<C: GenericClient + Sync>("),
        "LIST adapter method missing:\n{adapter}"
    );
    assert!(
        adapter.contains("work_order::include_customer_for_work_order().bind(db, &source_id)"),
        "the adapter must call the generated query:\n{adapter}"
    );
    assert!(
        adapter.contains("result.insert(row.source_id, Some(resp));"),
        "the batch map must key by source id:\n{adapter}"
    );
    assert!(
        adapter.contains("use crate::domain::shop::customer::dto_response::CustomerResponse;"),
        "the target response DTO import is missing:\n{adapter}"
    );
    assert!(
        adapter.contains("use cornucopia_queries::queries::shop::customer;"),
        "the target query module import is missing (child hydration):\n{adapter}"
    );

    // ── 3. Query handler: the session-owning wrappers ───────────────────
    let query = read(&output.join("src/domain/shop/work_order/query.rs"));
    assert!(
        query.contains("pub async fn fetch_customer_for_work_order(&self, source_id: Uuid"),
        "GET query wrapper missing:\n{query}"
    );
    assert!(
        query.contains(
            "pub async fn fetch_customer_batch_for_work_order(&self, source_ids: &[Uuid]"
        ),
        "LIST query wrapper missing:\n{query}"
    );
    assert!(
        query.contains(
            "set_rls_session_vars(&tx, api_key_id, organization_id, user_id, &role).await?;"
        ),
        "the wrapper must install the RLS session vars:\n{query}"
    );
    assert!(
        query
            .contains("let result = self.repo.fetch_customer_for_work_order(&tx, source_id).await"),
        "the wrapper must call the adapter on its transaction:\n{query}"
    );

    // ── 4. Handler: hydrate the scalar, 501 the array ───────────────────
    let handler = read(&output.join("src/api/shop/work_order_handler.rs"));
    assert!(
        handler.contains(
            "included.customer = state.shop_work_order_queries.fetch_customer_for_work_order(id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await"
        ),
        "GET handler must hydrate through the query layer:\n{handler}"
    );
    assert!(
        handler.contains(
            "state.shop_work_order_queries.fetch_customer_batch_for_work_order(&ids, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await"
        ),
        "LIST handler must batch-hydrate through the query layer:\n{handler}"
    );

    // The dot-notation path in the SAME handler is an unsupported shape: the
    // plane refuses it per path, while `customer` hydrates above.
    assert!(
        handler.contains("\"customer.org\" => Some(\"dot-notation"),
        "the unsupported shape must be named in the per-path guard:\n{handler}"
    );
    assert!(
        handler.contains("status: axum::http::StatusCode::NOT_IMPLEMENTED,"),
        "an unsupported shape must fail closed:\n{handler}"
    );
    assert!(
        !handler.contains("state.shop_work_order_queries.fetch_customer_org_for_work_order"),
        "no hydration call may be emitted for an unsupported shape:\n{handler}"
    );

    // The reverse relationship (`customer?include=work_order`) is the array
    // shape — refused with its own construct name, and NO adapter/query/wrapper
    // surface is emitted for it.
    let customer_handler = read(&output.join("src/api/shop/customer_handler.rs"));
    assert!(
        customer_handler.contains("\"work_order\" => Some(\"array / reverse-FK"),
        "the array shape must be named in its guard:\n{customer_handler}"
    );
    let customer_adapter =
        read(&output.join("src/domain/shop/customer/cornucopia_repository_impl.rs"));
    assert!(
        !customer_adapter.contains("fetch_work_order_for_customer"),
        "no adapter method may exist for an unsupported shape:\n{customer_adapter}"
    );
    let customer_query = read(&output.join("src/domain/shop/customer/query.rs"));
    assert!(
        !customer_query.contains("fetch_work_order_for_customer"),
        "no query wrapper may exist for an unsupported shape:\n{customer_query}"
    );

    // ── 5. Parse gate: every emitted file is syntactically valid Rust ────
    for rel in [
        "src/api/shop/work_order_handler.rs",
        "src/domain/shop/work_order/query.rs",
        "src/domain/shop/work_order/cornucopia_repository_impl.rs",
        "src/api/shop/customer_handler.rs",
        "src/domain/shop/customer/query.rs",
        "src/domain/shop/customer/cornucopia_repository_impl.rs",
    ] {
        assert_parses(&output.join(rel));
    }
}

#[tokio::test]
async fn cornucopia_include_hydration_flag_off_is_the_p0_surface() {
    let (_fixture, output) = generate("cornucopia-off").await;

    let sql = read(&output.join("queries/shop/work_order.sql"));
    assert!(
        !sql.contains("include_customer"),
        "flag off must emit no include SQL:\n{sql}"
    );

    let adapter = read(&output.join("src/domain/shop/work_order/cornucopia_repository_impl.rs"));
    assert!(
        !adapter.contains("fetch_customer_for_work_order"),
        "flag off must emit no include adapter methods:\n{adapter}"
    );

    let query = read(&output.join("src/domain/shop/work_order/query.rs"));
    assert!(
        !query.contains("fetch_customer_for_work_order"),
        "flag off must emit no include query wrappers:\n{query}"
    );

    // Issue #781 P0: every include request answers the guarded 501.
    let handler = read(&output.join("src/api/shop/work_order_handler.rs"));
    assert!(
        handler.contains("if !include_paths.is_empty()"),
        "flag off keeps the guarded 501:\n{handler}"
    );
    assert!(
        handler.contains("status: axum::http::StatusCode::NOT_IMPLEMENTED,"),
        "flag off keeps the 501 status:\n{handler}"
    );
    assert!(
        !handler.contains("fetch_customer_for_work_order"),
        "flag off must not call a hydration method:\n{handler}"
    );
}
