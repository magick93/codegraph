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

class InvoiceType {
    String number
    refers CustomerType[1] customer
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

# `root` + NO explicit `allow_include` ⇒ auto-discovery, which is the only
# resolver path that turns a child's scalar reference into the reverse-FK
# (array) shape. `WorkOrderType.customer` is optional ⇒ optional group key;
# `InvoiceType.customer` is required ⇒ plain group key.
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

    // ── 4. Handler: hydrate the scalar, 501 the dot path ────────────────
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

    // ── 5. Reverse-FK (array) shape: SQL, adapter, wrapper, handler ──────
    //
    // `customer?include=work_order` (optional FK) and `?include=invoice`
    // (required FK) are one-to-many: the target rows are found by the
    // TARGET's own FK column, never by a source subquery or a JOIN.
    let customer_sql = read(&output.join("queries/shop/customer.sql"));
    assert!(
        customer_sql.contains("--! include_work_order_for_customer (source_id) : ("),
        "array single query missing:\n{customer_sql}"
    );
    assert!(
        customer_sql.contains(
            "SELECT \"id\", \"customer_id\", \"reference\", \"total\", \"created_at\", \"updated_at\"\nFROM \"shop\".\"work_order\"\nWHERE \"customer_id\" = :source_id;"
        ),
        "the single array query must scan the target's reverse FK:\n{customer_sql}"
    );
    assert!(
        customer_sql.contains(
            "--! include_work_order_batch_for_customer (source_ids) : (source_id?, id, customer_id?, reference, total, created_at, updated_at)"
        ),
        "array batch query missing (optional reverse FK ⇒ `source_id?`):\n{customer_sql}"
    );
    assert!(
        customer_sql.contains(
            "SELECT \"customer_id\" AS \"source_id\", \"id\", \"customer_id\", \"reference\", \"total\", \"created_at\", \"updated_at\"\nFROM \"shop\".\"work_order\"\nWHERE \"customer_id\" = ANY(:source_ids::uuid[]);"
        ),
        "the batch array query must key by the reverse FK and cast uuid[]:\n{customer_sql}"
    );
    assert!(
        customer_sql.contains("--! include_invoice_for_customer (source_id) : ("),
        "the second reverse include (required FK) must emit too:\n{customer_sql}"
    );
    assert!(
        customer_sql.contains(
            "--! include_invoice_batch_for_customer (source_ids) : (source_id, id, customer_id, number, total, created_at, updated_at)"
        ),
        "a required reverse FK hints a non-nullable key (`source_id`):\n{customer_sql}"
    );
    assert!(
        !customer_sql.contains("SELECT \"customer_id\" FROM \"shop\".\"customer\""),
        "the array shape opens no source subquery:\n{customer_sql}"
    );

    let customer_adapter =
        read(&output.join("src/domain/shop/customer/cornucopia_repository_impl.rs"));
    assert!(
        customer_adapter
            .contains(") -> Result<Vec<WorkOrderResponse>, Box<dyn std::error::Error>> {"),
        "array GET adapter must return the collection:\n{customer_adapter}"
    );
    assert!(
        customer_adapter
            .contains("let mut out: Vec<WorkOrderResponse> = Vec::with_capacity(rows.len());"),
        "array GET adapter must push each row:\n{customer_adapter}"
    );
    assert!(
        customer_adapter
            .contains("customer::include_work_order_for_customer().bind(db, &source_id)"),
        "the array adapter must call the generated query:\n{customer_adapter}"
    );
    assert!(
        customer_adapter.contains(
            ") -> Result<std::collections::HashMap<Uuid, Vec<WorkOrderResponse>>, Box<dyn std::error::Error>> {"
        ),
        "array LIST adapter must group into a Vec map:\n{customer_adapter}"
    );
    assert!(
        customer_adapter.contains("result.entry(*id).or_insert_with(Vec::new);"),
        "the array batch must seed every requested source id:\n{customer_adapter}"
    );
    // Optional reverse FK ⇒ Option match with a skip arm…
    assert!(
        customer_adapter.contains("let key = match row.source_id {"),
        "the optional reverse FK must match the Option:\n{customer_adapter}"
    );
    assert!(
        customer_adapter.contains("None => continue,"),
        "the optional reverse FK skips a NULL key:\n{customer_adapter}"
    );
    assert!(
        customer_adapter.contains("result.entry(key).or_default().push(resp);"),
        "the array batch pushes under the source key:\n{customer_adapter}"
    );
    // …required reverse FK ⇒ a plain Uuid key.
    assert!(
        customer_adapter.contains("let key = row.source_id;"),
        "the required reverse FK (invoice) groups on a plain Uuid:\n{customer_adapter}"
    );

