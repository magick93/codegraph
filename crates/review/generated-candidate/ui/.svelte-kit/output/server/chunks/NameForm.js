import { _ as escape_html, h as attr, r as bind_props } from "./server2.js";
import { r as createAppForm } from "./structured-wrapper-field.js";
import { n as common_edit } from "./dist.js";
import "./button.js";
import { t as common_name_new } from "./common_name_new.js";
import { t as common_name_title } from "./common_name_title.js";
import { z } from "zod";
// src/lib/components/forms/NameForm.svelte
function NameForm($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { mode, initialData = void 0, onsubmit } = $$props;
		const nameSchema = z.object({
			family_name: z.string().optional(),
			formatted_name: z.string().optional(),
			given_name: z.string().optional()
		});
		const defaultValues = {
			family_name: String(initialData?.family_name ?? ""),
			formatted_name: String(initialData?.formatted_name ?? ""),
			given_name: String(initialData?.given_name ?? "")
		};
		const form = createAppForm(() => ({
			defaultValues,
			validators: { onSubmit: nameSchema },
			onSubmit: async ({ value }) => {
				form.setErrorMap({ onSubmit: void 0 });
				const payload = { ...value };
				if (payload["family_name"] === "") delete payload["family_name"];
				if (payload["formatted_name"] === "") delete payload["formatted_name"];
				if (payload["given_name"] === "") delete payload["given_name"];
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
		$$renderer.push(`<form class="space-y-6"${attr("aria-label", mode === "create" ? common_name_new() : common_edit() + " " + common_name_title())} data-testid="name-form">`);
		if (form.AppForm) {
			$$renderer.push("<!--[-->");
			form.AppForm($$renderer, {
				children: ($$renderer) => {
					{
						function children($$renderer, field) {
							if (field.TextField) {
								$$renderer.push("<!--[-->");
								field.TextField($$renderer, {
									label: "Family Name",
									testid: "name-field-family_name",
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
								name: "family_name",
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
									label: "Formatted Name",
									testid: "name-field-formatted_name",
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
								name: "formatted_name",
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
									label: "Given Name",
									testid: "name-field-given_name",
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
								name: "given_name",
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
								if (serverMessage) $$renderer.push(`<!--[0--><div class="rounded-md bg-destructive/10 border border-destructive/20 p-4 text-sm text-destructive" role="alert" data-testid="name-form-error">${escape_html(typeof serverMessage === "string" ? serverMessage : "Request failed")}</div>`);
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
		bind_props($$props, { nameSchema });
	});
}
export { NameForm as t };

//# sourceMappingURL=NameForm.js.map