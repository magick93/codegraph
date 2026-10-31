import { _ as escape_html, a as ensure_array_like, i as derived, u as stringify } from "../../../../../chunks/server2.js";
import "../../../../../chunks/navigation.js";
import { a as Title, i as Root, n as Description, r as Header } from "../../../../../chunks/card.js";
import "../../../../../chunks/badge.js";
import "../../../../../chunks/button.js";
import "../../../../../chunks/data101.remote.js";
// src/routes/(app)/settings/webhooks/+page.svelte
function _page($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { data } = $$props;
		let endpoints = derived(() => data.endpoints);
		$$renderer.push(`<div class="space-y-6 p-6"><div class="flex items-center justify-between"><div><h2 class="text-2xl font-bold tracking-tight">Webhooks</h2> <p class="text-muted-foreground">Send real-time event notifications to external URLs.</p></div> `);
		$$renderer.push(`<!----></div> `);
		if (endpoints().length === 0) $$renderer.push(`<!--[0--><div class="py-12 text-center"><p class="text-muted-foreground">No webhook endpoints configured yet.</p> <p class="text-sm text-muted-foreground mt-1">Create one to start receiving event notifications.</p></div>`);
		else {
			$$renderer.push(`<!--[-1--><div class="grid gap-4"><!--[-->`);
			const each_array = ensure_array_like(endpoints());
			for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
				let endpoint = each_array[$$index];
				if (Root) {
					$$renderer.push("<!--[-->");
					Root($$renderer, {
						children: ($$renderer) => {
							if (Header) {
								$$renderer.push("<!--[-->");
								Header($$renderer, {
									class: "flex flex-row items-center gap-4",
									children: ($$renderer) => {
										$$renderer.push(`<div class="flex-1"><div class="flex items-center gap-2">`);
										if (Title) {
											$$renderer.push("<!--[-->");
											Title($$renderer, {
												class: "text-base font-mono text-sm",
												children: ($$renderer) => {
													$$renderer.push(`<!---->${escape_html(endpoint.url)}`);
												},
												$$slots: { default: true }
											});
											$$renderer.push("<!--]-->");
										} else {
											$$renderer.push("<!--[!-->");
											$$renderer.push("<!--]-->");
										}
										$$renderer.push(` `);
										if (endpoint.is_active) $$renderer.push("<!--[0-->");
										else $$renderer.push("<!--[-1-->");
										$$renderer.push(`<!--]--></div> `);
										if (endpoint.description) {
											$$renderer.push("<!--[0-->");
											if (Description) {
												$$renderer.push("<!--[-->");
												Description($$renderer, {
													children: ($$renderer) => {
														$$renderer.push(`<!---->${escape_html(endpoint.description)}`);
													},
													$$slots: { default: true }
												});
												$$renderer.push("<!--]-->");
											} else {
												$$renderer.push("<!--[!-->");
												$$renderer.push("<!--]-->");
											}
										} else $$renderer.push("<!--[-1-->");
										$$renderer.push(`<!--]--></div> <div class="flex gap-2">`);
										`${stringify(endpoint.id)}`;
										$$renderer.push(`<!----> `);
										`${stringify(endpoint.id)}`;
										$$renderer.push(`<!----> `);
										endpoint.id;
										$$renderer.push(`<!----></div>`);
									},
									$$slots: { default: true }
								});
								$$renderer.push("<!--]-->");
							} else {
								$$renderer.push("<!--[!-->");
								$$renderer.push("<!--]-->");
							}
						},
						$$slots: { default: true }
					});
					$$renderer.push("<!--]-->");
				} else {
					$$renderer.push("<!--[!-->");
					$$renderer.push("<!--]-->");
				}
			}
			$$renderer.push(`<!--]--></div>`);
		}
		$$renderer.push(`<!--]--></div>`);
	});
}
export { _page as default };

//# sourceMappingURL=_page.svelte.js.map