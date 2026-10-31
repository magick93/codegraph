import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_identifier_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Identifier_TitleInputs */
/**
* | output |
* | --- |
* | "Identifier" |
*
* @param {Common_Identifier_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_identifier_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Identifier";
});
export { common_identifier_title as t };

//# sourceMappingURL=common_identifier_title.js.map