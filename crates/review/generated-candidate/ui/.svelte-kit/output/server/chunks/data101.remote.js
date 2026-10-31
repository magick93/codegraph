import { t as __exportAll } from "./rolldown-runtime.js";
import { a as command, n as query } from "./remote.js";
import { t as apiFetch } from "./api.js";
import { init_remote_functions } from "@sveltejs/kit/internal/server";
// src/routes/(app)/settings/webhooks/data.remote.ts
var data_remote_exports = /* @__PURE__ */ __exportAll({
	deleteEndpoint: () => deleteEndpoint,
	getEndpoints: () => getEndpoints
});
var getEndpoints = /* @__PURE__ */ query("unchecked", async () => {
	const response = await apiFetch("/api/v1/webhooks/webhook-endpoints");
	if (!response.ok) return [];
	const data = await response.json();
	return Array.isArray(data) ? data : data.data ?? [];
});
var deleteEndpoint = /* @__PURE__ */ command("unchecked", async (id) => {
	const response = await apiFetch(`/api/v1/webhooks/webhook-endpoints/${id}`, { method: "DELETE" });
	if (!response.ok) {
		const body = await response.json().catch(() => null);
		return {
			ok: false,
			message: body?.error ?? body?.message ?? `Request failed (${response.status})`
		};
	}
	return { ok: true };
});
init_remote_functions(data_remote_exports, "src/routes/(app)/settings/webhooks/data.remote.ts", "19rk310");
for (const [name, fn] of Object.entries(data_remote_exports)) {
	fn.__.id = "19rk310/" + name;
	fn.__.name = name;
}
export { deleteEndpoint as n, getEndpoints as r, data_remote_exports as t };

//# sourceMappingURL=data101.remote.js.map