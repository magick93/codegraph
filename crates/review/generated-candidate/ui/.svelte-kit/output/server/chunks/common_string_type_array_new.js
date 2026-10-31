import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_string_type_array_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_String_Type_Array_NewInputs */
/**
* | output |
* | --- |
* | "New String Type Array" |
*
* @param {Common_String_Type_Array_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_string_type_array_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New String Type Array";
});
export { common_string_type_array_new as t };

//# sourceMappingURL=common_string_type_array_new.js.map