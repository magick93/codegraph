import { _ as escape_html, a as ensure_array_like, i as derived, u as stringify } from "../../../../../../chunks/server2.js";
import "../../../../../../chunks/navigation.js";
import { a as Title, i as Root, n as Description, r as Header, t as Content } from "../../../../../../chunks/card.js";
import "../../../../../../chunks/badge.js";
import "../../../../../../chunks/button.js";
import "../../../../../../chunks/data102.remote.js";
// src/routes/(app)/settings/webhooks/[id]/+page.svelte
function _page($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { data } = $$props;
		let endpoint = derived(() => data.endpoint);
		let subscriptions = derived(() => data.subscriptions);
		let deliveries = derived(() => data.deliveries);
		const endpointId = derived(() => endpoint().id);
		function statusBadge(delivery) {
			if (delivery.delivered_at) return {
				label: "Delivered",
				variant: "default",
				class: "bg-green-100 text-green-800"
			};
			if (delivery.attempt > 0) return {
				label: `Failed (attempt ${delivery.attempt})`,
				variant: "destructive",
				class: ""
			};
			return {
				label: "Pending",
				variant: "outline",
				class: "text-muted-foreground"
			};
		}
		$$renderer.push(`<div class="space-y-6 p-6">`);
		$$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> <div class="flex items-center justify-between"><div><div class="flex items-center gap-2"><h2 class="text-2xl font-bold tracking-tight font-mono text-sm">${escape_html(endpoint().url)}</h2> `);
		if (endpoint().is_active) $$renderer.push("<!--[0-->");
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></div> `);
		if (endpoint().description) $$renderer.push(`<!--[0--><p class="text-muted-foreground">${escape_html(endpoint().description)}</p>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></div> <div class="flex gap-2">`);
		$$renderer.push(`<!----> `);
		`${stringify(endpointId())}`;
		$$renderer.push(`<!----> `);
		$$renderer.push(`<!----></div></div> `);
		if (Root) {
			$$renderer.push("<!--[-->");
			Root($$renderer, {
				children: ($$renderer) => {
					if (Header) {
						$$renderer.push("<!--[-->");
						Header($$renderer, {
							children: ($$renderer) => {
								if (Title) {
									$$renderer.push("<!--[-->");
									Title($$renderer, {
										children: ($$renderer) => {
											$$renderer.push(`<!---->Event Subscriptions`);
										},
										$$slots: { default: true }
									});
									$$renderer.push("<!--]-->");
								} else {
									$$renderer.push("<!--[!-->");
									$$renderer.push("<!--]-->");
								}
								$$renderer.push(` `);
								if (Description) {
									$$renderer.push("<!--[-->");
									Description($$renderer, {
										children: ($$renderer) => {
											$$renderer.push(`<!---->Subscribe this endpoint to domain events.`);
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
					$$renderer.push(` `);
					if (Content) {
						$$renderer.push("<!--[-->");
						Content($$renderer, {
							children: ($$renderer) => {
								if (subscriptions().length === 0) $$renderer.push(`<!--[0--><p class="text-sm text-muted-foreground py-4 text-center">No subscriptions configured.</p>`);
								else {
									$$renderer.push(`<!--[-1--><div class="space-y-2"><!--[-->`);
									const each_array = ensure_array_like(subscriptions());
									for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
										let sub = each_array[$$index];
										$$renderer.push(`<div class="flex items-center justify-between rounded-md border p-3"><div><span class="font-mono text-sm">${escape_html(sub.event_domain)}.${escape_html(sub.event_entity ?? "*")}.${escape_html(sub.event_type ?? "*")}</span></div> `);
										$$renderer.push(`<!----></div>`);
									}
									$$renderer.push(`<!--]--></div>`);
								}
								$$renderer.push(`<!--]-->`);
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
		$$renderer.push(` `);
		if (Root) {
			$$renderer.push("<!--[-->");
			Root($$renderer, {
				children: ($$renderer) => {
					if (Header) {
						$$renderer.push("<!--[-->");
						Header($$renderer, {
							children: ($$renderer) => {
								if (Title) {
									$$renderer.push("<!--[-->");
									Title($$renderer, {
										children: ($$renderer) => {
											$$renderer.push(`<!---->Delivery Log`);
										},
										$$slots: { default: true }
									});
									$$renderer.push("<!--]-->");
								} else {
									$$renderer.push("<!--[!-->");
									$$renderer.push("<!--]-->");
								}
								$$renderer.push(` `);
								if (Description) {
									$$renderer.push("<!--[-->");
									Description($$renderer, {
										children: ($$renderer) => {
											$$renderer.push(`<!---->Recent webhook delivery attempts.`);
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
					$$renderer.push(` `);
					if (Content) {
						$$renderer.push("<!--[-->");
						Content($$renderer, {
							children: ($$renderer) => {
								if (deliveries().length === 0) $$renderer.push(`<!--[0--><p class="text-sm text-muted-foreground py-4 text-center">No deliveries yet.</p>`);
								else {
									$$renderer.push(`<!--[-1--><div class="space-y-2"><!--[-->`);
									const each_array_1 = ensure_array_like(deliveries());
									for (let $$index_1 = 0, $$length = each_array_1.length; $$index_1 < $$length; $$index_1++) {
										let delivery = each_array_1[$$index_1];
										$$renderer.push(`<div class="flex items-center justify-between rounded-md border p-3 text-sm"><div class="flex items-center gap-3">`);
										statusBadge(delivery).class;
										$$renderer.push(`<!----> <span class="text-muted-foreground">Attempt ${escape_html(delivery.attempt)}</span> <span class="text-muted-foreground">${escape_html(new Date(delivery.created_at).toLocaleString())}</span></div></div>`);
									}
									$$renderer.push(`<!--]--></div>`);
								}
								$$renderer.push(`<!--]-->`);
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
		$$renderer.push(`</div>`);
	});
}
export { _page as default };

//# sourceMappingURL=_page.svelte.js.map