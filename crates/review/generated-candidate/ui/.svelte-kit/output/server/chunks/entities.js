import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_confirm_delete_description.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Confirm_Delete_DescriptionInputs */
/**
* | output |
* | --- |
* | "Are you sure? This action cannot be undone." |
*
* @param {Common_Confirm_Delete_DescriptionInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_confirm_delete_description = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Are you sure? This action cannot be undone.";
});
// src/lib/paraglide/messages/common_confirm_delete_title.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{ entity: NonNullable<unknown> }} Common_Confirm_Delete_TitleInputs */
/**
* | output |
* | --- |
* | "Delete {entity}" |
*
* @param {Common_Confirm_Delete_TitleInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_confirm_delete_title = ((i, options = {}) => {
	options.locale ?? getLocale();
	return `Delete ${i?.entity}`;
});
// src/lib/paraglide/messages/common_created_at.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Created_AtInputs */
/**
* | output |
* | --- |
* | "Created At" |
*
* @param {Common_Created_AtInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_created_at = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Created At";
});
// src/lib/paraglide/messages/common_delete.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_DeleteInputs */
/**
* | output |
* | --- |
* | "Delete" |
*
* @param {Common_DeleteInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_delete = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Delete";
});
// src/lib/paraglide/messages/common_delete_failed.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{ error: NonNullable<unknown> }} Common_Delete_FailedInputs */
/**
* | output |
* | --- |
* | "Delete failed: {error}" |
*
* @param {Common_Delete_FailedInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_delete_failed = ((i, options = {}) => {
	options.locale ?? getLocale();
	return `Delete failed: ${i?.error}`;
});
// src/lib/paraglide/messages/common_updated_at.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_Updated_AtInputs */
/**
* | output |
* | --- |
* | "Updated" |
*
* @param {Common_Updated_AtInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_updated_at = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Updated";
});
// src/lib/components/ui/alert-dialog/index.js
function Action(_props) {}
function Cancel(_props) {}
function Content(_props) {}
function Description(_props) {}
function Footer(_props) {}
function Header(_props) {}
function Root(_props) {}
function Title(_props) {}
// node_modules/@crewbase/entities/index.ts
function TimeAgo(_props) {}
export { Description as a, Root as c, common_delete_failed as d, common_delete as f, common_confirm_delete_description as h, Content as i, Title as l, common_confirm_delete_title as m, Action as n, Footer as o, common_created_at as p, Cancel as r, Header as s, TimeAgo as t, common_updated_at as u };

//# sourceMappingURL=entities.js.map