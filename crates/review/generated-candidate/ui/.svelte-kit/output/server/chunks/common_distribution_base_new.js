import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_distribution_base_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Distribution_Base_NewInputs */
/**
* | output |
* | --- |
* | "New Distribution Base" |
*
* @param {Common_Distribution_Base_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_distribution_base_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Distribution Base";
});
export { common_distribution_base_new as t };

//# sourceMappingURL=common_distribution_base_new.js.map