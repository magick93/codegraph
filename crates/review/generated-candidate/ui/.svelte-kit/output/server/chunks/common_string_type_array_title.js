import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_string_type_array_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_String_Type_Array_TitleInputs */
/**
* | output |
* | --- |
* | "String Type Array" |
*
* @param {Common_String_Type_Array_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_string_type_array_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "String Type Array";
});
export { common_string_type_array_title as t };

//# sourceMappingURL=common_string_type_array_title.js.map