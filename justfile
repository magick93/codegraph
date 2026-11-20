# ci-local (issue #628) — one command mirroring the CI matrix
# (.github/workflows/ci.yml): lint, workspace tests, audit, ops lane.
#
# Pairing: the root rust-toolchain.toml pin (#627) supplies the compiler
# parity — this justfile supplies the command parity. The drift contract is
# enforced by crates/codegraph/tests/ci_local_contract.rs: every non-exempt
# `run:` step in ci.yml must appear here (runner-only steps — sudo/curl
# installs and version probes — are exempt; their work is replaced by the
# WARN + skip guards below).
#
# Per-gate degradation: a gate whose tooling is absent (psql/Postgres,
# hurl, cargo-audit, wasm32 target) prints a WARN line and SKIPS (exit 0)
# so the full matrix completes on any machine; a gate whose tooling is
# present and that FAILS fails the run — exactly like CI, modulo the
# runner-only steps.
#
# Usage:
#   just ci-local    (or just)       # full matrix
#   just lint | test | audit | ops   # one leg
#   just ops-db-integration          # one ops gate
#   just ops-service-ops-hurl

# Full CI matrix — `just ci-local` is the one-command local gate; merge
# requires this green.
ci-local: lint test audit ops

default: ci-local

# CI lint job: check + fmt --check + clippy -D warnings (including the
# #751 file/function size lint — the thresholds live in clippy.toml).
lint:
    cargo check --workspace
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings -D clippy::too_many_lines

# CI test job: workspace tests under CI's RUSTFLAGS. AGENTS.md mandates
# local full-suite runs mirror the flag — without it every test binary
# links with full debuginfo (~3.6 GB each across ~200 test targets).
test:
    RUSTFLAGS="-C debuginfo=0" cargo test --workspace

# CI audit job; skip-WARN when cargo-audit is not installed.
audit:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v cargo-audit >/dev/null 2>&1; then
        echo "WARN audit — cargo-audit not installed; skipping (install: cargo install cargo-audit)"
        exit 0
    fi
    cargo audit

# CI test-ops-integration job: the ignored gates (service_ops_compile_gate
# is wired into CI by issue #629's pin work — mirrored here ahead of it).
ops: ops-db-integration ops-generator-tests ops-wrapper-compile ops-hooks-runtime ops-wasm-compile ops-service-ops-compile ops-service-ops-hurl ops-invariants-live

ops-db-integration:
    #!/usr/bin/env bash
    set -euo pipefail
    DB_URL="${DATABASE_URL:-postgres://postgres:postgres@localhost:5432/postgres}"
    if ! command -v psql >/dev/null 2>&1 || ! psql "$DB_URL" -c 'select 1' >/dev/null 2>&1; then
        echo "WARN ops — Postgres unreachable ($DB_URL); skipping db_integration"
        exit 0
    fi
    # Issue #708: mirror CI's fail-closed gate. When the target serves the
    # Supabase band, export CODEGRAPH_REQUIRE_SUPABASE=1 so a missing
    # capability FAILS naming the image (exactly like CI); on a plain
    # Postgres target, leave it unset (today's skip behaviour) but say so
    # loudly.
    SUPA=$(psql "$DB_URL" -Atc "SELECT coalesce(string_agg(name, ','), '') FROM (SELECT 'auth schema' AS name WHERE NOT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname='auth') UNION ALL SELECT 'pgmq' WHERE NOT EXISTS (SELECT 1 FROM pg_available_extensions WHERE name='pgmq') UNION ALL SELECT 'pg_tle' WHERE NOT EXISTS (SELECT 1 FROM pg_available_extensions WHERE name='pg_tle') UNION ALL SELECT 'http' WHERE NOT EXISTS (SELECT 1 FROM pg_available_extensions WHERE name='http')) missing;" 2>/dev/null || echo "unreachable")
    if [ -z "$SUPA" ]; then
        export CODEGRAPH_REQUIRE_SUPABASE=1
    else
        echo "WARN ops — $DB_URL is plain Postgres (missing: ${SUPA}): the 2 full-band tests will SKIP."
        echo "        point DATABASE_URL at public.ecr.aws/supabase/postgres:17.6.1.132, or set CODEGRAPH_REQUIRE_SUPABASE=1 to fail instead of skip"
    fi
    cargo test -p codegraph-ops --test db_integration -- --ignored --nocapture

