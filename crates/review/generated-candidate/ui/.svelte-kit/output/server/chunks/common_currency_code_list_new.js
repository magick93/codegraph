import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_currency_code_list_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Currency_Code_List_NewInputs */
/**
* | output |
* | --- |
* | "New Currency Code List" |
*
* @param {Common_Currency_Code_List_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_currency_code_list_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Currency Code List";
});
export { common_currency_code_list_new as t };

//# sourceMappingURL=common_currency_code_list_new.js.map