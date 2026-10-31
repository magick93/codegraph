import { _ as escape_html, h as attr } from "../../../../../../chunks/server2.js";
import { r as goto } from "../../../../../../chunks/navigation.js";
import { t as page } from "../../../../../../chunks/state.js";
import "../../../../../../chunks/button.js";
import { t as common_back_to_list } from "../../../../../../chunks/common_back_to_list.js";
import { i as Root, t as Content } from "../../../../../../chunks/card.js";
import { a as Description, c as Root$1, d as common_delete_failed, f as common_delete, h as common_confirm_delete_description, i as Content$1, l as Title, m as common_confirm_delete_title, n as Action, o as Footer, p as common_created_at, r as Cancel, s as Header, u as common_updated_at } from "../../../../../../chunks/entities.js";
import { n as common_deleting, t as common_entity_deleted } from "../../../../../../chunks/common_entity_deleted.js";
import { t as recruiting_grant_review_note_field_application_id } from "../../../../../../chunks/recruiting_grant_review_note_field_application_id.js";
import { t as recruiting_grant_review_note_title } from "../../../../../../chunks/recruiting_grant_review_note_title.js";
import { n as deleteItem } from "../../../../../../chunks/data94.remote.js";
import { r as common_cancel, t as toast } from "../../../../../../chunks/dist.js";
// src/routes/(app)/recruiting/grant-review-note/[grant_review_note_id]/+page.svelte
function _page($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { data } = $$props;
		let item = data.item;
		const basePath = "/recruiting/grant-review-note";
		let deleteDialogOpen = false;
		let deleting = false;
		async function handleDelete() {
			deleting = true;
			try {
				const result = await deleteItem({ id: item.id });
				if (!result.ok) throw new Error(result.message ?? "Request failed");
				toast.success(common_entity_deleted({ entity: "GrantReviewNote" }));
				goto(basePath);
			} catch (e) {
				toast.error(common_delete_failed({ error: e.message }));
			} finally {
				deleting = false;
				deleteDialogOpen = false;
			}
		}
		$$renderer.push(`<div class="space-y-6"><div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between"><div><a${attr("href", basePath)} class="text-sm text-muted-foreground hover:text-foreground hover:underline">← ${escape_html(common_back_to_list())}</a> <h2 class="mt-1 text-xl font-semibold">${escape_html(recruiting_grant_review_note_title())}</h2></div> <div class="flex gap-2">`);
		if (page.data.organization?.role === "owner") {
			$$renderer.push("<!--[0-->");
			`${item.id}`;
		} else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> `);
		if (page.data.organization?.role === "owner") $$renderer.push("<!--[0-->");
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></div></div> `);
		if (Root) {
			$$renderer.push("<!--[-->");
			Root($$renderer, {
				children: ($$renderer) => {
					if (Content) {
						$$renderer.push("<!--[-->");
						Content($$renderer, {
							class: "p-0",
							children: ($$renderer) => {
								$$renderer.push(`<dl class="divide-y divide-border"><div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_grant_review_note_field_application_id())}</dt> <dd class="text-sm sm:col-span-2" data-testid="grant_review_note-field-application_id">${escape_html(item.application_id ?? "—")}</dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(common_created_at())}</dt> <dd class="text-sm sm:col-span-2">`);
								if (item.created_at) {
									$$renderer.push("<!--[0-->");
									item.created_at;
								} else $$renderer.push(`<!--[-1-->—`);
								$$renderer.push(`<!--]--></dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(common_updated_at())}</dt> <dd class="text-sm sm:col-span-2">`);
								if (item.updated_at) {
									$$renderer.push("<!--[0-->");
									item.updated_at;
								} else $$renderer.push(`<!--[-1-->—`);
								$$renderer.push(`<!--]--></dd></div></dl>`);
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
		$$renderer.push(`</div> `);
		if (Root$1) {
			$$renderer.push("<!--[-->");
			Root$1($$renderer, {
				open: deleteDialogOpen,
				onOpenChange: (v) => deleteDialogOpen = v,
				children: ($$renderer) => {
					if (Content$1) {
						$$renderer.push("<!--[-->");
						Content$1($$renderer, {
							"data-testid": "confirm-dialog",
							children: ($$renderer) => {
								if (Header) {
									$$renderer.push("<!--[-->");
									Header($$renderer, {
										children: ($$renderer) => {
											if (Title) {
												$$renderer.push("<!--[-->");
												Title($$renderer, {
													children: ($$renderer) => {
														$$renderer.push(`<!---->${escape_html(common_confirm_delete_title({ entity: "GrantReviewNote" }))}`);
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
														$$renderer.push(`<!---->${escape_html(common_confirm_delete_description())}`);
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
								if (Footer) {
									$$renderer.push("<!--[-->");
									Footer($$renderer, {
										children: ($$renderer) => {
											if (Cancel) {
												$$renderer.push("<!--[-->");
												Cancel($$renderer, {
													children: ($$renderer) => {
														$$renderer.push(`<!---->${escape_html(common_cancel())}`);
													},
													$$slots: { default: true }
												});
												$$renderer.push("<!--]-->");
											} else {
												$$renderer.push("<!--[!-->");
												$$renderer.push("<!--]-->");
											}
											$$renderer.push(` `);
											if (Action) {
												$$renderer.push("<!--[-->");
												Action($$renderer, {
													onclick: handleDelete,
													disabled: deleting,
													"data-testid": "confirm-dialog-confirm",
													class: "bg-destructive text-destructive-foreground hover:bg-destructive/90",
													children: ($$renderer) => {
														$$renderer.push(`<!---->${escape_html(deleting ? common_deleting() : common_delete())}`);
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
	});
}
export { _page as default };

//# sourceMappingURL=_page.svelte.js.map