ops-generator-tests:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo test -p codegraph --test ops_generator_tests -- --ignored --nocapture

ops-wrapper-compile:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo test -p codegraph --test wrapper_compile_tests -- --ignored --nocapture

ops-hooks-runtime:
    #!/usr/bin/env bash
    set -euo pipefail
    DB_URL="${DATABASE_URL:-postgres://postgres:postgres@localhost:5432/postgres}"
    if ! command -v psql >/dev/null 2>&1 || ! psql "$DB_URL" -c 'select 1' >/dev/null 2>&1; then
        echo "WARN ops — Postgres unreachable ($DB_URL); skipping hooks_cornucopia_runtime_tests"
        exit 0
    fi
    cargo test -p codegraph --test hooks_cornucopia_runtime_tests -- --ignored --nocapture

ops-wasm-compile:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v rustup >/dev/null 2>&1; then
        echo "WARN ops — rustup not installed; cannot ensure wasm32-unknown-unknown; skipping worker_wasm_compile_tests"
        exit 0
    fi
    if ! rustup target list --installed 2>/dev/null | grep -q wasm32-unknown-unknown; then
        echo "INFO ops — installing wasm32-unknown-unknown (rust-toolchain.toml declares it)"
        rustup target add wasm32-unknown-unknown
    fi
    cargo test -p codegraph --test worker_wasm_compile_tests -- --ignored --nocapture

ops-service-ops-compile:
    #!/usr/bin/env bash
    set -euo pipefail
    # The three service_ops_compile_gate tests are DISK-BACKED by
    # construction (issue #734): each scratch tree carries a full cargo
    # target, and it lands under CARGO_TARGET_TMPDIR (<workspace>/target/tmp),
    # falling back to <workspace>/target/gate-tmp — never std::env::temp_dir().
    # `TMPDIR=/var/tmp` is therefore obsolete for THIS gate; the other compile
    # pins that still build generated apps out of the temp dir
    # (mox_plane/media/workers_gateway/wrapper/planless/grpc compile tests,
    # ignored ops_generator_tests) still need it.
    cargo test -p codegraph --test service_ops_compile_gate -- --ignored --nocapture

ops-service-ops-hurl:
    #!/usr/bin/env bash
    set -euo pipefail
    DB_URL="${DATABASE_URL:-postgres://postgres:postgres@localhost:5432/postgres}"
    if ! command -v psql >/dev/null 2>&1 || ! psql "$DB_URL" -c 'select 1' >/dev/null 2>&1; then
        echo "WARN ops — Postgres unreachable ($DB_URL); skipping service_ops_hurl_gate"
        exit 0
    fi
    if ! command -v hurl >/dev/null 2>&1; then
        echo "WARN ops — hurl not installed; skipping service_ops_hurl_gate (CI installs the 8.1.0 .deb)"
        exit 0
    fi
    cargo test -p codegraph --test service_ops_hurl_gate -- --ignored --nocapture

ops-invariants-live:
    #!/usr/bin/env bash
    set -euo pipefail
    # Design-invariant enforcement over real HTTP (issue #666 slices
    # 4d+4e): violating create 409 + compensation, violating update/transition
    # 409 pre-write, member-invariant denial of the root save — with psql
    # proofs. Same prerequisites as ops-service-ops-hurl.
    DB_URL="${DATABASE_URL:-postgres://postgres:postgres@localhost:5432/postgres}"
    if ! command -v psql >/dev/null 2>&1 || ! psql "$DB_URL" -c 'select 1' >/dev/null 2>&1; then
        echo "WARN ops — Postgres unreachable ($DB_URL); skipping ddd_invariants_live_gate"
        exit 0
    fi
    if ! command -v hurl >/dev/null 2>&1; then
        echo "WARN ops — hurl not installed; skipping ddd_invariants_live_gate (CI installs the 8.1.0 .deb)"
        exit 0
    fi
    cargo test -p codegraph --test ddd_invariants_live_gate -- --ignored --nocapture
