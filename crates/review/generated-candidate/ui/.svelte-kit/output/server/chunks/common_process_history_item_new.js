import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_process_history_item_new.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Process_History_Item_NewInputs */
/**
* | output |
* | --- |
* | "New Process History Item" |
*
* @param {Common_Process_History_Item_NewInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_process_history_item_new = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "New Process History Item";
});
export { common_process_history_item_new as t };

//# sourceMappingURL=common_process_history_item_new.js.map