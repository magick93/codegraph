import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_amount_field_currency.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Amount_Field_CurrencyInputs */
/**
* | output |
* | --- |
* | "Currency" |
*
* @param {Common_Amount_Field_CurrencyInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_amount_field_currency = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Currency";
});
// src/lib/paraglide/messages/common_amount_field_value.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Amount_Field_ValueInputs */
/**
* | output |
* | --- |
* | "Value" |
*
* @param {Common_Amount_Field_ValueInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_amount_field_value = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Value";
});
export { common_amount_field_currency as n, common_amount_field_value as t };

//# sourceMappingURL=common_amount_field_value.js.map