    let customer_query = read(&output.join("src/domain/shop/customer/query.rs"));
    assert!(
        customer_query.contains(") -> Result<Vec<WorkOrderResponse>, ShopError> {"),
        "array GET wrapper must declare the collection return:\n{customer_query}"
    );
    assert!(
        customer_query.contains(
            ") -> Result<std::collections::HashMap<Uuid, Vec<WorkOrderResponse>>, ShopError> {"
        ),
        "array LIST wrapper must declare the grouped return:\n{customer_query}"
    );
    assert!(
        customer_query
            .contains("let result = self.repo.fetch_work_order_for_customer(&tx, source_id).await"),
        "the array wrapper must call the adapter on its transaction:\n{customer_query}"
    );

    let customer_handler = read(&output.join("src/api/shop/customer_handler.rs"));
    // The GET arm wraps the collection in `Some(…)` — the DTO field is
    // `Option<Vec<…>>` and an empty vec is a present value.
    assert!(
        customer_handler.contains(
            "included.work_order = Some(state.shop_customer_queries.fetch_work_order_for_customer(id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await"
        ),
        "array GET handler must wrap the collection in Some:\n{customer_handler}"
    );
    assert!(
        customer_handler.contains("                    })?);"),
        "the Some wrapper must close with `)?);`:\n{customer_handler}"
    );
    assert!(
        customer_handler.contains(
            "included.invoice = Some(state.shop_customer_queries.fetch_invoice_for_customer(id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await"
        ),
        "the required-FK reverse include must hydrate too:\n{customer_handler}"
    );
    // The LIST arm serialises the SAME batch map shape.
    assert!(
        customer_handler.contains(
            "serde_json::to_value(state.shop_customer_queries.fetch_work_order_batch_for_customer(&ids, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await"
        ),
        "array LIST handler must batch-hydrate:\n{customer_handler}"
    );
    // No per-path 501 remains for the array shape.
    assert!(
        !customer_handler.contains("array / reverse-FK"),
        "the array shape must no longer answer 501:\n{customer_handler}"
    );

    // ── 6. Parse gate: every emitted file is syntactically valid Rust ────
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

    // The reverse-FK plane is part of the SAME switch: no array SQL, no array
    // adapter, no array wrapper, and the same guarded 501 on the customer
    // handler.
    let customer_sql = read(&output.join("queries/shop/customer.sql"));
    assert!(
        !customer_sql.contains("include_work_order"),
        "flag off must emit no array include SQL:\n{customer_sql}"
    );
    let customer_adapter =
        read(&output.join("src/domain/shop/customer/cornucopia_repository_impl.rs"));
    assert!(
        !customer_adapter.contains("fetch_work_order_for_customer"),
        "flag off must emit no array adapter methods:\n{customer_adapter}"
    );
    let customer_query = read(&output.join("src/domain/shop/customer/query.rs"));
    assert!(
        !customer_query.contains("fetch_work_order_for_customer"),
        "flag off must emit no array query wrappers:\n{customer_query}"
    );
    let customer_handler = read(&output.join("src/api/shop/customer_handler.rs"));
    assert!(
        customer_handler.contains("if !include_paths.is_empty()"),
        "flag off keeps the guarded 501 on the reverse side:\n{customer_handler}"
    );
    assert!(
        !customer_handler.contains("included.work_order"),
        "flag off must not hydrate the reverse collection:\n{customer_handler}"
    );
}
