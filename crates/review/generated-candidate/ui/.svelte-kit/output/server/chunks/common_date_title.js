import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_date_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Date_TitleInputs */
/**
* | output |
* | --- |
* | "Date" |
*
* @param {Common_Date_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_date_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Date";
});
export { common_date_title as t };

//# sourceMappingURL=common_date_title.js.map