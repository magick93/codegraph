import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_process_history_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Process_History_NewInputs */
/**
* | output |
* | --- |
* | "New Process History" |
*
* @param {Common_Process_History_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_process_history_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Process History";
});
export { common_process_history_new as t };

//# sourceMappingURL=common_process_history_new.js.map