import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_position_schedule_type_code_list_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Position_Schedule_Type_Code_List_TitleInputs */
/**
* | output |
* | --- |
* | "Position Schedule Type Code List" |
*
* @param {Common_Position_Schedule_Type_Code_List_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_position_schedule_type_code_list_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Position Schedule Type Code List";
});
export { common_position_schedule_type_code_list_title as t };

//# sourceMappingURL=common_position_schedule_type_code_list_title.js.map