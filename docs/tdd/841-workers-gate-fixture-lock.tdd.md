# TDD — the workers-gate fixture lock tracks the pherolang pin (issue #841)

Status: **LANDED** on `fix/workers-gate-fixture-lock`.

## 0. The defect, verified

`crates/codegraph/tests/fixtures/workers_gate/Cargo.lock` is a REAL committed
lockfile for the fixture's OWN workspace — the WS2 workers live gate's
regeneration step runs `cargo run --release -p gate-graph` inside it. It was
last refreshed against `phero-* @ 4f497e1` (`xstate-ir @ 380580b6`), and it
held `bumpalo 3.20.3` for `grafeo-common`.

The workspace then moved to `phero-* @ 958acd0b` + `xstate-core @ da0c804d`
(the machine-reference era, issue #815). That graph adds
`xstate-parser → oxc_allocator 0.77 → bumpalo =3.19.0`, which cannot coexist
with the locked `3.20.3`. Nothing refreshed the fixture lock, so the fixture
workspace became **unsatisfiable**, not merely out of date.

The nightly Workers Gate (`.github/workflows/workers-gate.yml`) has been red
since 2026-11-25. The failure is pre-build and needs no Postgres, no docker and
no browser:

```text
$ cd crates/codegraph/tests/fixtures/workers_gate
$ cargo metadata --locked --format-version 1
    Updating git repository `https://github.com/yestechgroup/pherolang.git`
    Updating git repository `https://github.com/yestechgroup/restingstate.git`
    Updating crates.io index
error: failed to select a version for `bumpalo`.
    ... required by package `oxc_allocator v0.77.0`
    ... which satisfies dependency `oxc_allocator = "^0.77"` of package `xstate-parser v0.2.1 (…restingstate.git?rev=da0c804d…#da0c804d)`
    ... which satisfies git dependency `xstate-parser` of package `phero-driver v0.4.0 (…pherolang.git?rev=958acd0b…#958acd0b)`
    ... which satisfies git dependency `phero-driver` of package `codegraph v0.1.0`
    ... which satisfies path dependency `codegraph` (locked to 0.1.0) of package `gate-graph v0.1.0 (…workers_gate/gate-graph)`
versions that meet the requirements `=3.19.0` are: 3.19.0

all possible versions conflict with previously selected packages

  previously selected package `bumpalo v3.20.3`
    ... which satisfies dependency `bumpalo = "^3"` (locked to 3.20.3) of package `grafeo-common v0.6.0`
    ... which satisfies dependency `grafeo-common = "=0.6.0"` (locked to 0.6.0) of package `codegraph-grafeo v0.1.0`

failed to select a version for `bumpalo` which could resolve this conflict
# exit 101
```

`--locked` is not what makes it fail: `cargo metadata --format-version 1` (no
flag) exits 101 with the SAME error and leaves the lock untouched, because
cargo does not repair a lock entry that conflicts with a newly required one —
`cargo update` is what rewrites it. The nightly gate therefore died at its
**regeneration** step, which is a plain `cargo run --release -p gate-graph`:

```text
$ gh run view 45825199718 --log-failed
running 1 test
fixture Supabase healthy
scratch database: workers_gate_18eb9a608e37bce4
regenerating generated-workers from the current templates…
thread 'workers_live_gate_supabase_stack_runs_the_full_workers_suite' panicked at
  crates/codegraph/tests/workers_live_gate.rs:1326:48:
workers regeneration failed — errors:
error: failed to select a version for `bumpalo`.
```

(`.github/workflows/workers-gate.yml` runs 45825199718 on 2026-11-26 and
45623090588 on 2026-11-25 are the two red runs; 45411126059 on 2026-11-24 —
before the #815 pin landed — is the last green one. Duration alone points at the
same place: 17–21 min green, 8–9 min red.)

## 1. Root cause

A pin move in the ROOT manifest does not propagate to a committed fixture
lock: the fixture is a separate workspace, so its lock is its own artifact and
only re-resolves when someone runs cargo in it. The root `Cargo.lock` was
refreshed with the #815 landing; the fixture lock was not, and NO PR-CI check
reads the fixture lock — the only consumer was the nightly live gate.

The 4-day delay is the second half of the defect: the drift was invisible
until the nightly lane ran, and that lane needs the full Supabase/docker tool
chain, so triage is expensive for what is a one-command fix.

## 2. The fix

Refresh that ONE lock with a TARGETED update (the diff stays small; a
delete-and-regenerate pulls unrelated patch bumps):

```bash
cd crates/codegraph/tests/fixtures/workers_gate
cargo update -p bumpalo --precise 3.19.0
cargo metadata --locked --format-version 1   # exit 0
```

The single `cargo update -p bumpalo` invocation is enough because the stale
git revs are unsatisfiable against the manifests — cargo drops them and
re-resolves the git closure in the same run.

### Enumerated lock diff (419 insertions, 17 deletions)

Git sources re-pinned (12 packages, all `#fragment` == `rev=`):

| package | before | after |
|---|---|---|
| `phero-dates`, `-diagnostics`, `-driver`, `-expr`, `-ifml`, `-ir`, `-parser`, `-strings`, `-syntax`, `-verify`, `-vocab` (0.4.0) | `4f497e1` | `958acd0b` |
| `xstate-ir` (0.2.1) | `380580b6` | `da0c804d` |

Added (37 entries — the machine-reference / XState compiler closure):

- `xstate-core 0.2.1`, `xstate-parser 0.2.1` (restingstate @ `da0c804d`)
- `phero-evidence 0.4.0` (pherolang @ `958acd0b`)
- `oxc_allocator`, `oxc_ast`, `oxc_ast_macros`, `oxc_data_structures`,
  `oxc_diagnostics`, `oxc_ecmascript`, `oxc_estree`, `oxc_index`,
  `oxc_parser`, `oxc_regular_expression`, `oxc_span`, `oxc_syntax` — all
  `0.77.3` except `oxc_index 3.1.0`
- `oxc-miette 2.7.1`, `oxc-miette-derive 2.7.1`, `nonmax 0.5.5`,
  `castaway 0.2.4`, `compact_str 0.9.1`, `cow-utils 0.1.3`,
  `static_assertions 1.1.0`, `seq-macro 0.3.6`, `owo-colors 4.4.0`,
  `textwrap 0.16.4`, `smawk 0.3.3`, `unicode-id-start 1.5.0`,
  `icu_locale_fallback 2.4.0` (+`_data`), `icu_segmenter 2.4.0`
  (+`_data`), `phf_generator 0.12.1`, `phf_macros 0.12.1`, `ryu 1.0.23`,
  `ryu-js 1.0.4`
- `bumpalo 3.19.0` (was `3.20.3` — REMOVED)
- `num-bigint 0.4.8` (a SECOND major beside the existing `0.5.1`, matching
  the root lock)

The refreshed lock now agrees with the root lock on every git rev, on
`bumpalo`, and on both `num-bigint` majors. No template, no root
`Cargo.lock`, and no other fixture is touched.

## 3. The pin (node-free PR CI)

`crates/codegraph/tests/workers_gate_fixture_lock_tests.rs` parses both locks
as TOML — no cargo, no build, no network, no shared `target/` lock (~0.1 s):

1. `fixture_lock_git_revisions_match_the_workspace_pin` — every git-sourced
   package present in BOTH locks must resolve to the same `#<commit>`
   fragment. Generic by design: a `phero-*`, `xstate-*`, `sigil-*` or
   `dsntk-*` pin move is caught by rev, not by a hardcoded package name. A
   vacuity guard (≥10 packages compared, `phero-driver` present) stops the
   check from silently comparing nothing.
2. `fixture_lock_resolves_bumpalo_from_the_workspace` — `bumpalo` must hold
   the workspace's version. This is the package that made the lock
   unsatisfiable; it rides beside the general rule as the concrete proof.

Both messages name the file, the package, the two commits (or versions), and
the fix command.

## 4. The gates

```text
cd crates/codegraph/tests/fixtures/workers_gate && cargo metadata --locked --format-version 1
  pre-fix:  exit 101 (bumpalo conflict, above)
  post-fix: exit 0

RUSTFLAGS="-C debuginfo=0" mbx test -p codegraph --test workers_gate_fixture_lock_tests
  pre-fix:  0 passed; 2 failed  (exit 101)
  post-fix: 2 passed            (exit 0)

mbx fmt --all -- --check                     # clean
mbx clippy -p codegraph --tests -- -D warnings   # clean
mbx test -p codegraph --test file_length_contract
```

The live gate (`mbx test -p codegraph --test workers_live_gate -- --ignored
--nocapture`) is the acceptance proof that the refreshed lock is not just
parseable but BUILDABLE: it regenerates, migrates, builds the 11-crate workers
workspace, boots the gateway + 10 workers and runs the hurl contracts through
the gateway. Its result is recorded in the PR.

## 5. The mutation proof

Restoring the pre-fix lock (and leaving every source change in place) turns
both checks red, so the pin bites on exactly this defect:

```text
$ cp <pre-fix lock> crates/codegraph/tests/fixtures/workers_gate/Cargo.lock
$ cd crates/codegraph/tests/fixtures/workers_gate && cargo metadata --locked --format-version 1
error: failed to select a version for `bumpalo`.   # exit 101

$ RUSTFLAGS="-C debuginfo=0" mbx test -p codegraph --test workers_gate_fixture_lock_tests
---- fixture_lock_git_revisions_match_the_workspace_pin stdout ----
12 fixture lock git revision(s) disagree with the workspace lock:
  phero-dates 0.4.0: fixture lock -> 4f497e17…, workspace lock -> 958acd0b…
  … (11 more: phero-diagnostics/driver/expr/ifml/ir/parser/strings/syntax/verify/vocab, xstate-ir)
  xstate-ir 0.2.1: fixture lock -> 380580b640…, workspace lock -> da0c804d3c…
---- fixture_lock_resolves_bumpalo_from_the_workspace stdout ----
bumpalo disagrees: fixture lock resolves 3.20.3, the workspace resolves 3.19.0.
test result: FAILED. 0 passed; 2 failed
```

Restoring the refreshed lock returns `cargo metadata --locked` to exit 0 and
the suite to 2/2 green.

## 6. Recorded follow-ups

- The root `Cargo.lock` and the fixture lock are two artifacts with no shared
  resolution step; this pin closes the DETECTION gap, not the maintenance
  one. A future `init`-style helper that refreshes every committed fixture
  lock is not proposed here (one fixture exists, and the pin's message names
  the command).
- `cargo metadata` in the fixture still fetches the git repos on a cold CARGO
  cache, so the PR-CI pin deliberately reads the locks as TEXT instead of
  shelling out to cargo.
