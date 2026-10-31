import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_currency_code_list_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Currency_Code_List_TitleInputs */
/**
* | output |
* | --- |
* | "Currency Code List" |
*
* @param {Common_Currency_Code_List_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_currency_code_list_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Currency Code List";
});
export { common_currency_code_list_title as t };

//# sourceMappingURL=common_currency_code_list_title.js.map