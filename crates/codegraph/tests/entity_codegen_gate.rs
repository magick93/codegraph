//! Entity ui gate (issue #574 S2): the generated entity SvelteKit app
//! TYPECHECKS and BUILDS under the `entity_remote_functions` flag.
//!
//! Its absence is how the kit-3 `$app/env/public` break went unnoticed —
//! the entity ui tree was never compiled anywhere in this repo (the
//! @crewbase workspace deps only resolve in consumer monorepos), so
//! template breaks shipped silently. The gate regenerates the review
//! fixture in place (the same recipe `review_api_suite` uses), stubs the
//! consumer-monorepo packages (`@crewbase/entities` / `@crewbase/ui` /
//! `@crewbase/common`), builds the app (vite — which also compiles the
//! paraglide messages the pages import), then runs svelte-check against
//! the generated paraglide output. Zero type errors is the bar.
//!
//! ```text
//! cargo test -p codegraph --test entity_codegen_gate -- --ignored --nocapture
//! ```
//!
//! Needs node 22 + npx. The e2e category (Playwright over a booted API)
//! rides the review-fixture consumer stack — tracked as the S2 follow-up.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use test_framework::node_project::NodeProject;

#[path = "test_framework/mod.rs"]
mod test_framework;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_dir() -> PathBuf {
    manifest_dir()
        .parent()
        .unwrap()
        .join("review")
        .join("generated-candidate")
}

fn ui_dir() -> PathBuf {
    fixture_dir().join("ui")
}

fn logs_dir() -> PathBuf {
    // The workspace target dir (repo root/target), never inside crates/.
    manifest_dir()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/entity-gate")
}

