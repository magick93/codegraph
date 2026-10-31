import { t as __exportAll } from "./rolldown-runtime.js";
import { a as command, n as query } from "./remote.js";
import { t as apiFetch } from "./api.js";
import { init_remote_functions } from "@sveltejs/kit/internal/server";
// src/routes/(app)/settings/webhooks/[id]/edit/data.remote.ts
var data_remote_exports = /* @__PURE__ */ __exportAll({
	createEndpoint: () => createEndpoint,
	getEndpoint: () => getEndpoint,
	updateEndpoint: () => updateEndpoint
});
var getEndpoint = /* @__PURE__ */ query("unchecked", async (id) => {
	const response = await apiFetch(`/api/v1/webhooks/webhook-endpoints/${id}`);
	if (!response.ok) throw new Error(`Failed to load webhook endpoint (${response.status})`);
	const json = await response.json();
	return json.data ?? json;
});
var createEndpoint = /* @__PURE__ */ command("unchecked", async (body) => {
	const response = await apiFetch("/api/v1/webhooks/webhook-endpoints", {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(body)
	});
	if (!response.ok) {
		const res = await response.json().catch(() => null);
		return {
			ok: false,
			message: res?.error ?? res?.message ?? `Request failed (${response.status})`
		};
	}
	const json = await response.json();
	return {
		ok: true,
		id: (json.data ?? json).id
	};
});
var updateEndpoint = /* @__PURE__ */ command("unchecked", async (id, body) => {
	const response = await apiFetch(`/api/v1/webhooks/webhook-endpoints/${id}`, {
		method: "PUT",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(body)
	});
	if (!response.ok) {
		const res = await response.json().catch(() => null);
		return {
			ok: false,
			message: res?.error ?? res?.message ?? `Request failed (${response.status})`
		};
	}
	const json = await response.json();
	return {
		ok: true,
		id: (json.data ?? json).id
	};
});
init_remote_functions(data_remote_exports, "src/routes/(app)/settings/webhooks/[id]/edit/data.remote.ts", "1sbg7b7");
for (const [name, fn] of Object.entries(data_remote_exports)) {
	fn.__.id = "1sbg7b7/" + name;
	fn.__.name = name;
}
export { updateEndpoint as i, data_remote_exports as n, getEndpoint as r, createEndpoint as t };

//# sourceMappingURL=data103.remote.js.map