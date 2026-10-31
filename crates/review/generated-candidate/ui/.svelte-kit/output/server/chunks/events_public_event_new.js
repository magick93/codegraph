import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/events_public_event_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Events_Public_Event_NewInputs */
/**
* | output |
* | --- |
* | "New Public Event" |
*
* @param {Events_Public_Event_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var events_public_event_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Public Event";
});
export { events_public_event_new as t };

//# sourceMappingURL=events_public_event_new.js.map