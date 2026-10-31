import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/rsvp_rsvp_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Rsvp_Rsvp_NewInputs */
/**
* | output |
* | --- |
* | "New Rsvp" |
*
* @param {Rsvp_Rsvp_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var rsvp_rsvp_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Rsvp";
});
export { rsvp_rsvp_new as t };

//# sourceMappingURL=rsvp_rsvp_new.js.map