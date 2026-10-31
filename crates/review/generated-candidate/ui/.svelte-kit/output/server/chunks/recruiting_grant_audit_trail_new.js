import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/recruiting_grant_audit_trail_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Grant_Audit_Trail_NewInputs */
/**
* | output |
* | --- |
* | "New Grant Audit Trail" |
*
* @param {Recruiting_Grant_Audit_Trail_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_grant_audit_trail_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Grant Audit Trail";
});
export { recruiting_grant_audit_trail_new as t };

//# sourceMappingURL=recruiting_grant_audit_trail_new.js.map