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
//! cargo test -p codegraph --test entity_codegen_gate -- --ignored --nocapture --test-threads=1
//! ```
//!
//! `--test-threads=1` is REQUIRED: both tests regenerate the same fixture
//! in place (and install into the same `node_modules`), so a parallel libtest
//! run corrupts itself. The nightly workflow
//! (`.github/workflows/entity-gate.yml`) always passes it.
//!
//! Needs node 22 + npx. The entity e2e lane (`entity_specs_run_green`,
//! issue #575) holds the emitted-config coverage invariant over every
//! emitted test file and runs the sessionless `auth` project live against
//! a booted API over `vite preview`; the UI-driven families stay the
//! consumer `testkit e2e` lane (they need the Supabase persona stack).
//!
//! Diagnostic attribution (issue #634): svelte-check prints the file path on
//! the line BEFORE the `Error:`/`Warn:` line, so the pre-#634 assertion
//! (which filtered lines containing `"Error:"`) could never match a path and
//! the "zero diagnostics on generated paths" gate was vacuous. The gate now
//! parses `path:line:col` + severity blocks, RECONCILES the parsed error
//! count against svelte-check's own summary line (and its exit status), and
//! asserts on every diagnostic inside a GENERATED path. Consumer-authored /
//! gate-owned paths are reported explicitly (and written to
//! `target/entity-gate/consumer-diagnostics.txt`) instead of silently
//! disappearing.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use codegraph_ops::proc::ManagedProcess;
use test_framework::node_project::NodeProject;
use test_framework::postgres::{GateDb, resolve_base_target};
use test_framework::{axum_server, playwright, process};

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

// ── svelte-check report parsing (issue #634) ────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Severity {
    Error,
    Warn,
}

#[derive(Debug, Clone)]
struct Diagnostic {
    path: PathBuf,
    line: u32,
    column: u32,
    severity: Severity,
    message: String,
}

impl Diagnostic {
    /// The path relative to the fixture's `ui/` directory, `/`-normalized
    /// when the diagnostic points inside it.
    fn ui_relative(&self, ui: &Path) -> Option<String> {
        let rel = self.path.strip_prefix(ui).ok()?;
        Some(rel.to_string_lossy().replace('\\', "/"))
    }
}

/// Parse a `path:line:col` header line. svelte-check prints absolute paths
/// on Unix; the trailing two `:`-segments are the position.
fn parse_path_header(line: &str) -> Option<(PathBuf, u32, u32)> {
    let (path, line_no, column) = {
        let mut parts = line.rsplitn(3, ':');
        let column: u32 = parts.next()?.parse().ok()?;
        let line_no: u32 = parts.next()?.parse().ok()?;
        let path = parts.next()?;
        (path, line_no, column)
    };
    if path.is_empty() || !Path::new(path).is_absolute() {
        return None;
    }
    Some((PathBuf::from(path), line_no, column))
}

/// Parse svelte-check's report: `path:line:col` header, then the
/// `Error:`/`Warn:` line, plus the final `svelte-check found N errors and
/// M warnings[ in K files]` summary. Unattributed `Error:`/`Warn:` lines
/// are a hard parser failure — that is exactly the shape drift the pre-#634
/// gate swallowed.
fn parse_svelte_check(output: &str) -> Result<(Vec<Diagnostic>, Option<(usize, usize)>), String> {
    let mut diagnostics = Vec::new();
    let mut pending: Option<(PathBuf, u32, u32)> = None;
    let mut summary = None;
    let mut unattributed = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim_end();

        if let Some(rest) = trimmed.strip_prefix("svelte-check found ") {
            let nums: Vec<usize> = rest
                .split(|c: char| !c.is_ascii_digit())
                .filter(|s| !s.is_empty())
                .filter_map(|s| s.parse().ok())
                .collect();
            if nums.len() >= 2 {
                summary = Some((nums[0], nums[1]));
            }
            continue;
        }

        if let Some((path, line_no, column)) = parse_path_header(trimmed) {
            pending = Some((path, line_no, column));
            continue;
        }

        let severity = if let Some(message) = trimmed
            .strip_prefix("Error: ")
            .or_else(|| trimmed.strip_prefix("Error:"))
        {
            Some((Severity::Error, message.trim().to_string()))
        } else {
            trimmed
                .strip_prefix("Warn: ")
                .or_else(|| trimmed.strip_prefix("Warn:"))
                .map(|message| (Severity::Warn, message.trim().to_string()))
        };
        if let Some((severity, message)) = severity {
            match pending.take() {
                Some((path, line_no, column)) => diagnostics.push(Diagnostic {
                    path,
                    line: line_no,
                    column,
                    severity,
                    message,
                }),
                None => unattributed.push(trimmed.to_string()),
            }
        }
    }

    if !unattributed.is_empty() {
        return Err(format!(
            "{} diagnostic line(s) carried no preceding `path:line:col` header — the gate \
             parser drifted from svelte-check's print shape (a vacuous attribution set is \
             how the pre-#634 gate always passed):\n{}",
            unattributed.len(),
            unattributed.join("\n")
        ));
    }
    Ok((diagnostics, summary))
}

