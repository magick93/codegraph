import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_effective_date_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Effective_Date_TitleInputs */
/**
* | output |
* | --- |
* | "Effective Date" |
*
* @param {Common_Effective_Date_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_effective_date_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Effective Date";
});
export { common_effective_date_title as t };

//# sourceMappingURL=common_effective_date_title.js.map