import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_name_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Name_TitleInputs */
/**
* | output |
* | --- |
* | "Name" |
*
* @param {Common_Name_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_name_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Name";
});
export { common_name_title as t };

//# sourceMappingURL=common_name_title.js.map