# IFML Multi-Output Code Generation: Implementation Plan

## Overview

Add multi-framework code generation to the IFML pipeline: the same IFML DSL model
generates routes, navigation, and data binding layers for SvelteKit, Next.js (React),
Vue/Nuxt, Flutter, and SwiftUI — driven by user-configurable targets.

---

## Architecture

```
Profile / CLI flags → [framework1, framework2, ...]
                           │
              ┌────────────┴────────────┐
              │                         │
         Tera instance            Tera instance
         (svelte/)                (react/)
              │                         │
         IfmlRouteGen              IfmlRouteGen
         IfmlNavGen                IfmlNavGen
              │                         │
         generated/svelte/        generated/react/
```

- **Template directory**: `templates/ifml/{framework}/` per target
- **Generator param**: `IfmlRouteGenerator::new(output_dir, "svelte")`
- **Output paths**: `OutputPaths::for_framework("svelte")` resolves framework-specific file patterns
- **Shared output**: Framework-agnostic files (types, OpenAPI, E2E tests) → `generated/common/`
- **Component library**: Template-level Tera parameter `component_lib`, default `"plain"`

---

## Task Breakdown

### Layer A: Template Migration (prerequisite for everything else)

**A1 — Migrate route generator from inline to Tera**

| Field | Value |
|-------|-------|
| Files | `route_generator.rs`, `templates/ifml/page_svelte.tera` |
| What | Replace `generate_page_svelte_inline()` + `generate_page_load_inline()` with `render_template()` calls. Merge `PageSvelteContext` and `PageLoadContext` into a single context struct. |
| Acceptance | `cargo test --test ifml_e2e_tests` produces identical output as before |
| Dependency | None |
| Agent | 1 |

**A2 — Reorganize templates into per-framework subdirectories**

| Field | Value |
|-------|-------|
| Files | `templates/ifml/` — move `page_svelte.tera` → `svelte/page.tera`, `page_load.tera` → `svelte/page_load.tera`, `page_list.tera` → `svelte/page_list.tera`, `page_form.tera` → `svelte/page_form.tera`, `page_details.tera` → `svelte/page_details.tera`, `navigation_map.tera` → `svelte/navigation_map.tera` |
| What | Rename/move existing templates into `templates/ifml/svelte/`. Update `IfmlNavigationGenerator` template reference to `ifml/svelte/navigation_map.tera`. |
| Acceptance | `cargo test --test ifml_e2e_tests` passes with updated template paths |
| Dependency | A1 |
| Agent | 1 |

---

### Layer B: Framework Parameterization

**B1 — Create `OutputPaths` resolver**

| Field | Value |
|-------|-------|
| Files | **New:** `crates/codegraph/src/generate/ifml/output_paths.rs`. **Modified:** `crates/codegraph/src/generate/ifml/mod.rs` (add `pub mod output_paths;`). |
| What | Define `OutputPaths` struct with closures for `route_page`, `route_load`, `navigation_map`, `route_helpers`. Implement `OutputPaths::for_framework(framework: &str) -> Self` for all 5 frameworks. |
| Acceptance | Unit tests: `test_output_paths_svelte`, `test_output_paths_react`, etc. verify correct path patterns |
| Dependency | A2 |
| Agent | 1 |

**B2 — Add framework parameter to `IfmlRouteGenerator`**

| Field | Value |
|-------|-------|
| Files | `route_generator.rs` |
| What | Add `framework: String` and `output_paths: OutputPaths` fields. Constructor takes `(output_dir, framework)`. Template resolution uses `ifml/{framework}/page.tera`. File output uses `output_paths`. Generate both `.svelte` and `.ts` (or framework equivalents) based on `output_paths.route_load`. |
| Acceptance | `cargo test --test ifml_e2e_tests` passes with `IfmlRouteGenerator::new(dir, "svelte")` |
| Dependency | B1 |
| Agent | 1 |

**B3 — Add framework parameter to `IfmlNavigationGenerator`**

| Field | Value |
|-------|-------|
| Files | `navigation_generator.rs` |
| What | Same pattern as B2. Template: `ifml/{framework}/navigation_map.tera`. Output paths use `output_paths.navigation_map` and `output_paths.route_helpers`. Kill the inline fallback — if template is missing, error with a clear message. |
| Acceptance | `cargo test --test ifml_e2e_tests` passes |
| Dependency | B1 |
| Agent | 1 |

**B4 — Add `--ifml-framework` CLI flag**

| Field | Value |
|-------|-------|
| Files | `cli.rs` (Run subcommand), `main.rs` (pass to generation) |
| What | Add `#[arg(long)] ifml_framework: Vec<String>` to `Run`. Default: `vec!["svelte".to_string()]`. Pass through `GeneratorOpts`. |
| Acceptance | `cargo run -- run --ifml-framework svelte --ifml-framework react ...` produces two output dirs |
| Dependency | B2, B3 |
| Agent | 2 |

