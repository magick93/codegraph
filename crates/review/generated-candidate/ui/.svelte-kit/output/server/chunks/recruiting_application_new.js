import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/recruiting_application_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Application_NewInputs */
/**
* | output |
* | --- |
* | "New Application" |
*
* @param {Recruiting_Application_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_application_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Application";
});
export { recruiting_application_new as t };

//# sourceMappingURL=recruiting_application_new.js.map