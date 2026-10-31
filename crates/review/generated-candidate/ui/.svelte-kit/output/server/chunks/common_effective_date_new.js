import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_effective_date_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Effective_Date_NewInputs */
/**
* | output |
* | --- |
* | "New Effective Date" |
*
* @param {Common_Effective_Date_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_effective_date_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Effective Date";
});
export { common_effective_date_new as t };

//# sourceMappingURL=common_effective_date_new.js.map