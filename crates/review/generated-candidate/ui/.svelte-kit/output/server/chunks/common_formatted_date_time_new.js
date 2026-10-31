import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_formatted_date_time_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Formatted_Date_Time_NewInputs */
/**
* | output |
* | --- |
* | "New Formatted Date Time" |
*
* @param {Common_Formatted_Date_Time_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_formatted_date_time_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Formatted Date Time";
});
export { common_formatted_date_time_new as t };

//# sourceMappingURL=common_formatted_date_time_new.js.map