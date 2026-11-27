# Ops testkit: runbook & failure-signature playbook

`crates/codegraph-ops` is the Rust test and deploy harness every
codegraph-generated app shares. The generated `testkit` binary wraps
`codegraph_ops::cli::main()`; this document is its runbook: what each suite
does, how long stages take at scale, how service lifecycle and freshness
work, and a failure-signature playbook where every row traces to a real
incident from the hr-specs validation run (the ops-reliability epic dogfood,
August-September 2026).

For the consumer integration guide (manifest keys, hooks, extensions, CI
wiring) see the "Ops Harness" section of the repository `AGENTS.md`.

---

## 1. Suites and stages

Every long-running child process streams its output under a `[label]`
prefix (`[build] Compiling foo`) instead of running silently. Each stage
prints its duration on completion (`✓ DB migrate (8m 31s)`), hooks are timed
separately (`hook <name>` rows), and the last 50 lines of a failed stage's
output are ALWAYS printed — failure tails are never gated behind
`--verbose`. `--verbose` additionally unmutes quiet stages (dependency
compilation, browser downloads) and echoes each stage's full captured
output inline.

### api

```
testkit api [--no-migrate] [--rebuild] [--regen] [--workers]
```

| Stage | What it does |
|-------|--------------|
| 0. Fast doctor (api subset) | Cheap stage-0 gate: psql `SELECT 1`, hurl, python3 (if smoke configured), api port, disk headroom. Fails in seconds with a hint instead of after the build. Marker: `Fast doctor`. |
| 0. Generate + build | Regenerate the app via the graph binary (hooks: `pre_generate`/`post_generate`), fail on generation errors unless `--allow-gen-errors`, generator-rev check, then `cargo build` in the generated app. Regeneration runs **from clean** (#796): the previous generation's `.codegraph-manifest.json` is snapshotted after the `pre_generate` hooks, and files it lists that the new generation did not write are removed (logged as `regen sweep: removed N stale file(s) … (roots: …)`). Hand-authored files are never in a manifest and are never touched; the ops manifest and `testkit/` are always kept. Skipped by `--skip-build` (generation alone by `--skip-generate`). |
| 1. Preflight | Port preflight (take over or reuse a registry-known prior server), output-tree completeness. |
| 2. Database | Reset + phased migration, app-role grant, API-key provisioning (main Bearer key; optional Org B key from `hurl.org_id_b`; optional read-only key when `hurl.limited_key = true`). Skipped with `--no-migrate`. |
| 3. Server | Boot the axum binary, wait for `/health`. Skipped when `--reuse` found a live registry server. |
| 4. Hurl API tests | One full-output log per executed hurl file under `test-results/hurl/` (pass or fail); `--retry N` re-runs failed files up to N times; `hurl = none` in the manifest skips. |
| 5. Curl smoke tests | CRUD round-trip against the configured smoke entity. |
| 6. DB inspection | Post-migration database state checks via psql. |
| 7. Health endpoint | `GET /health` contract. |
| 8. Cross-tenant RLS isolation | Org A cannot read org B; out-of-scope writes raise P0403. |
| 9. Server log check | Scan the server log for panics/errors. |
| 10. Graceful shutdown | SIGTERM the server; verify clean exit. |
| 11. Regeneration validation | `api --regen` only: regenerate again and `cargo check`. |

hr-specs validation-run numbers at consumer scale: generation ~3 min; the
app's release build 55-66 min cold, ~1 min warm.

### Generated hurl contracts (#463)

The `hurl_contract` generator (default/fullstack profiles) emits the API
contract suite the stage-4 harness runs — no hand-written hurl files:

| File | Contract |
|------|----------|
| `{nn}_{domain}_{entity}.hurl` (from 10, entity generation order) | LIST envelope, CREATE 201 + `data.id` capture (required FK parents created first via preceding POSTs with their own captures), GET-by-id field echo, zero-uuid 404, PUT roundtrip + GET verify, DELETE 204 + GET 404 — `Authorization: Bearer {{api_key}}` on every request |
| `01_auth.hurl` | missing key → 401, garbage key → 401 (once per run, anchored at the first entity) |
| `03_scope_denial_403.hurl` | read-only key (`{{api_key_limited}}`): read 200, write 403 `FORBIDDEN` + `INSUFFICIENT_SCOPE` |
| `04_cross_tenant_404.hurl` | org-A create captured, org-B GET → 404 (the silent RLS filter) |
| `08_rls_isolation.hurl` | stage-8 convention file: org-B list never contains the org-A row; the manifest `[hurl].skip` names it so the main loop never runs it (stage 8 does, with `api_key_a`/`api_key_b` only) |

The ops generator emits the `[hurl]` manifest section (dir, skip, org ids,
`limited_key = true`) exactly when `hurl_contract` is in the plan, and the
api suite provisions the matching keys (main/org-B/read-only). Org ids MUST
stay aligned between `OpsHurl::default()` and
`hurl_contract::ORG_ID_A/ORG_ID_B`. A configured hurl dir that yields zero
runnable files is warned (never silently skipped). The generator↔harness
coupling is pinned by `hurl_contract_tests`; `review_api_suite` is the live
acceptance gate over real HTTP (app_user pool mode + the authz contracts).

### e2e

```
testkit e2e [--skip-ui-build] [--retry-failed] [-- extra playwright args...]
```

Requires a `[[supabase]]` manifest section and `database.e2e`.

| Stage | What it does |
|-------|--------------|
| Fast doctor (e2e subset) | npx, pnpm, supabase CLI, chromium, api+ui ports, disk. No DB probe. Marker: `Fast doctor`. |
| Port preflight | api + ui ports resolved through the services registry (takeover / `--reuse`). |
| E2E 0. freshness (pre-check, --skip-build) | With `--skip-build` only: fail a stale binary in seconds instead of after ~10 minutes of supabase/DB work. |
| E2E 1. Supabase | The PINNED CLI `npx --yes supabase@3.2.0 start` (or detect running), then `pre_e2e` hooks (e.g. the pgmq patch). |
| E2E 2. Generate | Build the graph binary (release), regenerate, output-tree check, generation-error gate (`--allow-gen-errors`), generator-rev check. |
| E2E 3. Database | Migration symlink + `supabase db reset` + seed (~2 min at consumer scale), `post_migrate` hooks, API-key provisioning. |
| E2E 4. Build | Build the app binary, then the post-build freshness check. |
| E2E 5. Start Services | Boot the API (unless reused), `pre_playwright` hooks (before the web build so synced sources get compiled), `Web build` (pnpm install + SvelteKit production build — failure is fatal; `--skip-ui-build` degrades it to a warning at the cost of a possibly stale bundle), boot vite preview. |
| E2E 6. Playwright Tests | Clear the transpile cache, resolve chromium, run the generated suite with `PUBLIC_*`/`SUPABASE_*` env. |
| E2E 7. Retry failed tests | `--retry-failed` only: rerun failures in-session via `--last-failed`; a green retry counts as transient and passes the suite. |
| Summary | Per-project tallies, failing titles (first 20), ready-to-paste retry commands, `--results` JSON. |

hr-specs validation-run numbers: supabase reset ~2 min; ~4.2k Playwright
tests ~40 min at 8-16 workers.

### cli

```
testkit cli
```

Starts the API itself (`api --keep`) when `/health` does not answer, then
exercises the generated admin CLI: Help & version, Config commands, CRUD
lifecycle, Error handling, Subcommand listing. Skipped with a warning when
the profile has no `has_cli` capability.

### ui

```
testkit ui [--headed] [-- extra playwright args...]
```

Playwright only — the API must already be running (`api --keep` first;
`cli` and `full` arrange this themselves). Installs UI deps and a missing
`dist/` best-effort, reads or provisions the API key
(`/tmp/codegraph-ops-api-key`), boots `vite preview` on the ui port, clears
the Playwright transpile cache, runs the suite.

### workers

```
testkit workers
```

Workers topology (per-domain workers + gateway, cornucopia provider):
Preflight (Postgres, hurl, gateway port 8787 + worker ports) → Regenerate
with the workers profile (wipes the workers output dir first) → Database
(plain Postgres reset + migrate + seed) → Build workers workspace → Boot
workers + gateway → Gateway smoke → Hurl contract tests through the
gateway.

`testkit api --workers` is the same path (the api-suite entry point for
Workers-topology consumers). The generated manifest carries a `[workers]`
section seeded from the run (`domains`, and optional `profile`/`output_dir`/
`binary_prefix`/`gateway_port`/`worker_base_port`); absent, the suite keeps
the historical hr-specs defaults.

The workers suite provisions the SAME extra API keys as the monolith api
suite — the main Bearer key, the Org B key when `hurl.org_id_b` is set, and
the read-only key when `hurl.limited_key = true` — and passes all three to
hurl as `api_key`, `api_key_b`, `api_key_limited`. The scope-denial
(`03_scope_denial_403.hurl`: read-only write → 403 `INSUFFICIENT_SCOPE`) and
cross-tenant (`04_cross_tenant_404.hurl`) contracts therefore RUN on the
workers slice (issue #828; a consumer fixture must not skip them). The
gateway smoke also repeat-creates the smoke entity 50× and fails on any 5xx
or transport error — the intermittent-create class can never pass silently.

### smoke

```
testkit smoke [--api-url URL] [--web-url URL] [--expected-commit SHA]
              [--auth-health-url URL] [--worker URL]...
```

Nine checks against a deployed environment: API health, health/ready (soft),
Swagger UI, OpenAPI spec, frontend, Supabase auth health (soft, opt-in),
integration worker pings (soft, repeatable), metrics, version
(`--expected-commit` mismatch warns).

### quality

```
testkit quality [extra cargo gates...]
```

`cargo test --workspace`, `cargo clippy --workspace -- -D warnings`,
`cargo fmt --all -- --check`, regenerate the generated app, `cargo check`
in the generated app. Extra gates (e.g. `doc`) run after.

### full

```
testkit full
```

`api` then ALWAYS `e2e` (an api failure does not skip e2e — the old bash
behavior). The exit code is the worse of the two suites; 0 only when both
pass.

---

## 2. Service lifecycle

### `--keep` and the services registry

`--keep` leaves servers running after the suite (faster iteration). Every
service the harness leaves running (api server, e2e app server, vite
preview) is recorded in the advisory registry
`.testkit/services.json` under the manifest root:

```json
{ "services": [ { "name": "api", "pid": 12345, "port": 3000,
                  "health": "/health", "started_at": "2026-09-30T00:00:00Z",
                  "profile": "debug", "suite": "api" } ] }
```

The registry is ADVISORY by contract: missing, stale, or corrupt JSON never
breaks a run (read as empty + a warning). `clean` kills registered services
by pid, then falls back to a `fuser -k` port sweep for unknown orphans.

### Port takeover and `--reuse`

When a suite needs a port that is already bound, the preflight resolves it
through the registry:

- Free → proceed.
- Registry-known live occupant (default) → **takeover**: SIGTERM → grace →
  SIGKILL on the recorded pid, entry removed, the suite boots its own
  server.
- Registry-known live occupant + `--reuse` → **reuse**: the suite does not
  boot its own instance. Documented caveat: generate/build still run, but
  the OLD server keeps serving — the harness assumes a reused service
  serves the current build, which is only true when nothing regenerated
  underneath it.
- Anything else (unknown occupant, dead pid) → the actionable error:
  `port N is not free ... free the port (e.g. fuser -k N/tcp)`.

### `clean` coverage

`testkit clean` stops registered services (registry teardown), sweeps the
api/ui ports plus the workers topology ports (gateway 8787 + worker range),
removes stale `{app}-*.pid` files, stops Supabase and removes its migration
symlinks, deletes the generated `src/`, `ui/`, `migrations/`, `queries/`,
`cornucopia-queries/` trees, and removes `/tmp/codegraph-ops-*.log` plus
the `/tmp/codegraph-ops-api-key` provision file.

`test-results/` (failed-run screenshots, traces, error contexts, artifact
bundles) is KEPT by default so a failed run stays debuggable;
`clean --deep` removes it too.

### Playwright transform-cache hygiene

Playwright transpiles each spec into `/tmp/playwright-transform-cache-{uid}/`
and reuses those files across runs. After a regeneration the cache served
STALE transpiled specs (see the failure table below), so the harness clears
the cache wholesale — entries are content-hash addressed with Playwright's
internal hash (source path + mtime dependent), which cannot be replicated
selectively. The e2e and ui suites clear it automatically right before
Playwright; `testkit --clear-cache <suite>` forces the clear at run start
for every other suite. `doctor` reports the cache's size.

---

## 3. Freshness model

Three independent gates keep a suite from testing stale code:

1. **Binary vs sources (mtime).** The app binary must be newer than the
   newest `.rs` file under `{app_dir}/src`; otherwise:
   `binary <path> is older than <src> — the suite would test stale code;
   rebuild (drop --skip-build) first`. A missing binary or a missing/wiped
   `src/` is equally fatal (the wiped-tree incident produced 15 baffling
   api failures before this check existed). Regeneration is
   write-if-changed — byte-identical output preserves mtimes, so a
   no-op regeneration does NOT re-arm the check. Instead a successful
   build post-touches the binary ("a successful build is a freshness
   statement"): pre_generate clean hooks may wipe + recreate `src/` with
   fresh mtimes while cargo skips the relink on byte-identical sources,
   and the touch keeps the mtime comparison truthful for exactly that
   case.
2. **Stage-0 fast-fail for `--skip-build`.** With the build skipped the
   binary cannot become fresher later, so the e2e suite checks freshness
   BEFORE Supabase (stage `E2E 0. freshness (pre-check, --skip-build)`) —
   seconds instead of ~10 minutes of DB work before the verdict. With a
   build enabled the post-build check stays authoritative.
3. **Generator-rev check.** Every regeneration stamps
   `.codegraph-manifest.json` at the output root with the checkout rev it
   ran from (JSON field `codegraphCommit`). The testkit embeds the rev its
   codegraph crates are pinned to (`CODEGRAPH_OPS_REV`, embedded at build
   time). A mismatch — generation ran under a stale graph binary — is a
   HARD error naming both revs and the manifest path, right after
   generation and before any DB work. `--allow-gen-rev-mismatch` downgrades
   it to a warning. Degradation contract: a missing manifest, a missing/
   empty `codegraphCommit` (older generators), or an empty embedded rev
   (path-dep build outside a git checkout) only warns — never spuriously
   fatal.

**Profiles.** The freshness gates are profile-aware: whichever binary
actually ran is the one checked. `--release` steers BOTH builds — the
generated app AND the graph-binary regeneration
(`cargo run --release -p <graph> -- run`, issue #307; debug regeneration is
dramatically slower on large models and every suite regenerates by
default). The api suite boots the release app binary under `--release`
(and under `--skip-build` when only a release binary exists); e2e and
workers regeneration ALWAYS build and run the release graph binary, so
their profile is inherent rather than flag-steered. `testkit doctor`
reports presence/age for both profiles of every binary; the binary-vs-src
staleness comparison runs against the profile-resolved binary only, so a
fresh release binary is never mislabeled by a stale debug twin (or vice
versa).

The check runs in every suite that regenerates: api stage 0 and stage 11
(`--regen`), e2e stage 2, workers stage 2.

---

## 4. Failure-signature playbook

Every row traces to a real incident from the hr-specs validation run of the
ops-reliability epic (consumer scale: ~4.2k Playwright specs).

| Signature | Cause | Fix |
|-----------|-------|-----|
| `port 3000 is not free (...)` | A previous `--keep` run left its server bound (or another process holds the port). | `testkit clean` (kills registry-known services); a registry-known server is also taken over automatically or reused with `--reuse`; unknown orphans: `fuser -k 3000/tcp`. |
| `binary ... is older than .../src` | The binary predates regenerated sources — `--skip-build` paired a stale binary with fresh output (or a mixed debug/release tree). | Rebuild: rerun WITHOUT `--skip-build` (or `api --rebuild`). |
| Playwright error-context file shows `Expected pattern` matching no file on disk | Stale Playwright transform cache after regeneration — Playwright executed old transpiled specs. | `testkit --clear-cache <suite>`; the e2e/ui suites also clear the cache automatically before every Playwright run. |
| `422 ... did not match any variant of untagged enum ...Body` | Consumer-side DTO vs test fixture body mismatch (untagged enum with a VO-omitted field). NOT a harness bug. | Fix the consumer DTO or fixture. |
| Rotating 15-45s timeouts on the largest tables under 8-16 workers | Parallel load saturation, not a functional failure. | Raise the affected timeouts, lower `PW_WORKER_COUNT`, or pass `--pw-retries N` to let Playwright absorb the flakes. |
| Builds die mid-run; `No space left on device` | /tmp or the app `target/` filesystem quota exhausted. | `testkit doctor` checks free space on all three filesystems (block below `[doctor].min_free_gb`, default 2 GB; warn below `warn_free_gb`, default 10 GB). Prune `test-results/artifacts-*`, the transform cache, and `target/`. |
| `Disk quota exceeded (os error 122)` / `EDQUOT` from a dependency build script during `cargo test -p codegraph --test service_ops_compile_gate` | The gate's scratch trees (a generated app plus its cargo `target/`, three tests in parallel) can fill the filesystem they sit on — `/tmp` on the standard host is a 31 GiB tmpfs with a user quota, so exhaustion surfaces as a *code* failure from deep inside a dependency build script. | Read the `scratch: <path>` line each gate test prints — it names the filesystem that filled. Those three dirs resolve to `CARGO_TARGET_TMPDIR` (`<workspace>/target/tmp`, fallback `<workspace>/target/gate-tmp`), so reclaim space THERE (`cargo clean`, prune `target/`), not on `/tmp`. `TMPDIR` does not move this gate; it still governs the other compile pins that build generated apps out of the temp dir — `mox_plane_compile_tests`, `media_compile_tests`, `workers_gateway_compile_tests`, `wrapper_compile_tests`, `planless_compile_tests`, `grpc_compile_tests` and the ignored `ops_generator_tests` — where `TMPDIR=/var/tmp` remains the workaround. If `CARGO_TARGET_DIR` itself points at tmpfs the gate follows it: point it at disk. |
| Generated output behaves like old generator code | Generation ran under a stale (e.g. debug) graph binary. | The generator-rev check now hard-errors; rebuild the graph binary from the pinned rev, or pass `--allow-gen-rev-mismatch` to accept the drift knowingly. |

---

## 5. Artifacts and triage

| Artifact | Location | Notes |
|----------|----------|-------|
| Hurl logs | `{root}/test-results/hurl/{hurl file name}.log` | One per executed file, pass or fail. |
| Server logs | `/tmp/codegraph-ops-app.log`, `-e2e-app.log`, `-sveltekit.log`, `-sveltekit-e2e.log`, `-cli.log` | COPIED (not moved) into bundles; `clean` owns deletion. |
| Playwright triage | `{ui_dir}/test-results/` | `error-context.md` page snapshots, `.last-run.json`; traces/screenshots stay here (bundles skip binaries). Kept by `clean`; removed by `clean --deep`. |
| Failure bundle | `{root}/test-results/artifacts-<utc-ts>/` | Assembled automatically when api/cli/e2e/ui/full/workers fails (unless `--no-bundle`), or on demand via `testkit bundle`. |
| Bundle manifest | `.../artifacts-<ts>/bundle.json` | Every copied/skipped source + reason, suite, sizes. |

Bundles collect `logs/` (server logs), `hurl/`, `playwright/` (summary
files only — binary artifacts are skipped with the reason recorded), and
the run's own `--results`/`--metrics` files. The total is capped by
`[bundle].max_mb` (default 200 MB): oversized sources are tail-copied with
a `…[bundle: copied the last N of M bytes]` marker; below a 1 MB tail
budget they are skipped. Beyond `[bundle].keep` (default 3) the oldest
`artifacts-*` dirs are pruned after each bundle.

On any failure the summary prints:

- the error plus a `hint:` line when one exists,
- the LAST summary line announcing the bundle (`▸ artifacts: ...`),
- for e2e, ready-to-paste retry commands:
  `testkit --config <manifest> e2e -- --last-failed` and up to 8
  `--grep "<title>"` variants.

### Results JSON (`--results FILE`)

Two shapes share the path. The completed-run report is written by api/e2e/
workers at their summary stage, on success AND failure:

```json
{
  "suite": "api",
  "manifest": "codegraph-ops.toml",
  "profile": "default",
  "passed": 54,
  "failed": 0,
  "failures": [{ "context": "bulk_create.hurl", "title": "create candidate" }],
  "transient_resolved": 0,
  "hook_failures": ["refresh-views"],
  "stages": [{ "name": "DB migrate", "duration_secs": 511 }],
  "exit": 0
}
```

`hook_failures` is omitted when empty. When a suite fails BEFORE reaching
its summary (port conflict, missing tool, stale binary, supabase reset),
the CLI level writes the flat early-failure report instead:

```json
{
  "suite": "api",
  "manifest": "codegraph-ops.toml",
  "stage": "1. Preflight",
  "error": "port 3000 is not free (...)",
  "exit": 1
}
```

A suite summary always wins over an earlier early-failure write.

---

## 6. Environment variable reference

Variables the HARNESS reads or exports (all verified in
`crates/codegraph-ops/src`); the final row is generated-config-side and
listed for completeness:

| Variable | Direction | Purpose |
|----------|-----------|---------|
| `PW_WORKER_COUNT` | read (generated config) | Consumed by the GENERATED `playwright.config.ts` (`Number(process.env.PW_WORKER_COUNT ?? '<default>')`). The harness never sets it; `doctor` reports the effective value informationally, and the generated persona fixtures require `PW_WORKER_COUNT` >= the config's `workers` setting. |
| `PUBLIC_API_URL` / `PUBLIC_API_KEY` | exported | Set for the vite preview server and the Playwright run (ui + e2e). |
| `PUBLIC_SVELTEKIT_URL` | exported | Playwright env (ui + e2e). |
| `SUPABASE_URL`, `SUPABASE_ANON_KEY`, `SUPABASE_SERVICE_ROLE_KEY` | exported | Playwright env, from the manifest `[[supabase]]` keys or the local-dev defaults. |
| `SUPABASE_JWT_SECRET` | exported | App server boot env (api + e2e suites). |
| `DATABASE_URL` | exported | App server boot (api or e2e db target) and Playwright env (when `database.e2e` is configured). |
| `CODEGRAPH_REQUIRE_SUPABASE` | read | When `1`/`true`, a db target lacking the Supabase band (`auth` schema, pgmq, pg_tle, http) is a FAIL naming what is missing and the image to point at — never a skip (issue #708). Unset, plain-Postgres targets keep the historical skip behaviour. CI's `test-ops-integration` exports it; `just ops-db-integration` sets it automatically when the reachable target serves the band. |
| `APP_DATABASE_URL` | exported (unless set) | The `app_user` serving pool for the generated server; defaults to the migration's `app_user` password. Set it yourself to override. |
| `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH` | read + exported | Override for the chromium binary; the e2e suite exports it when a system chromium is found, otherwise falls back to `npx playwright install chromium`. |
| `CODEGRAPH_ROOT` | exported via `--codegraph-root` | Inherited by hooks, cargo, and the generated app; lets manifests reference the codegraph checkout via `{env:CODEGRAPH_ROOT}`. |
| `CORNUCOPIA_DATABASE_URL` | exported | Build-time Postgres URL for the cornucopia provider's `build.rs` (also set for app-binary spawns). |
| `PSQL_PATH`, `NPX_PATH` | read | Directory overrides for psql / npx discovery (PATH and common install locations are probed otherwise). |
| `API_KEY` | read (generated config) | NOT consumed by the harness itself: the generated IFML Playwright config uses it for env-gated Bearer auth (`extraHTTPHeaders`) when running against an authenticated API (issue #574 S1; the deprecated `IFML_API_KEY` spelling still reads as a fallback in the generated server helper). |

Files: the shared API-key provision file is
`/tmp/codegraph-ops-api-key`. `read_or_provision_api_key` (used by the
e2e, ui, and cli suites) reads and health-verifies it, and otherwise
provisions a key via `public.create_api_key()` and persists it there;
`clean` deletes the file. The api suite provisions its keys directly in
stage 2 (no file). Hurl files live under the manifest's `hurl.dir`;
per-file logs under `{root}/test-results/hurl/`.

---

## 7. Exit codes and CI wiring

| Code | Meaning |
|------|---------|
| 0 | Success (also `--help`). |
| 1 | Any harness-reported failure: config error, missing tool, failed checks or tests, doctor blocking problems. |
| 2 | Timeout (`OpsError::Timeout`). Unknown flags/arguments also exit 2 via clap before the harness runs. |

`full` runs api then e2e unconditionally and returns the worse of the two
codes (0 only when both pass).

For aggregation:

- `--metrics FILE` appends stage timings once per run: TSV by default
  (`timestamp\tsubcommand\tstage\tduration_secs` plus a `TOTAL` row), or a
  JSON array via `--metrics-format json` (one object per stage plus a
  `TOTAL` row carrying the wall-clock total). The CLI-level appender
  (`finish_ok`) is the single writer and honours `--metrics-format`.
- `--results FILE` (api, e2e, workers) carries pass/fail counts, failing
  titles, per-stage durations, and the exit code — see section 5.

CI wiring: run the api suite against the **Supabase postgres image** (the
`auth` schema is load-bearing; a plain `postgres:<n>` service cannot serve
this suite — issue #640), with metrics export:

```bash
cargo run -p testkit -- api --metrics ci.tsv
```

**Which mode CI uses (issue #709, shape B):** the suite probes the
configured `[database.api]` target with the shared Supabase-compatibility
predicate (`codegraph_ops::supabase::compatibility_gate` — the same probe
the `db_integration` band tests use) and, when it already serves the band,
**skips `supabase start` entirely and runs against it** — no manifest
`[[supabase]]` section is needed in this mode (there is no stack to
start). Point the job's service at
`public.ecr.aws/supabase/postgres:17.6.1.132` and the suite is
shape-B from the first request. The stack-startup path (the pinned
`npx --yes supabase@3.2.0 start`, issue #743) remains the fallback for a
down stack. Shape A — `npx supabase start` inside the CI job — is a
recorded alternative, deliberately out of scope: it needs Docker-in-Docker
and 1–2 minutes of stack boot per PR.

**The repo's own acceptance gate (`review_api_suite`, issue #764)** uses the
same shape-B contract, and it is the reference wiring for a host-side run:

- `crates/codegraph/tests/review_api_suite.rs` resolves its base target
  through `test_framework::postgres::resolve_base_target()` —
  `DATABASE_URL` when it is reachable **and** can host per-run databases,
  else the pinned Supabase-image container on `127.0.0.1:15432` (reused
  across warm runs).
- It HARD-asserts `compatibility_gate == Run` before doing any slow work. A
  plain `postgres:<n>` target aborts in seconds naming
  `public.ecr.aws/supabase/postgres:17.6.1.132`; the 403/404/RLS legs are
  not skippable (a silent skip would make the gate vacuous).
- It retargets the WHOLE `[database.api]` block (host/port/user/password +
  the per-run scratch database) at the resolved base target, then invokes
  the plain `testkit api` — no `--no-migrate`. Stage 2 applies the band and
  writes `platform._migrations`; the app's boot migrator then applies ZERO
  files (asserted from the app log as `boot-migrations: 0 applied`). The
  pre-#756 test-side `seaql_migrations` shim is gone: one tracking table is
  the honest consumer path.
- It drops the scratch database at the end and exports nothing that would
  disable `APP_DATABASE_URL` pool mode.

**The committed fixture manifest shape**
(`crates/review/generated-candidate/codegraph-ops.toml`) is generated
EXCEPT for a hand-maintained `[supabase]` section (and comment header). The
drift pin (`review_fixture_drift_tests`) compares the generated half
line-for-line with comments and that section normalized out; the section's
contract lives in `crates/codegraph/tests/review_fixture_maintained.rs`
(every in-place refresh — the documented maintenance helper AND the PR-CI
`grafeo_candidate_inspect_output` wipe — carries it forward, and the
fixture-owned `supabase/` project is a committed extra). The section is the
**manual full-stack fallback**: it points `[database.api]` at the fixture
stack (db port 54350 ⇔ `[database.api].port` ⇔ the Kong health URL on
54351), and `crates/review/generated-candidate/supabase/config.toml` pins
ports 54350+ deliberately disjoint from every other local stack
(crewbase 54321-54327, trader-rex 54331-54337, workers-gate 54340-54349),
speaks the pinned CLI's `[local_smtp]` key spelling (issue #729), and
disables the CLI's own migration/seed passes (the harness applies the
generated band). Manual run:

```bash
cd crates/review/generated-candidate
npx --yes supabase@3.2.0 start
cargo run -p testkit -- api
```


**What this mode does NOT exercise:** GoTrue and PostgREST are not
installed by the postgres image and are not required by the suite's core
legs (migrate → app boot → hurl contracts → curl smoke → RLS; the hurl
contracts authenticate with API keys against the generated axum app, and
the 401 leg is served by axum itself). The persona/auth legs — JWT login
flows, Supabase client auth, browser sessions — are **shadowed** in this
lane; they remain the consumer `testkit e2e` lane's job.

**CI lane status:** the codegraph repo's own CI does **not yet** run the
testkit api lane. Wiring it surfaced issue #756 (a generated app without
the admin CLI could not boot over an already-applied band: the boot re-seed
hit the #700 tenant trigger with no session org) — that blocker is FIXED:
every migration plane now records/reads ONE `platform._migrations` table and
boot adopts legacy bands instead of re-applying them (`boot_migration_tests`,
wired into `test-ops-integration`). The lane wiring itself is a later wave;
the contract pin
(`ops_ci_lane_contract::the_testkit_api_lane_is_wired_into_ci_and_mirrored_in_the_justfile`)
stays ignore-marked until then. The codegraph repo's `test-ops-integration`
job (Supabase-image service + the fail-closed `CODEGRAPH_REQUIRE_SUPABASE`
gate, issue #708) is the reference pattern for the DB tier; see the "Ops
Harness" section of `AGENTS.md` for the consumer integration guide.

---

## 8. Hooks

Hooks are manifest `[[hooks]]` entries: named `sh -c "{exec} {args...}"`
steps run in the repo root at pipeline points. Each hook streams under
`[hook:<name>]`, is timed as its own `hook <name>` metrics row (without
splitting the enclosing suite stage), and has explicit fatality:

- `fatal` missing or `true` → a failure ABORTS the suite (the error carries
  the hook's output tail).
- `fatal = false` → a failure only warns, and the hook's name is collected
  into the results JSON's `hook_failures`.

One hook point overrides the flag entirely: `post_e2e` is always
warn-only — cleanup hooks must run on every e2e path and never mask the
real failure. The workers suite also runs its `post_generate` warn-only.

| Hook point | Suites | Fatality |
|------------|--------|----------|
| `pre_api` / `post_api` | api | per-hook `fatal` (missing = fatal) |
| `pre_generate` | api, e2e, workers | per-hook `fatal` |
| `post_generate` | api, e2e | per-hook `fatal`; workers: warn-only |
| `post_migrate` | api, e2e | per-hook `fatal` |
| `pre_e2e` | e2e | per-hook `fatal` |
| `pre_playwright` | e2e | per-hook `fatal` |
| `post_e2e` | e2e | ALWAYS warn-only |

Worked examples from hr-specs (its `codegraph-ops.toml` is the reference):

```toml
# pgmq patch needs the supabase containers up; a failure SHOULD stop the run.
[[hooks]]
name = "pgmq-patch"
exec = "scripts/patch-pgmq.sh"
on = "pre_e2e"

# View refresh after migration is best-effort: never block the suite on it.
[[hooks]]
name = "refresh-views"
exec = "scripts/refresh-hr-reports-views.sh"
on = "post_migrate"
fatal = false

# Sync the generated UI into the monorepo before the production build.
[[hooks]]
name = "sync-ui"
exec = "scripts/rsync-ui.sh"
on = "pre_playwright"
```

Consumers upgrading from pre-#357 manifests: a hook that previously failed
without aborting (the old blanket warn-only behavior) now needs an explicit
`fatal = false`, or it aborts the suite.