**B5 — Wire framework multiplier in dispatch**

| Field | Value |
|-------|-------|
| Files | `crates/codegraph/src/generate/mod.rs` (generator list construction) |
| What | Read `opts.ifml_frameworks`. For each framework, create an `IfmlRouteGenerator` and `IfmlNavigationGenerator` with that framework's output dir (subdirectory of main output). |
| Acceptance | E2E test with 2 frameworks produces both `generated/svelte/` and `generated/react/` dirs |
| Dependency | B4 |
| Agent | 2 |

---

### Layer C: Profile System

**C1 — Extend profile config for IFML framework multiplier**

| Field | Value |
|-------|-------|
| Files | `crates/codegraph-config/src/lib.rs` or the profiles config types |
| What | Add `ifml_frameworks: Option<Vec<FrameworkTarget>>` to `ProfileDef`/`ResolvedProfile`. Each `FrameworkTarget` has `name: String`, `output: Option<PathBuf>`, `target: Option<String>` (ui/mobile). Parse `[profile.X.ifml]` section from TOML. |
| Acceptance | Unit test: `test_parse_profile_with_ifml_frameworks` |
| Dependency | None (can parallel with A/B) |
| Agent | 3 |

**C2 — Register framework-specific capabilities**

| Field | Value |
|-------|-------|
| Files | `profiles.rs` (IFML capabilities), `profile.rs` (base capabilities) |
| What | Add entries for each framework generator variant: `ifml_route_svelte`, `ifml_route_react`, `ifml_route_vue`, `ifml_route_flutter`, `ifml_route_swiftui`, `ifml_navigation_svelte`, `ifml_navigation_react`, etc. Each requires `ifml_backend` + `framework_{name}` features. |
| Acceptance | `capabilities()` contains all new entries. Profile validation accepts `ifml_route_svelte` in `[ui]` section. |
| Dependency | C1 |
| Agent | 3 |

**C3 — Profile expander to generator instances**

| Field | Value |
|-------|-------|
| Files | `crates/codegraph/src/profile.rs` or `mod.rs` |
| What | `BuildPlan::from_profile()` expands `ifml_frameworks` × generators into concrete generator instances. If profile has frameworks `[svelte, react]` and generators `["ifml_route", "ifml_navigation"]`, the build plan produces 4 entries: `ifml_route_svelte`, `ifml_route_react`, `ifml_navigation_svelte`, `ifml_navigation_react`. |
| Acceptance | Profile with 2 frameworks × 2 generators = 4 entries in plan |
| Dependency | C1, C2 |
| Agent | 3 |

**C4 — Wire per-framework output dirs from profile**

| Field | Value |
|-------|-------|
| Files | `mod.rs` (dispatch), B5 |
| What | When a framework specifies an `output` override in the profile, use that instead of the default `output_dir/{framework}/` subdirectory. Pass to `IfmlRouteGenerator::new(overridden_dir, framework)`. |
| Acceptance | Profile with `output = "./my-svelte-app"` for svelte generates files there |
| Dependency | C3, B5 |
| Agent | 3 |

---

### Layer D: New Framework Templates

**D1 — Next.js (React) templates**

| Field | Value |
|-------|-------|
| Files | **New:** `templates/ifml/react/page.tera`, `templates/ifml/react/page_load.tera`, `templates/ifml/react/page_form.tera`, `templates/ifml/react/navigation_map.tera` |
| What | App Router conventions: `'use client'` + `export default function Page()`, server data loading with `async function`, React Router config for nav map. Match SvelteKit output behavior functionally. |
| Acceptance | E2E test: generate with `--ifml-framework react`, verify output compiles (syntax-check the generated TSX) |
| Dependency | B5 |
| Agent | 4 |

**D2 — Vue/Nuxt templates**

| Field | Value |
|-------|-------|
| Files | **New:** `templates/ifml/vue/page.tera`, `templates/ifml/vue/page_form.tera`, `templates/ifml/vue/navigation_map.tera` |
| What | Nuxt 3 conventions: `<script setup lang="ts">`, `definePageMeta`, `useFetch` for data loading, Vue Router config for nav map. No separate `page_load` — Nuxt auto-imports server data. |
| Acceptance | E2E test with `--ifml-framework vue` |
| Dependency | B5 |
| Agent | 5 |

**D3 — Flutter templates**

