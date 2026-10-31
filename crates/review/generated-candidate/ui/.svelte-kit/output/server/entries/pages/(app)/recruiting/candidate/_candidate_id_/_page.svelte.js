import { _ as escape_html, a as ensure_array_like, d as unsubscribe_stores, h as attr, l as store_get } from "../../../../../../chunks/server2.js";
import { i as invalidateAll, r as goto } from "../../../../../../chunks/navigation.js";
import { t as page } from "../../../../../../chunks/state.js";
import "../../../../../../chunks/button.js";
import { a as Title, i as Root, r as Header, t as Content } from "../../../../../../chunks/card.js";
import "../../../../../../chunks/badge.js";
import { n as deleteDistributionGuidelinesChild, r as deleteQualificationChild, s as transitionItem } from "../../../../../../chunks/data78.remote.js";
import { r as pushChild, t as entityNavigation } from "../../../../../../chunks/entity-navigation.js";
import { t as common_back_to_list } from "../../../../../../chunks/common_back_to_list.js";
import { a as Description, c as Root$1, d as common_delete_failed, f as common_delete, h as common_confirm_delete_description, i as Content$1, l as Title$1, m as common_confirm_delete_title, n as Action, o as Footer, p as common_created_at, r as Cancel, s as Header$1, u as common_updated_at } from "../../../../../../chunks/entities.js";
import { n as PUBLIC_API_URL } from "../../../../../../chunks/public.js";
import { n as recruiting_candidate_field_status, r as recruiting_candidate_field_candidate_id, t as recruiting_candidate_field_uri } from "../../../../../../chunks/recruiting_candidate_field_uri.js";
import { t as recruiting_candidate_title } from "../../../../../../chunks/recruiting_candidate_title.js";
import { t as createSupabaseClient } from "../../../../../../chunks/supabase.js";
import { r as common_cancel, t as toast } from "../../../../../../chunks/dist.js";
import { r as getLocale } from "../../../../../../chunks/runtime.js";
// src/lib/paraglide/messages/common_child_deleted.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Child_DeletedInputs */
/**
* | output |
* | --- |
* | "Child deleted" |
*
* @param {Common_Child_DeletedInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_child_deleted = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Child deleted";
});
// src/lib/paraglide/messages/common_related_records.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Related_RecordsInputs */
/**
* | output |
* | --- |
* | "Related Records" |
*
* @param {Common_Related_RecordsInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_related_records = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Related Records";
});
// src/lib/paraglide/messages/common_transition_failed.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{ error: NonNullable<unknown> }} Common_Transition_FailedInputs */
/**
* | output |
* | --- |
* | "Transition failed: {error}" |
*
* @param {Common_Transition_FailedInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_transition_failed = ((i, options = {}) => {
	options.locale ?? getLocale();
	return `Transition failed: ${i?.error}`;
});
// src/lib/paraglide/messages/common_transitioned_to.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{ state: NonNullable<unknown> }} Common_Transitioned_ToInputs */
/**
* | output |
* | --- |
* | "Transitioned to {state}" |
*
* @param {Common_Transitioned_ToInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_transitioned_to = ((i, options = {}) => {
	options.locale ?? getLocale();
	return `Transitioned to ${i?.state}`;
});
// src/lib/paraglide/messages/recruiting_candidate_field_application_process_history.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Candidate_Field_Application_Process_HistoryInputs */
/**
* | output |
* | --- |
* | "Application Process History" |
*
* @param {Recruiting_Candidate_Field_Application_Process_HistoryInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_candidate_field_application_process_history = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Application Process History";
});
// src/lib/paraglide/messages/recruiting_candidate_field_birth_date.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Candidate_Field_Birth_DateInputs */
/**
* | output |
* | --- |
* | "Birth Date" |
*
* @param {Recruiting_Candidate_Field_Birth_DateInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_candidate_field_birth_date = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Birth Date";
});
// src/lib/paraglide/messages/recruiting_candidate_field_compensation_expectation.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Candidate_Field_Compensation_ExpectationInputs */
/**
* | output |
* | --- |
* | "Compensation Expectation" |
*
* @param {Recruiting_Candidate_Field_Compensation_ExpectationInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_candidate_field_compensation_expectation = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Compensation Expectation";
});
// src/lib/paraglide/messages/recruiting_candidate_field_compensation_expectation_currency.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Candidate_Field_Compensation_Expectation_CurrencyInputs */
/**
* | output |
* | --- |
* | "Compensation Expectation Currency" |
*
* @param {Recruiting_Candidate_Field_Compensation_Expectation_CurrencyInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_candidate_field_compensation_expectation_currency = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Compensation Expectation Currency";
});
// src/lib/paraglide/messages/recruiting_candidate_field_distribution_guidelines_id.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Candidate_Field_Distribution_Guidelines_IdInputs */
/**
* | output |
* | --- |
* | "Distribution Guidelines Id" |
*
* @param {Recruiting_Candidate_Field_Distribution_Guidelines_IdInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_candidate_field_distribution_guidelines_id = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Distribution Guidelines Id";
});
// src/lib/paraglide/messages/recruiting_candidate_field_external_identifier.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Candidate_Field_External_IdentifierInputs */
/**
* | output |
* | --- |
* | "External Identifier" |
*
* @param {Recruiting_Candidate_Field_External_IdentifierInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_candidate_field_external_identifier = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "External Identifier";
});
// src/lib/paraglide/messages/recruiting_candidate_field_family_name.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Candidate_Field_Family_NameInputs */
/**
* | output |
* | --- |
* | "Family Name" |
*
* @param {Recruiting_Candidate_Field_Family_NameInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_candidate_field_family_name = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Family Name";
});
// src/lib/paraglide/messages/recruiting_candidate_field_gender.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Candidate_Field_GenderInputs */
/**
* | output |
* | --- |
* | "Gender" |
*
* @param {Recruiting_Candidate_Field_GenderInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_candidate_field_gender = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Gender";
});
// src/lib/paraglide/messages/recruiting_candidate_field_given_name.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Candidate_Field_Given_NameInputs */
/**
* | output |
* | --- |
* | "Given Name" |
*
* @param {Recruiting_Candidate_Field_Given_NameInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_candidate_field_given_name = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Given Name";
});
// src/lib/paraglide/messages/recruiting_candidate_field_person_name.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Candidate_Field_Person_NameInputs */
/**
* | output |
* | --- |
* | "Person Name" |
*
* @param {Recruiting_Candidate_Field_Person_NameInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_candidate_field_person_name = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Person Name";
});
// src/lib/paraglide/messages/recruiting_candidate_field_position_schedule_type_codes.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Candidate_Field_Position_Schedule_Type_CodesInputs */
/**
* | output |
* | --- |
* | "Position Schedule Type Codes" |
*
* @param {Recruiting_Candidate_Field_Position_Schedule_Type_CodesInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_candidate_field_position_schedule_type_codes = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Position Schedule Type Codes";
});
// src/lib/paraglide/messages/recruiting_candidate_field_position_titles.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Candidate_Field_Position_TitlesInputs */
/**
* | output |
* | --- |
* | "Position Titles" |
*
* @param {Recruiting_Candidate_Field_Position_TitlesInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_candidate_field_position_titles = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Position Titles";
});
// src/lib/paraglide/messages/recruiting_candidate_field_qualifications.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Candidate_Field_QualificationsInputs */
/**
* | output |
* | --- |
* | "Qualifications" |
*
* @param {Recruiting_Candidate_Field_QualificationsInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_candidate_field_qualifications = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Qualifications";
});
// src/lib/paraglide/messages/recruiting_candidate_field_referred_by_application_id.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Candidate_Field_Referred_By_Application_IdInputs */
/**
* | output |
* | --- |
* | "Referred By Application Id" |
*
* @param {Recruiting_Candidate_Field_Referred_By_Application_IdInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_candidate_field_referred_by_application_id = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Referred By Application Id";
});
// src/lib/components/ui/WorkflowPanel.svelte
function WorkflowPanel($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { state, onTransition, loading = false } = $$props;
		$$renderer.push(`<div class="rounded-lg border border-border bg-card p-4" data-testid="workflow-panel"><h3 class="mb-3 text-sm font-semibold text-card-foreground">Workflow</h3> <div class="mb-3 flex items-center gap-2"><span class="text-sm text-muted-foreground">Status:</span> `);
		state.current_state, state.is_terminal;
		$$renderer.push(`<!----> `);
		if (state.approval_state) {
			$$renderer.push(`<!--[0--><span class="text-sm text-muted-foreground">Approval:</span> `);
			$$renderer.push(`<!---->`);
		} else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></div> `);
		if (state.is_terminal) $$renderer.push(`<!--[0--><p class="text-sm text-muted-foreground">This workflow has reached a terminal state.</p>`);
		else if (state.available_transitions.length > 0) {
			$$renderer.push(`<!--[1--><div class="flex flex-wrap gap-2"><!--[-->`);
			const each_array = ensure_array_like(state.available_transitions);
			for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) each_array[$$index];
			$$renderer.push(`<!--]--></div>`);
		} else $$renderer.push(`<!--[-1--><p class="text-sm text-muted-foreground">No transitions available.</p>`);
		$$renderer.push(`<!--]--></div>`);
	});
}
// src/lib/api/client.ts
var BASE_URL = PUBLIC_API_URL ?? "http://localhost:3000";
var ApiError = class extends Error {
	status;
	statusText;
	body;
	constructor(status, statusText, body) {
		super(`${status} ${statusText}${body ? ": " + (typeof body === "string" ? body : JSON.stringify(body)) : ""}`);
		this.status = status;
		this.statusText = statusText;
		this.body = body;
		this.name = "ApiError";
	}
};
async function authHeaders() {
	const { data: { session } } = await createSupabaseClient().auth.getSession();
	if (session?.access_token) return { "Authorization": `Bearer ${session.access_token}` };
	return {};
}
async function apiDelete(path) {
	const url = new URL(`/api/v1${path}`, BASE_URL);
	const auth = await authHeaders();
	const res = await fetch(url.toString(), {
		method: "DELETE",
		headers: {
			"X-Correlation-ID": crypto.randomUUID(),
			...auth
		}
	});
	if (!res.ok) {
		const errBody = await res.text().catch(() => null);
		console.error(`[api-client] ${res.status} ${res.statusText} ${url.pathname}`, errBody);
		throw new ApiError(res.status, res.statusText, errBody);
	}
}
// src/lib/components/ChildSection.svelte
function ChildSection($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { items, label, moduleName, domain, pathSegment, parentId, displayField, childSectionTid, addBtnTid, actionsTid, actionsMenuTid, actionEditTid, actionDeleteTid, manageBtnTid, hasChildren, deleteDialogTid, deleteDialogConfirmTid, menuActions, primaryActions, confirmDelete, onRefresh, onManage } = $$props;
		let deleteId = null;
		async function performDelete(childId) {
			try {
				await apiDelete(`/${domain}/${pathSegment}/${childId}`);
				toast.success(common_child_deleted());
				await onRefresh();
			} catch (e) {
				toast.error(common_delete_failed({ error: e.message }));
			}
		}
		function confirmDeleteItem() {
			const childId = deleteId;
			deleteId = null;
			if (childId !== null) performDelete(childId);
		}
		$$renderer.push(`<div class="border rounded-lg overflow-hidden"><button class="flex w-full items-center justify-between px-4 py-3 text-sm font-medium bg-muted/50 hover:bg-muted transition-colors"${attr("data-testid", childSectionTid)} type="button"><span class="flex items-center gap-2"><span>${escape_html(label)}</span> <span class="text-xs text-muted-foreground font-normal">(${escape_html(items.length)})</span></span> <span class="text-muted-foreground">${escape_html("▶")}</span></button> `);
		$$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> `);
		if (confirmDelete) {
			$$renderer.push("<!--[0-->");
			if (Root$1) {
				$$renderer.push("<!--[-->");
				Root$1($$renderer, {
					open: deleteId !== null,
					onOpenChange: (v) => {
						if (!v) deleteId = null;
					},
					children: ($$renderer) => {
						if (Content$1) {
							$$renderer.push("<!--[-->");
							Content$1($$renderer, {
								"data-testid": deleteDialogTid,
								children: ($$renderer) => {
									if (Header$1) {
										$$renderer.push("<!--[-->");
										Header$1($$renderer, {
											children: ($$renderer) => {
												if (Title$1) {
													$$renderer.push("<!--[-->");
													Title$1($$renderer, {
														children: ($$renderer) => {
															$$renderer.push(`<!---->${escape_html(common_confirm_delete_title({ entity: label }))}`);
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
														onclick: confirmDeleteItem,
														"data-testid": deleteDialogConfirmTid,
														class: "bg-destructive text-destructive-foreground hover:bg-destructive/90",
														children: ($$renderer) => {
															$$renderer.push(`<!---->${escape_html(common_delete())}`);
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
		} else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></div>`);
	});
}
// src/routes/(app)/recruiting/candidate/[candidate_id]/+page.svelte
function _page($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		var $$store_subs;
		let { data } = $$props;
		let item = data.item;
		const basePath = "/recruiting/candidate";
		let transitioning = false;
		let distribution_guidelinesItems = data.children?.["distribution_guidelines"] ?? [];
		let qualificationItems = data.children?.["qualification"] ?? [];
		function manageChild(domain, pathSegment, childId) {
			pushChild({
				label: "Candidate",
				path: `${basePath}/${item.id}`,
				entityType: "Candidate",
				entityId: item.id
			}, {
				parentId: item.id,
				parentType: "Candidate",
				parentLabel: "Candidate"
			});
			goto(`/${domain}/${pathSegment}/${childId}`);
		}
		function refreshChildren() {
			invalidateAll().then(() => {
				distribution_guidelinesItems = data.children?.["distribution_guidelines"] ?? [];
				qualificationItems = data.children?.["qualification"] ?? [];
			});
		}
		async function deleteDistributionGuidelines(childId) {
			const result = await deleteDistributionGuidelinesChild(childId);
			if (!result.ok) toast.error(common_delete_failed({ error: result.message ?? "Request failed" }));
			return result;
		}
		async function deleteQualification(childId) {
			const result = await deleteQualificationChild(childId);
			if (!result.ok) toast.error(common_delete_failed({ error: result.message ?? "Request failed" }));
			return result;
		}
		async function handleTransition(targetState) {
			transitioning = true;
			try {
				const result = await transitionItem({
					id: item.id,
					to: targetState
				});
				if (!result.ok) throw new Error(result.message ?? "Request failed");
				const ws = result.state;
				item.workflow_state = {
					current_state: ws.current_state,
					is_terminal: ws.is_terminal,
					available_transitions: ws.available_transitions
				};
				toast.success(common_transitioned_to({ state: targetState }));
			} catch (e) {
				toast.error(common_transition_failed({ error: e.message }));
			} finally {
				transitioning = false;
			}
		}
		$$renderer.push(`<div class="space-y-6"><div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between"><div>`);
		if (store_get($$store_subs ??= {}, "$entityNavigation", entityNavigation).breadcrumbs.length > 0) {
			$$renderer.push(`<!--[0--><nav class="flex items-center gap-1 text-sm text-muted-foreground" aria-label="Breadcrumb"><a${attr("href", basePath)} class="hover:text-foreground hover:underline">← ${escape_html(common_back_to_list())}</a> <!--[-->`);
			const each_array = ensure_array_like(store_get($$store_subs ??= {}, "$entityNavigation", entityNavigation).breadcrumbs);
			for (let i = 0, $$length = each_array.length; i < $$length; i++) {
				let crumb = each_array[i];
				$$renderer.push(`<span class="mx-1">/</span> <button class="hover:text-foreground hover:underline" type="button">${escape_html(crumb.label)} (${escape_html(crumb.entityId.slice(0, 8))})</button>`);
			}
			$$renderer.push(`<!--]--> <span class="mx-1">/</span> <span class="text-foreground font-medium">Candidate</span></nav>`);
		} else $$renderer.push(`<!--[-1--><a${attr("href", basePath)} class="text-sm text-muted-foreground hover:text-foreground hover:underline">← Back to list</a>`);
		$$renderer.push(`<!--]--> <h2 class="mt-1 text-xl font-semibold">${escape_html(recruiting_candidate_title())}</h2></div> <div class="flex gap-2">`);
		if (page.data.organization?.role === "owner") {
			$$renderer.push("<!--[0-->");
			`${item.id}`;
		} else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></div></div> `);
		if (item.workflow_state) {
			$$renderer.push("<!--[0-->");
			WorkflowPanel($$renderer, {
				state: item.workflow_state,
				onTransition: handleTransition,
				loading: transitioning
			});
		} else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> `);
		if (Root) {
			$$renderer.push("<!--[-->");
			Root($$renderer, {
				children: ($$renderer) => {
					if (Content) {
						$$renderer.push("<!--[-->");
						Content($$renderer, {
							class: "p-0",
							children: ($$renderer) => {
								$$renderer.push(`<dl class="divide-y divide-border"><div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_candidate_field_birth_date())}</dt> <dd class="text-sm sm:col-span-2" data-testid="candidate-field-birth_date">${escape_html(item.birth_date ?? "—")}</dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_candidate_field_family_name())}</dt> <dd class="text-sm sm:col-span-2" data-testid="candidate-field-family_name">${escape_html(item.family_name ?? "—")}</dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_candidate_field_given_name())}</dt> <dd class="text-sm sm:col-span-2" data-testid="candidate-field-given_name">${escape_html(item.given_name ?? "—")}</dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_candidate_field_application_process_history())}</dt> <dd class="text-sm sm:col-span-2" data-testid="candidate-field-application_process_history">`);
								if (item.application_process_history) {
									$$renderer.push("<!--[0-->");
									const obj = item.application_process_history;
									$$renderer.push(`${escape_html((void 0)(item.application_process_history))} `);
									if (obj.schemeId) $$renderer.push(`<!--[0--><span class="text-muted-foreground text-xs">(${escape_html(obj.schemeId)})</span>`);
									else $$renderer.push("<!--[-1-->");
									$$renderer.push(`<!--]-->`);
								} else $$renderer.push(`<!--[-1-->—`);
								$$renderer.push(`<!--]--></dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_candidate_field_candidate_id())}</dt> <dd class="text-sm sm:col-span-2" data-testid="candidate-field-candidate_id">${escape_html(item.candidate_id ?? "—")}</dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_candidate_field_compensation_expectation())}</dt> <dd class="text-sm sm:col-span-2" data-testid="candidate-field-compensation_expectation">${escape_html(item.compensation_expectation ?? "—")}</dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_candidate_field_compensation_expectation_currency())}</dt> <dd class="text-sm sm:col-span-2" data-testid="candidate-field-compensation_expectation_currency">${escape_html(item.compensation_expectation_currency ?? "—")}</dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_candidate_field_distribution_guidelines_id())}</dt> <dd class="text-sm sm:col-span-2" data-testid="candidate-field-distribution_guidelines_id">${escape_html(item.distribution_guidelines_id ?? "—")}</dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_candidate_field_external_identifier())}</dt> <dd class="text-sm sm:col-span-2" data-testid="candidate-field-external_identifier">`);
								if (item.external_identifier) {
									$$renderer.push("<!--[0-->");
									const obj = item.external_identifier;
									$$renderer.push(`${escape_html(obj.value ?? obj.code ?? obj.name ?? obj.id ?? "—")} `);
									if (obj.schemeId) $$renderer.push(`<!--[0--><span class="text-muted-foreground text-xs">(${escape_html(obj.schemeId)})</span>`);
									else $$renderer.push("<!--[-1-->");
									$$renderer.push(`<!--]-->`);
								} else $$renderer.push(`<!--[-1-->—`);
								$$renderer.push(`<!--]--></dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_candidate_field_gender())}</dt> <dd class="text-sm sm:col-span-2" data-testid="candidate-field-gender">${escape_html(item.gender ?? "—")}</dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_candidate_field_person_name())}</dt> <dd class="text-sm sm:col-span-2" data-testid="candidate-field-person_name">`);
								if (item.person_name) {
									$$renderer.push("<!--[0-->");
									const obj = item.person_name;
									$$renderer.push(`${escape_html((void 0)(item.person_name))} `);
									if (obj.schemeId) $$renderer.push(`<!--[0--><span class="text-muted-foreground text-xs">(${escape_html(obj.schemeId)})</span>`);
									else $$renderer.push("<!--[-1-->");
									$$renderer.push(`<!--]-->`);
								} else $$renderer.push(`<!--[-1-->—`);
								$$renderer.push(`<!--]--></dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_candidate_field_position_schedule_type_codes())}</dt> <dd class="text-sm sm:col-span-2" data-testid="candidate-field-position_schedule_type_codes">`);
								if (Array.isArray(item.position_schedule_type_codes)) $$renderer.push(`<!--[0-->${escape_html(item.position_schedule_type_codes.length)} items`);
								else $$renderer.push(`<!--[-1-->—`);
								$$renderer.push(`<!--]--></dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_candidate_field_position_titles())}</dt> <dd class="text-sm sm:col-span-2" data-testid="candidate-field-position_titles">`);
								if (Array.isArray(item.position_titles) && item.position_titles.length > 0) {
									$$renderer.push(`<!--[0--><ul class="list-disc list-inside"><!--[-->`);
									const each_array_1 = ensure_array_like(item.position_titles);
									for (let $$index_1 = 0, $$length = each_array_1.length; $$index_1 < $$length; $$index_1++) {
										let val = each_array_1[$$index_1];
										$$renderer.push(`<li>${escape_html(val)}</li>`);
									}
									$$renderer.push(`<!--]--></ul>`);
								} else $$renderer.push(`<!--[-1-->—`);
								$$renderer.push(`<!--]--></dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_candidate_field_qualifications())}</dt> <dd class="text-sm sm:col-span-2" data-testid="candidate-field-qualifications">`);
								if (Array.isArray(item.qualifications) && item.qualifications.length > 0) {
									$$renderer.push(`<!--[0--><ul class="list-disc list-inside"><!--[-->`);
									const each_array_2 = ensure_array_like(item.qualifications);
									for (let $$index_2 = 0, $$length = each_array_2.length; $$index_2 < $$length; $$index_2++) {
										let id = each_array_2[$$index_2];
										$$renderer.push(`<li>${escape_html((void 0)(id))}`);
										if ("schemeId" in id && id.schemeId) $$renderer.push(`<!--[0--><span class="text-muted-foreground text-xs">(${escape_html(id.schemeId)})</span>`);
										else $$renderer.push("<!--[-1-->");
										$$renderer.push(`<!--]--></li>`);
									}
									$$renderer.push(`<!--]--></ul>`);
								} else $$renderer.push(`<!--[-1-->—`);
								$$renderer.push(`<!--]--></dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_candidate_field_referred_by_application_id())}</dt> <dd class="text-sm sm:col-span-2" data-testid="candidate-field-referred_by_application_id">${escape_html(item.referred_by_application_id ?? "—")}</dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_candidate_field_status())}</dt> <dd class="text-sm sm:col-span-2" data-testid="candidate-field-status">${escape_html(item.status ?? "—")}</dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(recruiting_candidate_field_uri())}</dt> <dd class="text-sm sm:col-span-2" data-testid="candidate-field-uri">${escape_html(item.uri ?? "—")}</dd></div> <div class="grid grid-cols-1 gap-1 px-4 py-3 sm:grid-cols-3 sm:gap-4"><dt class="text-sm font-medium text-muted-foreground">${escape_html(common_created_at())}</dt> <dd class="text-sm sm:col-span-2">`);
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
											$$renderer.push(`<!---->${escape_html(common_related_records())}`);
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
							class: "space-y-2",
							children: ($$renderer) => {
								ChildSection($$renderer, {
									items: distribution_guidelinesItems,
									label: "DistributionGuidelines",
									moduleName: "distribution_guidelines",
									domain: "recruiting",
									pathSegment: "distribution-guidelines",
									parentId: item.id,
									displayField: "start_date",
									childSectionTid: "child-section-distribution_guidelines",
									addBtnTid: "child-section-distribution_guidelines-add-btn",
									actionsTid: "distribution_guidelines-actions",
									actionsMenuTid: "distribution_guidelines-actions-menu",
									actionEditTid: "distribution_guidelines-action-edit",
									actionDeleteTid: "distribution_guidelines-action-delete",
									manageBtnTid: "manage-distribution_guidelines-btn",
									hasChildren: "false",
									deleteDialogTid: "distribution_guidelines-delete-dialog",
									deleteDialogConfirmTid: "distribution_guidelines-delete-dialog-confirm",
									menuActions: [],
									primaryActions: [],
									confirmDelete: false,
									onRefresh: refreshChildren,
									onDelete: deleteDistributionGuidelines,
									onManage: manageChild
								});
								$$renderer.push(`<!----> `);
								ChildSection($$renderer, {
									items: qualificationItems,
									label: "Qualification",
									moduleName: "qualification",
									domain: "recruiting",
									pathSegment: "qualification",
									parentId: item.id,
									displayField: "qualification_name",
									childSectionTid: "child-section-qualification",
									addBtnTid: "child-section-qualification-add-btn",
									actionsTid: "qualification-actions",
									actionsMenuTid: "qualification-actions-menu",
									actionEditTid: "qualification-action-edit",
									actionDeleteTid: "qualification-action-delete",
									manageBtnTid: "manage-qualification-btn",
									hasChildren: "false",
									deleteDialogTid: "qualification-delete-dialog",
									deleteDialogConfirmTid: "qualification-delete-dialog-confirm",
									menuActions: [],
									primaryActions: [],
									confirmDelete: false,
									onRefresh: refreshChildren,
									onDelete: deleteQualification,
									onManage: manageChild
								});
								$$renderer.push(`<!---->`);
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
		if ($$store_subs) unsubscribe_stores($$store_subs);
	});
}
export { _page as default };

//# sourceMappingURL=_page.svelte.js.map