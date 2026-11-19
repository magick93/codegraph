//! The pinned Supabase CLI + image specs (issues #743, #708).
//!
//! `npx supabase` resolves npm `latest`; an unpinned CLI rejected consumer
//! config keys the day npm shipped a breaking release (issue #729, one lane
//! over). Every Supabase CLI spawn in the harness goes through
//! [`npx_supabase_args`], and every "the target is not Supabase-compatible"
//! failure names [`SUPABASE_POSTGRES_IMAGE`] — the one image the repo's own
//! gates run (ifml-gate.yml, entity-gate.yml,
//! `crates/codegraph/tests/test_framework/postgres.rs`) — so a red lane
//! always says exactly what to install.
//!
//! Lockstep: `.github/workflows/workers-gate.yml`'s prefetch names the same
//! version literal, and the node-free pins in
//! `crates/codegraph/tests/workers_live_gate.rs` (workflow side) plus
//! `crates/codegraph-ops/tests/supabase_pin_tests.rs` (harness side) hold
//! the pairing.
//!
//! The Supabase compatibility probe ([`compatibility_gate`], issue #708)
//! lives here too — ONE predicate for every consumer (the `db_integration`
//! band tests, the idempotency twin, and the api suite's shape-B
//! already-up check, issue #709). Under `CODEGRAPH_REQUIRE_SUPABASE=1` a
//! missing capability FAILS naming what is missing and the image; unset,
//! the caller may skip as before.

use crate::db::psql_query;
use crate::pg::PgTarget;

/// The Supabase CLI version the harness and the workers live gate pin
/// together. Bare `npx supabase` resolves npm `latest`; `supabase@3.2.0`
/// is the version the workers live gate validates its fixture against
/// (issue #729: the pinned CLI rejects `[inbucket]`, and the fixture's
/// `[local_smtp]` rename is what gets `start` past config load).
pub const SUPABASE_CLI_VERSION: &str = "3.2.0";

/// The Supabase-flavoured Postgres image the repo's own CI and gates run.
/// `db_integration`'s fail-closed mode (`CODEGRAPH_REQUIRE_SUPABASE`,
/// issue #708) names it in the failure message so a red lane says what to
/// point `DATABASE_URL` at.
pub const SUPABASE_POSTGRES_IMAGE: &str = "public.ecr.aws/supabase/postgres:17.6.1.132";

/// `supabase@{SUPABASE_CLI_VERSION}` — the spec npm resolves.
pub fn pinned_cli_spec() -> String {
    format!("supabase@{SUPABASE_CLI_VERSION}")
}

/// npx argument vector for a pinned Supabase CLI invocation:
/// `["--yes", "supabase@3.2.0", <args…>]`. Feed the result to
/// `run_blocking("supabase", "npx", …)`.
pub fn npx_supabase_args(args: &[&str]) -> Vec<String> {
    let mut argv = vec!["--yes".to_string(), pinned_cli_spec()];
    argv.extend(args.iter().map(|s| (*s).to_string()));
    argv
}

/// Human-readable spelling of the pinned command, for log lines:
/// `npx --yes supabase@3.2.0 start`.
pub fn npx_supabase_display(args: &[&str]) -> String {
    let mut words = vec!["npx".to_string(), "--yes".to_string(), pinned_cli_spec()];
    words.extend(args.iter().map(|s| (*s).to_string()));
    words.join(" ")
}

/// The capabilities the generated platform band depends on, with the SQL
/// probe naming each one. Order is display order for the failure message.
const CAPABILITY_PROBES: &[(&str, &str)] = &[
    (
        "auth schema",
        "EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = 'auth')",
    ),
    (
        "pgmq",
        "EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'pgmq')",
    ),
    (
        "pg_tle",
        "EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'pg_tle')",
    ),
    (
        "http",
        "EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'http')",
    ),
    (
        "uuid-ossp",
        "EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'uuid-ossp')",
    ),
];

/// True when `CODEGRAPH_REQUIRE_SUPABASE` asks the lane to fail closed
/// (`1` / `true`, case-insensitive) instead of skipping on a plain
/// Postgres target (issue #708).
pub fn require_supabase() -> bool {
    require_supabase_value(
        std::env::var("CODEGRAPH_REQUIRE_SUPABASE")
            .unwrap_or_default()
            .as_str(),
    )
}

/// The pure half of [`require_supabase`] — testable without touching the
/// process env.
fn require_supabase_value(value: &str) -> bool {
    value == "1" || value.eq_ignore_ascii_case("true")
}

/// Parse the probe's single aggregate row (`name1,name2` — empty when
/// compatible) into the missing-capability list. An unparseable row fails
/// closed rather than claiming compatibility.
fn missing_capabilities_from_row(row: &str) -> Vec<&'static str> {
    let missing: Vec<&'static str> = row
        .trim()
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .filter_map(|name| {
            CAPABILITY_PROBES
                .iter()
                .find(|(label, _)| *label == name)
                .map(|(label, _)| *label)
        })
        .collect();
    if missing.is_empty() && !row.trim().is_empty() {
        return vec!["probe answer unparseable"];
    }
    missing
}

/// Build the probe SQL: one aggregate row naming every missing capability
/// (empty string when the target serves the band).
fn probe_sql() -> String {
    let arms: Vec<String> = CAPABILITY_PROBES
        .iter()
        .map(|(label, predicate)| format!("SELECT '{label}' AS name WHERE NOT {predicate}"))
        .collect();
    format!(
        "SELECT coalesce(string_agg(name, ','), '') FROM ({} ) missing;",
        arms.join(" UNION ALL ")
    )
}

