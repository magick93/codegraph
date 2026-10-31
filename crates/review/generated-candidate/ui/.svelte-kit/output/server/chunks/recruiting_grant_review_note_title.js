import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/recruiting_grant_review_note_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Grant_Review_Note_TitleInputs */
/**
* | output |
* | --- |
* | "Grant Review Note" |
*
* @param {Recruiting_Grant_Review_Note_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_grant_review_note_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Grant Review Note";
});
export { recruiting_grant_review_note_title as t };

//# sourceMappingURL=recruiting_grant_review_note_title.js.map