import { _ as escape_html, i as derived } from "../../../../../../../chunks/server2.js";
import { t as page } from "../../../../../../../chunks/state.js";
import "../../../../../../../chunks/navigation.js";
import "../../../../../../../chunks/button.js";
import "../../../../../../../chunks/input.js";
import "../../../../../../../chunks/textarea.js";
import "../../../../../../../chunks/data103.remote.js";
// src/routes/(app)/settings/webhooks/[id]/edit/+page.svelte
function _page($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		const isEdit = derived(() => !!page.params.id);
		derived(() => page.params.id);
		const existing = derived(() => data?.endpoint ?? null);
		existing()?.url;
		existing()?.description;
		existing() && JSON.stringify(existing().headers ?? {}, null, 2);
		let $$settled = true;
		let $$inner_renderer;
		function $$render_inner($$renderer) {
			$$renderer.push(`<div class="space-y-6 p-6 max-w-2xl"><div><h2 class="text-2xl font-bold tracking-tight">${escape_html(isEdit() ? "Edit Webhook" : "New Webhook")}</h2> <p class="text-muted-foreground">${escape_html(isEdit() ? "Update the webhook endpoint configuration." : "Create a new webhook endpoint to receive event notifications.")}</p></div> `);
			$$renderer.push("<!--[-1-->");
			$$renderer.push(`<!--]--> <form class="space-y-4"><div class="space-y-2">`);
			$$renderer.push(`<!----> `);
			$$renderer.push(`<!----></div> <div class="space-y-2">`);
			$$renderer.push(`<!----> `);
			$$renderer.push(`<!----></div> <div class="space-y-2">`);
			$$renderer.push(`<!----> `);
			$$renderer.push(`<!----></div> <div class="flex gap-3">`);
			$$renderer.push(`<!----> `);
			$$renderer.push(`<!----></div></form></div>`);
		}
		do {
			$$settled = true;
			$$inner_renderer = $$renderer.copy();
			$$render_inner($$inner_renderer);
		} while (!$$settled);
		$$renderer.subsume($$inner_renderer);
	});
}
export { _page as default };

//# sourceMappingURL=_page.svelte.js.map