/// Paths the gate does NOT assert (consumer-authored or gate-owned): the
/// openapi-typescript stub + the consumer api module the fixture's
/// hand-authored forms import, and the hand-authored `src/forms` tree the
/// gate tsconfig excludes. Everything else under `ui/src` is generator
/// output and must typecheck clean — the allowlist is fail-closed (a new
/// generated directory is asserted by default).
///
/// `forms/` (top-level, NOT `src/`) is the legacy cosmos form plane whose
/// imports resolve through the consumer monorepo's design system
/// (`@community-os/ui`) — the gate stubs that package, so a diagnostic
/// there measures the stub, not the generator. Its diagnostics are
/// recorded (printed + written) on every run.
fn consumer_owned(path: &str) -> bool {
    matches!(
        path,
        "src/lib/api/types.ts" | "src/lib/api/index.ts" | "src/forms"
    ) || path.starts_with("src/forms/")
        || path.starts_with("src/lib/paraglide/")
        || path.starts_with("forms/")
}

fn generated_diagnostics<'a>(diagnostics: &'a [Diagnostic], ui: &Path) -> Vec<&'a Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .filter(|d| {
            d.ui_relative(ui)
                .is_some_and(|rel| !consumer_owned(&rel))
        })
        .collect()
}

fn consumer_diagnostics<'a>(diagnostics: &'a [Diagnostic], ui: &Path) -> Vec<&'a Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .filter(|d| {
            d.ui_relative(ui)
                .is_none_or(|rel| consumer_owned(&rel))
        })
        .collect()
}