| Field | Value |
|-------|-------|
| Files | **New:** `templates/ifml/flutter/page.tera`, `templates/ifml/flutter/navigation_map.tera` |
| What | Dart + Flutter: `StatelessWidget` or `StatefulWidget`, `Scaffold` + `AppBar`, `DataTable` for lists, `Form` + `TextFormField` for forms, named routes in `MaterialApp`. |
| Acceptance | E2E test with `--ifml-framework flutter` |
| Dependency | B5 |
| Agent | 5 |

**D4 — SwiftUI templates**

| Field | Value |
|-------|-------|
| Files | **New:** `templates/ifml/swiftui/page.tera`, `templates/ifml/swiftui/navigation_map.tera` |
| What | SwiftUI: `View` struct with `NavigationStack`, `List` + `ForEach` for list, `Form` + `TextField` for forms, `NavigationLink` for nav map, `.sheet` for modal views. |
| Acceptance | E2E test with `--ifml-framework swiftui` |
| Dependency | B5 |
| Agent | 5 |

---

### Layer E: VS Code Extension

**E1 — Add codegen settings**

| Field | Value |
|-------|-------|
| Files | `package.json` (contributes.configuration), `src/extension.ts` |
| What | Add 3 settings: `ifml.codegen.targets` (array of string, enum: svelte/react/vue/flutter/swiftui, default `["svelte"]`), `ifml.codegen.outputDir` (string, default `"generated"`), `ifml.codegen.lastRun` (string, default `""`). Register `onDidChangeConfiguration` handler to update status bar. |
| Acceptance | Settings appear in VS Code settings UI |
| Dependency | None |
| Agent | 6 |

**E2 — Implement `ifml.selectCodegenTargets` command**

| Field | Value |
|-------|-------|
| Files | `commands/register.ts`, `package.json` (commands) |
| What | Quick Pick with checkboxes for all 5 frameworks. Current selection from `ifml.codegen.targets`. On accept, write to settings. Command title: "IFML: Select Code Generation Targets". |
| Acceptance | Quick Pick shows, toggles persist, status bar updates |
| Dependency | E1 |
| Agent | 6 |

**E3 — Update `ifml.generate` command with framework flags**

| Field | Value |
|-------|-------|
| Files | `commands/register.ts`, `lsp/client.ts` (binary discovery already exists) |
| What | Read `ifml.codegen.targets` and `ifml.codegen.outputDir`. Build terminal command with `--ifml-framework` flags. Fall back to `cargo run` if binary not found. Write `lastRun` timestamp after successful execution. |
| Acceptance | `ifml.generate` produces output with only selected frameworks |
| Dependency | E1, B4 |
| Agent | 6 |

**E4 — Update status bar with active targets**

| Field | Value |
|-------|-------|
| Files | `status-bar.ts`, `extension.ts` |
| What | Add second status bar item (or extend existing) showing active targets. Text format: `IFML: SvelteKit ✓ Next.js ✕ ...`. Click opens `ifml.selectCodegenTargets`. Priority: 99 (below LSP status). |
| Acceptance | Status bar shows active targets, click opens Quick Pick |
| Dependency | E2 |
| Agent | 6 |

**E5 — Add codegen panel to diagram WebView sidebar**

| Field | Value |
|-------|-------|
| Files | `webview/panel.ts`, `webview/sync.ts`, `webview/src/App.svelte`, `webview/src/property-sheet/PropertySheet.svelte`, `webview/src/sync.ts`, `webview/src/types.ts` |
| What | **Extension side:** add `sendCodegenConfig()` method to `SyncEngine`. Handle `sync/codegenToggle` → update settings. Handle `sync/codegenRun` → execute `ifml.generate`. **WebView side:** add "Code Generation" section to `PropertySheet.svelte` with toggle switches, "Generate All" button, last-run timestamp. Add `CodegenConfig` and `FrameworkInfo` to types. |
| Acceptance | Toggle in WebView updates settings; Generate button triggers codegen |
| Dependency | E3 |
| Agent | 7 |

---

### Layer F: Testing

**F1 — Unit tests for OutputPaths**

| Field | Value |
|-------|-------|
| Files | `output_paths.rs` (inline tests) |
| What | Test every framework's path patterns. Test edge cases: empty name, name with spaces, name with special chars. |
| Acceptance | All path patterns produce expected strings |
| Dependency | B1 |

**F2 — Integration tests for multi-framework generation**

| Field | Value |
|-------|-------|
| Files | `crates/codegraph/tests/ifml_e2e_tests.rs` (or new test file) |
| What | E2E test with 2 frameworks. Pipeline: parse `.ifml` → ingest → generate with `--ifml-framework svelte --ifml-framework react`. Verify both output dirs exist with expected file structure. Enable/disable framework via `BuildPlan` toggling. |
| Acceptance | Tests pass in CI |
| Dependency | B5, D1 |

**F3 — VS Code extension tests**

