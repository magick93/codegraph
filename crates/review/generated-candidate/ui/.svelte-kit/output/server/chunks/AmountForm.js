import { _ as escape_html, h as attr, r as bind_props } from "./server2.js";
import { r as createAppForm } from "./structured-wrapper-field.js";
import { t as common_amount_new } from "./common_amount_new.js";
import { t as common_amount_title } from "./common_amount_title.js";
import { n as common_edit } from "./dist.js";
import "./button.js";
import { z } from "zod";
// src/lib/components/forms/AmountForm.svelte
function AmountForm($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { mode, initialData = void 0, onsubmit } = $$props;
		const amountSchema = z.object({
			currency: z.enum([
				"USD",
				"EUR",
				"GBP",
				"JPY",
				"AUD"
			]).optional(),
			value: z.number().optional()
		});
		const defaultValues = {
			currency: String(initialData?.currency ?? ""),
			value: initialData?.value != null ? Number(initialData?.value) : void 0
		};
		const form = createAppForm(() => ({
			defaultValues,
			validators: { onSubmit: amountSchema },
			onSubmit: async ({ value }) => {
				form.setErrorMap({ onSubmit: void 0 });
				const payload = { ...value };
				if (payload["currency"] === "") delete payload["currency"];
				if (payload["value"] === void 0 || Number.isNaN(payload["value"])) delete payload["value"];
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
		$$renderer.push(`<form class="space-y-6"${attr("aria-label", mode === "create" ? common_amount_new() : common_edit() + " " + common_amount_title())} data-testid="amount-form">`);
		if (form.AppForm) {
			$$renderer.push("<!--[-->");
			form.AppForm($$renderer, {
				children: ($$renderer) => {
					{
						function children($$renderer, field) {
							if (field.SelectField) {
								$$renderer.push("<!--[-->");
								field.SelectField($$renderer, {
									label: "Currency",
									testid: "amount-field-currency",
									values: [
										"USD",
										"EUR",
										"GBP",
										"JPY",
										"AUD"
									],
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
								name: "currency",
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
									label: "Value",
									testid: "amount-field-value",
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
								name: "value",
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
								if (serverMessage) $$renderer.push(`<!--[0--><div class="rounded-md bg-destructive/10 border border-destructive/20 p-4 text-sm text-destructive" role="alert" data-testid="amount-form-error">${escape_html(typeof serverMessage === "string" ? serverMessage : "Request failed")}</div>`);
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
		bind_props($$props, { amountSchema });
	});
}
export { AmountForm as t };

//# sourceMappingURL=AmountForm.js.map