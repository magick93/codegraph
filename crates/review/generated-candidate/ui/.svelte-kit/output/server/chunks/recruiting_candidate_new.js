import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/recruiting_candidate_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Candidate_NewInputs */
/**
* | output |
* | --- |
* | "New Candidate" |
*
* @param {Recruiting_Candidate_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_candidate_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Candidate";
});
export { recruiting_candidate_new as t };

//# sourceMappingURL=recruiting_candidate_new.js.map