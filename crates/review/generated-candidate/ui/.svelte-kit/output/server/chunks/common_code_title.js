import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_code_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Code_TitleInputs */
/**
* | output |
* | --- |
* | "Code" |
*
* @param {Common_Code_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_code_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Code";
});
export { common_code_title as t };

//# sourceMappingURL=common_code_title.js.map