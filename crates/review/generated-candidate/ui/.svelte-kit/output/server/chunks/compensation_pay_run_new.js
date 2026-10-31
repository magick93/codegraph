import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/compensation_pay_run_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Compensation_Pay_Run_NewInputs */
/**
* | output |
* | --- |
* | "New Pay Run" |
*
* @param {Compensation_Pay_Run_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var compensation_pay_run_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Pay Run";
});
export { compensation_pay_run_new as t };

//# sourceMappingURL=compensation_pay_run_new.js.map