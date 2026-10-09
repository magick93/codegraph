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
testkit api [--no-migrate] [--rebuild] [--regen]
```

| Stage | What it does |
|-------|--------------|
| 0. Fast doctor (api subset) | Cheap stage-0 gate: psql `SELECT 1`, hurl, python3 (if smoke configured), api port, disk headroom. Fails in seconds with a hint instead of after the build. Marker: `Fast doctor`. |
| 0. Generate + build | Regenerate the app via the graph binary (hooks: `pre_generate`/`post_generate`), fail on generation errors unless `--allow-gen-errors`, generator-rev check, then `cargo build` in the generated app. Skipped by `--skip-build` (generation alone by `--skip-generate`). |
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
| E2E 1. Supabase | `npx supabase start` (or detect running), then `pre_e2e` hooks (e.g. the pgmq patch). |
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
| `APP_DATABASE_URL` | exported (unless set) | The `app_user` serving pool for the generated server; defaults to the migration's `app_user` password. Set it yourself to override. |
| `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH` | read + exported | Override for the chromium binary; the e2e suite exports it when a system chromium is found, otherwise falls back to `npx playwright install chromium`. |
| `CODEGRAPH_ROOT` | exported via `--codegraph-root` | Inherited by hooks, cargo, and the generated app; lets manifests reference the codegraph checkout via `{env:CODEGRAPH_ROOT}`. |
| `CORNUCOPIA_DATABASE_URL` | exported | Build-time Postgres URL for the cornucopia provider's `build.rs` (also set for app-binary spawns). |
| `PSQL_PATH`, `NPX_PATH` | read | Directory overrides for psql / npx discovery (PATH and common install locations are probed otherwise). |
| `IFML_API_KEY` | read (generated config) | NOT consumed by the harness itself: the generated IFML Playwright config uses it for env-gated Bearer auth (`extraHTTPHeaders`) when running against an authenticated API. |

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

CI wiring: run the api suite with a Postgres service and metrics export:

```bash
cargo run -p testkit -- api --metrics ci.tsv
```

The codegraph repo's own `test-ops-integration` job (postgres service +
the `--ignored` integration tests) is the reference pattern; see the
"Ops Harness" section of `AGENTS.md` for the consumer integration guide.

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
