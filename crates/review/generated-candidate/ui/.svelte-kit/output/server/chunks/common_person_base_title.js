import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_person_base_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Person_Base_TitleInputs */
/**
* | output |
* | --- |
* | "Person Base" |
*
* @param {Common_Person_Base_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_person_base_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Person Base";
});
export { common_person_base_title as t };

//# sourceMappingURL=common_person_base_title.js.map