fn render_diagnostic(d: &Diagnostic, ui: &Path) -> String {
    let where_ = d
        .ui_relative(ui)
        .or_else(|| {
            d.path
                .strip_prefix(ui.parent().unwrap_or(ui))
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
        })
        .unwrap_or_else(|| d.path.to_string_lossy().into_owned());
    format!(
        "{}:{}:{} — {}",
        where_,
        d.line,
        d.column,
        d.message
    )
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
    // fixture also carries consumer-authored artifacts (src/forms/*, the
    // gate-owned api stubs) whose fidelity is not the generator's contract
    // — diagnostics against those are reported, not asserted (issue #634:
    // every one is printed AND written to a file, never silently dropped).
    let (diagnostics, summary) = parse_svelte_check(&output)
        .unwrap_or_else(|e| panic!("svelte-check report unparsable: {e}\n--- output ---\n{output}"));
    let (reported_errors, _reported_warnings) = summary.unwrap_or_else(|| {
        panic!(
            "svelte-check printed no `svelte-check found N errors …` summary — the gate \
             cannot reconcile its attribution set against the compiler's own count:\n{output}"
        )
    });
    let parsed_errors = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    assert_eq!(
        parsed_errors, reported_errors,
        "gate parser drift: attributed {parsed_errors} error diagnostic(s) but svelte-check \
         reported {reported_errors}. A mismatch means the report shape moved and the \
         generated-path assertion is blind again (issue #634):\n{output}"
    );
    assert_eq!(
        check_ok,
        reported_errors == 0,
        "svelte-check's exit status ({check_ok}) disagrees with its own summary \
         ({reported_errors} errors) — the gate must fail on the compiler's verdict, not \
         disown it"
    );

    let generated = generated_diagnostics(&diagnostics, &ui_dir());
    let consumer = consumer_diagnostics(&diagnostics, &ui_dir());
    if !consumer.is_empty() {
        let report = consumer
            .iter()
            .map(|d| render_diagnostic(d, &ui_dir()))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(logs_dir().join("consumer-diagnostics.txt"), &report)
            .map_err(|e| e.to_string())
            .unwrap();
        println!(
            "gate: {} NON-GENERATED (consumer-authored/gate-owned) error(s) — recorded, not \
             asserted (target/entity-gate/consumer-diagnostics.txt):\n{report}",
            consumer.len()
        );
    }
    assert!(
        generated.is_empty(),
        "svelte-check reported {} error(s) on GENERATED files:\n{}",
        generated.len(),
        generated
            .iter()
            .map(|d| render_diagnostic(d, &ui_dir()))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

// ── The entity e2e lane (issue #575) ────────────────────────────────────────

/// The generated app crate (package `app`) — the review fixture root IS the
/// app crate (Cargo.toml + src/ + migrations/ at the top).
fn app_dir() -> PathBuf {
    fixture_dir()
}

/// The isolated cargo target dir for the generated-app build (never the
/// host workspace's — the fixture app pins its own dep set).
fn gate_target_dir() -> PathBuf {
    logs_dir().join("cargo-target")
}

/// Every emitted playwright file under `ui/tests` — the lane's coverage
/// contract: the emitted `playwright.config.ts` project table must match
/// each one (`npx playwright test --list` is the oracle). #575 was exactly
/// the orphan class — `tests/e2e/auth.test.ts` matched NO project, so the
/// entity auth suite never executed in the repo's own lane and nobody
/// could tell.
fn emitted_playwright_files() -> Vec<String> {
    fn walk(dir: &Path, ui_root: &Path, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, ui_root, out);
            } else {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.ends_with(".test.ts") || name.ends_with(".spec.ts") {
                    let rel = path
                        .strip_prefix(ui_root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.push(rel);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&ui_dir().join("tests"), &ui_dir(), &mut out);
    out.sort();
    out
}

/// The entity e2e lane (issue #575): the EMITTED playwright config's
/// project table must cover every emitted test file (the `--list`
/// coverage invariant), and the sessionless slice the repo gate can
/// serve — the `auth` project (pure API probes + the unguarded root
/// page, which lands on the `(auth)` login view) — runs green against a
/// booted API over a `vite preview` of the built tree. The UI-driven
/// families (`generated/**`, `webhooks`, journeys) need the Supabase
/// persona stack (the global-setup provisions BaseJump orgs + per-worker
/// sessions through GoTrue) and stay the consumer `testkit e2e` lane's
/// job — their file paths are still held to the coverage invariant, so a
/// future config regression names the orphan file instead of dying
/// silently.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "entity e2e lane: run with --ignored (needs node, chromium, postgres)"]
async fn entity_specs_run_green() {
    if !have_tool("node") || !have_tool("npx") {
        eprintln!("skipping: node/npx not found");
        return;
    }
    let Ok(base) = resolve_base_target().await else {
        eprintln!("skipping: no usable postgres (DATABASE_URL + docker fallback failed)");
        return;
    };

    regenerate_fixture().await.unwrap();
    stub_crewbase_packages().unwrap();
    stub_api_types().unwrap();
    // The shell-plane route conflict (see entity_app_builds_and_typechecks):
    // remove the shell tree before sync.
    let routes = ui_dir().join("src/routes");
    for shell_dir in ["common", "compensation", "events", "recruiting", "rsvp"] {
        let dir = routes.join(shell_dir);
        if dir.is_dir() {
            fs::remove_dir_all(&dir).unwrap();
        }
    }
    // The retired store plane (see entity_app_builds_and_typechecks): stale
    // CRUD stores import the retired public key and fail the BUILD (vite
    // resolves `$env/static/public` strictly), not just the check.
    let stores = ui_dir().join("src/lib/stores");
    if stores.is_dir() {
        for entry in fs::read_dir(&stores).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name != "entity-navigation.ts" && name.ends_with(".ts") {
                fs::remove_file(&path).unwrap();
            }
        }
    }

    let project = NodeProject::new(ui_dir(), logs_dir());
    project.ensure_install().unwrap();
    project.sync_sveltekit().unwrap();

    // ── 1. The coverage invariant: every emitted file matches a project ──
    //
    // The oracle evaluates the EMITTED config's own testMatch regex sources
    // against `tests/`-relative paths — deliberately NOT playwright's
    // --list output, which matches against the ABSOLUTE checkout path and
    // is therefore checkout-dependent: a repo cloned into a directory
    // containing an `e2e/` segment (the `codegraph-e2e` worktree showed
    // this) leaks into the `e2e` project's negative-lookahead and makes
    // orphans match. The emitted config must not depend on where the repo
    // lives, so the invariant enforces the path-independent contract.
    let config_text = fs::read_to_string(ui_dir().join("playwright.config.ts"))
        .map_err(|e| format!("read playwright.config.ts: {e}"))
        .unwrap();
    let sources: Vec<String> = config_text
        .lines()
        .filter(|l| l.contains("testMatch:"))
        .filter_map(|l| {
            let first = l.find('/')?;
            let last = l.rfind('/')?;
            (last > first).then(|| l[first + 1..last].to_string())
        })
        .collect();
    assert!(
        !sources.is_empty(),
        "no testMatch regex sources found in the emitted playwright.config.ts"
    );
    let emitted = emitted_playwright_files();
    let script = logs_dir().join("config-coverage.cjs");
    // Emitted paths are ui-relative and already start with `tests/` — the
    // exact form the config's testDir anchors produce.
    let files_json = serde_json::to_string(&emitted).unwrap();
    let sources_json = serde_json::to_string(&sources).unwrap();
    fs::write(
        &script,
        format!(
            "const files = {files_json};\n\
             const sources = {sources_json};\n\
             const missing = files.filter((f) => !sources.some((s) => new RegExp(s).test(f)));\n\
             if (missing.length) {{\n\
             \x20 console.error('ORPHANS ' + JSON.stringify(missing, null, 2));\n\
             \x20 process.exit(1);\n\
             }}\n\
             console.log('coverage ok: ' + files.length + ' files, ' + sources.length + ' projects');\n"
        ),
    )
    .unwrap();
    let coverage = Command::new("node")
        .arg(&script)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("run node coverage check: {e}"))
        .unwrap();
    let coverage_out = format!(
        "{}{}",
        String::from_utf8_lossy(&coverage.stdout),
        String::from_utf8_lossy(&coverage.stderr)
    );
    println!("lane: {}", coverage_out.trim());
    assert!(
        coverage.status.success(),
        "the emitted playwright.config.ts matches no project for some emitted test file(s):\n{coverage_out}"
    );
    println!(
        "lane: coverage invariant holds — {} emitted file(s), all matched by a project",
        emitted.len()
    );

    // ── 2. The live sessionless slice: the `auth` project over a real API ──
    let Ok(()) = playwright::install_chromium(&project) else {
        eprintln!("skipping: chromium not available and install failed");
        return;
    };

    let db = GateDb::create(base).await.unwrap();
    db.apply_prelude().await.unwrap();
    db.apply_migrations(&app_dir().join("migrations"))
        .await
        .unwrap();
    let api_key = db.provision_api_key().await.unwrap();

    let result = async {
        let bin = axum_server::build_app(
            &app_dir().join("Cargo.toml"),
            &app_dir(),
            &gate_target_dir(),
            &logs_dir(),
            "app",
        )?;
        let database_url = db.url();
        let server = axum_server::boot_server(
            &bin,
            &[
                ("DATABASE_URL", database_url.as_str()),
                ("SUPABASE_JWT_SECRET", "entity-gate-test-secret"),
                ("APP_NAME", "app"),
            ],
            "entity-gate-app",
            &logs_dir(),
        )
        .await?;

        // Serve the BUILT tree over `vite preview` — the emitted config has
        // no webServer block (the ops ui suite boots the server itself), so
        // the gate owns the process and points PUBLIC_SVELTEKIT_URL at it.
        let built = project.run_build()?;
        assert!(built, "vite build failed before the spec run");
        let preview_port = process::free_port();
        let preview_log = logs_dir().join("vite-preview.log");
        let mut cmd = Command::new("npx");
        cmd.args([
            "vite",
            "preview",
            "--port",
            &preview_port.to_string(),
            "--strictPort",
        ])
        .current_dir(ui_dir());
        // Alive until the closure ends — the Drop keeps the preview server
        // up for the whole spec run, then SIGTERMs it.
        let _preview =
            ManagedProcess::spawn(cmd, "entity-gate-preview", &preview_log).map_err(|e| {
                format!(
                    "spawn vite preview: {e} — log tail:\n{}",
                    fs::read_to_string(&preview_log).unwrap_or_default()
                )
            })?;
        let preview_url = format!("http://127.0.0.1:{preview_port}/");
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        loop {
            let reachable = tokio::process::Command::new("curl")
                .args(["-sf", "--max-time", "2", &preview_url])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await
                .map(|s| s.success())
                .unwrap_or(false);
            if reachable {
                break;
            }
            if std::time::Instant::now() >= deadline {
                let tail = fs::read_to_string(&preview_log).unwrap_or_default();
                return Err(format!(
                    "vite preview not ready within 60s. log tail:\n{tail}"
                ));
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // The EMITTED config's env contract (issue #574 S1 kept the entity
        // test plane on the PUBLIC_* spellings; the apiFetch session/env
        // fallback reads API_KEY). No Supabase env — the globalSetup prints
        // its skip line and the persona stack stays out of the gate. The
        // emitted config carries no reporter block (the ops suite owns the
        // console UX), so the json reporter is selected on the CLI — its
        // output lands in PLAYWRIGHT_JSON_OUTPUT_NAME.
        let api_origin = format!("http://127.0.0.1:{}", server.port);
        let ui_origin = format!("http://127.0.0.1:{preview_port}");
        let report = playwright::run_specs_with(
            &project,
            &logs_dir().join("playwright-report.json"),
            &["--project=auth", "--reporter=json"],
            &[
                ("PUBLIC_API_URL", api_origin.as_str()),
                ("PUBLIC_API_KEY", api_key.as_str()),
                ("PUBLIC_SVELTEKIT_URL", ui_origin.as_str()),
            ],
        )?;
        println!(
            "lane: auth project — passed {}, failed {}\n{}",
            report.passed,
            report.failed,
            report
                .titles
                .iter()
                .map(|t| format!("  - {t}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert!(
            report.failed == 0,
            "{} auth spec(s) failed — see {}/playwright-report.json:\n{:#?}",
            report.failed,
            logs_dir().display(),
            report.failures
        );
        // The suite MUST actually have run — an empty report means the
        // project matched nothing (the #575 shape, again).
        assert!(
            report.passed >= 3,
            "the auth project ran {} spec(s) — expected the full suite (3); \
             the config coverage regressed",
            report.passed
        );
        Ok::<(), String>(())
    }
    .await;

    db.cleanup().await;
    result.unwrap();
}

// ── The report-parser pins (issue #634) ─────────────────────────────────────
//
// Node-free: these run in PR CI (`cargo test -p codegraph --test
// entity_codegen_gate`) and are what keep the gate's attribution honest —
// the pre-#634 gate asserted over a set that was always empty.

#[test]
fn parser_attributes_the_path_printed_on_the_preceding_line() {
    // The EXACT shape captured from the ws6 gate run (svelte-check prints
    // the path, then `Error:` on the next line, then an indented code
    // frame) — the shape the pre-#634 `contains("Error:")` filter could
    // never match to a path.
    let output = "\
Loading svelte-check in workspace: /tmp/app/ui
Getting Svelte diagnostics...

/home/u/app/ui/src/lib/forms/fields/select-variant.svelte:10:32
Error: Cannot find module '$lib/components/ui/native-select/index.js'. (ts)
\timport * as NativeSelect from '$lib/components/ui/native-select/index.js';
\t                              ^

/home/u/app/ui/src/routes/request/+page.svelte:7:2
Warn: `<span>` with a mouseover handler must have an ARIA role (svelte)
<span class=\"tooltip\">

====================================
svelte-check found 1 error and 1 warning in 2 files
";
    let (diagnostics, summary) = parse_svelte_check(output).expect("parse");
    assert_eq!(summary, Some((1, 1)));
    assert_eq!(diagnostics.len(), 2);
    let error = &diagnostics[0];
    assert_eq!(error.severity, Severity::Error);
    assert_eq!(
        error.path,
        PathBuf::from("/home/u/app/ui/src/lib/forms/fields/select-variant.svelte")
    );
    assert_eq!((error.line, error.column), (10, 32));
    assert!(error.message.contains("Cannot find module"), "{error:?}");
    assert_eq!(diagnostics[1].severity, Severity::Warn);
}

#[test]
fn parser_summary_shape_without_the_files_suffix() {
    let output = "\
====================================
svelte-check found 0 errors and 0 warnings
";
    let (diagnostics, summary) = parse_svelte_check(output).expect("parse");
    assert!(diagnostics.is_empty());
    assert_eq!(summary, Some((0, 0)));
}

#[test]
fn parser_refuses_unattributed_diagnostics() {
    // If svelte-check's shape moves, an `Error:` with no path header must
    // be a HARD failure — silently zeroing the attribution set is the
    // vacuous-gate failure mode (issue #634).
    let err = parse_svelte_check("Error: something broke (ts)\n").unwrap_err();
    assert!(err.contains("no preceding"), "{err}");
}

#[test]
fn generated_paths_are_fail_closed_and_consumer_paths_are_recorded() {
    let ui = Path::new("/tmp/app/ui");
    let diag = |path: &str, severity| Diagnostic {
        path: PathBuf::from(path),
        line: 1,
        column: 1,
        severity,
        message: "x".into(),
    };
    let diagnostics = vec![
        // Generated planes — asserted (including src/lib/forms, the #634
        // allowlist hole, and a NEW directory nobody listed).
        diag("/tmp/app/ui/src/lib/forms/fields/text.svelte", Severity::Error),
        diag("/tmp/app/ui/src/lib/media/MediaField.svelte", Severity::Error),
        diag("/tmp/app/ui/src/lib/brand-new/thing.ts", Severity::Error),
        // Recorded only.
        diag("/tmp/app/ui/src/lib/api/types.ts", Severity::Error),
        diag("/tmp/app/ui/forms/amount_form.svelte", Severity::Error),
        diag("/outside/the/fixture.ts", Severity::Error),
        // Warnings never fail the gate.
        diag("/tmp/app/ui/src/routes/+page.svelte", Severity::Warn),
    ];
    let generated: Vec<String> = generated_diagnostics(&diagnostics, ui)
        .iter()
        .map(|d| d.ui_relative(ui).unwrap())
        .collect();
    assert_eq!(
        generated,
        vec![
            "src/lib/forms/fields/text.svelte",
            "src/lib/media/MediaField.svelte",
            "src/lib/brand-new/thing.ts",
        ]
    );
    let consumer: Vec<String> = consumer_diagnostics(&diagnostics, ui)
        .iter()
        .map(|d| {
            d.ui_relative(ui)
                .unwrap_or_else(|| d.path.to_string_lossy().into_owned())
        })
        .collect();
    assert_eq!(
        consumer,
        vec![
            "src/lib/api/types.ts",
            "forms/amount_form.svelte",
            "/outside/the/fixture.ts",
        ]
    );
}
