import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/recruiting_application_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Application_TitleInputs */
/**
* | output |
* | --- |
* | "Application" |
*
* @param {Recruiting_Application_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_application_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Application";
});
export { recruiting_application_title as t };

//# sourceMappingURL=recruiting_application_title.js.map