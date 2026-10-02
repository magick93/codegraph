//! Contract tests for hr-specs template improvements upstreamed into the
//! built-in templates (upstream-override parity):
//!
//! 1. `ui/scaffold/structured_wrapper_field.tera` — sub-field dedup via
//!    `Map` + `for=`/`id=` a11y pairing on array sub-fields.
//! 2. `ui/scaffold/test_helpers.tera` — `safeCleanup` bulk teardown helper.
//! 3. `webhook/api_endpoints.tera` — `last_delivery` preview via
//!    `LEFT JOIN LATERAL` + `DeliverySummary` serialization.
//! 4. `webhook/dispatch.tera` — dispatcher takes a `CancellationToken`,
//!    selects on cancellation, and drains one final batch before exiting.

use codegraph::generate::{ProjectConfig, template_engine};
use std::path::Path;

fn test_tera() -> tera::Tera {
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    template_engine::create_tera(&template_dir).unwrap()
}

fn project_context() -> tera::Context {
    tera::Context::from_serialize(serde_json::json!({
        "project": ProjectConfig::default(),
    }))
    .unwrap()
}

fn render(template: &str) -> String {
    test_tera()
        .render(template, &project_context())
        .unwrap_or_else(|e| panic!("render {template} failed: {e}"))
}

// ── 1. structured_wrapper_field.tera ─────────────────────────────────────────

#[test]
fn structured_wrapper_field_dedupes_subfields_via_map() {
    let out = render("ui/scaffold/structured_wrapper_field.tera");
    assert!(
        out.contains("new Map(subFields"),
        "visible/hidden split should dedupe sub-fields via Map: {out}"
    );
    // Both the visible and hidden branches are deduped.
    assert_eq!(
        out.matches("new Map(subFields").count(),
        2,
        "both visibleByDefault and hiddenFields should be deduped"
    );
}

#[test]
fn structured_wrapper_field_pairs_label_for_with_input_id() {
    let out = render("ui/scaffold/structured_wrapper_field.tera");
    // Array sub-fields: label for= matches input id= (with row idx).
    let for_count = out.matches("for=\"{fieldName}-{idx}-{sf.name}\"").count();
    // `data-testid=` contains `id=` as a substring — subtract those.
    let raw_id_count = out.matches("id=\"{fieldName}-{idx}-{sf.name}\"").count();
    let testid_count = out
        .matches("data-testid=\"{fieldName}-{idx}-{sf.name}\"")
        .count();
    let id_count = raw_id_count - testid_count;
    assert!(for_count >= 2, "array labels should carry for= attributes");
    assert!(id_count >= 2, "array inputs should carry id= attributes");
    assert_eq!(
        for_count, id_count,
        "every array sub-field label for= should pair with an input id="
    );

    // Scalar sub-fields keep their (idx-less) pairing too.
    let scalar_for = out.matches("for=\"{fieldName}-{sf.name}\"").count();
    let scalar_id = out.matches("id=\"{fieldName}-{sf.name}\"").count()
        - out.matches("data-testid=\"{fieldName}-{sf.name}\"").count();
    assert_eq!(
        scalar_for, scalar_id,
        "scalar sub-field for=/id= pairing should match"
    );
}

// ── 2. test_helpers.tera ─────────────────────────────────────────────────────

#[test]
fn test_helpers_exports_safe_cleanup() {
    let out = render("ui/scaffold/test_helpers.tera");
    assert!(
        out.contains("export async function safeCleanup("),
        "test_helpers should export safeCleanup: {out}"
    );
    assert!(
        out.contains("created: { path: string; id: string }[]"),
        "safeCleanup should accept a path/id pair list"
    );
    // Best-effort: per-item try/catch with a console.warn on failure.
    assert!(out.contains("console.warn"), "cleanup failures should warn");
}

#[test]
fn test_helpers_exports_create_entity_via_acme_api() {
    let out = render("ui/scaffold/test_helpers.tera");
    assert!(
        out.contains("export async function createEntityViaAcmeApi("),
        "test_helpers should export createEntityViaAcmeApi"
    );
    // It only composes the exported createEntityViaApi helper.
    assert!(
        out.contains("return createEntityViaApi(path, data);"),
        "createEntityViaAcmeApi should delegate to createEntityViaApi"
    );
}

// ── 3. webhook/api_endpoints.tera ────────────────────────────────────────────

#[test]
fn api_endpoints_response_carries_last_delivery_preview() {
    let out = render("webhook/api_endpoints.tera");
    assert!(
        out.contains("pub struct DeliverySummary"),
        "should define DeliverySummary"
    );
    for field in [
        "pub id: Uuid,",
        "pub response_status: Option<i32>,",
        "pub response_body: Option<String>,",
        "pub attempt: i32,",
        "pub delivered_at:",
        "pub created_at:",
    ] {
        assert!(out.contains(field), "DeliverySummary should carry {field}");
    }
    assert!(
        out.contains("pub last_delivery: Option<DeliverySummary>,"),
        "WebhookEndpointResponse should carry last_delivery"
    );
    // hr-specs-specific field must NOT be upstreamed into the response shape.
    assert!(
        !out.contains("pub hmac_secret: Option<String>"),
        "hmac_secret must not be exposed on the endpoint response"
    );
}

#[test]
fn api_endpoints_list_and_get_use_lateral_join_for_last_delivery() {
    let out = render("webhook/api_endpoints.tera");
    assert_eq!(
        out.matches("LEFT JOIN LATERAL").count(),
        2,
        "list + get-by-id handlers should each join the latest delivery"
    );
    assert_eq!(
        out.matches("ORDER BY created_at DESC\n                LIMIT 1")
            .count(),
        2,
        "the LATERAL subquery should pick the single latest delivery"
    );
    // Both handlers map the joined row into the response.
    assert_eq!(
        out.matches("last_delivery: match r.try_get_by_index::<Option<Uuid>>(7)")
            .count(),
        2,
        "list + get handlers should hydrate last_delivery from column 7"
    );
    // create/update construct (or delegate with) a preview-less response.
    assert!(
        out.contains("last_delivery: None,"),
        "create should return last_delivery: None"
    );
}

// ── 4. webhook/dispatch.tera ─────────────────────────────────────────────────

#[test]
fn dispatch_run_takes_shutdown_token_and_selects_on_cancellation() {
    let out = render("webhook/dispatch.tera");
    assert!(
        out.contains("use tokio_util::sync::CancellationToken;"),
        "should import CancellationToken"
    );
    assert!(
        out.contains("pub async fn run(&self, shutdown: CancellationToken)"),
        "run should accept a shutdown token: {out}"
    );
    assert!(
        out.contains("tokio::select!"),
        "run should select between the ticker and shutdown"
    );
    assert!(
        out.contains("_ = shutdown.cancelled() =>"),
        "run should watch shutdown.cancelled()"
    );
}

#[test]
fn dispatch_drains_final_batch_on_cancellation() {
    let out = render("webhook/dispatch.tera");
    let cancelled_branch = out
        .split("_ = shutdown.cancelled() =>")
        .nth(1)
        .expect("cancelled branch");
    assert!(
        cancelled_branch.contains("self.dispatch_pending()"),
        "cancellation should drain one final batch of pending messages"
    );
    // Builtin delete/retry semantics stay intact below run().
    assert!(
        out.contains("all_inserts_succeeded"),
        "delete-only-when-inserts-succeeded semantics must be preserved"
    );
    assert!(
        out.contains("SELECT pgmq.delete($1, $2)"),
        "pgmq delete on processed messages must be preserved"
    );
}
