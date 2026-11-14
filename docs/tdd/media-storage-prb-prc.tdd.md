# TDD evidence report — Media storage plane, PR-B + PR-C + R7 (codegraph#625)

**Branch:** `media-plane-prb-prc` (off `master` @ 908e4328, one PR covering
issues #630 + #631 + #632; epic #625, PR-A #626 is the base)
**Runner:** `cargo test -p <crate>` per crate; workspace gates run with
`RUSTFLAGS="-C debuginfo=0"` (mirrors CI).
**Source plan:** `/tmp/opencode/media-prb-prc.plan.md` (working notes); the
design is magick93/codegraph#625 (the reviewed memo distilled).

## Issue coverage

| Issue | Deliverables landed |
|---|---|
| #630 (PR-B, Supabase leg) | `supabase.tera` adapter + `MediaHttp` seam + native reqwest transport; per-domain AppState slots + boot construction; `pub mod media;` (PR-A's emission was unreachable without it); `023000_media_storage.sql` (bucket + org-scoped `storage.objects` RLS); handler-site deletion cleanup; signed display route (307 to the minted URL); `$lib/media` fallback upload control; `hurl/09_media.hurl` |
| #631 (R7, deletion) | overwrite-in-place key lowering (`media/keys.rs`); pre-rendered DELETE-handler cleanup block (no child-row read — the key is a pure function of org/entity/id/field); `testkit storage --gc` (self-discovering, report-by-default) |
| #632 (PR-C, R2 leg) | `r2.tera` adapter (S3 REST + hand-rolled SigV4, region as a parameter, no SDK); **wasm32 compile pin legs for both planes with NO HTTP dependency**; SigV4 KAT executed against the AWS-published S3 GET vector; `[[r2_buckets]]` wrangler emission (queue-block pattern); presign issuance route (presigned mode); route revival-lite |

## Task report (RED → GREEN)

| Task | RED evidence | GREEN evidence | What the passing tests guarantee |
|---|---|---|---|
| T1 adapters + seam | `media_pipeline_tests` → 3 FAILED (exact-file-set pins; the PR-A "no adapters" assertion flipped deliberately) | `ok. 6 passed` (commit 3162bffa) | File sets per provider (trait + noop + transport + ONE adapter); provider purity (no reqwest/SigV4 inside adapters, transport injected via `MediaHttp`); `http.rs` cfg-gated for the wasm pin; s3 provider refused with a named error until its slice |
| T2 SigV4 + wasm pin | `r2_sigv4_matches_the_published_aws_vector` FAILED (region hardcoded `auto` — the vector needs `us-east-1`) | `ok. 6 passed` (region/service became request parameters) | The emitted SigV4 core reproduces the AWS-published signature `f0e8bdb8…` (verified against python first, then pinned); BOTH planes compile natively AND on `wasm32-unknown-unknown` with async-trait/thiserror/hmac/sha2/hex only — no reqwest on wasm; the teeth pin rejects a renamed trait method |
| T3 one provider per app | `mixed_providers_across_domains_are_config_error_naming_both` FAILED (fixture bug: core had no media) | `ok. 11 plan + 49 init` (commit 01d91f68) | Mixed providers = `Error::Config` naming both domains + both providers; doctor runs the SAME shared validator — they cannot drift |
| T4 storage bootstrap migration | in-crate unit KATs + pipeline pins (supabase ⇒ `023000_media_storage.sql`; r2 ⇒ none) | `ok. 4 in-crate + 2 pipeline` (commit ca5d9d0a) | Bucket upsert carries `file_size_limit` + `allowed_mime_types` from config; org-scoped policy ONLY when the template's first segment is `{org}` (the expressible subset — the app gate stays authoritative); plain-Postgres fail-soft; sqlite targets refused |
| T5 AppState slots + boot wiring | `scaffold_wires_the_per_domain_media_slot_and_module_declaration` FAILED (empty generation order — fixture bug) | `ok. 2` (commit 392db340) | Per-domain `{domain}_media_storage` slots; server.rs constructs the transport ONCE + each adapter from provider env (missing env ⇒ `None` + warn — 503 semantics); `pub mod media;` declared (PR-A's emission was dead without it — no fixture carried `[media]`, so the missing declaration was invisible); flag-off byte-identity via the canaries — which CAUGHT a first-cut server.tera two-newline leak, fixed with the balanced-whitespace pattern |
| T6 deletion cleanup (#631) | `delete_handler_carries_media_cleanup_when_media_declared` (written with the impl) | `ok. 2` (commit ba6d7676) | The DELETE handler recomputes keys (R7) and deletes best-effort post-command pre-204; block placement pinned; absent flag-off + under workers (the recorded worker-placement deferral); 4 in-crate key KATs incl. brace-escaped config names |
| T7 `testkit storage --gc` | `storage::tests` written with the impl (dlq pattern) | `codegraph-ops ok. 253` (commit 6dfd5b7f) | Self-discovering media child tables (url+mime_type shape); bucket filter interpolated only from safe names; report-by-default, `--gc` deletes; plain-Postgres fail-soft |
| T8 route revival-lite + presign + wrangler | `through_app_mode_emits_child_row_upload_routes` FAILED (multiline slot reference — assertion fixed to be whitespace-tolerant) | `ok. 17 pipeline + 4 wrangler + contract pin` (commit 16e0a392) | Routes ONLY for through-app/presigned (R5 — client-direct emits nothing); the CHILD-row model (R4 — the dormant template wrote parent columns); dispatch through the AppState slot, NEVER the hooks-crate registry; presigned mounts `POST …/presign` and no multipart PUT (router + api contract, both mode-aware); `[[r2_buckets]]` only for r2 media domains (TOML-parse pinned); hurl contract emits the deterministic authn legs only |
| T9 UI fallback control | `media_fields_emit_the_fallback_upload_control` (written with the impl) | `ok. 17` (commit 16e0a392) | `$lib/media/{media.ts,MediaField.svelte}` presence-gated; testid contract; transport-agnostic `upload` callback (the page/override supplies transport — memo R6 tiering); no media ⇒ no lib/media files |

## Test specification (guarantee table)

| # | What is guaranteed | Test file | Result |
|---|---|---|---|
| 1 | Adapter emission + provider purity + exact file sets | `tests/media_pipeline_tests.rs` | PASS (17) |
| 2 | SigV4 KAT vs AWS-published vector (EXECUTED, not string-pinned) | `tests/media_compile_tests.rs` | PASS (6) |
| 3 | Native + wasm32 compile legs, NO HTTP dep on wasm | same | PASS |
| 4 | Pin teeth (renamed trait method ⇒ compile failure) | same | PASS |
| 5 | One provider per app + doctor parity | `tests/media_plan_tests.rs` + `tests/init_tests.rs` | PASS (11 + 49) |
| 6 | Migration content + presence rules | `codegraph-generate/src/media/migration.rs::tests` + pipeline | PASS (4 + 2) |
| 7 | Key lowering + cleanup block shape | `codegraph-generate/src/media/keys.rs::tests` | PASS (4) |
| 8 | Handler cleanup presence/placement/negatives | `tests/media_pipeline_tests.rs` | PASS |
| 9 | gc SQL shape/safety | `codegraph-ops/src/storage.rs::tests` | PASS (4) |
| 10 | Wrangler `[[r2_buckets]]` gating (TOML-parsed) | `scaffold/worker.rs::tests` | PASS (4) |
| 11 | UI control emission + flag-off absence | `tests/media_pipeline_tests.rs` | PASS |
| 12 | Byte-identity: no `[media]` ⇒ unchanged trees | `ux_rules_byte_identity_tests` (4), `forms_byte_identity_tests` (4), `policy_rls_snapshot_tests` (7) | PASS |
| 13 | api contract media routes (mode-aware, literal `{id}`) | `api::contract::tests` | PASS |

## Final gate sweep (all green)

- `cargo test -p codegraph` — full crate suite, zero failures (incl. the 17
  media pipeline pins, 6 compile pins, plan 11, init 49, template_harness 118,
  evt/ux/forms/policy_rls canaries, profile_smoke 33, api_contract 15).
- `cargo test -p codegraph-generate --lib` — 1005 (incl. the 7 media modules'
  KATs, wrangler pins, capability audit).
- `cargo test -p codegraph-core --lib` (121), `-p codegraph-config` (149),
  `-p codegraph-ops` (253).
- `cargo clippy -p codegraph-generate -p codegraph-core -p codegraph-config
  -p codegraph-ops -p codegraph --all-targets` clean; `cargo fmt --check`
  clean.

## Coverage and known gaps (all recorded deferrals)

- **Workers-topology media runtime** — the wrangler `[[r2_buckets]]` block
  emits, but worker AppState construction (binding put via `env.BUCKET`) is
  the recorded deferral (the #450 event-consumer precedent: warn + skip).
  Worker handlers compile WITHOUT the media cleanup block (pinned).
- **DTO-level batch display hydration in list/detail loads** — the signed
  display surface is the media route (307 redirect); hydrating
  `{field}_display_url` into DTOs needs query-plane surgery and is deferred.
- **UI page-level wiring** — the fallback control is emitted; the generated
  edit-page wiring (data.remote.ts `upload{Field}` commands + TanStack branch)
  is #633's slice together with the POM upload method + e2e family. The
  nightly `entity_codegen_gate` (svelte-check + vite build) is the arbiter
  for emitted Svelte — PR CI stays node-free by policy.
- **Client-direct browser uploader** — needs the provider session in the
  browser (supabase-js); consumers use the override hook (crewbase
  AvatarPicker, the vertical slice). The built-in control is transport-
  agnostic by construction.
- **S3 adapter** — refused with a named config error; R2's adapter accepts
  any S3-compatible endpoint today (`R2_S3_ENDPOINT`).
- **gc byte sweep** — v1 deletes `storage.objects` catalog rows over psql
  (dlq/replay precedent); the service-key REST sweep is the follow-up.
- **hurl happy paths** — provider-dependent legs (2xx/503 by env) live in
  the runtime lane (#625 open question 8); the static contract pins the
  deterministic authn surface.

## Deviations from the plan (recorded)

1. **SigV4 region/service moved onto `SigV4Request`** (plan said const):
   the KAT demands `us-east-1` while R2 answers `auto` — a parameter keeps
   the core provider-independent AND makes the KAT possible.
2. **`build_scaffold_domains` now returns `Result`** — the scaffold is an
   emission surface for media slots, so it shares the run path's media
   validation (callers updated: scaffold, worker, seed, evt_events).
3. **Media hurl emission lives in `MediaStorageGenerator`** (global, once
   per run) rather than the per-entity hurl_contract generator — the media
   surface is domain-scoped, not entity-numbered (the `09_` band).
4. **`media_route.tera` handler renames** (`display`/`remove`, not
   `download`/`delete`) — the dormant names collided with the entity CRUD
   surface's vocabulary; the router mounts match.
