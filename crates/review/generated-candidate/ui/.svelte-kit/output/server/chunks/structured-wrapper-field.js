import { _ as escape_html, a as ensure_array_like, c as spread_props, f as getContext, h as attr, i as derived, p as setContext, r as bind_props, t as attr_class, u as stringify } from "./server2.js";
import "./index-server2.js";
import "./button.js";
import { r as getLocale } from "./runtime.js";
import "./input.js";
import { FieldApi, FormApi, FormGroupApi } from "@tanstack/form-core";
// src/lib/paraglide/messages/common_create.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_CreateInputs */
/**
* | output |
* | --- |
* | "Create" |
*
* @param {Common_CreateInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_create = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Create";
});
// src/lib/paraglide/messages/common_save.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_SaveInputs */
/**
* | output |
* | --- |
* | "Save Changes" |
*
* @param {Common_SaveInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_save = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Save Changes";
});
// src/lib/paraglide/messages/common_saving.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_SavingInputs */
/**
* | output |
* | --- |
* | "Saving…" |
*
* @param {Common_SavingInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_saving = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Saving…";
});
// node_modules/@tanstack/svelte-store/dist/useSelector.svelte.js
/**
* Selects a slice of state from an atom or store and exposes it through a
* rune-friendly holder object.
*
* Read the selected value from `.current`.
*
* @example
* ```ts
* const count = useSelector(counterStore, (state) => state.count)
* console.log(count.current)
* ```
*
* @example
* ```ts
* const doubled = useSelector(countAtom, (value) => value * 2)
* ```
*/
function useSelector(source, selector = (d) => d, options = {}) {
	options.compare;
	let slice = selector(source.get());
	return { get current() {
		return slice;
	} };
}
// node_modules/@tanstack/svelte-form/dist/Field.svelte
function createField(opts) {
	const options = opts();
	const api = new FieldApi(options);
	const extendedApi = api;
	const storeSub = useSelector(api.store, (state) => options.mode === "array" ? state.meta._arrayVersion || 0 : state.value);
	const metaIsTouchedSub = useSelector(api.store, (state) => state.meta.isTouched);
	const metaIsBlurredSub = useSelector(api.store, (state) => state.meta.isBlurred);
	const metaIsDirtySub = useSelector(api.store, (state) => state.meta.isDirty);
	const metaErrorMapSub = useSelector(api.store, (state) => state.meta.errorMap);
	const metaErrorSourceMapSub = useSelector(api.store, (state) => state.meta.errorSourceMap);
	const metaIsValidatingSub = useSelector(api.store, (state) => state.meta.isValidating);
	Object.defineProperty(extendedApi, "state", { get() {
		const trackedValue = storeSub.current;
		const isTouched = metaIsTouchedSub.current;
		const isBlurred = metaIsBlurredSub.current;
		const isDirty = metaIsDirtySub.current;
		const errorMap = metaErrorMapSub.current;
		const errorSourceMap = metaErrorSourceMapSub.current;
		const isValidating = metaIsValidatingSub.current;
		const baseState = api.store.state;
		return {
			...baseState,
			value: options.mode === "array" ? baseState.value : trackedValue,
			meta: {
				...baseState.meta,
				isTouched,
				isBlurred,
				isDirty,
				errorMap,
				errorSourceMap,
				isValidating
			}
		};
	} });
	return extendedApi;
}
function Field$1($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { children, $$slots, $$events, ...fieldOptions } = $$props;
		children($$renderer, createField(() => {
			return fieldOptions;
		}));
		$$renderer.push(`<!---->`);
	});
}
// node_modules/@tanstack/svelte-form/dist/FormGroup.svelte
function createFormGroup(opts) {
	const options = opts();
	const api = new FormGroupApi(options);
	const extendedApi = api;
	const storeSub = useSelector(api.store);
	Object.defineProperty(extendedApi, "state", { get() {
		return storeSub.current;
	} });
	return extendedApi;
}
function FormGroup($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { children, $$slots, $$events, ...formGroupOptions } = $$props;
		children($$renderer, createFormGroup(() => {
			return formGroupOptions;
		}));
		$$renderer.push(`<!---->`);
	});
}
// node_modules/@tanstack/svelte-form/dist/Subscribe.svelte
function Subscribe($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { children, store, selector = (state) => state } = $$props;
		children($$renderer, useSelector(store, selector).current);
		$$renderer.push(`<!---->`);
	});
}
// node_modules/@tanstack/svelte-form/dist/createForm.svelte.js
function createForm(opts) {
	const options = opts?.();
	const api = new FormApi(options);
	const extendedApi = api;
	extendedApi.Field = (internal, props) => Field$1(internal, {
		...props,
		form: api
	});
	extendedApi.FormGroup = (internal, props) => FormGroup(internal, {
		...props,
		form: api
	});
	extendedApi.useSelector = (selector) => useSelector(api.store, selector);
	/** @deprecated Use `form.useSelector` instead. */
	extendedApi.useStore = extendedApi.useSelector;
	extendedApi.Subscribe = (internal, props) => Subscribe(internal, {
		...props,
		store: api.store
	});
	api.mount;
	return extendedApi;
}
// node_modules/@tanstack/svelte-form/dist/context-keys.js
var fieldContextKey = "__tanstack_field_context_key";
var formContextKey = "__tanstack_form_context_key";
// node_modules/@tanstack/svelte-form/dist/AppForm.svelte
function AppForm($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		const { children, form } = $$props;
		setContext(formContextKey, form);
		children?.($$renderer);
		$$renderer.push(`<!---->`);
	});
}
// node_modules/@tanstack/svelte-form/dist/InnerAppField.svelte
function InnerAppField($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		const { children, field, fieldComponents } = $$props;
		setContext(fieldContextKey, field);
		children?.($$renderer, Object.assign(field, fieldComponents));
		$$renderer.push(`<!---->`);
	});
}
// node_modules/@tanstack/svelte-form/dist/AppField.svelte
function AppField($$renderer, $$props) {
	const { children: childrenProp, form, fieldComponents, fieldProps } = $$props;
	{
		function children($$renderer, field) {
			InnerAppField($$renderer, {
				field,
				children: childrenProp,
				fieldComponents
			});
		}
		if (form.Field) {
			$$renderer.push("<!--[-->");
			form.Field($$renderer, spread_props([fieldProps, {
				children,
				$$slots: { default: true }
			}]));
			$$renderer.push("<!--]-->");
		} else {
			$$renderer.push("<!--[!-->");
			$$renderer.push("<!--]-->");
		}
	}
}
// node_modules/@tanstack/svelte-form/dist/createFormCreator.svelte.js
function createFormCreatorContexts() {
	function useFieldContext() {
		const field = getContext(fieldContextKey);
		if (!field) throw new Error("`fieldContext` only works when within a `fieldComponent` passed to `createFormCreator`");
		return field;
	}
	function useFormContext() {
		const form = getContext(formContextKey);
		if (!form) throw new Error("`formContext` only works when within a `formComponent` passed to `createFormCreator`");
		return form;
	}
	return {
		useFieldContext,
		useFormContext
	};
}
function createFormCreator({ fieldComponents, formComponents }) {
	function createAppForm(props) {
		const form = createForm(props);
		const AppForm$1 = (internal, props) => {
			return AppForm(internal, {
				...props,
				form
			});
		};
		const AppField$1 = (internal, { children, ...fieldProps }) => AppField(internal, {
			fieldProps,
			form,
			fieldComponents,
			children
		});
		return Object.assign(form, {
			AppField: AppField$1,
			AppForm: AppForm$1,
			...formComponents
		});
	}
	function getFormType(_formProps) {}
	return {
		createAppForm,
		getFormType
	};
}
// src/lib/forms/form-context.ts
var { useFieldContext, useFormContext } = createFormCreatorContexts();
// src/lib/forms/fields/TextField.svelte
function TextField($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { label, testid, required = false, description = "", disabled = false } = $$props;
		const field = useFieldContext();
		const errors = derived(() => field.state.meta.errors.map((e) => typeof e === "string" ? e : e?.message ?? "").filter(Boolean));
		$$renderer.push(`<div class="space-y-2"><label class="text-sm font-medium"${attr("for", testid)}>${escape_html(label)}`);
		if (required) $$renderer.push(`<!--[0--> <span class="text-destructive">*</span>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></label> <input${attr("id", testid)}${attr("name", field.name)} class="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm" type="text"${attr("value", field.state.value)}${attr("required", required, true)}${attr("aria-required", required ? "true" : void 0)}${attr("disabled", disabled, true)}${attr("data-testid", testid)}${attr("data-validate-message", errors()[0] ?? "")}${attr("aria-invalid", errors().length > 0 ? "true" : void 0)}/> `);
		if (description) $$renderer.push(`<!--[0--><p class="text-muted-foreground text-sm">${escape_html(description)}</p>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> `);
		if (errors().length > 0) $$renderer.push(`<!--[0--><p class="text-destructive text-sm"${attr("data-testid", `${stringify(testid)}-error`)}>${escape_html(errors()[0])}</p>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></div>`);
	});
}
// src/lib/forms/fields/EmailField.svelte
function EmailField($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { label, testid, required = false, description = "", disabled = false } = $$props;
		const field = useFieldContext();
		const errors = derived(() => field.state.meta.errors.map((e) => typeof e === "string" ? e : e?.message ?? "").filter(Boolean));
		$$renderer.push(`<div class="space-y-2"><label class="text-sm font-medium"${attr("for", testid)}>${escape_html(label)}`);
		if (required) $$renderer.push(`<!--[0--> <span class="text-destructive">*</span>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></label> <input${attr("id", testid)}${attr("name", field.name)} class="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm" type="email"${attr("value", field.state.value)}${attr("required", required, true)}${attr("aria-required", required ? "true" : void 0)}${attr("disabled", disabled, true)}${attr("data-testid", testid)}${attr("data-validate-message", errors()[0] ?? "")}${attr("aria-invalid", errors().length > 0 ? "true" : void 0)}/> `);
		if (description) $$renderer.push(`<!--[0--><p class="text-muted-foreground text-sm">${escape_html(description)}</p>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> `);
		if (errors().length > 0) $$renderer.push(`<!--[0--><p class="text-destructive text-sm"${attr("data-testid", `${stringify(testid)}-error`)}>${escape_html(errors()[0])}</p>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></div>`);
	});
}
// src/lib/forms/fields/NumberField.svelte
function NumberField($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { label, testid, required = false, description = "", disabled = false } = $$props;
		const field = useFieldContext();
		const errors = derived(() => field.state.meta.errors.map((e) => typeof e === "string" ? e : e?.message ?? "").filter(Boolean));
		$$renderer.push(`<div class="space-y-2"><label class="text-sm font-medium"${attr("for", testid)}>${escape_html(label)}`);
		if (required) $$renderer.push(`<!--[0--> <span class="text-destructive">*</span>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></label> <input${attr("id", testid)}${attr("name", field.name)} class="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm" type="number" step="any"${attr("value", field.state.value ?? "")}${attr("required", required, true)}${attr("aria-required", required ? "true" : void 0)}${attr("disabled", disabled, true)}${attr("data-testid", testid)}${attr("data-validate-message", errors()[0] ?? "")}${attr("aria-invalid", errors().length > 0 ? "true" : void 0)}/> `);
		if (description) $$renderer.push(`<!--[0--><p class="text-muted-foreground text-sm">${escape_html(description)}</p>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> `);
		if (errors().length > 0) $$renderer.push(`<!--[0--><p class="text-destructive text-sm"${attr("data-testid", `${stringify(testid)}-error`)}>${escape_html(errors()[0])}</p>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></div>`);
	});
}
// src/lib/forms/fields/CheckboxField.svelte
function CheckboxField($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { label, testid, required = false, description = "", disabled = false } = $$props;
		const field = useFieldContext();
		const errors = derived(() => field.state.meta.errors.map((e) => typeof e === "string" ? e : e?.message ?? "").filter(Boolean));
		$$renderer.push(`<div class="flex items-center gap-3">`);
		field.state.value, errors()[0];
		$$renderer.push(`<!----> <span class="text-sm font-medium">${escape_html(label)}</span> `);
		if (errors().length > 0) $$renderer.push(`<!--[0--><p class="text-destructive text-sm"${attr("data-testid", `${stringify(testid)}-error`)}>${escape_html(errors()[0])}</p>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></div>`);
	});
}
// src/lib/forms/fields/SelectField.svelte
function SelectField($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { label, testid, values, required = false, description = "", disabled = false } = $$props;
		const field = useFieldContext();
		const errors = derived(() => field.state.meta.errors.map((e) => typeof e === "string" ? e : e?.message ?? "").filter(Boolean));
		$$renderer.push(`<div class="space-y-2"><label class="text-sm font-medium"${attr("for", testid)}>${escape_html(label)}`);
		if (required) $$renderer.push(`<!--[0--> <span class="text-destructive">*</span>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></label> `);
		$$renderer.select({
			id: testid,
			name: field.name,
			class: "flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm",
			value: field.state.value,
			onchange: (e) => field.handleChange(e.currentTarget.value),
			onblur: field.handleBlur,
			required,
			"aria-required": required ? "true" : void 0,
			disabled,
			"data-testid": testid,
			"data-validate-message": errors()[0] ?? "",
			"aria-invalid": errors().length > 0 ? "true" : void 0
		}, ($$renderer) => {
			$$renderer.option({
				value: "",
				disabled: required
			}, ($$renderer) => {
				$$renderer.push(`Select…`);
			});
			$$renderer.push(`<!--[-->`);
			const each_array = ensure_array_like(values);
			for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
				let val = each_array[$$index];
				$$renderer.option({ value: val }, ($$renderer) => {
					$$renderer.push(`${escape_html(val)}`);
				});
			}
			$$renderer.push(`<!--]-->`);
		});
		$$renderer.push(` `);
		if (description) $$renderer.push(`<!--[0--><p class="text-muted-foreground text-sm">${escape_html(description)}</p>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> `);
		if (errors().length > 0) $$renderer.push(`<!--[0--><p class="text-destructive text-sm"${attr("data-testid", `${stringify(testid)}-error`)}>${escape_html(errors()[0])}</p>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></div>`);
	});
}
// src/lib/forms/fields/DateField.svelte
function DateField($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { label, testid, required = false, description = "", disabled = false } = $$props;
		const field = useFieldContext();
		const errors = derived(() => field.state.meta.errors.map((e) => typeof e === "string" ? e : e?.message ?? "").filter(Boolean));
		$$renderer.push(`<div class="space-y-2"><label class="text-sm font-medium"${attr("for", testid)}>${escape_html(label)}`);
		if (required) $$renderer.push(`<!--[0--> <span class="text-destructive">*</span>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></label> <input${attr("id", testid)}${attr("name", field.name)} class="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm" type="date"${attr("value", field.state.value)}${attr("required", required, true)}${attr("aria-required", required ? "true" : void 0)}${attr("disabled", disabled, true)}${attr("data-testid", testid)}${attr("data-validate-message", errors()[0] ?? "")}${attr("aria-invalid", errors().length > 0 ? "true" : void 0)}/> `);
		if (description) $$renderer.push(`<!--[0--><p class="text-muted-foreground text-sm">${escape_html(description)}</p>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> `);
		if (errors().length > 0) $$renderer.push(`<!--[0--><p class="text-destructive text-sm"${attr("data-testid", `${stringify(testid)}-error`)}>${escape_html(errors()[0])}</p>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></div>`);
	});
}
// src/lib/forms/fields/DateTimeField.svelte
function DateTimeField($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { label, testid, required = false, description = "", disabled = false } = $$props;
		const field = useFieldContext();
		const errors = derived(() => field.state.meta.errors.map((e) => typeof e === "string" ? e : e?.message ?? "").filter(Boolean));
		$$renderer.push(`<div class="space-y-2"><label class="text-sm font-medium"${attr("for", testid)}>${escape_html(label)}`);
		if (required) $$renderer.push(`<!--[0--> <span class="text-destructive">*</span>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></label> <input${attr("id", testid)}${attr("name", field.name)} class="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm" type="datetime-local"${attr("value", field.state.value)}${attr("required", required, true)}${attr("aria-required", required ? "true" : void 0)}${attr("disabled", disabled, true)}${attr("data-testid", testid)}${attr("data-validate-message", errors()[0] ?? "")}${attr("aria-invalid", errors().length > 0 ? "true" : void 0)}/> `);
		if (description) $$renderer.push(`<!--[0--><p class="text-muted-foreground text-sm">${escape_html(description)}</p>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> `);
		if (errors().length > 0) $$renderer.push(`<!--[0--><p class="text-destructive text-sm"${attr("data-testid", `${stringify(testid)}-error`)}>${escape_html(errors()[0])}</p>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></div>`);
	});
}
// src/lib/forms/fields/TextareaField.svelte
function TextareaField($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { label, testid, required = false, description = "", disabled = false } = $$props;
		const field = useFieldContext();
		const errors = derived(() => field.state.meta.errors.map((e) => typeof e === "string" ? e : e?.message ?? "").filter(Boolean));
		$$renderer.push(`<div class="space-y-2"><label class="text-sm font-medium"${attr("for", testid)}>${escape_html(label)}`);
		if (required) $$renderer.push(`<!--[0--> <span class="text-destructive">*</span>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></label> <textarea${attr("id", testid)}${attr("name", field.name)} class="flex min-h-[60px] w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm"${attr("required", required, true)}${attr("aria-required", required ? "true" : void 0)}${attr("disabled", disabled, true)}${attr("data-testid", testid)}${attr("data-validate-message", errors()[0] ?? "")}${attr("aria-invalid", errors().length > 0 ? "true" : void 0)}>`);
		const $$body = escape_html(field.state.value);
		if ($$body) $$renderer.push(`${$$body}`);
		$$renderer.push(`</textarea> `);
		if (description) $$renderer.push(`<!--[0--><p class="text-muted-foreground text-sm">${escape_html(description)}</p>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> `);
		if (errors().length > 0) $$renderer.push(`<!--[0--><p class="text-destructive text-sm"${attr("data-testid", `${stringify(testid)}-error`)}>${escape_html(errors()[0])}</p>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></div>`);
	});
}
// src/lib/forms/fields/SubscribeButton.svelte
function SubscribeButton($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { label, testid } = $$props;
		const form = useFormContext();
		const pending = form.useSelector((s) => s.isSubmitting);
		const canSubmit = form.useSelector((s) => s.canSubmit);
		$$renderer.push(`<button type="submit" class="inline-flex h-9 items-center justify-center rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground shadow disabled:pointer-events-none disabled:opacity-50"${attr("disabled", !canSubmit.current || pending.current, true)}${attr("data-testid", testid)}>${escape_html(pending.current ? "Saving…" : label)}</button>`);
	});
}
// src/lib/forms/fields/ResetButton.svelte
function ResetButton($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { label = "Reset" } = $$props;
		useFormContext();
		$$renderer.push(`<button type="button" class="inline-flex h-9 items-center justify-center rounded-md border border-input bg-transparent px-4 text-sm font-medium shadow-sm">${escape_html(label)}</button>`);
	});
}
// src/lib/forms/form.ts
var { createAppForm } = createFormCreator({
	fieldComponents: {
		TextField,
		EmailField,
		NumberField,
		CheckboxField,
		SelectField,
		DateField,
		DateTimeField,
		TextareaField
	},
	formComponents: {
		SubscribeButton,
		ResetButton
	}
});
// src/lib/components/ui/spinner/index.js
function Spinner(_props) {}
// src/lib/components/ui/field/index.js
function Field(_props) {}
// src/lib/components/ui/structured-wrapper-field/StructuredWrapperField.svelte
function StructuredWrapperField($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { label, required = false, subFields, isArray = false, value = isArray ? [] : {}, onchange, disabled = false, fieldName = "" } = $$props;
		let items = isArray ? Array.isArray(value) ? value : [{}] : [];
		let expandedItems = /* @__PURE__ */ new Set();
		const visibleByDefault = derived(() => [...new Map(subFields.filter((f) => f.showByDefault).map((f) => [f.name, f])).values()]);
		const hiddenFields = derived(() => [...new Map(subFields.filter((f) => !f.showByDefault).map((f) => [f.name, f])).values()]);
		if (!isArray) {
			$$renderer.push(`<!--[0--><div class="space-y-3"><div class="text-sm font-medium">${escape_html(label)}`);
			if (required) $$renderer.push(`<!--[0--><span class="text-destructive">*</span>`);
			else $$renderer.push("<!--[-1-->");
			$$renderer.push(`<!--]--></div> <!--[-->`);
			const each_array = ensure_array_like(visibleByDefault());
			for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
				each_array[$$index];
				if (Field) {
					$$renderer.push("<!--[-->");
					$$renderer.push("<!--]-->");
				} else {
					$$renderer.push("<!--[!-->");
					$$renderer.push("<!--]-->");
				}
			}
			$$renderer.push(`<!--]--> `);
			if (hiddenFields().length > 0) {
				$$renderer.push("<!--[0-->");
				$$renderer.push("<!--[-1-->");
				$$renderer.push(`<!--]-->`);
			} else $$renderer.push("<!--[-1-->");
			$$renderer.push(`<!--]--></div>`);
		} else {
			$$renderer.push(`<!--[-1--><div class="space-y-2"><div class="text-sm font-medium">${escape_html(label)}`);
			if (required) $$renderer.push(`<!--[0--><span class="text-destructive">*</span>`);
			else $$renderer.push("<!--[-1-->");
			$$renderer.push(`<!--]--></div> <!--[-->`);
			const each_array_2 = ensure_array_like(items);
			for (let idx = 0, $$length = each_array_2.length; idx < $$length; idx++) {
				let item = each_array_2[idx];
				$$renderer.push(`<div${attr_class(`rounded-md border p-3 space-y-2 ${expandedItems.has(idx) ? "border-primary" : "border-border"}`)}><div class="flex gap-2 items-end"><!--[-->`);
				const each_array_3 = ensure_array_like(visibleByDefault());
				for (let $$index_2 = 0, $$length = each_array_3.length; $$index_2 < $$length; $$index_2++) {
					let sf = each_array_3[$$index_2];
					$$renderer.push(`<div class="flex-1 space-y-1"><label${attr("for", `${stringify(fieldName)}-${stringify(idx)}-${stringify(sf.name)}`)} class="text-xs text-muted-foreground uppercase tracking-wide">${escape_html(sf.label)}`);
					if (sf.required) $$renderer.push(`<!--[0-->*`);
					else $$renderer.push("<!--[-1-->");
					$$renderer.push(`<!--]--></label> `);
					`${stringify(fieldName)}${stringify(idx)}${stringify(sf.name)}`, `${stringify(fieldName)}${stringify(idx)}${stringify(sf.name)}`, item[sf.name];
					$$renderer.push(`<!----></div>`);
				}
				$$renderer.push(`<!--]--> `);
				if (hiddenFields().length > 0) {
					$$renderer.push("<!--[0-->");
					expandedItems.has(idx);
				} else $$renderer.push("<!--[-1-->");
				$$renderer.push(`<!--]--> `);
				$$renderer.push(`<!----></div> `);
				if (expandedItems.has(idx) && hiddenFields().length > 0) {
					$$renderer.push(`<!--[0--><div class="grid grid-cols-2 gap-2 border-t pt-2"><!--[-->`);
					const each_array_4 = ensure_array_like(hiddenFields());
					for (let $$index_3 = 0, $$length = each_array_4.length; $$index_3 < $$length; $$index_3++) {
						let sf = each_array_4[$$index_3];
						$$renderer.push(`<div class="space-y-1"><label${attr("for", `${stringify(fieldName)}-${stringify(idx)}-${stringify(sf.name)}`)} class="text-xs text-muted-foreground uppercase tracking-wide">${escape_html(sf.label)}</label> `);
						`${stringify(fieldName)}${stringify(idx)}${stringify(sf.name)}`, item[sf.name];
						$$renderer.push(`<!----></div>`);
					}
					$$renderer.push(`<!--]--></div>`);
				} else $$renderer.push("<!--[-1-->");
				$$renderer.push(`<!--]--></div>`);
			}
			$$renderer.push(`<!--]--> `);
			$$renderer.push(`<!----></div>`);
		}
		$$renderer.push(`<!--]-->`);
		bind_props($$props, { value });
	});
}
export { common_save as a, common_saving as i, Spinner as n, common_create as o, createAppForm as r, StructuredWrapperField as t };

//# sourceMappingURL=structured-wrapper-field.js.map