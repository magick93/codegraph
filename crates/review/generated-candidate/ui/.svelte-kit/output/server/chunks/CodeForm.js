import { _ as escape_html, h as attr, r as bind_props } from "./server2.js";
import { r as createAppForm } from "./structured-wrapper-field.js";
import { n as common_edit } from "./dist.js";
import { t as common_code_new } from "./common_code_new.js";
import { t as common_code_title } from "./common_code_title.js";
import "./button.js";
import { z } from "zod";
// src/lib/components/forms/CodeForm.svelte
function CodeForm($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { mode, initialData = void 0, onsubmit } = $$props;
		const codeSchema = z.object({});
		const defaultValues = {};
		const form = createAppForm(() => ({
			defaultValues,
			validators: { onSubmit: codeSchema },
			onSubmit: async ({ value }) => {
				form.setErrorMap({ onSubmit: void 0 });
				const payload = { ...value };
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
		$$renderer.push(`<form class="space-y-6"${attr("aria-label", mode === "create" ? common_code_new() : common_edit() + " " + common_code_title())} data-testid="code-form">`);
		if (form.AppForm) {
			$$renderer.push("<!--[-->");
			form.AppForm($$renderer, {
				children: ($$renderer) => {
					if (form.state.errorMap.onSubmit) {
						$$renderer.push("<!--[0-->");
						{
							function children($$renderer, serverMessage) {
								if (serverMessage) $$renderer.push(`<!--[0--><div class="rounded-md bg-destructive/10 border border-destructive/20 p-4 text-sm text-destructive" role="alert" data-testid="code-form-error">${escape_html(typeof serverMessage === "string" ? serverMessage : "Request failed")}</div>`);
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
		bind_props($$props, { codeSchema });
	});
}
export { CodeForm as t };

//# sourceMappingURL=CodeForm.js.map