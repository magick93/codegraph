import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_amount_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Amount_NewInputs */
/**
* | output |
* | --- |
* | "New Amount" |
*
* @param {Common_Amount_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_amount_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Amount";
});
export { common_amount_new as t };

//# sourceMappingURL=common_amount_new.js.map