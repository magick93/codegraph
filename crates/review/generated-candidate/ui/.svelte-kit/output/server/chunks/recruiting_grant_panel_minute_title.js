import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/recruiting_grant_panel_minute_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Recruiting_Grant_Panel_Minute_TitleInputs */
/**
* | output |
* | --- |
* | "Grant Panel Minute" |
*
* @param {Recruiting_Grant_Panel_Minute_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var recruiting_grant_panel_minute_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Grant Panel Minute";
});
export { recruiting_grant_panel_minute_title as t };

//# sourceMappingURL=recruiting_grant_panel_minute_title.js.map