import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_name_field_family_name.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Name_Field_Family_NameInputs */
/**
* | output |
* | --- |
* | "Family Name" |
*
* @param {Common_Name_Field_Family_NameInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_name_field_family_name = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Family Name";
});
// src/lib/paraglide/messages/common_name_field_formatted_name.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Name_Field_Formatted_NameInputs */
/**
* | output |
* | --- |
* | "Formatted Name" |
*
* @param {Common_Name_Field_Formatted_NameInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_name_field_formatted_name = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Formatted Name";
});
// src/lib/paraglide/messages/common_name_field_given_name.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Name_Field_Given_NameInputs */
/**
* | output |
* | --- |
* | "Given Name" |
*
* @param {Common_Name_Field_Given_NameInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_name_field_given_name = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Given Name";
});
export { common_name_field_formatted_name as n, common_name_field_family_name as r, common_name_field_given_name as t };

//# sourceMappingURL=common_name_field_given_name.js.map