import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/compensation_pay_run_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Compensation_Pay_Run_TitleInputs */
/**
* | output |
* | --- |
* | "Pay Run" |
*
* @param {Compensation_Pay_Run_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var compensation_pay_run_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Pay Run";
});
export { compensation_pay_run_title as t };

//# sourceMappingURL=compensation_pay_run_title.js.map