import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_process_history_item_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Process_History_Item_TitleInputs */
/**
* | output |
* | --- |
* | "Process History Item" |
*
* @param {Common_Process_History_Item_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_process_history_item_title = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Process History Item";
});
export { common_process_history_item_title as t };

//# sourceMappingURL=common_process_history_item_title.js.map