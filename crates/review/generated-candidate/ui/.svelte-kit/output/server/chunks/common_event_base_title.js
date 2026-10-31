import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_event_base_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Event_Base_TitleInputs */
/**
* | output |
* | --- |
* | "Event Base" |
*
* @param {Common_Event_Base_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_event_base_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Event Base";
});
export { common_event_base_title as t };

//# sourceMappingURL=common_event_base_title.js.map