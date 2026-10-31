import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_code_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Code_NewInputs */
/**
* | output |
* | --- |
* | "New Code" |
*
* @param {Common_Code_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_code_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Code";
});
export { common_code_new as t };

//# sourceMappingURL=common_code_new.js.map