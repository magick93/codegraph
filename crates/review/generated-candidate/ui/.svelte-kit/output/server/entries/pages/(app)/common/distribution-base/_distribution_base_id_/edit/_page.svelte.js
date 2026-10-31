import { _ as escape_html, h as attr } from "../../../../../../../chunks/server2.js";
import { r as goto } from "../../../../../../../chunks/navigation.js";
import { n as common_entity_updated, r as common_back_to_detail, t as common_update_failed } from "../../../../../../../chunks/common_update_failed.js";
import { t as DistributionBaseForm } from "../../../../../../../chunks/DistributionBaseForm.js";
import { t as common_distribution_base_title } from "../../../../../../../chunks/common_distribution_base_title.js";
import { r as updateItem } from "../../../../../../../chunks/data19.remote.js";
import { n as common_edit, t as toast } from "../../../../../../../chunks/dist.js";
import { i as Root, t as Content } from "../../../../../../../chunks/card.js";
// src/routes/(app)/common/distribution-base/[distribution_base_id]/edit/+page.svelte
function _page($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		const basePath = "/common/distribution-base";
		let { data } = $$props;
		async function handleSubmit(formData) {
			const result = await updateItem({
				id: data.item.id,
				data: formData
			});
			if (!result.ok) {
				toast.error(common_update_failed({ entity: "DistributionBase" }));
				return {
					ok: false,
					message: result.message,
					fieldErrors: result.fieldErrors
				};
			}
			toast.success(common_entity_updated({ entity: "DistributionBase" }));
			goto(`${basePath}/${data.item.id}`);
			return { ok: true };
		}
		$$renderer.push(`<div class="space-y-6"><div><a${attr("href", `${basePath}/${data.item.id}`)} class="text-sm text-muted-foreground hover:text-foreground hover:underline">← ${escape_html(common_back_to_detail())}</a> <h2 class="mt-1 text-xl font-semibold">${escape_html(common_edit())} ${escape_html(common_distribution_base_title())}</h2></div> `);
		if (Root) {
			$$renderer.push("<!--[-->");
			Root($$renderer, {
				children: ($$renderer) => {
					if (Content) {
						$$renderer.push("<!--[-->");
						Content($$renderer, {
							class: "p-4 sm:p-6",
							children: ($$renderer) => {
								DistributionBaseForm($$renderer, {
									mode: "edit",
									initialData: data.item,
									onsubmit: handleSubmit
								});
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