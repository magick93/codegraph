import { _ as escape_html, h as attr, r as bind_props } from "./server2.js";
import { r as createAppForm } from "./structured-wrapper-field.js";
import { n as common_edit } from "./dist.js";
import "./button.js";
import { t as rsvp_rsvp_new } from "./rsvp_rsvp_new.js";
import { t as rsvp_rsvp_title } from "./rsvp_rsvp_title.js";
import { z } from "zod";
// src/lib/components/forms/RsvpForm.svelte
function RsvpForm($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { mode, initialData = void 0, onsubmit } = $$props;
		const rsvpSchema = z.object({
			event_id: z.uuid(),
			status: z.enum([
				"Confirmed",
				"Pending",
				"Cancelled"
			])
		});
		const defaultValues = {
			event_id: String(initialData?.event_id ?? ""),
			status: String(initialData?.status ?? ""),
			timestamp: String(initialData?.timestamp ?? "").replace(/Z$/, "")
		};
		const form = createAppForm(() => ({
			defaultValues,
			validators: { onSubmit: rsvpSchema },
			onSubmit: async ({ value }) => {
				form.setErrorMap({ onSubmit: void 0 });
				const payload = { ...value };
				if (payload["timestamp"]) payload["timestamp"] = String(payload["timestamp"]).includes("Z") ? payload["timestamp"] : payload["timestamp"] + ":00Z";
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
		$$renderer.push(`<form class="space-y-6"${attr("aria-label", mode === "create" ? rsvp_rsvp_new() : common_edit() + " " + rsvp_rsvp_title())} data-testid="rsvp-form">`);
		if (form.AppForm) {
			$$renderer.push("<!--[-->");
			form.AppForm($$renderer, {
				children: ($$renderer) => {
					{
						function children($$renderer, field) {
							if (field.TextField) {
								$$renderer.push("<!--[-->");
								field.TextField($$renderer, {
									label: "Event Id",
									testid: "rsvp-field-event_id",
									required: "true",
									description: "The event this RSVP is for (required value object that resolves to an entity)"
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
								name: "event_id",
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
							if (field.SelectField) {
								$$renderer.push("<!--[-->");
								field.SelectField($$renderer, {
									label: "Status",
									testid: "rsvp-field-status",
									values: [
										"Confirmed",
										"Pending",
										"Cancelled"
									],
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
								name: "status",
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
									label: "Timestamp",
									testid: "rsvp-field-timestamp",
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
								name: "timestamp",
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
								if (serverMessage) $$renderer.push(`<!--[0--><div class="rounded-md bg-destructive/10 border border-destructive/20 p-4 text-sm text-destructive" role="alert" data-testid="rsvp-form-error">${escape_html(typeof serverMessage === "string" ? serverMessage : "Request failed")}</div>`);
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
		bind_props($$props, { rsvpSchema });
	});
}
export { RsvpForm as t };

//# sourceMappingURL=RsvpForm.js.map