| Field | Value |
|-------|-------|
| Files | `test/extension.test.ts` |
| What | Add tests for: `ifml.selectCodegenTargets` registers, `ifml.codegen.targets` setting exists, `ifml.generate` builds correct command string with framework flags, status bar updates on config change. |
| Acceptance | All extension tests pass |
| Dependency | E1, E2, E3, E4 |

---

## Dependency Graph

```
A1 → A2 → B1 → B2 → B4 → B5 → D1 (React)
                        │       D2 (Vue)
                        │       D3 (Flutter)
                        │       D4 (SwiftUI)
                        │
                C1 → C2 → C3 → C4
                        │
                        └──→ E1 → E2 → E3 → E5
                              └→ E4
```

**Parallelizable batches:**

| Batch | Agents | Tasks |
|-------|--------|-------|
| 1 | Agent 1 | A1, A2, B1 |
| 1 | Agent 3 | C1, C2 (no dependency on A/B) |
| 1 | Agent 6 | E1 (no dependency on Rust) |
| 2 | Agent 1 | B2, B3 |
| 2 | Agent 3 | C3 |
| 2 | Agent 6 | E2, E4 |
| 3 | Agent 2 | B4, B5 (depends on B2, B3) |
| 3 | Agent 3 | C4 (depends on C3, B5) |
| 3 | Agent 4 | D1 (depends on B5) |
| 3 | Agent 5 | D2, D3, D4 (depends on B5) |
| 3 | Agent 6 | E3 (depends on E2, B4) |
| 4 | Agent 7 | E5 (depends on E3) |

---

## File Change Summary

### Rust: `crates/codegraph/`

| File | Change |
|------|--------|
| `src/cli.rs` | Add `--ifml-framework` flag to `Run` |
| `src/main.rs` | Pass `ifml_frameworks` to generation opts |
| `src/generate/mod.rs` | Framework multiplier in dispatch, pass opts |
| `src/generate/traits.rs` | No change (already flexible) |
| `src/generate/template_engine.rs` | No change (already loads `**/*.tera`) |
| `src/generate/ifml/mod.rs` | Add `output_paths` pub mod |
| `src/generate/ifml/output_paths.rs` | **New** — `OutputPaths` struct + `for_framework()` |
| `src/generate/ifml/route_generator.rs` | Framework param, template-based rendering |
| `src/generate/ifml/navigation_generator.rs` | Framework param, template-only (no inline fallback) |
| `src/generate/ifml/profiles.rs` | Framework-specific capability entries |
| `src/profile.rs` | Framework multiplier in profile resolution |
| `templates/ifml/` | Reorganized into `svelte/`, `react/`, `vue/`, `flutter/`, `swiftui/` subdirs |
| `tests/ifml_e2e_tests.rs` | Multi-framework E2E tests |

### VS Code: `codegraph-vscode/`

| File | Change |
|------|--------|
| `package.json` | 3 new settings, 1 new command, 1 new keybinding |
| `src/extension.ts` | Config change handler |
| `src/commands/register.ts` | `ifml.selectCodegenTargets`, updated `ifml.generate` |
| `src/status-bar.ts` | Active targets display |
| `src/webview/panel.ts` | Codegen config sync, message handlers |
| `src/webview/sync.ts` | `CodegenConfig` types, `sendCodegenConfig` |
| `webview/src/types.ts` | `CodegenConfig`, `FrameworkInfo` types |
| `webview/src/sync.ts` | No change (already generic) |
| `webview/src/App.svelte` | Import codegen panel |
| `webview/src/property-sheet/PropertySheet.svelte` | Codegen section UI |
| `test/extension.test.ts` | New tests for codegen commands and settings |

---

## First-User Experience

When a developer opens a `.ifml` file for the first time after this feature ships:

1. Status bar shows: `IFML: SvelteKit ✓` (default target)
2. User opens diagram WebView → sees "Code Generation" panel in sidebar
3. User toggles "Next.js" → setting updates, status bar shows both
4. User clicks "Generate All" → terminal opens, frameworks run sequentially
5. Output appears in `generated/svelte/` and `generated/nextjs/`
6. User opens generated files → full routes, navigation, and data binding
7. On subsequent saves, user presses `Ctrl+Shift+G` to regenerate

---

## Future Considerations (not in scope)

- **User-provided custom templates**: Override any `templates/ifml/{framework}/*.tera` with project-local template files. Would require adding a `--template-override` flag and a template search path.
- **Incremental generation**: Track file hashes and only re-render when the IFML model changed. Worth investigating after multi-framework is stable.
- **Component library presets**: Add `component_lib` parameter to framework config with Tera partials per library (shadcn, MUI, Vuetify). Template macros + include.
- **Custom framework backends**: A plugin system for third-party frameworks. Requires a `IfmlBackend` trait that external crates can implement. Far future.
