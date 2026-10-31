import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/recruiting_grant_review_note_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Grant_Review_Note_NewInputs */
/**
* | output |
* | --- |
* | "New Grant Review Note" |
*
* @param {Recruiting_Grant_Review_Note_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_grant_review_note_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Grant Review Note";
});
export { recruiting_grant_review_note_new as t };

//# sourceMappingURL=recruiting_grant_review_note_new.js.map