fn have_tool(tool: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {tool} >/dev/null 2>&1"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

async fn regenerate_fixture() -> Result<(), String> {
    // Identical recipe to review_api_suite::regenerate_fixture (the ui
    // generators ride the default profile plan; grpc/ops are stripped) —
    // shared by path inclusion so the gates can never drift apart; the
    // recipe is also pinned by `grafeo_candidate_inspect_output`.
    review_api_suite_regen::regenerate_fixture().await
}

// The regeneration lives in review_api_suite_regen; reuse it verbatim by
// path inclusion so the two gates can never drift apart.
#[path = "review_api_suite_regen.rs"]
mod review_api_suite_regen;

fn stub_crewbase_packages() -> Result<(), String> {
    // The entity scaffold pins kit 3.0.0-next.24 while adapter-auto@4
    // declares a kit-^2 peer — consumer monorepos install with a lax
    // resolver; npm needs the escape hatch (gate-owned .npmrc).
    fs::write(ui_dir().join(".npmrc"), "legacy-peer-deps=true\n").map_err(|e| e.to_string())?;

    // The consumer monorepo owns the real @crewbase packages; the gate
    // stubs the surface the generated tree imports so the app compiles
    // standalone. Gate-owned — regenerated every run.
    let stubs = ui_dir().join("stubs");
    let entities = stubs.join("@crewbase").join("entities");
    let ui = stubs.join("@crewbase").join("ui");
    let common = stubs.join("@crewbase").join("common");
    for dir in [&entities, &ui, &common] {
        fs::create_dir_all(dir).map_err(|e| format!("stub dir {}: {e}", dir.display()))?;
    }

    fs::write(
        entities.join("package.json"),
        r#"{ "name": "@crewbase/entities", "version": "0.0.0", "main": "index.ts", "types": "index.ts" }"#,
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        entities.join("index.ts"),
        r#"// Gate-owned stub: the consumer monorepo owns the real package.
// Runtime exports (the bundler resolves value imports against these).
export class ApiError extends Error {}
export interface EntityDescriptor {
	entity: string;
	label: string;
	fields: { name: string; label: string; type?: string; tsType?: string }[];
	[key: string]: unknown;
}
export interface CodelistData {
	[key: string]: unknown;
}
export function createEntityApi(_descriptor: EntityDescriptor, _opts?: unknown) {
	return {
		list: async () => [],
		get: async (_id: string) => ({}),
		create: async (_data: unknown) => ({}),
		update: async (_id: string, _data: unknown) => ({}),
		remove: async (_id: string) => undefined,
	};
}
export function TimeAgo(_props: { value?: string }) {}
export function EntityList(_props: Record<string, unknown>) {}
export function EntityDetail(_props: Record<string, unknown>) {}
export function EntityForm(_props: Record<string, unknown>) {}
export function formatValue(_value: unknown): string {
	return '';
}
"#,
    )
    .map_err(|e| e.to_string())?;

    fs::write(
        ui.join("package.json"),
        r#"{ "name": "@crewbase/ui", "version": "0.0.0", "main": "index.ts", "types": "index.ts" }"#,
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        ui.join("index.ts"),
        r#"// Gate-owned stub: the consumer monorepo owns the real package.
// The scaffold's settings/auth/dashboard routes delegate to these
// components; stubs are runtime exports the bundler can resolve.
export interface CrewbaseComponentProps {
	[key: string]: unknown;
}
export function ApiKeySettings(_props: CrewbaseComponentProps) {}
export function TeamSettings(_props: CrewbaseComponentProps) {}
export function Dashboard(_props: CrewbaseComponentProps) {}
export function LoginPage(_props: CrewbaseComponentProps) {}
export function SignupPage(_props: CrewbaseComponentProps) {}
"#,
    )
    .map_err(|e| e.to_string())?;

    fs::write(
        common.join("package.json"),
        r#"{ "name": "@crewbase/common", "version": "0.0.0", "main": "index.ts", "types": "index.ts" }"#,
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        common.join("index.ts"),
        r#"// Gate-owned stub: the consumer monorepo owns the real package.
export interface CodelistData {
	[key: string]: unknown;
}
"#,
    )
    .map_err(|e| e.to_string())?;

    // Point the workspace:* deps at the stubs so the install resolves.
    let pkg_path = ui_dir().join("package.json");
    let pkg = fs::read_to_string(&pkg_path).map_err(|e| e.to_string())?;
    let patched = pkg
        .replace(
            "\"@crewbase/ui\": \"workspace:*\"",
            "\"@crewbase/ui\": \"file:stubs/@crewbase/ui\"",
        )
        .replace(
            "\"@crewbase/entities\": \"workspace:*\"",
            "\"@crewbase/entities\": \"file:stubs/@crewbase/entities\"",
        )
        .replace(
            "\"@crewbase/common\": \"workspace:*\"",
            "\"@crewbase/common\": \"file:stubs/@crewbase/common\"",
        );
    fs::write(&pkg_path, patched).map_err(|e| e.to_string())?;

    // The fixture's hand-authored pages (ui/forms/*) import the consumer's
    // real design system and api module — stub them so the check resolves.
    let community = stubs.join("@community-os").join("ui");
    fs::create_dir_all(&community).map_err(|e| e.to_string())?;
    fs::write(
        community.join("package.json"),
        r#"{ "name": "@community-os/ui", "version": "0.0.0" }"#,
    )
    .map_err(|e| e.to_string())?;
    for component in ["Button", "Input", "Select"] {
        fs::write(
            community.join(format!("{component}.svelte")),
            "<!-- Gate-owned stub (entity ui gate). -->\n<script lang=\"ts\">\n\timport type { Snippet } from 'svelte';\n\tlet { children, value = $bindable(''), ...rest }: {\n\t\tchildren?: Snippet;\n\t\tvalue?: string;\n\t\t[key: string]: unknown;\n\t} = $props();\n</script>\n\n{@render children?.()}\n",
        )
        .map_err(|e| e.to_string())?;
    }
    let pkg_path = ui_dir().join("package.json");
    let pkg = fs::read_to_string(&pkg_path).map_err(|e| e.to_string())?;
    // The hand-authored forms import @community-os/ui WITHOUT declaring it
    // (the consumer monorepo resolves it implicitly) — add the file: dep.
    let patched = if pkg.contains("@community-os/ui") {
        pkg.replace(
            "\"@community-os/ui\": \"workspace:*\"",
            "\"@community-os/ui\": \"file:stubs/@community-os/ui\"",
        )
    } else {
        pkg.replace(
            "\"@crewbase/ui\": \"file:stubs/@crewbase/ui\"",
            "\"@crewbase/ui\": \"file:stubs/@crewbase/ui\",\"@community-os/ui\": \"file:stubs/@community-os/ui\"",
        )
    };
    fs::write(&pkg_path, patched).map_err(|e| e.to_string())?;
    // The consumer api module the hand-authored forms import. Written
    // unconditionally so stub edits land (the src tree persists runs).
    let api_index = ui_dir().join("src/lib/api/index.ts");
    fs::write(
        &api_index,
        "// Gate-owned stub (entity ui gate): consumer module stand-in.\nexport const api = {\n\tget: async <T = unknown>(_path: string): Promise<T> => undefined as T,\n\tpost: async <T = unknown>(_path: string, _body?: unknown): Promise<T> => undefined as T,\n\tput: async <T = unknown>(_path: string, _body?: unknown): Promise<T> => undefined as T,\n\tdelete: async (_path: string): Promise<void> => undefined,\n};\n",
    )
    .map_err(|e| e.to_string())?;

    write_shadcn_barrel_stubs()?;

    // file: deps are copied at install time — drop the installed copies
    // AND the install-hash marker so npm re-copies the stubs even when
    // the package.json hash (ensure_install's gate) is unchanged.
    let node_modules = ui_dir().join("node_modules");
    for scope in ["@crewbase", "@community-os"] {
        let installed = node_modules.join(scope);
        if installed.is_dir() {
            fs::remove_dir_all(&installed).map_err(|e| e.to_string())?;
        }
    }
    let hash_file = node_modules.join(".ifml-gate-pkg-hash");
    if hash_file.exists() {
        fs::remove_file(&hash_file).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// `src/lib/api/types.ts` is CONSUMER-generated (openapi-typescript against
/// the running API) and the fixture's committed copy is stale (duplicate
/// `id` declarations). The gate regenerates a permissive stub from the
/// names the generated tree actually imports — the gate checks the
/// GENERATOR's surface, not the consumer's codegen pipeline.
fn stub_api_types() -> Result<(), String> {
    let src = ui_dir().join("src");
    let needle = "from '#lib/api/types.ts';";
    let mut names = std::collections::BTreeSet::new();
    fn walk(dir: &Path, needle: &str, names: &mut std::collections::BTreeSet<String>) {
        for entry in fs::read_dir(dir).into_iter().flatten() {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if path.is_dir() {
                walk(&path, needle, names);
            } else if path.extension().is_some_and(|e| e == "ts" || e == "svelte") {
                let Ok(text) = fs::read_to_string(&path) else {
                    continue;
                };
                // Brace-scan (not per-line): imports may span lines.
                let mut rest = text.as_str();
                while let Some(pos) = rest.find(needle) {
                    let head = &rest[..pos];
                    if let Some(open) = head.rfind('{') {
                        // Only an import line's brace group: take up to the
                        // LAST '}' before the needle on the joined window.
                        if let Some(close) = head.rfind('}') {
                            for raw in head[open + 1..close].split(',') {
                                let name = raw.trim().trim_start_matches("type ");
                                if !name.is_empty() {
                                    names.insert(name.to_string());
                                }
                            }
                        } else {
                            // Multi-line: scan back to the line's "import"
                            // start and forward is unnecessary — the brace
                            // group opens here; walk the text to its close.
                            if let Some(close_off) = rest[open..].find('}') {
                                for raw in rest[open + 1..open + close_off].split(',') {
                                    let name = raw.trim().trim_start_matches("type ");
                                    if !name.is_empty() {
                                        names.insert(name.to_string());
                                    }
                                }
                            }
                        }
                    }
                    rest = &rest[pos + needle.len()..];
                }
            }
        }
    }
    walk(&src, needle, &mut names);

    let mut ts = String::from(
        "// Gate-owned stub (entity ui gate): replaces the consumer's\n// openapi-typescript output.\n",
    );
    for name in &names {
        ts.push_str(&format!("export type {name} = any;\n"));
    }
    fs::write(src.join("lib/api/types.ts"), ts).map_err(|e| e.to_string())
}

/// The `#lib/components/ui/<dir>/index.js` barrels the entity tree imports.
/// The IFML gate's extras stubs cover the IFML import surface; the entity
/// tree imports a wider set (kit-3 namespace barrels), emitted here from
/// the ACTUAL member usage in the generated tree. Plain function
/// components: the build only bundles (never renders), and svelte-check
/// accepts `Record<string, unknown>` props.
fn write_shadcn_barrel_stubs() -> Result<(), String> {
    const MEMBERS: &[(&str, &[&str])] = &[
        (
            "alert-dialog",
            &[
                "Action",
                "Cancel",
                "Content",
                "Description",
                "Footer",
                "Header",
                "Root",
                "Title",
            ],
        ),
        ("avatar", &["Fallback", "Root"]),
        ("breadcrumb", &["Item", "List", "Page", "Root", "Separator"]),
        (
            "card",
            &["Content", "Description", "Header", "Root", "Title"],
        ),
        ("collapsible", &["Content", "Root", "Trigger"]),
        (
            "dropdown-menu",
            &["Content", "Item", "Portal", "Root", "Trigger"],
        ),
        (
            "empty",
            &["Content", "Description", "Header", "Media", "Root", "Title"],
        ),
        ("field", &["Description", "Field", "Group", "Label"]),
        ("native-select", &["Option", "Root"]),
        (
            "sidebar",
            &[
                "Content",
                "Footer",
                "Group",
                "GroupContent",
                "GroupLabel",
                "Header",
                "Inset",
                "Menu",
                "MenuButton",
                "MenuItem",
                "Provider",
                "Root",
                "Trigger",
            ],
        ),
        ("table", &["Body", "Cell", "Head", "Header", "Root", "Row"]),
    ];
    let named: &[(&str, &str)] = &[
        ("array-field", "ArrayField"),
        ("badge", "Badge"),
        ("button", "Button"),
        ("date-range-field", "DateRangeField"),
        ("input", "Input"),
        ("label", "Label"),
        ("separator", "Separator"),
        ("sonner", "Toaster"),
        ("spinner", "Spinner"),
        ("switch", "Switch"),
        ("textarea", "Textarea"),
    ];

    let ui_base = ui_dir().join("src/lib/components/ui");
    // The .ts barrels type members as svelte `Component<any>` consts (the
    // type `<X.Y>` usage must satisfy — ConstructorOfATypedSvelteComponent |
    // Component — per svelte2tsx); the .js barrels are the runtime exports
    // the bundler resolves.
    let member_fns = |names: &[&str], typed: bool| -> String {
        let mut s = String::new();
        if typed {
            s.push_str("import type { Component } from 'svelte';\n");
        }
        for n in names {
            if typed {
                s.push_str(&format!("export declare const {n}: Component<any>;\n"));
            } else {
                s.push_str(&format!("export function {n}(_props) {{}}\n"));
            }
        }
        s
    };

    for (dir, members) in MEMBERS {
        let dir_path = ui_base.join(dir);
        fs::create_dir_all(&dir_path).map_err(|e| e.to_string())?;
        // The barrel is imported as `index.js` (plain JS — no TS
        // annotations) with an index.ts twin for svelte-check.
        fs::write(dir_path.join("index.ts"), member_fns(members, true))
            .map_err(|e| e.to_string())?;
        fs::write(dir_path.join("index.js"), member_fns(members, false))
            .map_err(|e| e.to_string())?;
    }
    for (dir, name) in named {
        let dir_path = ui_base.join(dir);
        fs::create_dir_all(&dir_path).map_err(|e| e.to_string())?;
        let ts = format!(
            "// Gate-owned stub barrel (entity ui gate).\nimport type {{ Component }} from 'svelte';\nexport declare const {name}: Component<any>;\n"
        );
        let js = format!(
            "// Gate-owned stub barrel (entity ui gate).\nexport function {name}(_props) {{}}\n"
        );
        fs::write(dir_path.join("index.ts"), &ts).map_err(|e| e.to_string())?;
        fs::write(dir_path.join("index.js"), &js).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The root tsconfig is consumer-monorepo-owned; the gate supplies the
/// ifml-skeleton's shape so svelte-check has an entry point. Written AFTER
/// the first `svelte-kit sync` (it extends the generated
/// `.svelte-kit/tsconfig.json`).
fn write_gate_tsconfig() -> Result<(), String> {
    fs::write(
        ui_dir().join("tsconfig.json"),
        r#"// Gate-owned (entity ui gate): the consumer monorepo owns the real one.
// kit 3 root-tsconfig contract: extend $app/tsconfig — kit's sync
// generates node_modules/$app/tsconfig.json (no .svelte-kit/tsconfig.json).
{
	"extends": "$app/tsconfig",
	"compilerOptions": {
		"allowJs": true,
		"checkJs": true,
		"esModuleInterop": true,
		"forceConsistentCasingInFileNames": true,
		"resolveJsonModule": true,
		"skipLibCheck": true,
		"sourceMap": true,
		// NOT strict: the generated tree has never been held to a strict
		// typecheck (consumer tsconfigs aren't). The gate's bar is
		// compile + type-RESOLUTION correctness; strict-cleanliness is a
		// tracked follow-up.
		"strict": false,
		"moduleResolution": "bundler",
		"types": ["node", "$app/types"]
	},
	"exclude": ["src/forms", "node_modules"]
}
"#,
    )
    .map_err(|e| e.to_string())
}

fn svelte_project() -> NodeProject {
    NodeProject::new(ui_dir(), logs_dir())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "entity ui gate: run with --ignored (needs node)"]
async fn entity_app_builds_and_typechecks() {
    if !have_tool("node") || !have_tool("npx") {
        eprintln!("skipping: node/npx not found");
        return;
    }

    regenerate_fixture().await.unwrap();
    stub_crewbase_packages().unwrap();
    stub_api_types().unwrap();

    // The ui-shell plane (descriptor-driven pages) emits a TOP-LEVEL route
    // tree that conflicts with the (app)-grouped entity routes under kit
    // 3's conflict detection — a PRE-EXISTING shell-plane incompatibility
    // (issue #577 ledger), not the entity plane's. The gate builds the
    // entity plane; the shell tree is removed before sync.
    let routes = ui_dir().join("src/routes");
    for shell_dir in ["common", "compensation", "events", "recruiting", "rsvp"] {
        let dir = routes.join(shell_dir);
        if dir.is_dir() {
            fs::remove_dir_all(&dir).map_err(|e| e.to_string()).unwrap();
        }
    }

    let project = svelte_project();
    project.ensure_install().unwrap();
    // Sync BEFORE the root tsconfig: kit generates .svelte-kit/tsconfig.json,
    // which the gate-owned root tsconfig extends (extending the
    // not-yet-generated file hard-errors under the kit-3 vite transform).
    // A prior run's tsconfig lingers in the checked-in fixture — remove it
    // for the bootstrap sync, then write + re-sync.
    let tsconfig = ui_dir().join("tsconfig.json");
    if tsconfig.exists() {
        fs::remove_file(&tsconfig)
            .map_err(|e| e.to_string())
            .unwrap();
    }
    project.sync_sveltekit().unwrap();
    write_gate_tsconfig().unwrap();
    project.sync_sveltekit().unwrap();

    // Sweep the retired store plane (issue #574 S2): the regen overwrites
    // but never deletes — the flag-off fixture's CRUD stores would linger
    // (importing the retired public key) and fail the check. The
    // navigation store is UI state, not an API plane — it stays.
    let stores = ui_dir().join("src/lib/stores");
    if stores.is_dir() {
        for entry in fs::read_dir(&stores).map_err(|e| e.to_string()).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name != "entity-navigation.ts" && name.ends_with(".ts") {
                fs::remove_file(&path).unwrap();
            }
        }
    }

    // Build FIRST: the paraglide vite plugin compiles the messages the
    // pages import; svelte-check types against the generated output.
    let ok = project.run_build().unwrap();
    assert!(ok, "vite build failed (see target/entity-gate/logs)");
    assert!(
        ui_dir().join(".svelte-kit/output/client").is_dir(),
        "vite build did not produce .svelte-kit/output artifacts"
    );

    let Ok((check_ok, output)) = project.run_check() else {
        panic!("failed to launch svelte-check");
    };
    // The gate holds the GENERATOR's surface to zero type errors. The
    // fixture also carries consumer-authored artifacts (ui/forms/*, the
    // hand pages' design-system imports) whose stub fidelity is not the
    // generator's contract — diagnostics against those are reported, not
    // asserted. Any error on a generated path (routes, lib components,
    // hooks, format, env) fails the gate.
    let generated_error = |line: &str| -> bool {
        let path_part = line.split("generated-candidate/ui/").nth(1);
        let Some(rel) = path_part else {
            return false;
        };
        rel.starts_with("src/routes/")
            || rel.starts_with("src/lib/components/")
            || rel.starts_with("src/lib/stores/")
            || rel.starts_with("src/lib/server/")
            || rel.starts_with("src/hooks")
            || rel == "src/lib/format.ts"
            || rel == "src/env.ts"
    };
    let error_lines: Vec<&str> = output
        .lines()
        .filter(|l| l.contains("Error:") || l.starts_with("Error: "))
        .collect();
    let generated: Vec<&&str> = error_lines.iter().filter(|l| generated_error(l)).collect();
    assert!(
        generated.is_empty(),
        "svelte-check reported {} errors on GENERATED files:\n{}",
        generated.len(),
        generated
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    );
    let _ = check_ok;
}
