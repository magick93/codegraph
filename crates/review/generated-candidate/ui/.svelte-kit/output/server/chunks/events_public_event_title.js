import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/events_public_event_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Events_Public_Event_TitleInputs */
/**
* | output |
* | --- |
* | "Public Event" |
*
* @param {Events_Public_Event_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var events_public_event_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Public Event";
});
export { events_public_event_title as t };

//# sourceMappingURL=events_public_event_title.js.map