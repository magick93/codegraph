import { t as __exportAll } from "./rolldown-runtime.js";
import { a as command, n as query } from "./remote.js";
import { t as apiFetch } from "./api.js";
import { error } from "@sveltejs/kit";
import { init_remote_functions } from "@sveltejs/kit/internal/server";
// src/routes/(app)/common/currency-code-list/[currency_code_list_id]/data.remote.ts
var data_remote_exports = /* @__PURE__ */ __exportAll({
	deleteItem: () => deleteItem,
	getItem: () => getItem
});
var getItem = /* @__PURE__ */ query("unchecked", async (id) => {
	const response = await apiFetch(`/api/v1/common/currency-code-list/${id}`);
	if (!response.ok) error(response.status, "Failed to load currencycodelist");
	const json = await response.json();
	return json.data ?? json;
});
var deleteItem = /* @__PURE__ */ command("unchecked", async (args) => {
	const response = await apiFetch(`/api/v1/common/currency-code-list/${args.id}`, { method: "DELETE" });
	if (!response.ok) {
		const body = await response.json().catch(() => null);
		return {
			ok: false,
			message: body?.error ?? body?.message ?? `Request failed (${response.status})`
		};
	}
	return { ok: true };
});
init_remote_functions(data_remote_exports, "src/routes/(app)/common/currency-code-list/[currency_code_list_id]/data.remote.ts", "1d25e5j");
for (const [name, fn] of Object.entries(data_remote_exports)) {
	fn.__.id = "1d25e5j/" + name;
	fn.__.name = name;
}
export { deleteItem as n, getItem as r, data_remote_exports as t };

//# sourceMappingURL=data10.remote.js.map