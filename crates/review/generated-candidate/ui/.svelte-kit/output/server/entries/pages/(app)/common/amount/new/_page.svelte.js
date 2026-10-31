import { _ as escape_html, h as attr } from "../../../../../../chunks/server2.js";
import { r as goto } from "../../../../../../chunks/navigation.js";
import { t as common_amount_new } from "../../../../../../chunks/common_amount_new.js";
import { t as common_back_to_list } from "../../../../../../chunks/common_back_to_list.js";
import { t as AmountForm } from "../../../../../../chunks/AmountForm.js";
import { n as common_create_failed, t as common_entity_created } from "../../../../../../chunks/common_entity_created.js";
import { t as createItem } from "../../../../../../chunks/data4.remote.js";
import { t as toast } from "../../../../../../chunks/dist.js";
import { i as Root, t as Content } from "../../../../../../chunks/card.js";
// src/routes/(app)/common/amount/new/+page.svelte
function _page($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		const basePath = "/common/amount";
		async function handleSubmit(data) {
			const result = await createItem({ data });
			if (!result.ok) {
				toast.error(common_create_failed({ entity: "Amount" }));
				return {
					ok: false,
					message: result.message,
					fieldErrors: result.fieldErrors
				};
			}
			toast.success(common_entity_created({ entity: "Amount" }));
			goto(`${basePath}/${result.id}`);
			return { ok: true };
		}
		$$renderer.push(`<div class="space-y-6"><div><a${attr("href", basePath)} class="text-sm text-muted-foreground hover:text-foreground hover:underline">← ${escape_html(common_back_to_list())}</a> <h2 class="mt-1 text-xl font-semibold">${escape_html(common_amount_new())}</h2></div> `);
		Root($$renderer, {
			children: ($$renderer) => {
				Content($$renderer, {
					class: "p-4 sm:p-6",
					children: ($$renderer) => {
						AmountForm($$renderer, {
							mode: "create",
							onsubmit: handleSubmit
						});
					},
					$$slots: { default: true }
				});
			},
			$$slots: { default: true }
		});
		$$renderer.push(`<!----></div>`);
	});
}
export { _page as default };

//# sourceMappingURL=_page.svelte.js.map