/// The verdict a caller acts on: run, skip (with the reason to print), or
/// fail (with the reason to panic/abort on).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatibilityGate {
    /// The target serves the full band — run the guarded work.
    Run,
    /// The target is plain Postgres and the env gate is unset — the
    /// caller may skip, printing `reason`.
    Skip { reason: String },
    /// The env gate is set and the target is missing capabilities — the
    /// caller must FAIL with `reason` (naming what is missing and the
    /// image to point at), never silently skip (issue #708).
    Fail { reason: String },
}

/// Probe the target for the Supabase platform-band capabilities and
/// decide run/skip/fail. ONE predicate for every consumer — the
/// `db_integration` band tests, the idempotency twin, and the api suite's
/// shape-B already-up check (issue #709).
///
/// Probe failure (unreachable target) counts as missing — under the env
/// gate that is a Fail, otherwise a Skip, matching the historical
/// `unwrap_or_default` semantics.
pub async fn compatibility_gate(t: &PgTarget) -> CompatibilityGate {
    let missing = probe_missing_capabilities(t).await;
    if missing.is_empty() {
        return CompatibilityGate::Run;
    }
    let list = missing.join(", ");
    if require_supabase() {
        CompatibilityGate::Fail {
            reason: fail_reason(&list),
        }
    } else {
        CompatibilityGate::Skip {
            reason: skip_reason(&list),
        }
    }
}

/// The fail-closed message (issue #708): names what is missing, the image
/// to point at, and the rule.
fn fail_reason(missing: &str) -> String {
    format!(
        "CODEGRAPH_REQUIRE_SUPABASE=1 but the target lacks Supabase \
         capabilities: {missing}. Point DATABASE_URL at \
         {SUPABASE_POSTGRES_IMAGE} (the image the repo's own gates run) — \
         a missing capability in this mode is a lane misconfiguration, \
         never a skip (issue #708)."
    )
}

/// The historical skip message, extended with the escape hatches.
fn skip_reason(missing: &str) -> String {
    format!(
        "target lacks Supabase-compatible auth schema / pgmq / pg_tle / \
         http (missing: {missing}); set CODEGRAPH_REQUIRE_SUPABASE=1 to \
         fail instead of skip, or point DATABASE_URL at \
         {SUPABASE_POSTGRES_IMAGE}"
    )
}

async fn probe_missing_capabilities(t: &PgTarget) -> Vec<&'static str> {
    match psql_query(t, &probe_sql()).await {
        Ok(row) => missing_capabilities_from_row(&row),
        Err(_) => vec!["unreachable"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npx_args_spell_the_pinned_invocation() {
        assert_eq!(
            npx_supabase_args(&["start"]),
            vec![
                "--yes".to_string(),
                "supabase@3.2.0".to_string(),
                "start".to_string()
            ]
        );
        assert_eq!(
            npx_supabase_args(&["db", "reset"]),
            vec![
                "--yes".to_string(),
                "supabase@3.2.0".to_string(),
                "db".to_string(),
                "reset".to_string(),
            ]
        );
    }

    #[test]
    fn display_names_the_pinned_spec() {
        assert_eq!(
            npx_supabase_display(&["start"]),
            "npx --yes supabase@3.2.0 start"
        );
        assert_eq!(pinned_cli_spec(), "supabase@3.2.0");
    }

    #[test]
    fn missing_capabilities_parse_the_probe_row() {
        assert!(missing_capabilities_from_row("").is_empty());
        assert!(missing_capabilities_from_row("\n").is_empty());
        assert_eq!(
            missing_capabilities_from_row("auth schema,pgmq"),
            vec!["auth schema", "pgmq"]
        );
        assert_eq!(
            missing_capabilities_from_row("auth schema"),
            vec!["auth schema"]
        );
    }

    #[test]
    fn an_unparseable_probe_row_fails_closed() {
        assert_eq!(
            missing_capabilities_from_row("something,unexpected"),
            vec!["probe answer unparseable"]
        );
    }

    #[test]
    fn the_probe_sql_names_every_capability_in_one_row() {
        let sql = probe_sql();
        for (label, _) in CAPABILITY_PROBES {
            assert!(sql.contains(label), "probe SQL must probe {label}");
        }
        assert!(sql.contains("string_agg"));
    }

    #[test]
    fn the_failure_message_names_the_image_and_what_is_missing() {
        let reason = fail_reason("auth schema");
        assert!(reason.contains(SUPABASE_POSTGRES_IMAGE));
        assert!(reason.contains("auth schema"));
        assert!(reason.contains("never a skip"));
        let skip = skip_reason("pgmq");
        assert!(skip.contains(SUPABASE_POSTGRES_IMAGE));
        assert!(skip.contains("CODEGRAPH_REQUIRE_SUPABASE=1"));
    }

    #[test]
    fn require_supabase_reads_the_env_gate_value() {
        assert!(require_supabase_value("1"));
        assert!(require_supabase_value("true"));
        assert!(require_supabase_value("TRUE"));
        assert!(!require_supabase_value("0"));
        assert!(!require_supabase_value(""));
        assert!(!require_supabase_value("false"));
    }
}
