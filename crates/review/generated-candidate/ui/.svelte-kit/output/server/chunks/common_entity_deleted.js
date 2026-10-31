import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_deleting.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_DeletingInputs */
/**
* | output |
* | --- |
* | "Deleting…" |
*
* @param {Common_DeletingInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_deleting = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Deleting…";
});
// src/lib/paraglide/messages/common_entity_deleted.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{ entity: NonNullable<unknown> }} Common_Entity_DeletedInputs */
/**
* | output |
* | --- |
* | "{entity} deleted successfully" |
*
* @param {Common_Entity_DeletedInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_entity_deleted = ((i, options = {}) => {
	options.locale ?? getLocale();
	return `${i?.entity} deleted successfully`;
});
export { common_deleting as n, common_entity_deleted as t };

//# sourceMappingURL=common_entity_deleted.js.map