import { _ as escape_html, h as attr, r as bind_props } from "./server2.js";
import { r as createAppForm } from "./structured-wrapper-field.js";
import { n as common_edit } from "./dist.js";
import "./button.js";
import { t as recruiting_grant_application_new } from "./recruiting_grant_application_new.js";
import { t as recruiting_grant_application_title } from "./recruiting_grant_application_title.js";
import { z } from "zod";
// src/lib/components/forms/GrantApplicationForm.svelte
function GrantApplicationForm($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { mode, initialData = void 0, onsubmit } = $$props;
		const grantApplicationSchema = z.object({
			amount: z.number(),
			approved: z.boolean().optional(),
			category: z.string().optional(),
			id: z.string(),
			notes: z.string().optional(),
			reference: z.string(),
			summary: z.string().optional(),
			title: z.string().optional()
		});
		const defaultValues = {
			amount: initialData?.amount != null ? Number(initialData?.amount) : void 0,
			approved: Boolean(initialData?.approved ?? false),
			category: String(initialData?.category ?? ""),
			id: String(initialData?.id ?? ""),
			lodged_on: String(initialData?.lodged_on ?? "").replace(/Z$/, ""),
			notes: String(initialData?.notes ?? ""),
			reference: String(initialData?.reference ?? ""),
			summary: String(initialData?.summary ?? ""),
			title: String(initialData?.title ?? "")
		};
		const form = createAppForm(() => ({
			defaultValues,
			validators: { onSubmit: grantApplicationSchema },
			onSubmit: async ({ value }) => {
				form.setErrorMap({ onSubmit: void 0 });
				const payload = { ...value };
				if (payload["category"] === "") delete payload["category"];
				if (payload["lodged_on"]) payload["lodged_on"] = String(payload["lodged_on"]).includes("Z") ? payload["lodged_on"] : payload["lodged_on"] + ":00Z";
				if (payload["notes"] === "") delete payload["notes"];
				if (payload["summary"] === "") delete payload["summary"];
				if (payload["title"] === "") delete payload["title"];
				const result = await onsubmit(payload);
				if (!result.ok) {
					if (result.fieldErrors) for (const fieldName of Object.keys(result.fieldErrors)) {
						const message = result.fieldErrors[fieldName];
						form.setFieldMeta(fieldName, (meta) => ({
							...meta,
							errors: [message]
						}));
					}
					form.setErrorMap({ onSubmit: result.message });
				}
			}
		}));
		$$renderer.push(`<form class="space-y-6"${attr("aria-label", mode === "create" ? recruiting_grant_application_new() : common_edit() + " " + recruiting_grant_application_title())} data-testid="grant_application-form">`);
		if (form.AppForm) {
			$$renderer.push("<!--[-->");
			form.AppForm($$renderer, {
				children: ($$renderer) => {
					{
						function children($$renderer, field) {
							if (field.NumberField) {
								$$renderer.push("<!--[-->");
								field.NumberField($$renderer, {
									label: "Amount",
									testid: "grant_application-field-amount",
									required: "true",
									description: ""
								});
								$$renderer.push("<!--]-->");
							} else {
								$$renderer.push("<!--[!-->");
								$$renderer.push("<!--]-->");
							}
						}
						if (form.AppField) {
							$$renderer.push("<!--[-->");
							form.AppField($$renderer, {
								name: "amount",
								children,
								$$slots: { default: true }
							});
							$$renderer.push("<!--]-->");
						} else {
							$$renderer.push("<!--[!-->");
							$$renderer.push("<!--]-->");
						}
					}
					$$renderer.push(` `);
					{
						function children($$renderer, field) {
							if (field.CheckboxField) {
								$$renderer.push("<!--[-->");
								field.CheckboxField($$renderer, {
									label: "Approved",
									testid: "grant_application-field-approved",
									required: "false",
									description: ""
								});
								$$renderer.push("<!--]-->");
							} else {
								$$renderer.push("<!--[!-->");
								$$renderer.push("<!--]-->");
							}
						}
						if (form.AppField) {
							$$renderer.push("<!--[-->");
							form.AppField($$renderer, {
								name: "approved",
								children,
								$$slots: { default: true }
							});
							$$renderer.push("<!--]-->");
						} else {
							$$renderer.push("<!--[!-->");
							$$renderer.push("<!--]-->");
						}
					}
					$$renderer.push(` `);
					{
						function children($$renderer, field) {
							if (field.TextField) {
								$$renderer.push("<!--[-->");
								field.TextField($$renderer, {
									label: "Category",
									testid: "grant_application-field-category",
									required: "false",
									description: ""
								});
								$$renderer.push("<!--]-->");
							} else {
								$$renderer.push("<!--[!-->");
								$$renderer.push("<!--]-->");
							}
						}
						if (form.AppField) {
							$$renderer.push("<!--[-->");
							form.AppField($$renderer, {
								name: "category",
								children,
								$$slots: { default: true }
							});
							$$renderer.push("<!--]-->");
						} else {
							$$renderer.push("<!--[!-->");
							$$renderer.push("<!--]-->");
						}
					}
					$$renderer.push(` `);
					{
						function children($$renderer, field) {
							if (field.TextField) {
								$$renderer.push("<!--[-->");
								field.TextField($$renderer, {
									label: "Id",
									testid: "grant_application-field-id",
									required: "true",
									description: ""
								});
								$$renderer.push("<!--]-->");
							} else {
								$$renderer.push("<!--[!-->");
								$$renderer.push("<!--]-->");
							}
						}
						if (form.AppField) {
							$$renderer.push("<!--[-->");
							form.AppField($$renderer, {
								name: "id",
								children,
								$$slots: { default: true }
							});
							$$renderer.push("<!--]-->");
						} else {
							$$renderer.push("<!--[!-->");
							$$renderer.push("<!--]-->");
						}
					}
					$$renderer.push(` `);
					{
						function children($$renderer, field) {
							if (field.DateTimeField) {
								$$renderer.push("<!--[-->");
								field.DateTimeField($$renderer, {
									label: "Lodged On",
									testid: "grant_application-field-lodged_on",
									required: "false",
									description: ""
								});
								$$renderer.push("<!--]-->");
							} else {
								$$renderer.push("<!--[!-->");
								$$renderer.push("<!--]-->");
							}
						}
						if (form.AppField) {
							$$renderer.push("<!--[-->");
							form.AppField($$renderer, {
								name: "lodged_on",
								children,
								$$slots: { default: true }
							});
							$$renderer.push("<!--]-->");
						} else {
							$$renderer.push("<!--[!-->");
							$$renderer.push("<!--]-->");
						}
					}
					$$renderer.push(` `);
					{
						function children($$renderer, field) {
							if (field.TextField) {
								$$renderer.push("<!--[-->");
								field.TextField($$renderer, {
									label: "Notes",
									testid: "grant_application-field-notes",
									required: "false",
									description: ""
								});
								$$renderer.push("<!--]-->");
							} else {
								$$renderer.push("<!--[!-->");
								$$renderer.push("<!--]-->");
							}
						}
						if (form.AppField) {
							$$renderer.push("<!--[-->");
							form.AppField($$renderer, {
								name: "notes",
								children,
								$$slots: { default: true }
							});
							$$renderer.push("<!--]-->");
						} else {
							$$renderer.push("<!--[!-->");
							$$renderer.push("<!--]-->");
						}
					}
					$$renderer.push(` `);
					{
						function children($$renderer, field) {
							if (field.TextField) {
								$$renderer.push("<!--[-->");
								field.TextField($$renderer, {
									label: "Reference",
									testid: "grant_application-field-reference",
									required: "true",
									description: ""
								});
								$$renderer.push("<!--]-->");
							} else {
								$$renderer.push("<!--[!-->");
								$$renderer.push("<!--]-->");
							}
						}
						if (form.AppField) {
							$$renderer.push("<!--[-->");
							form.AppField($$renderer, {
								name: "reference",
								children,
								$$slots: { default: true }
							});
							$$renderer.push("<!--]-->");
						} else {
							$$renderer.push("<!--[!-->");
							$$renderer.push("<!--]-->");
						}
					}
					$$renderer.push(` `);
					{
						function children($$renderer, field) {
							if (field.TextField) {
								$$renderer.push("<!--[-->");
								field.TextField($$renderer, {
									label: "Summary",
									testid: "grant_application-field-summary",
									required: "false",
									description: ""
								});
								$$renderer.push("<!--]-->");
							} else {
								$$renderer.push("<!--[!-->");
								$$renderer.push("<!--]-->");
							}
						}
						if (form.AppField) {
							$$renderer.push("<!--[-->");
							form.AppField($$renderer, {
								name: "summary",
								children,
								$$slots: { default: true }
							});
							$$renderer.push("<!--]-->");
						} else {
							$$renderer.push("<!--[!-->");
							$$renderer.push("<!--]-->");
						}
					}
					$$renderer.push(` `);
					{
						function children($$renderer, field) {
							if (field.TextField) {
								$$renderer.push("<!--[-->");
								field.TextField($$renderer, {
									label: "Title",
									testid: "grant_application-field-title",
									required: "false",
									description: ""
								});
								$$renderer.push("<!--]-->");
							} else {
								$$renderer.push("<!--[!-->");
								$$renderer.push("<!--]-->");
							}
						}
						if (form.AppField) {
							$$renderer.push("<!--[-->");
							form.AppField($$renderer, {
								name: "title",
								children,
								$$slots: { default: true }
							});
							$$renderer.push("<!--]-->");
						} else {
							$$renderer.push("<!--[!-->");
							$$renderer.push("<!--]-->");
						}
					}
					$$renderer.push(` `);
					if (form.state.errorMap.onSubmit) {
						$$renderer.push("<!--[0-->");
						{
							function children($$renderer, serverMessage) {
								if (serverMessage) $$renderer.push(`<!--[0--><div class="rounded-md bg-destructive/10 border border-destructive/20 p-4 text-sm text-destructive" role="alert" data-testid="grant_application-form-error">${escape_html(typeof serverMessage === "string" ? serverMessage : "Request failed")}</div>`);
								else $$renderer.push("<!--[-1-->");
								$$renderer.push(`<!--]-->`);
							}
							if (form.Subscribe) {
								$$renderer.push("<!--[-->");
								form.Subscribe($$renderer, {
									selector: (state) => state.errorMap.onSubmit,
									children,
									$$slots: { default: true }
								});
								$$renderer.push("<!--]-->");
							} else {
								$$renderer.push("<!--[!-->");
								$$renderer.push("<!--]-->");
							}
						}
					} else $$renderer.push("<!--[-1-->");
					$$renderer.push(`<!--]--> `);
					{
						function children($$renderer, { canSubmit, isSubmitting }) {
							$$renderer.push(`<div class="flex flex-col-reverse gap-3 pt-4 sm:flex-row sm:justify-end">`);
							$$renderer.push(`<!----> `);
							$$renderer.push(`<!----></div>`);
						}
						if (form.Subscribe) {
							$$renderer.push("<!--[-->");
							form.Subscribe($$renderer, {
								selector: (state) => ({
									canSubmit: state.canSubmit,
									isSubmitting: state.isSubmitting
								}),
								children,
								$$slots: { default: true }
							});
							$$renderer.push("<!--]-->");
						} else {
							$$renderer.push("<!--[!-->");
							$$renderer.push("<!--]-->");
						}
					}
				},
				$$slots: { default: true }
			});
			$$renderer.push("<!--]-->");
		} else {
			$$renderer.push("<!--[!-->");
			$$renderer.push("<!--]-->");
		}
		$$renderer.push(`</form>`);
		bind_props($$props, { grantApplicationSchema });
	});
}
export { GrantApplicationForm as t };

//# sourceMappingURL=GrantApplicationForm.js.map