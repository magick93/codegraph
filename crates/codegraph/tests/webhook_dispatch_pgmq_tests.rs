//! The pgmq statements the webhook dispatchers emit (issue #825).
//!
//! pgmq resolves `delete` by parameter type: it ships BOTH
//! `delete(text, bigint)` and `delete(text, bigint[])` (pgmq 1.5.1), so an
//! untyped `pgmq.delete($1, $2)` is ambiguous the moment the SERVER has to
//! infer the parameter types. That is exactly what the workers
//! dispatcher's driver does — `tokio-postgres`' `client.execute(sql,
//! &[…])` prepares with no parameter types, both parameters arrive as
//! `unknown`, and Postgres refuses the call:
//!
//! ```text
//! ERROR:  function pgmq.delete(unknown, unknown) is not unique
//! HINT:  Could not choose a best candidate function. You might need to add explicit type casts.
//! ```
//!
//! Every dead-letter move and every processed-message delete on a workers
//! consumer failed (962 refusals in the reporting consumer's Postgres
//! log): nothing was ever removed, so each drain replayed what it had
//! already processed. The monolith drain stayed healthy on the SAME text —
//! SeaORM/sqlx declares each parameter's type from the bound value, so the
//! call resolves before the server has to guess.
//!
//! No upstream gate saw it because the events runtime gate drives the
//! MONOLITH dispatcher (the typed binding) against a stand-in whose
//! `pgmq.delete` had ONE overload — faithful in behaviour, not in overload
//! set. Both gaps close here:
//!
//! 1. **Node-free (PR CI)** — every `pgmq.delete` in both dispatch
//!    templates carries the explicit casts, and the statement extractor
//!    reads Rust string continuations and escapes the way the compiler
//!    does.
//! 2. **The workers leg, replayed (the ops lane)** — every pgmq statement
//!    of the rendered workers dispatcher PREPARES under tokio-postgres'
//!    untyped binding against the shared stand-in (`test_framework::pgmq`,
//!    which mirrors the overload pair), and the processed-message delete —
//!    the emitted text, bound exactly as the dispatcher binds it — removes
//!    the row.
//!
//! ```bash
//! cargo test -p codegraph --test webhook_dispatch_pgmq_tests -- --ignored --nocapture
//! ```

use std::collections::BTreeSet;
use std::path::Path;

use codegraph::generate::ProjectConfig;
use test_framework::postgres::{GateDb, resolve_base_target};

// The shared pgmq stand-in (issue #825): one overload set for every gate.
#[path = "test_framework/mod.rs"]
mod test_framework;

/// The typed spelling every `pgmq.delete` call site carries: the casts the
/// server's own hint asks for, so function resolution no longer depends
/// on the client's binding discipline.
const TYPED_DELETE: &str = "SELECT pgmq.delete($1::text, $2::bigint)";

/// Render a dispatch template with the events plane ON — the shape every
/// generated dispatcher carries today, and the only one that renders the
/// `dead_letter` twin of the delete. `project` is injected exactly as
/// `render_template_with_project` injects it.
fn render(template: &str, extra: serde_json::Value) -> String {
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = codegraph::generate::template_engine::create_tera(&template_dir)
        .unwrap_or_else(|e| panic!("load templates: {e}"));
    let mut project = ProjectConfig::default();
    project.codegen.event_idempotency = true;
    project.codegen.event_archive = true;
    let mut ctx = extra;
    ctx.as_object_mut().expect("a JSON object context").insert(
        "project".to_string(),
        serde_json::to_value(&project).unwrap(),
    );
    let ctx = tera::Context::from_serialize(&ctx).expect("tera context");
    tera.render(template, &ctx)
        .unwrap_or_else(|e| panic!("render {template} failed: {e}"))
}

fn monolith_dispatcher() -> String {
    render(
        "webhook/dispatch.tera",
        serde_json::json!({ "evt_events": true }),
    )
}

fn workers_dispatcher() -> String {
    render(
        "webhook/dispatch_worker.tera",
        serde_json::json!({
            "name": "orders",
            "queue_max_retries": 5,
            "has_events": true,
            "extra_event_queues": [],
        }),
    )
}

