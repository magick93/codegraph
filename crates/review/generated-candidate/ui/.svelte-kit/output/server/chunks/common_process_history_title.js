import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_process_history_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Process_History_TitleInputs */
/**
* | output |
* | --- |
* | "Process History" |
*
* @param {Common_Process_History_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_process_history_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Process History";
});
export { common_process_history_title as t };

//# sourceMappingURL=common_process_history_title.js.map