import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_back_to_list.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Back_To_ListInputs */
/**
* | output |
* | --- |
* | "Back to list" |
*
* @param {Common_Back_To_ListInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_back_to_list = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Back to list";
});
export { common_back_to_list as t };

//# sourceMappingURL=common_back_to_list.js.map