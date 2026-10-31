import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_identifier_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Identifier_NewInputs */
/**
* | output |
* | --- |
* | "New Identifier" |
*
* @param {Common_Identifier_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_identifier_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Identifier";
});
export { common_identifier_new as t };

//# sourceMappingURL=common_identifier_new.js.map