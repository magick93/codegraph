import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_event_base_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Event_Base_NewInputs */
/**
* | output |
* | --- |
* | "New Event Base" |
*
* @param {Common_Event_Base_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_event_base_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Event Base";
});
export { common_event_base_new as t };

//# sourceMappingURL=common_event_base_new.js.map