import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/recruiting_candidate_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Candidate_TitleInputs */
/**
* | output |
* | --- |
* | "Candidate" |
*
* @param {Recruiting_Candidate_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_candidate_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Candidate";
});
export { recruiting_candidate_title as t };

//# sourceMappingURL=recruiting_candidate_title.js.map