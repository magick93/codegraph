import { t as __exportAll } from "./rolldown-runtime.js";
import { a as command, n as query } from "./remote.js";
import { t as apiFetch } from "./api.js";
import { error } from "@sveltejs/kit";
import { init_remote_functions } from "@sveltejs/kit/internal/server";
// src/routes/(app)/recruiting/grant-audit-trail/[grant_audit_trail_id]/data.remote.ts
var data_remote_exports = /* @__PURE__ */ __exportAll({
	deleteItem: () => deleteItem,
	getItem: () => getItem
});
var getItem = /* @__PURE__ */ query("unchecked", async (id) => {
	const response = await apiFetch(`/api/v1/recruiting/grant-audit-trail/${id}`);
	if (!response.ok) error(response.status, "Failed to load grantaudittrail");
	const json = await response.json();
	return json.data ?? json;
});
var deleteItem = /* @__PURE__ */ command("unchecked", async (args) => {
	const response = await apiFetch(`/api/v1/recruiting/grant-audit-trail/${args.id}`, { method: "DELETE" });
	if (!response.ok) {
		const body = await response.json().catch(() => null);
		return {
			ok: false,
			message: body?.error ?? body?.message ?? `Request failed (${response.status})`
		};
	}
	return { ok: true };
});
init_remote_functions(data_remote_exports, "src/routes/(app)/recruiting/grant-audit-trail/[grant_audit_trail_id]/data.remote.ts", "1jg1qec");
for (const [name, fn] of Object.entries(data_remote_exports)) {
	fn.__.id = "1jg1qec/" + name;
	fn.__.name = name;
}
export { deleteItem as n, getItem as r, data_remote_exports as t };

//# sourceMappingURL=data86.remote.js.map