/// Every pgmq statement in rendered Rust source: the string literals that
/// contain a `pgmq.` call. Comments are skipped (a doc comment may quote a
/// statement nobody executes); `\"`, `\\`, and the backslash-newline
/// continuation (which also swallows the next line's leading whitespace)
/// are read the way the compiler reads them.
fn pgmq_statements(rendered: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let (mut in_string, mut in_char, mut skip_leading_ws) = (false, false, false);
    for line in rendered.lines() {
        let mut chars = line.chars().peekable();
        if in_string && skip_leading_ws {
            while matches!(chars.peek(), Some(' ' | '\t')) {
                chars.next();
            }
            skip_leading_ws = false;
        }
        while let Some(ch) = chars.next() {
            if in_char {
                // A character literal: its content is never a string.
                if ch == '\\' {
                    chars.next();
                } else if ch == '\'' {
                    in_char = false;
                }
                continue;
            }
            if !in_string && ch == '/' && chars.peek() == Some(&'/') {
                // A line comment: nothing after it on this line is code.
                break;
            }
            if in_string && ch == '\\' {
                match chars.next() {
                    Some('"') => current.push('"'),
                    Some('\\') => current.push('\\'),
                    // Any other escape is passed through verbatim.
                    Some(other) => {
                        current.push('\\');
                        current.push(other);
                    }
                    // A backslash at end of line continues the literal.
                    None => skip_leading_ws = true,
                }
                continue;
            }
            if ch == '"' {
                if in_string {
                    in_string = false;
                    if current.contains("pgmq.") {
                        statements.push(std::mem::take(&mut current));
                    } else {
                        current.clear();
                    }
                } else {
                    in_string = true;
                }
                continue;
            }
            if !in_string && ch == '\'' {
                in_char = true;
                continue;
            }
            if in_string {
                current.push(ch);
            }
        }
    }
    statements
}

fn pgmq_deletes(statements: &[String]) -> Vec<&str> {
    statements
        .iter()
        .map(String::as_str)
        .filter(|statement| statement.contains("pgmq.delete("))
        .collect()
}

// ── 1. Node-free: the delete sites are typed in both templates ────────────

/// Every `pgmq.delete` in both dispatch templates is the typed spelling.
/// An untyped `$1, $2` resolves only when the client declares its
/// parameter types — true of the monolith drain (sqlx), false of the
/// workers drain (tokio-postgres), which is why the monolith twin is
/// pinned too: one spelling, so neither leg can drift back.
#[test]
fn every_pgmq_delete_in_the_dispatch_templates_is_typed() {
    for (template, dispatcher) in [
        ("webhook/dispatch.tera", monolith_dispatcher()),
        ("webhook/dispatch_worker.tera", workers_dispatcher()),
    ] {
        let statements = pgmq_statements(&dispatcher);
        let deletes = pgmq_deletes(&statements);
        assert!(
            deletes.len() >= 2,
            "{template} must carry both delete sites (the drain and the dead-letter move); found {deletes:?}"
        );
        for statement in &deletes {
            assert_eq!(
                *statement, TYPED_DELETE,
                "{template}: an untyped pgmq.delete is ambiguous — pgmq ships delete(text, bigint) AND delete(text, bigint[]), and the workers drain binds no parameter types"
            );
        }
    }
}

