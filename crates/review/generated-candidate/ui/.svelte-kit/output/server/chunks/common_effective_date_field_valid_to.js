import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_effective_date_field_valid_from.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Effective_Date_Field_Valid_FromInputs */
/**
* | output |
* | --- |
* | "Valid From" |
*
* @param {Common_Effective_Date_Field_Valid_FromInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_effective_date_field_valid_from = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Valid From";
});
// src/lib/paraglide/messages/common_effective_date_field_valid_to.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Effective_Date_Field_Valid_ToInputs */
/**
* | output |
* | --- |
* | "Valid To" |
*
* @param {Common_Effective_Date_Field_Valid_ToInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_effective_date_field_valid_to = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Valid To";
});
export { common_effective_date_field_valid_from as n, common_effective_date_field_valid_to as t };

//# sourceMappingURL=common_effective_date_field_valid_to.js.map