import { _ as escape_html, h as attr, r as bind_props } from "./server2.js";
import { r as createAppForm } from "./structured-wrapper-field.js";
import { n as common_edit } from "./dist.js";
import "./button.js";
import { t as common_gender_code_list_new } from "./common_gender_code_list_new.js";
import { t as common_gender_code_list_title } from "./common_gender_code_list_title.js";
import { z } from "zod";
// src/lib/components/forms/GenderCodeListForm.svelte
function GenderCodeListForm($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { mode, initialData = void 0, onsubmit } = $$props;
		const genderCodeListSchema = z.object({
			code: z.string(),
			display_name: z.string(),
			sort_order: z.number().optional()
		});
		const defaultValues = {
			code: String(initialData?.code ?? ""),
			display_name: String(initialData?.display_name ?? ""),
			sort_order: initialData?.sort_order != null ? Number(initialData?.sort_order) : void 0
		};
		const form = createAppForm(() => ({
			defaultValues,
			validators: { onSubmit: genderCodeListSchema },
			onSubmit: async ({ value }) => {
				form.setErrorMap({ onSubmit: void 0 });
				const payload = { ...value };
				if (payload["sort_order"] === void 0 || Number.isNaN(payload["sort_order"])) delete payload["sort_order"];
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
		$$renderer.push(`<form class="space-y-6"${attr("aria-label", mode === "create" ? common_gender_code_list_new() : common_edit() + " " + common_gender_code_list_title())} data-testid="gender_code_list-form">`);
		if (form.AppForm) {
			$$renderer.push("<!--[-->");
			form.AppForm($$renderer, {
				children: ($$renderer) => {
					{
						function children($$renderer, field) {
							if (field.TextField) {
								$$renderer.push("<!--[-->");
								field.TextField($$renderer, {
									label: "Code",
									testid: "gender_code_list-field-code",
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
								name: "code",
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
									label: "Display Name",
									testid: "gender_code_list-field-display_name",
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
								name: "display_name",
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
							if (field.NumberField) {
								$$renderer.push("<!--[-->");
								field.NumberField($$renderer, {
									label: "Sort Order",
									testid: "gender_code_list-field-sort_order",
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
								name: "sort_order",
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
								if (serverMessage) $$renderer.push(`<!--[0--><div class="rounded-md bg-destructive/10 border border-destructive/20 p-4 text-sm text-destructive" role="alert" data-testid="gender_code_list-form-error">${escape_html(typeof serverMessage === "string" ? serverMessage : "Request failed")}</div>`);
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
		bind_props($$props, { genderCodeListSchema });
	});
}
export { GenderCodeListForm as t };

//# sourceMappingURL=GenderCodeListForm.js.map