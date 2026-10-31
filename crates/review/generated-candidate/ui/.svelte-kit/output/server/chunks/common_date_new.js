import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_date_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Date_NewInputs */
/**
* | output |
* | --- |
* | "New Date" |
*
* @param {Common_Date_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_date_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Date";
});
export { common_date_new as t };

//# sourceMappingURL=common_date_new.js.map