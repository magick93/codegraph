import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/rsvp_rsvp_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Rsvp_Rsvp_TitleInputs */
/**
* | output |
* | --- |
* | "Rsvp" |
*
* @param {Rsvp_Rsvp_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var rsvp_rsvp_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Rsvp";
});
export { rsvp_rsvp_title as t };

//# sourceMappingURL=rsvp_rsvp_title.js.map