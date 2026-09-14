//! Gate-owned scaffolding writers for full-stack SvelteKit tests: the vite
//! `/api` proxy (with Bearer injection), the gate playwright config (the
//! ONLY playwright config), the ui stub components, and the gate-owned
//! `tests/ifml/sweep-create.spec.ts` sweep assertions.
//!
//! The SvelteKit skeleton (package.json, tsconfig, app.html, app.d.ts,
//! vite/svelte configs) is GENERATOR-PROVIDED (Wave B, G1): the
//! `ifml-skeleton` generator emits it and the gate does not write it.
//! Generator output is never overwritten except the files listed as
//! gate-owned below; see [`write_extras`].

use std::fs;
use std::path::Path;

const VITE_CONFIG: &str = r#"// Gate-owned: unlike the generator's skeleton proxy this
// injects the gate API key (IFML_GATE_API_KEY) as a Bearer header.
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

/// Gate-owned sweep assertions (#205 Wave A). Written (overwritten) beside
/// the generated specs after every generation so Wave B fixes them by
/// changing GENERATORS, never specs. The generator's stale-spec cleanup
/// removes this file during regeneration; the gate rewrites it afterwards.
const SWEEP_SPEC: &str = r#"// Gate-owned sweep assertions (issue #205 Wave A). DO NOT EDIT inside the
// generated tree — this file is rewritten by the gate harness
// (crates/codegraph/tests/test_framework/extras.rs).
import { test, expect } from '@playwright/test';

const COLLECTION = '/api/v1/refunds/refund-request';

test.describe('IFML sweep', () => {
	test('create round trip persists a new refund request from the plain form route', async ({
		page,
		request
	}) => {
		await page.goto('/refundrequestform');
		const form = page.getByTestId('editor-form');
		await form.locator('[name="title"]').fill('Sweep create');
		await form.locator('[name="amount"]').fill('4242');
		const submitRequest = page.waitForRequest(
			(req) => req.url().includes('/refund-request') && ['POST', 'PUT'].includes(req.method()),
			{ timeout: 10_000 }
		);
		await form.locator('button').first().click();
		const submit = await submitRequest;
		expect(
			`${submit.method()} ${new URL(submit.url()).pathname}`,
			`create mode (no ?id) must submit 'POST ${COLLECTION}' with no trailing item id`
		).toBe(`POST ${COLLECTION}`);
		await page.waitForURL(/\/refundrequestdetail/);
		const response = await request.get(COLLECTION);
		expect(response.ok(), `GET ${COLLECTION} must succeed`).toBeTruthy();
		const body = await response.json();
		const items: Array<{ title?: string }> = Array.isArray(body) ? body : (body.data ?? []);
		expect(
			items.some((item) => item.title === 'Sweep create'),
			`the created item must persist in ${COLLECTION}`
		).toBe(true);
	});

	test('details shows the persisted values of the loaded item', async ({ page, request }) => {
		const created = await (
			await request.post(COLLECTION, {
				data: {
					title: 'Sweep details',
					amount: 42,
					requesterEmail: 'sweep@example.com',
					submittedAt: '2024-01-15T10:30:00Z',
					urgent: true,
					status: 'draft',
					reason: 'Damaged'
				}
			})
		).json();
		const id = created.data?.id ?? created.id;
		expect(id, `POST ${COLLECTION} must return the created id`).toBeTruthy();
		await page.goto(`/refundrequestdetail?id=${id}`);
		await expect(page.getByTestId('card')).toContainText('Sweep details');
	});

	test('sibling xor containers render one labeled tabs group on the landmark view', async ({
		page
	}) => {
		await page.goto('/home');
		await expect(page.getByTestId('shipping-label')).toBeVisible();
		await expect(page.getByTestId('payment-label')).toBeVisible();
		await expect(page.getByTestId('shipping-label')).toHaveText('Shipping');
		await expect(page.getByTestId('payment-label')).toHaveText('Payment');
		expect(await page.getByTestId('tabs').count()).toBe(1);
	});
});
"#;

const SKELETON_README: &str = r#"# Gate-provided scaffolding

The files in this directory tree marked "Gate-owned" are written by the IFML
codegen validation gate (`crates/codegraph/tests/ifml_codegen_gate.rs`), not
by the generator. The SvelteKit skeleton (package.json with the SvelteKit
toolchain, tsconfig, src/app.html, src/app.d.ts, vite/svelte configs) is
GENERATOR-PROVIDED since Wave B (G1): the `ifml-skeleton` generator emits it
and the gate no longer writes any skeleton parts.

Gate-owned (always overwritten): `vite.config.ts` (the `/api` proxy with the
gate's Bearer key injection), `playwright.config.ts` (the ONLY playwright
config), `src/lib/components/ui/**` stub components, and
`tests/ifml/sweep-create.spec.ts` (the #205 sweep assertions). Everything
else is generator output.
"#;

/// Write the gate-owned scaffolding. The SvelteKit skeleton (package.json,
/// vite/svelte configs, tsconfig.json, src/app.html, src/app.d.ts) is NOT
/// written here — the `ifml-skeleton` generator provides it (Wave B, G1).
/// Generator output is never overwritten; the vite config (proxy Bearer
/// injection), the playwright config, the ui stubs, and the sweep spec are
/// gate-owned and refreshed every run.
pub fn write_extras(svelte: &Path) -> Result<(), String> {
    let write_owned = |rel: &str, content: &str| -> Result<(), String> {
        let path = svelte.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&path, content).map_err(|e| e.to_string())
    };
    write_owned("vite.config.ts", VITE_CONFIG)?;

    // Gate-owned playwright config: webServer (vite preview) + JSON reporter.
    // The generator's minimal e2e-only config (written only when absent) is
    // superseded on every run.
    write_owned("playwright.config.ts", PLAYWRIGHT_CONFIG)?;

    // Gate-owned #205 sweep assertions (create round trip + details values).
    write_owned("tests/ifml/sweep-create.spec.ts", SWEEP_SPEC)?;

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
        "tabs",
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

    fs::write(
        ui.join("tabs/tabs.svelte"),
        r#"<script lang="ts">
	// Gate-owned stub of the shadcn-svelte Tabs (presentation-container slot
	// for the issue #200 sibling xor group assertions).
	let { testid, children }: {
		testid?: string;
		children?: import('svelte').Snippet;
	} = $props();
</script>

<div data-testid={testid}>
	{@render children?.()}
</div>
"#,
    )
    .map_err(|e| e.to_string())?;

    fs::write(svelte.join("SKELETON.md"), SKELETON_README).map_err(|e| e.to_string())?;
    Ok(())
}
