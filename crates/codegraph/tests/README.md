# codegraph tests

Layered test suite for the codegraph generator. CI lanes:

- **PR CI (`test` job)** — node-free, DB-free: unit tests, byte-goldens,
  string pins, and the **compile pins** (below).
- **`test-ops-integration`** — the `--ignored` tests (real Postgres:
  ops harness, hooks runtime pin, worker wasm32 gate, wrapper compile
  gate).
- **Nightly** — live-app gates (`review_api_suite`, `ifml_codegen_gate`,
  `child_tenant_contract_tests`, `media_runtime_tests`).

Local full-suite runs MUST mirror CI's `RUSTFLAGS: -C debuginfo=0`:
`RUSTFLAGS="-C debuginfo=0" cargo test --workspace` (see AGENTS.md
"Testing" — without it every test binary links full debuginfo).

## Compile-pin policy (issue #551)

**Every generator that emits Rust ships a test that COMPILES its output.**
String asserts ("the generated source contains X") are not enough — they
passed through both #517 (unresolvable `mox_types` mirror) and #536 (wasm
break). The full-app compile gate (`grafeo_e2e_tests ::
grafeo_generated_code_compiles`) is the whole-tree backstop, but a
failure there is neither plane-attributable nor cheap; per-plane pins
make emitter breakages fail in seconds naming the plane.

### The template

`mox_plane_compile_tests.rs` is the reference implementation:

1. Generate the fixture plane in-process (`GrafeoEngine::in_memory()` +
   `run_generators_with_opts`) into a **fixed materialization root**
   (`std::env::temp_dir()/…`, wiped per run — a failed run's tree stays
   inspectable, the #550 convention).
2. Assemble a scratch cargo workspace carrying ONLY the plane's files and
   dependency set (write your own `Cargo.toml`s; never copy manifests
   with git+rev deps — pins must resolve offline via path deps).
3. `cargo check` it with a **shared cached target dir** (a subdir of the
   workspace `target/`, env-overridable) so repeat runs stay warm.
4. Compile on native AND `wasm32-unknown-unknown` where the generator
   claims both targets.

Existing pins: `mox_plane_compile_tests` (#517),
`repository_compile_tests` and `command_compile_tests` (#551),
`worker_wasm_compile_tests` (#550), `workers_gateway_compile_tests`
(#521), `wrapper_compile_tests` (#348), `grafeo_e2e_tests` compile gate
(backstop). The survey table with the per-plane assignments lives in
AGENTS.md ("Compile-pin policy").

### Adding a pin for a new generator

1. New test file `crates/codegraph/tests/<plane>_compile_tests.rs`
   following the template above.
2. Presence-guard the emitted files first (silent-emission regressions —
   the #517 class — must fail before `cargo check` runs).
3. Teeth-validate à la #550: temporarily break the emission (a broken
   import or a deleted method), run the pin, watch it fail naming the
   plane, revert. Note the validation in the test header.
4. Wire the gate into AGENTS.md ("Testing" block + the policy survey
   table).

### Shared scaffolding

- `test_framework/cornucopia_queries_stub.rs` — runtime-capable stub for
  the generated `cornucopia-queries` crate (its `GenericClient` surface
  must track `templates/scaffold/db_client.tera`; all consumers fail
  loudly on drift).
- `test_framework/` — `NodeProject`, `postgres.rs` GateDb,
  `axum_server.rs`, `playwright.rs` for the live gates.

Gate Postgres targets (`test_framework/postgres.rs`): `apply_prelude` runs on a
superuser ADMIN target resolved from the BASE target (that is `DATABASE_URL`'s
role, or the docker fallback container when the URL is unusable) — the role
itself when it is a superuser, else `GATE_ADMIN_USER` (default
`supabase_admin`) with `GATE_ADMIN_PASSWORD` (default: the base URL's
password). The admin is resolved only by the prelude, so a gate that never
calls `apply_prelude` needs no admin role at all. The app plane (migrations,
API keys, the booted app's URL) never gains that privilege, except on a base
that is already a superuser, where the app plane IS the admin target — which
is exactly why the generated `SET LOCAL ROLE app_user` is the RLS boundary.
The docker fallback container is created with `POSTGRES_USER=supabase_admin`,
and a reused container whose contract does not match (init user, image,
`auth.users`, pgcrypto, `authenticator` role) is recreated after a ~30 s
grace, announced by one `gate: recreating …` line on stdout (the lanes run
`--nocapture`).

### Follow-up ledger (planes still riding the backstop only)

Tracked follow-ups reference the AGENTS.md policy section:

- `api/*` scoped pins (handlers, router, openapi) — covered today by the
  full-app gate only.
- Query-plane mox leg (`query.tera` referencing `mox_types::Resource`)
  — needs a fixture with mox ops in the command/query pin.
- Integration catalog/config handlers under the monolith (#521 pins the
  gateway leg).
- Cornucopia repository impl scoped pin (#550 covers it whole-worker).
