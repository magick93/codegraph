import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_back_to_detail.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Back_To_DetailInputs */
/**
* | output |
* | --- |
* | "Back to detail" |
*
* @param {Common_Back_To_DetailInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_back_to_detail = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Back to detail";
});
// src/lib/paraglide/messages/common_entity_updated.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{ entity: NonNullable<unknown> }} Common_Entity_UpdatedInputs */
/**
* | output |
* | --- |
* | "{entity} updated successfully" |
*
* @param {Common_Entity_UpdatedInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_entity_updated = ((i, options = {}) => {
	options.locale ?? getLocale();
	return `${i?.entity} updated successfully`;
});
// src/lib/paraglide/messages/common_update_failed.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{ entity: NonNullable<unknown> }} Common_Update_FailedInputs */
/**
* | output |
* | --- |
* | "Failed to update {entity}" |
*
* @param {Common_Update_FailedInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_update_failed = ((i, options = {}) => {
	options.locale ?? getLocale();
	return `Failed to update ${i?.entity}`;
});
export { common_entity_updated as n, common_back_to_detail as r, common_update_failed as t };

//# sourceMappingURL=common_update_failed.js.map