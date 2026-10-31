import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_formatted_date_time_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Formatted_Date_Time_TitleInputs */
/**
* | output |
* | --- |
* | "Formatted Date Time" |
*
* @param {Common_Formatted_Date_Time_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_formatted_date_time_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Formatted Date Time";
});
export { common_formatted_date_time_title as t };

//# sourceMappingURL=common_formatted_date_time_title.js.map