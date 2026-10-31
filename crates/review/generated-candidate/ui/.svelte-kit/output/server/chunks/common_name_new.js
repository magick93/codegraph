import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_name_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Name_NewInputs */
/**
* | output |
* | --- |
* | "New Name" |
*
* @param {Common_Name_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_name_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Name";
});
export { common_name_new as t };

//# sourceMappingURL=common_name_new.js.map