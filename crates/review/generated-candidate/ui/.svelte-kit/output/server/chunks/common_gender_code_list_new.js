import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_gender_code_list_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Gender_Code_List_NewInputs */
/**
* | output |
* | --- |
* | "New Gender Code List" |
*
* @param {Common_Gender_Code_List_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_gender_code_list_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Gender Code List";
});
export { common_gender_code_list_new as t };

//# sourceMappingURL=common_gender_code_list_new.js.map