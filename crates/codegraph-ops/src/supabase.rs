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
}
