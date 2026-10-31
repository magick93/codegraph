import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_create_failed.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{ entity: NonNullable<unknown> }} Common_Create_FailedInputs */
/**
* | output |
* | --- |
* | "Failed to create {entity}" |
*
* @param {Common_Create_FailedInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_create_failed = ((i, options = {}) => {
	options.locale ?? getLocale();
	return `Failed to create ${i?.entity}`;
});
// src/lib/paraglide/messages/common_entity_created.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{ entity: NonNullable<unknown> }} Common_Entity_CreatedInputs */
/**
* | output |
* | --- |
* | "{entity} created successfully" |
*
* @param {Common_Entity_CreatedInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_entity_created = ((i, options = {}) => {
	options.locale ?? getLocale();
	return `${i?.entity} created successfully`;
});
export { common_create_failed as n, common_entity_created as t };

//# sourceMappingURL=common_entity_created.js.map