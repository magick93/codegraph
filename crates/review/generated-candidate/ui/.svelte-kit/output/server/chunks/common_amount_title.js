import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_amount_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Amount_TitleInputs */
/**
* | output |
* | --- |
* | "Amount" |
*
* @param {Common_Amount_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_amount_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Amount";
});
export { common_amount_title as t };

//# sourceMappingURL=common_amount_title.js.map