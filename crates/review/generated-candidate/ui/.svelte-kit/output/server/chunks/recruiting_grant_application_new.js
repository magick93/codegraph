import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/recruiting_grant_application_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Grant_Application_NewInputs */
/**
* | output |
* | --- |
* | "New Grant Application" |
*
* @param {Recruiting_Grant_Application_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_grant_application_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Grant Application";
});
export { recruiting_grant_application_new as t };

//# sourceMappingURL=recruiting_grant_application_new.js.map