import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_distribution_base_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Distribution_Base_TitleInputs */
/**
* | output |
* | --- |
* | "Distribution Base" |
*
* @param {Common_Distribution_Base_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_distribution_base_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Distribution Base";
});
export { common_distribution_base_title as t };

//# sourceMappingURL=common_distribution_base_title.js.map