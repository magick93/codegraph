import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_no.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_NoInputs */
/**
* | output |
* | --- |
* | "No" |
*
* @param {Common_NoInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_no = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "No";
});
// src/lib/paraglide/messages/common_yes.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_YesInputs */
/**
* | output |
* | --- |
* | "Yes" |
*
* @param {Common_YesInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_yes = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Yes";
});
export { common_no as n, common_yes as t };

//# sourceMappingURL=common_yes.js.map