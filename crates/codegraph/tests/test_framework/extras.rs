//! Gate-owned scaffolding writers for full-stack SvelteKit tests: the
//! skeleton configs the generator does not emit yet (package.json,
//! vite.config.ts with the `/api` proxy, svelte.config.js, tsconfig, app
//! shell), the playwright config, and the ui stub components. Generator
//! output is never overwritten; see [`write_extras`].

use std::fs;
use std::path::Path;

const PACKAGE_JSON: &str = r#"{
  "name": "ifml-gate-svelte",
  "private": true,
  "version": "0.0.0",
  "type": "module",
  "scripts": {
    "dev": "vite dev",
    "build": "vite build",
    "preview": "vite preview"
  },
  "devDependencies": {
    "@playwright/test": "1.62.0",
    "@sveltejs/adapter-auto": "^4.0.0",
    "@sveltejs/kit": "^2.20.0",
    "@sveltejs/vite-plugin-svelte": "^5.0.0",
    "@types/node": "^22.0.0",
    "svelte": "^5.56.0",
    "svelte-check": "^4.1.0",
    "typescript": "^5.8.0",
    "vite": "^6.3.5"
  }
}
"#;

const VITE_CONFIG: &str = r#"// Gate-provided scaffolding (Wave A). Wave B moves this into the generator.
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

const apiOrigin = process.env.IFML_API_ORIGIN ?? 'http://127.0.0.1:3000';

const apiProxy = {
	'/api': {
		target: apiOrigin,
		changeOrigin: true,
		configure: (proxy: any) => {
			proxy.on('proxyReq', (proxyReq: any) => {
				if (process.env.IFML_GATE_API_KEY) {
					proxyReq.setHeader('authorization', `Bearer ${process.env.IFML_GATE_API_KEY}`);
				}
			});
		}
	}
};

export default defineConfig({
	plugins: [sveltekit()],
	server: { proxy: apiProxy },
	preview: { proxy: apiProxy }
});
"#;

const SVELTE_CONFIG: &str = r#"// Gate-provided scaffolding (Wave A). Wave B moves this into the generator.
import adapter from '@sveltejs/adapter-auto';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	preprocess: vitePreprocess(),
	kit: { adapter: adapter() }
};

export default config;
"#;

const TSCONFIG: &str = r#"// Gate-provided scaffolding (Wave A). Wave B moves this into the generator.
{
	"extends": "./.svelte-kit/tsconfig.json",
	"compilerOptions": {
		"allowJs": true,
		"checkJs": true,
		"esModuleInterop": true,
		"forceConsistentCasingInFileNames": true,
		"resolveJsonModule": true,
		"skipLibCheck": true,
		"sourceMap": true,
		"strict": true,
		"moduleResolution": "bundler"
	}
}
"#;

const APP_HTML: &str = r#"<!-- Gate-provided scaffolding (Wave A). Wave B moves this into the generator. -->
<!doctype html>
<html lang="en">
	<head>
		<meta charset="utf-8" />
		<meta name="viewport" content="width=device-width, initial-scale=1" />
		%sveltekit.head%
	</head>
	<body data-sveltekit-preload-data="hover">
		<div style="display: contents">%sveltekit.body%</div>
	</body>
</html>
"#;

const APP_D_TS: &str = r#"// Gate-provided scaffolding (Wave A). Wave B moves this into the generator.
declare global {
	namespace App {}
}

export {};
"#;

const PLAYWRIGHT_CONFIG: &str = r#"// Gate-owned Playwright config (supersedes the generator's minimal config
// until G1 adds webServer + JSON reporter support to the generator).
import { defineConfig } from '@playwright/test';

const port = Number(process.env.IFML_PREVIEW_PORT ?? 4173);

export default defineConfig({
	testDir: './tests',
	timeout: 30_000,
	use: { baseURL: `http://127.0.0.1:${port}` },
	reporter: [
		['json', { outputFile: process.env.IFML_GATE_REPORT ?? 'test-results/report.json' }],
		['list']
	],
	webServer: {
		command: `npm run preview -- --port ${port} --strictPort --host 127.0.0.1`,
		port,
		reuseExistingServer: !process.env.CI,
		timeout: 120_000
	}
});
"#;

