import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_gender_code_list_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Gender_Code_List_TitleInputs */
/**
* | output |
* | --- |
* | "Gender Code List" |
*
* @param {Common_Gender_Code_List_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_gender_code_list_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Gender Code List";
});
export { common_gender_code_list_title as t };

//# sourceMappingURL=common_gender_code_list_title.js.map