/// The workers dispatcher addresses exactly the pgmq surface this pin
/// exercises, so the runtime leg below cannot silently go stale on a
/// template edit that renames or drops a call.
#[test]
fn the_workers_dispatcher_pgmq_surface_is_pinned() {
    let surface: BTreeSet<String> = pgmq_statements(&workers_dispatcher()).into_iter().collect();
    let expected: BTreeSet<String> = [
        "SELECT msg_id, read_ct, message FROM pgmq.read($1, 30, 100)",
        "SELECT pgmq.create($1) WHERE NOT EXISTS (SELECT 1 FROM pgmq.list_queues() WHERE queue_name = $1)",
        "SELECT pgmq.send($1, $2)",
        TYPED_DELETE,
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    let unexpected: Vec<&String> = surface
        .iter()
        .filter(|statement| !expected.contains(*statement))
        .collect();
    let missing: Vec<&String> = expected
        .iter()
        .filter(|statement| !surface.contains(*statement))
        .collect();
    assert!(
        unexpected.is_empty() && missing.is_empty(),
        "the workers dispatcher's pgmq statements moved: unexpected {unexpected:?}, missing {missing:?}"
    );
}

/// The extractor reads what the compiler reads: a backslash-newline
/// continuation joins the lines without the indentation, `\"` is a quote
/// inside the SQL, a char literal never opens a string, and a doc comment
/// quoting a statement contributes nothing.
#[test]
fn the_extractor_reads_continuations_escapes_and_comments() {
    let source = r#"
/// Drain: `"SELECT pgmq.delete($1, $2)"` lives in a comment.
const MARKER: char = '"';
let create = "SELECT pgmq.create($1) WHERE NOT EXISTS (SELECT 1 \
              FROM pgmq.list_queues() WHERE queue_name = $1)";
let quoted = "SELECT pgmq.send($1, '{\"event\": \"x\"}'::jsonb)";
let other = "SELECT 1";
let read = "SELECT msg_id FROM pgmq.read($1, 30, 100)";
"#;
    assert_eq!(
        pgmq_statements(source),
        vec![
            "SELECT pgmq.create($1) WHERE NOT EXISTS (SELECT 1 FROM pgmq.list_queues() WHERE queue_name = $1)",
            "SELECT pgmq.send($1, '{\"event\": \"x\"}'::jsonb)",
            "SELECT msg_id FROM pgmq.read($1, 30, 100)",
        ]
    );
}

// ── 2. The workers leg, replayed against the shared stand-in ──────────────

/// The workers dispatcher's pgmq statements, under the binding discipline
/// that broke them.
///
/// 1. Every statement PREPARES with no declared parameter types (the
///    `client.execute(sql, &[…])` path) against the shared stand-in, which
///    mirrors pgmq's two `delete` overloads.
/// 2. The processed-message delete — the emitted text, bound exactly as
///    `delete_pgmq_msg` binds it (`&queue`, `&msg_id`) — removes the row.
#[tokio::test]
#[ignore = "needs Postgres (DATABASE_URL) + the per-run database provisioning; the test-ops-integration lane"]
async fn workers_dispatcher_pgmq_statements_resolve_under_untyped_binding() {
    let statements = pgmq_statements(&workers_dispatcher());
    let deletes = pgmq_deletes(&statements);
    assert!(
        deletes.len() == 2,
        "the workers dispatcher carries exactly the drain delete and the dead-letter move: {deletes:?}"
    );

    let base = resolve_base_target()
        .await
        .expect("resolve a Postgres base target (DATABASE_URL or the docker fallback)");
    let db = GateDb::create(base)
        .await
        .expect("create the per-run database");
    let url = db.url();

    let result = replay_under_untyped_binding(&url, &statements).await;
    db.cleanup().await;
    result.expect("the workers dispatcher's pgmq statements resolve and delete");
}

async fn replay_under_untyped_binding(
    url: &str,
    statements: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await?;
    tokio::spawn(async move {
        // The statements carry their own error surface; the driver task
        // only completes with an error once the client is dropped.
        let _ = connection.await;
    });
    client
        .batch_execute(test_framework::pgmq::PGMQ_FUNCTIONAL_SHIM)
        .await?;

    // The stand-in is faithful where it matters: the overload pair.
    let overloads: i64 = client
        .query_one(
            "SELECT count(*) FROM pg_proc p \
             JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE n.nspname = 'pgmq' AND p.proname = 'delete'",
            &[],
        )
        .await?
        .get(0);
    assert_eq!(
        overloads, 2,
        "the stand-in must mirror pgmq's two delete overloads (issue #825)"
    );

    // 1. Each distinct statement prepares with NO declared parameter types.
    let mut seen = BTreeSet::new();
    for statement in statements {
        if !seen.insert(statement.as_str()) {
            continue;
        }
        client.prepare(statement).await.unwrap_or_else(|e| {
            panic!(
                "`{statement}` does not resolve under untyped binding: {}",
                describe(&e)
            )
        });
    }

    // 2. The processed-message delete, bound as the dispatcher binds it.
    //    The queue and the probe envelope are seeded with literals: only
    //    the delete is under test.
    client
        .batch_execute(
            "SELECT pgmq.create('events_probe'); \
             SELECT pgmq.send('events_probe', '{\"event\":\"Probe\"}'::jsonb);",
        )
        .await?;
    let msg_id: i64 = client
        .query_one("SELECT msg_id FROM pgmq.q_events_probe LIMIT 1", &[])
        .await?
        .get(0);
    let queue = "events_probe".to_string();
    let delete = statements
        .iter()
        .rev()
        .find(|statement| statement.contains("pgmq.delete("))
        .expect("the processed-message delete");
    client
        .execute(delete.as_str(), &[&queue, &msg_id])
        .await
        .unwrap_or_else(|e| {
            panic!(
                "the processed-message delete `{delete}` failed as in production: {}",
                describe(&e)
            )
        });
    let left: i64 = client
        .query_one("SELECT count(*) FROM pgmq.q_events_probe", &[])
        .await?
        .get(0);
    assert_eq!(
        left, 0,
        "the processed message must be removed from its queue"
    );
    Ok(())
}

/// A tokio-postgres error with its server detail. `Display` renders only
/// the error KIND (`db error` — the `DbError` sits in `source()`), which
/// is how this defect read in every consumer log; the pin names the
/// SQLSTATE and the server message instead.
fn describe(error: &tokio_postgres::Error) -> String {
    let mut text = error.to_string();
    if let Some(db) = error.as_db_error() {
        text.push_str(&format!(" ({}: {})", db.code().code(), db.message()));
    }
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        text.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    text
}