const SKELETON_README: &str = r#"# Gate-provided scaffolding

The files in this directory tree marked "Gate-provided scaffolding (Wave A)"
are written by the IFML codegen validation gate (`crates/codegraph/tests/
ifml_codegen_gate.rs`), not by the generator. Wave B (G1) moves the skeleton
into the generator; afterwards the gate stops writing these files.

Gate-owned (always overwritten): `package.json`, `vite.config.ts`,
`playwright.config.ts`, `src/lib/components/ui/**` stub components. Written
only when absent: `svelte.config.js`, `tsconfig.json`, `src/app.html`,
`src/app.d.ts`. Everything else is generator output.
"#;

/// Write the SvelteKit skeleton the generator does not emit yet. Generator
/// output is never overwritten; the ui stubs, the playwright config, and
/// package.json are gate-owned and refreshed every run. package.json must be
/// gate-owned because the IFML e2e generator emits a minimal e2e-only stub
/// (playwright + typescript) whenever it is absent, which cannot build the
/// SvelteKit app.
pub fn write_extras(svelte: &Path) -> Result<(), String> {
    let write_if_absent = |rel: &str, content: &str| -> Result<(), String> {
        let path = svelte.join(rel);
        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(&path, content).map_err(|e| e.to_string())?;
        }
        Ok(())
    };
    let write_owned = |rel: &str, content: &str| -> Result<(), String> {
        let path = svelte.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&path, content).map_err(|e| e.to_string())
    };
    write_owned("package.json", PACKAGE_JSON)?;
    write_owned("vite.config.ts", VITE_CONFIG)?;
    write_if_absent("svelte.config.js", SVELTE_CONFIG)?;
    write_if_absent("tsconfig.json", TSCONFIG)?;
    write_if_absent("src/app.html", APP_HTML)?;
    write_if_absent("src/app.d.ts", APP_D_TS)?;

    // Gate-owned playwright config: webServer (vite preview) + JSON reporter.
    write_owned("playwright.config.ts", PLAYWRIGHT_CONFIG)?;

    // Gate-owned stub components for the shadcn-svelte pack. They accept the
    // props the generator passes to mapped components (data, fields, testid,
    // rowTestid, on:select, onclick, bind:open, params, children snippet) and
    // render children + data-testids so the generated specs' selectors work.
    let ui = svelte.join("src/lib/components/ui");
    for dir in [
        "button",
        "table",
        "select",
        "dialog",
        "card",
        "navigation-menu",
        "pagination",
        "input",
    ] {
        fs::create_dir_all(ui.join(dir)).map_err(|e| e.to_string())?;
    }
    fs::write(
        ui.join("button/button.svelte"),
        r#"<script lang="ts">
	// Gate-owned stub of the shadcn-svelte Button.
	let {
		onclick,
		testid,
		disabled = false,
		children
	}: {
		onclick?: (event: any) => void;
		testid?: string;
		disabled?: boolean;
		children?: import('svelte').Snippet;
	} = $props();

	function handle_click(event: Event) {
		if (!onclick) {
			return;
		}
		const form = (event.currentTarget as HTMLButtonElement).closest('form');
		if (form) {
			// Re-fire as a real submit so generated handlers observe
			// SubmitEvent semantics (currentTarget = form, validation runs).
			form.addEventListener(
				'submit',
				(e) => {
					e.preventDefault();
					onclick(e);
				},
				{ once: true }
			);
			form.requestSubmit();
		} else {
			onclick(event);
		}
	}
</script>

<button type="button" data-testid={testid} {disabled} onclick={handle_click}>
	{@render children?.()}
</button>
"#,
    )
    .map_err(|e| e.to_string())?;

    fs::write(
        ui.join("table/table.svelte"),
        r#"<script lang="ts">
	// Gate-owned stub of the shadcn-svelte Table.
	let {
		data = [],
		fields = [],
		testid,
		rowTestid,
		'on:select': on_select,
		onselect,
		...rest
	}: {
		data?: any;
		fields?: string[];
		testid?: string;
		rowTestid?: string;
		'on:select'?: (row: any) => void;
		onselect?: (row: any) => void;
		[key: string]: any;
	} = $props();

	void rest;

	const handle_select = (row: Record<string, unknown>) =>
		(on_select ?? onselect)?.(row);
</script>

<table data-testid={testid}>
	<thead>
		<tr>
			{#each fields as field}
				<th>{field}</th>
			{/each}
		</tr>
	</thead>
	<tbody>
		{#each data as item (item.id)}
			<tr data-testid={rowTestid} onclick={() => handle_select(item)}>
				{#each fields as field}
					<td>{item[field]}</td>
				{/each}
			</tr>
		{/each}
	</tbody>
</table>
"#,
    )
    .map_err(|e| e.to_string())?;

    fs::write(
        ui.join("select/select.svelte"),
        r#"<script lang="ts">
	// Gate-owned stub of the shadcn-svelte Select.
	let {
		testid,
		value = $bindable(''),
		values = []
	}: {
		testid?: string;
		value?: string;
		values?: string[];
	} = $props();
</script>

<select data-testid={testid} bind:value>
	{#each values as v}
		<option value={v}>{v}</option>
	{/each}
</select>
"#,
    )
    .map_err(|e| e.to_string())?;

    fs::write(
        ui.join("dialog/dialog.svelte"),
        r#"<script lang="ts">
	// Gate-owned stub of the shadcn-svelte Dialog.
	let {
		open = $bindable(true),
		testid,
		children
	}: {
		open?: boolean;
		testid?: string;
		children?: import('svelte').Snippet;
	} = $props();
</script>

{#if open}
	<div role="dialog" data-testid={testid}>
		{@render children?.()}
	</div>
{/if}
"#,
    )
    .map_err(|e| e.to_string())?;

    fs::write(
        ui.join("card/card.svelte"),
        r#"<script lang="ts">
	// Gate-owned stub of the shadcn-svelte Card.
	let {
		testid,
		item,
		fields,
		params,
		children,
		...rest
	}: {
		testid?: string;
		item?: any;
		fields?: string[];
		params?: Record<string, string>;
		children?: import('svelte').Snippet;
		[key: string]: any;
	} = $props();

	void item;
	void fields;
	void rest;
</script>

<div
	class="card"
	data-testid={testid}
	data-params={params ? Object.keys(params).join(',') : undefined}
>
	{#if children}
		{@render children()}
	{:else}
		<span class="card-body">{item?.title ?? 'Card'}</span>
	{/if}
</div>
<style>
	/* Mapped invocations are self-closing: give the card an intrinsic box so
	   visibility assertions see it. */
	.card {
		padding: 0.75rem;
	}
</style>
"#,
    )
    .map_err(|e| e.to_string())?;

    fs::write(
        ui.join("navigation-menu/navigation-menu.svelte"),
        r#"<script lang="ts">
	// Gate-owned stub of the shadcn-svelte NavigationMenu.
	let {
		testid,
		children
	}: {
		testid?: string;
		children?: import('svelte').Snippet;
	} = $props();
</script>

<nav data-testid={testid}>
	{@render children?.()}
</nav>
"#,
    )
    .map_err(|e| e.to_string())?;

    fs::write(
        ui.join("pagination/pagination.svelte"),
        r#"<script lang="ts">
	// Gate-owned stub of the shadcn-svelte Pagination.
	let { testid, page = 0, pageCount = 1 }: { testid?: string; page?: number; pageCount?: number } =
		$props();
</script>

<nav data-testid={testid} data-page={page} data-page-count={pageCount}></nav>
"#,
    )
    .map_err(|e| e.to_string())?;

    fs::write(
        ui.join("input/input.svelte"),
        r#"<script lang="ts">
	// Gate-owned stub of the shadcn-svelte Input.
	let { testid, value = $bindable(''), type = 'text' }: {
		testid?: string;
		value?: string;
		type?: string;
	} = $props();
</script>

<input data-testid={testid} {type} bind:value />
"#,
    )
    .map_err(|e| e.to_string())?;

    fs::write(svelte.join("SKELETON.md"), SKELETON_README).map_err(|e| e.to_string())?;
    Ok(())
}
