import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/recruiting_grant_audit_trail_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Grant_Audit_Trail_TitleInputs */
/**
* | output |
* | --- |
* | "Grant Audit Trail" |
*
* @param {Recruiting_Grant_Audit_Trail_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_grant_audit_trail_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Grant Audit Trail";
});
export { recruiting_grant_audit_trail_title as t };

//# sourceMappingURL=recruiting_grant_audit_trail_title.js.map