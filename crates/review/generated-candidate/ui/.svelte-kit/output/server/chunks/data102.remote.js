import { t as __exportAll } from "./rolldown-runtime.js";
import { a as command, n as query } from "./remote.js";
import { t as apiFetch } from "./api.js";
import { init_remote_functions } from "@sveltejs/kit/internal/server";
// src/routes/(app)/settings/webhooks/[id]/data.remote.ts
var data_remote_exports = /* @__PURE__ */ __exportAll({
	deleteSubscription: () => deleteSubscription,
	getEndpointData: () => getEndpointData,
	rotateSecret: () => rotateSecret,
	toggleActive: () => toggleActive
});
var getEndpointData = /* @__PURE__ */ query("unchecked", async (id) => {
	const base = `/api/v1/webhooks/webhook-endpoints/${id}`;
	const [endpointRes, subsRes, delsRes] = await Promise.all([
		apiFetch(base),
		apiFetch(`${base}/subscriptions`),
		apiFetch(`${base}/deliveries`)
	]);
	if (!endpointRes.ok) throw new Error(`Failed to load webhook endpoint (${endpointRes.status})`);
	const endpoint = await endpointRes.json().then((j) => j.data ?? j);
	const subs = subsRes.ok ? await subsRes.json() : [];
	const deliveries = delsRes.ok ? await delsRes.json() : [];
	return {
		endpoint,
		subscriptions: Array.isArray(subs) ? subs : subs.data ?? [],
		deliveries: Array.isArray(deliveries) ? deliveries : deliveries.data ?? []
	};
});
var toggleActive = /* @__PURE__ */ command("unchecked", async (id, isActive) => {
	const response = await apiFetch(`/api/v1/webhooks/webhook-endpoints/${id}`, {
		method: "PUT",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ is_active: isActive })
	});
	if (!response.ok) {
		const body = await response.json().catch(() => null);
		return {
			ok: false,
			message: body?.error ?? body?.message ?? `Request failed (${response.status})`
		};
	}
	return { ok: true };
});
var rotateSecret = /* @__PURE__ */ command("unchecked", async (id) => {
	const response = await apiFetch(`/api/v1/webhooks/webhook-endpoints/${id}/rotate-secret`, {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({})
	});
	if (!response.ok) {
		const body = await response.json().catch(() => null);
		return {
			ok: false,
			message: body?.error ?? body?.message ?? `Request failed (${response.status})`
		};
	}
	const json = await response.json();
	return {
		ok: true,
		secret: (json.data ?? json).secret
	};
});
var deleteSubscription = /* @__PURE__ */ command("unchecked", async (id) => {
	const response = await apiFetch(`/api/v1/webhooks/webhook-subscriptions/${id}`, { method: "DELETE" });
	if (!response.ok) {
		const body = await response.json().catch(() => null);
		return {
			ok: false,
			message: body?.error ?? body?.message ?? `Request failed (${response.status})`
		};
	}
	return { ok: true };
});
init_remote_functions(data_remote_exports, "src/routes/(app)/settings/webhooks/[id]/data.remote.ts", "j7twhc");
for (const [name, fn] of Object.entries(data_remote_exports)) {
	fn.__.id = "j7twhc/" + name;
	fn.__.name = name;
}
export { toggleActive as a, rotateSecret as i, deleteSubscription as n, getEndpointData as r, data_remote_exports as t };

//# sourceMappingURL=data102.remote.js.map