import { _ as escape_html, a as ensure_array_like, i as derived } from "../../../../../chunks/server2.js";
import { r as goto } from "../../../../../chunks/navigation.js";
import { t as page } from "../../../../../chunks/state.js";
import { _ as common_no_results, a as Header, c as Title, d as Head, f as Header$1, h as common_search_placeholder, i as Description, l as Body, m as Row, n as formatFallback, o as Media, p as Root$1, r as Content, s as Root, t as SearchInput, u as Cell } from "../../../../../chunks/SearchInput.js";
import "../../../../../chunks/button.js";
import "../../../../../chunks/badge.js";
import { n as common_no, t as common_yes } from "../../../../../chunks/common_yes.js";
import { i as compensation_pay_run_field_pay_run_id, n as compensation_pay_run_field_total_amount, r as compensation_pay_run_field_run_date, t as compensation_pay_run_field_total_amount_currency } from "../../../../../chunks/compensation_pay_run_field_total_amount_currency.js";
import "../../../../../chunks/compensation_pay_run_new.js";
import { r as getLocale } from "../../../../../chunks/runtime.js";
// src/lib/paraglide/messages/compensation_pay_run_empty_description.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Compensation_Pay_Run_Empty_DescriptionInputs */
/**
* | output |
* | --- |
* | "Create your first pay run to get started." |
*
* @param {Compensation_Pay_Run_Empty_DescriptionInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var compensation_pay_run_empty_description = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Create your first pay run to get started.";
});
// src/lib/paraglide/messages/compensation_pay_run_empty_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Compensation_Pay_Run_Empty_TitleInputs */
/**
* | output |
* | --- |
* | "No Pay Run Yet" |
*
* @param {Compensation_Pay_Run_Empty_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var compensation_pay_run_empty_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "No Pay Run Yet";
});
// src/routes/(app)/compensation/pay-runs/+page.svelte
function _page($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		const basePath = "/compensation/pay-runs";
		let { data } = $$props;
		let searchQuery = "";
		const columns = [
			{
				key: "pay_run_id",
				label: compensation_pay_run_field_pay_run_id()
			},
			{
				key: "run_date",
				label: compensation_pay_run_field_run_date()
			},
			{
				key: "total_amount",
				label: compensation_pay_run_field_total_amount()
			},
			{
				key: "total_amount_currency",
				label: compensation_pay_run_field_total_amount_currency()
			}
		];
		const displayRows = derived(() => searchQuery ? data.items.filter((item) => Object.values(item).some((v) => String(v ?? "").toLowerCase().includes(searchQuery.toLowerCase()))) : data.items);
		function handleRowClick(row) {
			goto(`${basePath}/${row["id"]}`);
		}
		function handleSearch(query) {
			searchQuery = query;
		}
		function formatValue(value) {
			if (value === null || value === void 0) return "—";
			if (typeof value === "boolean") return value ? common_yes() : common_no();
			if (Array.isArray(value)) return value.map((v) => typeof v === "object" && v !== null ? formatFallback(v) : String(v)).join(", ");
			if (typeof value === "object" && value !== null) return formatFallback(value);
			return String(value);
		}
		function isBooleanField(value) {
			return typeof value === "boolean";
		}
		$$renderer.push(`<div class="space-y-4"><div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between"><div class="w-full sm:w-64">`);
		SearchInput($$renderer, {
			value: searchQuery,
			onSearch: handleSearch,
			placeholder: common_search_placeholder({ entity: "PayRun" }),
			"data-testid": "pay_run-search"
		});
		$$renderer.push(`<!----></div> `);
		if (page.data.organization?.role === "owner") $$renderer.push("<!--[0-->");
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></div> `);
		if (displayRows().length === 0 && !searchQuery) {
			$$renderer.push("<!--[0-->");
			if (Root) {
				$$renderer.push("<!--[-->");
				Root($$renderer, {
					"data-testid": "pay_run-empty",
					children: ($$renderer) => {
						if (Header) {
							$$renderer.push("<!--[-->");
							Header($$renderer, {
								children: ($$renderer) => {
									if (Media) {
										$$renderer.push("<!--[-->");
										Media($$renderer, {
											variant: "icon",
											children: ($$renderer) => {
												$$renderer.push(`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"></path><polyline points="14 2 14 8 20 8"></polyline></svg>`);
											},
											$$slots: { default: true }
										});
										$$renderer.push("<!--]-->");
									} else {
										$$renderer.push("<!--[!-->");
										$$renderer.push("<!--]-->");
									}
									$$renderer.push(` `);
									if (Title) {
										$$renderer.push("<!--[-->");
										Title($$renderer, {
											children: ($$renderer) => {
												$$renderer.push(`<!---->${escape_html(compensation_pay_run_empty_title())}`);
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
												$$renderer.push(`<!---->${escape_html(compensation_pay_run_empty_description())}`);
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
						if (page.data.organization?.role === "owner") {
							$$renderer.push("<!--[0-->");
							if (Content) {
								$$renderer.push("<!--[-->");
								Content($$renderer, {
									children: ($$renderer) => {},
									$$slots: { default: true }
								});
								$$renderer.push("<!--]-->");
							} else {
								$$renderer.push("<!--[!-->");
								$$renderer.push("<!--]-->");
							}
						} else $$renderer.push("<!--[-1-->");
						$$renderer.push(`<!--]-->`);
					},
					$$slots: { default: true }
				});
				$$renderer.push("<!--]-->");
			} else {
				$$renderer.push("<!--[!-->");
				$$renderer.push("<!--]-->");
			}
		} else if (displayRows().length === 0 && searchQuery) $$renderer.push(`<!--[1--><div class="py-12 text-center text-muted-foreground" data-testid="pay_run-no-results">${escape_html(common_no_results({ query: searchQuery }))}</div>`);
		else {
			$$renderer.push(`<!--[-1--><div class="rounded-md border">`);
			if (Root$1) {
				$$renderer.push("<!--[-->");
				Root$1($$renderer, {
					"data-testid": "pay_run-table",
					children: ($$renderer) => {
						if (Header$1) {
							$$renderer.push("<!--[-->");
							Header$1($$renderer, {
								children: ($$renderer) => {
									if (Row) {
										$$renderer.push("<!--[-->");
										Row($$renderer, {
											children: ($$renderer) => {
												$$renderer.push(`<!--[-->`);
												const each_array = ensure_array_like(columns);
												for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
													let col = each_array[$$index];
													if (Head) {
														$$renderer.push("<!--[-->");
														Head($$renderer, {
															children: ($$renderer) => {
																$$renderer.push(`<!---->${escape_html(col.label)}`);
															},
															$$slots: { default: true }
														});
														$$renderer.push("<!--]-->");
													} else {
														$$renderer.push("<!--[!-->");
														$$renderer.push("<!--]-->");
													}
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
						if (Body) {
							$$renderer.push("<!--[-->");
							Body($$renderer, {
								children: ($$renderer) => {
									$$renderer.push(`<!--[-->`);
									const each_array_1 = ensure_array_like(displayRows());
									for (let $$index_2 = 0, $$length = each_array_1.length; $$index_2 < $$length; $$index_2++) {
										let row = each_array_1[$$index_2];
										if (Row) {
											$$renderer.push("<!--[-->");
											Row($$renderer, {
												class: "cursor-pointer hover:bg-muted/50",
												onclick: () => handleRowClick(row),
												onkeydown: (e) => {
													if (e.key === "Enter") handleRowClick(row);
												},
												tabindex: "0",
												children: ($$renderer) => {
													$$renderer.push(`<!--[-->`);
													const each_array_2 = ensure_array_like(columns);
													for (let $$index_1 = 0, $$length = each_array_2.length; $$index_1 < $$length; $$index_1++) {
														let col = each_array_2[$$index_1];
														if (Cell) {
															$$renderer.push("<!--[-->");
															Cell($$renderer, {
																children: ($$renderer) => {
																	if (isBooleanField(row[col.key])) {
																		$$renderer.push("<!--[0-->");
																		row[col.key];
																	} else $$renderer.push(`<!--[-1-->${escape_html(formatValue(row[col.key]))}`);
																	$$renderer.push(`<!--]-->`);
																},
																$$slots: { default: true }
															});
															$$renderer.push("<!--]-->");
														} else {
															$$renderer.push("<!--[!-->");
															$$renderer.push("<!--]-->");
														}
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
			$$renderer.push(`</div> <div class="flex items-center justify-between text-sm text-muted-foreground" data-testid="pay_run-pagination"><span>Showing ${escape_html(displayRows().length)} result${escape_html(displayRows().length === 1 ? "" : "s")}</span> <div class="flex gap-2">`);
			data.page;
			$$renderer.push(`<!----> `);
			data.items.length, data.pageSize;
			$$renderer.push(`<!----></div></div>`);
		}
		$$renderer.push(`<!--]--></div>`);
	});
}
export { _page as default };

//# sourceMappingURL=_page.svelte.js.map