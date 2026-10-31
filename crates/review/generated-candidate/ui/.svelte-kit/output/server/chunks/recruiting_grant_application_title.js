import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/recruiting_grant_application_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Grant_Application_TitleInputs */
/**
* | output |
* | --- |
* | "Grant Application" |
*
* @param {Recruiting_Grant_Application_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_grant_application_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Grant Application";
});
export { recruiting_grant_application_title as t };

//# sourceMappingURL=recruiting_grant_application_title.js.map