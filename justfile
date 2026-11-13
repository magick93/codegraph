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

# CI lint job: check + fmt --check + clippy -D warnings.
lint:
    cargo check --workspace
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings

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
ops: ops-db-integration ops-generator-tests ops-wrapper-compile ops-hooks-runtime ops-wasm-compile ops-service-ops-compile ops-service-ops-hurl

ops-db-integration:
    #!/usr/bin/env bash
    set -euo pipefail
    DB_URL="${DATABASE_URL:-postgres://postgres:postgres@localhost:5432/postgres}"
    if ! command -v psql >/dev/null 2>&1 || ! psql "$DB_URL" -c 'select 1' >/dev/null 2>&1; then
        echo "WARN ops — Postgres unreachable ($DB_URL); skipping db_integration"
        exit 0
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
