import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_person_base_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Person_Base_NewInputs */
/**
* | output |
* | --- |
* | "New Person Base" |
*
* @param {Common_Person_Base_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_person_base_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Person Base";
});
export { common_person_base_new as t };

//# sourceMappingURL=common_person_base_new.js.map