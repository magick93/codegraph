import { n as attributes, r as bind_props } from "./server2.js";
import { r as getLocale } from "./runtime.js";
import "./input.js";
// src/lib/paraglide/messages/common_next.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_NextInputs */
/**
* | output |
* | --- |
* | "Next" |
*
* @param {Common_NextInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_next = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Next";
});
// src/lib/paraglide/messages/common_no_results.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{ query: NonNullable<unknown> }} Common_No_ResultsInputs */
/**
* | output |
* | --- |
* | "No results found for “{query}”" |
*
* @param {Common_No_ResultsInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_no_results = ((i, options = {}) => {
	options.locale ?? getLocale();
	return `No results found for “${i?.query}”`;
});
// src/lib/paraglide/messages/common_previous.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_PreviousInputs */
/**
* | output |
* | --- |
* | "Previous" |
*
* @param {Common_PreviousInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_previous = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Previous";
});
// src/lib/paraglide/messages/common_search_placeholder.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{ entity: NonNullable<unknown> }} Common_Search_PlaceholderInputs */
/**
* | output |
* | --- |
* | "Search {entity}..." |
*
* @param {Common_Search_PlaceholderInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_search_placeholder = ((i, options = {}) => {
	options.locale ?? getLocale();
	return `Search ${i?.entity}...`;
});
// src/lib/components/ui/table/index.js
function Body(_props) {}
function Cell(_props) {}
function Head(_props) {}
function Header$1(_props) {}
function Root$1(_props) {}
function Row(_props) {}
// src/lib/components/ui/empty/index.js
function Content(_props) {}
function Description(_props) {}
function Header(_props) {}
function Media(_props) {}
function Root(_props) {}
function Title(_props) {}
// src/lib/format.ts
/** The pre-W2 duck-typing chain, kept as the last resort for values whose
* shape generation could not resolve (dynamic objects only). */
function formatFallback(obj) {
	for (const key of [
		"value",
		"code",
		"name",
		"id"
	]) if (key in obj && obj[key] != null) return String(obj[key]);
	const vals = Object.values(obj).filter((v) => v != null && typeof v !== "object");
	return vals.length ? String(vals[0]) : JSON.stringify(obj);
}
// src/lib/components/ui/SearchInput.svelte
function SearchInput($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { value = "", onSearch, placeholder = "Search...", debounceMs = 300, "data-testid": testId, $$slots, $$events, ...rest } = $$props;
		$$renderer.push(`<div${attributes({
			class: "relative",
			...rest
		})}>`);
		$$renderer.push(`<!----> `);
		if (value) $$renderer.push(`<!--[0--><button type="button" class="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground" aria-label="Clear search">✕</button>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></div>`);
		bind_props($$props, { value });
	});
}
export { common_no_results as _, Header as a, Title as c, Head as d, Header$1 as f, common_previous as g, common_search_placeholder as h, Description as i, Body as l, Row as m, formatFallback as n, Media as o, Root$1 as p, Content as r, Root as s, SearchInput as t, Cell as u, common_next as v };

//# sourceMappingURL=SearchInput.js.map