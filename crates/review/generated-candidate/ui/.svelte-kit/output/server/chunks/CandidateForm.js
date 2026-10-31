import { _ as escape_html, h as attr, r as bind_props } from "./server2.js";
import { r as createAppForm, t as StructuredWrapperField } from "./structured-wrapper-field.js";
import { n as common_edit } from "./dist.js";
import "./button.js";
import { t as recruiting_candidate_new } from "./recruiting_candidate_new.js";
import { t as recruiting_candidate_title } from "./recruiting_candidate_title.js";
import "./array-field.js";
import { z } from "zod";
// src/lib/components/forms/CandidateForm.svelte
function CandidateForm($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { mode, initialData = void 0, onsubmit } = $$props;
		const candidateSchema = z.object({
			birth_date: z.iso.date().optional(),
			family_name: z.string(),
			given_name: z.string(),
			candidate_id: z.string(),
			compensation_expectation: z.number().optional(),
			compensation_expectation_currency: z.enum([
				"USD",
				"EUR",
				"GBP",
				"JPY",
				"AUD"
			]).optional(),
			distribution_guidelines_id: z.uuid().optional(),
			gender: z.enum([
				"Male",
				"Female",
				"NotSpecified",
				"Other"
			]).optional(),
			position_schedule_type_codes: z.enum([
				"FullTime",
				"PartTime",
				"FlexTime",
				"SharedTime"
			]).optional(),
			position_titles: z.array(z.string()).optional(),
			qualifications: z.array(z.string()).optional(),
			referred_by_application_id: z.uuid().optional(),
			status: z.enum([
				"active",
				"inactive",
				"withdrawn"
			]).optional(),
			uri: z.string().optional()
		});
		const defaultValues = {
			birth_date: String(initialData?.birth_date ?? ""),
			family_name: String(initialData?.family_name ?? ""),
			given_name: String(initialData?.given_name ?? ""),
			application_process_history: typeof initialData?.application_process_history === "object" && initialData?.application_process_history !== null && !Array.isArray(initialData?.application_process_history) ? initialData?.application_process_history : {},
			candidate_id: String(initialData?.candidate_id ?? ""),
			compensation_expectation: initialData?.compensation_expectation != null ? Number(initialData?.compensation_expectation) : void 0,
			compensation_expectation_currency: String(initialData?.compensation_expectation_currency ?? ""),
			distribution_guidelines_id: String(initialData?.distribution_guidelines_id ?? ""),
			external_identifier: typeof initialData?.external_identifier === "object" && initialData?.external_identifier !== null && !Array.isArray(initialData?.external_identifier) ? initialData?.external_identifier : {},
			gender: String(initialData?.gender ?? ""),
			person_name: typeof initialData?.person_name === "object" && initialData?.person_name !== null && !Array.isArray(initialData?.person_name) ? initialData?.person_name : {},
			position_schedule_type_codes: String(initialData?.position_schedule_type_codes ?? ""),
			position_titles: Array.isArray(initialData?.position_titles) ? initialData?.position_titles.map(String) : [],
			qualifications: Array.isArray(initialData?.qualifications) ? initialData?.qualifications : [],
			referred_by_application_id: String(initialData?.referred_by_application_id ?? ""),
			status: String(initialData?.status ?? ""),
			uri: String(initialData?.uri ?? "")
		};
		const form = createAppForm(() => ({
			defaultValues,
			validators: { onSubmit: candidateSchema },
			onSubmit: async ({ value }) => {
				form.setErrorMap({ onSubmit: void 0 });
				const payload = { ...value };
				if (payload["birth_date"] === "") delete payload["birth_date"];
				if (payload["compensation_expectation"] === void 0 || Number.isNaN(payload["compensation_expectation"])) delete payload["compensation_expectation"];
				if (payload["compensation_expectation_currency"] === "") delete payload["compensation_expectation_currency"];
				if (payload["distribution_guidelines_id"] === "") delete payload["distribution_guidelines_id"];
				if (payload["gender"] === "") delete payload["gender"];
				if (payload["position_schedule_type_codes"]) payload["position_schedule_type_codes"] = [{ code: payload["position_schedule_type_codes"] }];
				if (payload["position_schedule_type_codes"] === "") delete payload["position_schedule_type_codes"];
				if (payload["referred_by_application_id"] === "") delete payload["referred_by_application_id"];
				if (payload["status"] === "") delete payload["status"];
				if (payload["uri"] === "") delete payload["uri"];
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
		$$renderer.push(`<form class="space-y-6"${attr("aria-label", mode === "create" ? recruiting_candidate_new() : common_edit() + " " + recruiting_candidate_title())} data-testid="candidate-form">`);
		if (form.AppForm) {
			$$renderer.push("<!--[-->");
			form.AppForm($$renderer, {
				children: ($$renderer) => {
					{
						function children($$renderer, field) {
							if (field.DateField) {
								$$renderer.push("<!--[-->");
								field.DateField($$renderer, {
									label: "Birth Date",
									testid: "candidate-field-birth_date",
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
								name: "birth_date",
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
									label: "Family Name",
									testid: "candidate-field-family_name",
									required: "true",
									description: "Last name"
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
									label: "Given Name",
									testid: "candidate-field-given_name",
									required: "true",
									description: "First name"
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
					{
						function children($$renderer, field) {
							$$renderer.push(`<div class="space-y-2">`);
							StructuredWrapperField($$renderer, {
								label: "Application Process History",
								required: "false",
								isArray: "false",
								fieldName: "application_process_history",
								subFields: [
									{
										name: "actionDate",
										label: "Action Date",
										required: false,
										showByDefault: true,
										description: `The date the action was executed.`
									},
									{
										name: "descriptions",
										label: "Descriptions",
										required: false,
										showByDefault: true,
										description: `Additional details about the history item.`
									},
									{
										name: "id",
										label: "Id",
										required: false,
										showByDefault: false,
										description: `The identifier for the history item.`
									}
								],
								value: field.state.value,
								onvaluechange: (v) => field.handleChange(v)
							});
							$$renderer.push(`<!----></div>`);
						}
						if (form.AppField) {
							$$renderer.push("<!--[-->");
							form.AppField($$renderer, {
								name: "application_process_history",
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
									label: "Candidate Id",
									testid: "candidate-field-candidate_id",
									required: "true",
									description: "Unique candidate identifier"
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
								name: "candidate_id",
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
									label: "Compensation Expectation",
									testid: "candidate-field-compensation_expectation",
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
								name: "compensation_expectation",
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
									label: "Compensation Expectation Currency",
									testid: "candidate-field-compensation_expectation_currency",
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
								name: "compensation_expectation_currency",
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
									label: "Distribution Guidelines Id",
									testid: "candidate-field-distribution_guidelines_id",
									required: "false",
									description: "Distribution guidelines (inline def with allOf)"
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
								name: "distribution_guidelines_id",
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
							$$renderer.push(`<div class="space-y-2">`);
							StructuredWrapperField($$renderer, {
								label: "External Identifier",
								required: "false",
								isArray: "false",
								fieldName: "external_identifier",
								subFields: [
									{
										name: "value",
										label: "Value",
										required: true,
										showByDefault: true,
										description: ``
									},
									{
										name: "schemeAgencyId",
										label: "Scheme Agency Id",
										required: false,
										showByDefault: true,
										description: ``
									},
									{
										name: "schemeId",
										label: "Scheme Id",
										required: false,
										showByDefault: false,
										description: ``
									},
									{
										name: "schemeVersionId",
										label: "Scheme Version Id",
										required: false,
										showByDefault: false,
										description: ``
									}
								],
								value: field.state.value,
								onvaluechange: (v) => field.handleChange(v)
							});
							$$renderer.push(`<!----></div>`);
						}
						if (form.AppField) {
							$$renderer.push("<!--[-->");
							form.AppField($$renderer, {
								name: "external_identifier",
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
									label: "Gender",
									testid: "candidate-field-gender",
									values: [
										"Male",
										"Female",
										"NotSpecified",
										"Other"
									],
									required: "false",
									description: "Gender codelist reference"
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
								name: "gender",
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
							$$renderer.push(`<div class="space-y-2">`);
							StructuredWrapperField($$renderer, {
								label: "Person Name",
								required: "false",
								isArray: "false",
								fieldName: "person_name",
								subFields: [
									{
										name: "familyName",
										label: "Family Name",
										required: false,
										showByDefault: true,
										description: ``
									},
									{
										name: "formattedName",
										label: "Formatted Name",
										required: false,
										showByDefault: true,
										description: ``
									},
									{
										name: "givenName",
										label: "Given Name",
										required: false,
										showByDefault: false,
										description: ``
									}
								],
								value: field.state.value,
								onvaluechange: (v) => field.handleChange(v)
							});
							$$renderer.push(`<!----></div>`);
						}
						if (form.AppField) {
							$$renderer.push("<!--[-->");
							form.AppField($$renderer, {
								name: "person_name",
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
									label: "Position Schedule Type Codes",
									testid: "candidate-field-position_schedule_type_codes",
									values: [
										"FullTime",
										"PartTime",
										"FlexTime",
										"SharedTime"
									],
									required: "false",
									description: "Schedule type preferences (array of codelist)"
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
								name: "position_schedule_type_codes",
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
							$$renderer.push(`<div class="space-y-2"><span class="text-sm font-medium">Position Titles</span> `);
							field.state.value;
							$$renderer.push(`<!----></div>`);
						}
						if (form.AppField) {
							$$renderer.push("<!--[-->");
							form.AppField($$renderer, {
								name: "position_titles",
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
							$$renderer.push(`<div class="space-y-2">`);
							StructuredWrapperField($$renderer, {
								label: "Qualifications",
								required: "false",
								isArray: "true",
								fieldName: "qualifications",
								subFields: [
									{
										name: "qualificationName",
										label: "Qualification Name",
										required: true,
										showByDefault: true,
										description: ``
									},
									{
										name: "dateAwarded",
										label: "Date Awarded",
										required: false,
										showByDefault: true,
										description: ``
									},
									{
										name: "issuer",
										label: "Issuer",
										required: false,
										showByDefault: false,
										description: ``
									}
								],
								value: field.state.value,
								onvaluechange: (v) => field.handleChange(v)
							});
							$$renderer.push(`<!----></div>`);
						}
						if (form.AppField) {
							$$renderer.push("<!--[-->");
							form.AppField($$renderer, {
								name: "qualifications",
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
									label: "Referred By Application Id",
									testid: "candidate-field-referred_by_application_id",
									required: "false",
									description: "Source application (entity reference)"
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
								name: "referred_by_application_id",
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
									testid: "candidate-field-status",
									values: [
										"active",
										"inactive",
										"withdrawn"
									],
									required: "false",
									description: "Current candidate status"
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
							if (field.TextField) {
								$$renderer.push("<!--[-->");
								field.TextField($$renderer, {
									label: "Uri",
									testid: "candidate-field-uri",
									required: "false",
									description: "Public profile URL"
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
								name: "uri",
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
								if (serverMessage) $$renderer.push(`<!--[0--><div class="rounded-md bg-destructive/10 border border-destructive/20 p-4 text-sm text-destructive" role="alert" data-testid="candidate-form-error">${escape_html(typeof serverMessage === "string" ? serverMessage : "Request failed")}</div>`);
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
		bind_props($$props, { candidateSchema });
	});
}
export { CandidateForm as t };

//# sourceMappingURL=CandidateForm.js.map