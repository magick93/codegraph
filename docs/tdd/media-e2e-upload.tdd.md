# TDD evidence report — Media test surface, WS-D (codegraph#633)

**Branch:** `ws/633-media-e2e` (off `4c38ad05`, the #659 DD9 head)
**Runner:** `cargo test -p <crate>` per crate; workspace gates run with
`RUSTFLAGS="-C debuginfo=0"` (mirrors CI).
**Scope:** issue #633 — the media plane's test surface, both planes,
POM-driven (zero raw testids in spec bodies, the #316/#318 contract).

## Issue coverage

| Deliverable | Landed |
|---|---|
| POM `upload(file)` / `removeMedia` on the kernel | `BasePage.uploadMedia(field, filePath)` (setInputFiles → busy clears → throw on the control's error → wait for the display), `BasePage.removeMedia(field)`, `BasePage.mediaDisplay(field)` — entity kernel (Tera, gated on the project declaring `[media]`) + IFML kernel (Rust, gated on a page carrying a file slot) |
| `{comp}Upload()` / `{comp}RemoveMedia()` convenience | `{Entity}Page.upload{Field}(filePath)` / `remove{Field}()` / `media{Field}Display()` per media field |
| Entity family `{seg}.media.test.ts` | New `ui/test/media.test.tera`; upload → reference round-trip (a reload re-resolves it server-side) → display renders → remove clears; wrong-mime + oversize rejections; fixtures written to a per-run temp dir |
| Wrong-mime (415) / oversize (413) rejection | Asserted through the control's error surface (client pre-flight mirrors the route's checks). **Status-code deferral**: the route answers `400` for both while utoipa documents 415/413 — `templates/api/media_route.tera` is the api plane's file (TODO(#633) in the spec header + the section below) |
| IFML parity: `input file` → real wired control | File slots pre-rendered in Rust (markup + page-script handler); the field is skipped from the value-bound slot plane (never enters the submit payload); the view's `data.remote.ts` gains `upload{Field}` (multipart FormData through `$lib/server/api`); `PomFieldKind::File` fills via `uploadMedia`; the render spec asserts the control is real and not value-bound |
| E2E family + fixtures gain media fields | Entity family fixtures seed via `build_test_data_json` (never the media field); the IFML fixture builder drops file fields from create bodies (a File in JSON is a `deny_unknown_fields` 422) |
| No-dep consumers untouched | Presence-gated everywhere; `[media]`-free trees are byte-identical (canaries below) |
| Nightly gate lane | `ifml_gate` fixture carries a media class + `contains` field + `[media]` + `field receipt -> input file`; `assert_categories` gains the `media` category |

## Task report (RED → GREEN)

RED evidence (`test:` commit `aea98870`, implementation stashed):

```text
media_e2e_upload_tests: 1 passed; 5 failed
  media_wrapper_fields_never_render_as_text_inputs_or_enter_test_data  FAILED
  pom_kernel_carries_the_media_upload_surface                          FAILED
  page_class_carries_media_conveniences_for_its_media_fields          FAILED
  media_family_emits_the_full_round_trip                               FAILED
  playwright_config_matches_the_media_family                           FAILED
ifml_media_upload_tests: 0 passed; 4 failed
  form_renders_a_real_non_value_bound_file_control                     FAILED
  remote_module_emits_the_server_side_multipart_upload                 FAILED
  pom_drives_the_file_field_through_the_kernel_upload                  FAILED
  without_the_remote_plane_the_file_input_is_still_not_value_bound     FAILED
```

(`without_media_the_plane_is_absent` passes on master by construction: it is
the negative control pinning that no media ⇒ nothing changes.)

| Task | RED | GREEN | What the passing tests guarantee |
|---|---|---|---|
| T1 media never leaks into the UI field plane | pin 1 FAILED (`photo` produced a generic `UiField` → text input + test-data sentinel) | `ok` | `collect_ui_fields` skips both shapes: the classifier's `MediaWrapper` and the mox `ValueObject` the domain's media plan names. No text input, no `testData()` key, no fill method — the create body cannot carry an unknown key |
| T2 kernel + page-class upload surface | pins 2/3 FAILED | `ok` | `uploadMedia`/`removeMedia`/`mediaDisplay` on `BasePage`; per-field conveniences on the page class; kernel surface parity (`pom_kernel_surface_matches_entity_kernel`) still green |
| T3 entity media family + config coverage | pins 4/5 FAILED | `ok` | The family is emitted only for entities with media route fields; the emitted playwright `crud` project matches `media` (no orphan under `entity_specs_run_green`'s coverage invariant); the spec body carries zero raw testids |
| T4 detail-page wiring | (covered by pins 2/4) | `ok` | The detail page renders the control per media field; the load resolves each display URL server-side (the route 307s to a signed provider URL); the detail route's `data.remote.ts` gains `get{Field}Display` / `upload{Field}` / `remove{Field}` |
| T5 IFML file slot | 4 pins FAILED | `ok` | A modeled `input file` renders a real control (input/display/busy/error testids, `onchange` handler, **no value binding**); the upload posts multipart through a server-side command; the submit payload never carries the File; the POM fills via `setInputFiles`; fixtures never post the field |
| T6 gate fixture + category | (nightly lane) | `media` category wired | The gate fixture carries a media class, a `contains` field, `[media]`, and the modeled file field; `assert_categories` requires the `media` spec title |
| T7 byte identity | — | `ok` (4 + 4 + 7) | `[media]`-free trees are unchanged: `forms_byte_identity_tests`, `ux_rules_byte_identity_tests`, `policy_rls_snapshot_tests` all green **without** re-blessing |

## Test specification (guarantee table)

| # | What is guaranteed | Test file | Result |
|---|---|---|---|
| 1 | Media fields never become UI fields (both classification shapes) | `tests/media_e2e_upload_tests.rs` | PASS (6) |
| 2 | Kernel upload/remove surface + page-class conveniences | same | PASS |
| 3 | Entity media family content + playwright config match | same | PASS |
| 4 | No `[media]` ⇒ no family, no kernel surface, config regex untouched | same | PASS |
| 5 | IFML `input file` → wired, non-value-bound control | `tests/ifml_media_upload_tests.rs` | PASS (4) |
| 6 | IFML server-side multipart upload (no browser credential) | same | PASS |
| 7 | POM file fill via the kernel `uploadMedia` + `{form}{Field}File()` | same | PASS |
| 8 | Flag-off IFML file field still never value-bound | same | PASS |
| 9 | Byte identity (entity + IFML, flag off) | `forms_byte_identity_tests` (4), `ux_rules_byte_identity_tests` (4), `policy_rls_snapshot_tests` (7) | PASS |
| 10 | IFML kernel parity (entity ⊆ IFML surface) | `ifml::e2e_test::kernel` unit test | PASS |

## Enumerated default-output diff

**None.** `[media]`-free generation is byte-identical: verified by diffing a
pre-change and post-change generation of (a) an `init`-scaffolded mox app
with the form flags off and (b) a full-default-profile entity+IFML app —
zero file differences, then confirmed by the three committed canaries
without re-blessing.

Media-declaring trees gain exactly: `$lib/media/MediaField.svelte`'s
display/remove surface (an existing file), the detail page's media card,
the detail route's `data.remote.ts` media exports, the detail load's
`media` payload entry, and the new `{seg}.media.test.ts`.

## Nightly lane

- `cargo test -p codegraph --test ifml_codegen_gate -- --ignored --nocapture`
  now requires the `media` category: every form view carrying a modeled
  `input file` emits `media file control renders for {view}`, which asserts
  through the POM that the control is visible, `type=file`, and carries no
  `value` attribute.
- The **live upload leg** (a real PUT → reference row → display) needs a
  configured storage provider. The gate lane runs plain Postgres, so the
  media route answers 503 — the category asserts the WIRED SHAPE and the
  entity plane's `.media.test.ts` owns the round trip (which itself needs
  `[media]` + a provider in the consumer's `testkit e2e` lane). No live leg
  is faked.

## Known gaps (all recorded deferrals)

- **Rejection status codes** — `media_route.tera` answers `400` for an
  unaccepted mime and an oversize upload while its utoipa annotations
  document `415`/`413`. The spec asserts the rejection the user sees (the
  control's message); the status split belongs to the API plane
  (`templates/api/**`, out of this workstream's scope). `TODO(#633)` sits at
  the spec header.
- **Flag-off entity-plane media transport** — the upload control renders on
  the remote-functions detail page only (the default profile). The flag-off
  client plane (`PUBLIC_API_KEY`) is deliberately not resurrected for a new
  surface; a media entity in a flag-off tree emits no media family.
- **Presigned-mode upload** — the family + control assume through-app
  multipart; presign issuance (`POST …/presign` + a direct provider PUT)
  needs a browser-side provider client, i.e. the documented client-direct
  deferral of #625 PR-B.
- **Multi-field ordering** — the family emits one block per media field;
  the display/remove helpers are per-field, so the "remove clears" leg
  seeds its own reference (it does not depend on the upload leg's order).
- **IFML `input file` without an id param** (a create-mode view) — the
  upload route needs an existing row, so the slot renders a plain file
  input (never value-bound) and carries no handler. Server-side create-time
  upload is a follow-up.
- **IFML client pre-flight** — the IFML slot posts without the
  `validateMediaFile` mirror the entity control runs; the route's own
  checks still reject (the error lands in the slot's error element).

## Surfaced pre-existing defects (out of scope, recorded)

- **`hurl_contract` media URL** — the emitted `hurl/09_media.hurl` addresses
  `PUT /api/v1/{domain}/{entity}/receipt/{id}/receipt` (an extra field
  segment before `{id}`) while the route is `/{id}/{field}`. It was invisible
  until a fixture carried a `[media]` field. `crates/codegraph-generate/src/
  api/hurl_contract.rs` is the api plane's file — not fixed here.
- **`router.tera` media routes are not capability-gated** — a profile that
  omits `media_route` still emits the router's `media_{entity}_{field}`
  reference (an E0433). The gate fixture now lists `media_route` +
  `media_storage`; the generator-list gating belongs to the api plane.
