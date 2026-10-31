import { t as __exportAll } from "./rolldown-runtime.js";
import { a as command, n as query } from "./remote.js";
import { t as apiFetch } from "./api.js";
import { error } from "@sveltejs/kit";
import { init_remote_functions } from "@sveltejs/kit/internal/server";
// src/routes/(app)/recruiting/candidate/[candidate_id]/data.remote.ts
var data_remote_exports = /* @__PURE__ */ __exportAll({
	deleteDistributionGuidelinesChild: () => deleteDistributionGuidelinesChild,
	deleteQualificationChild: () => deleteQualificationChild,
	getDistributionGuidelinesChildren: () => getDistributionGuidelinesChildren,
	getItem: () => getItem,
	getQualificationChildren: () => getQualificationChildren,
	transitionItem: () => transitionItem
});
var getItem = /* @__PURE__ */ query("unchecked", async (id) => {
	const response = await apiFetch(`/api/v1/recruiting/candidate/${id}`);
	if (!response.ok) error(response.status, "Failed to load candidate");
	const json = await response.json();
	const item = json.data ?? json;
	try {
		const wf = await apiFetch(`/api/v1/recruiting/candidate/${id}/workflow`);
		if (wf.ok) {
			const wfJson = await wf.json();
			item.workflow_state = wfJson.data ?? wfJson;
		}
	} catch {}
	return item;
});
var transitionItem = /* @__PURE__ */ command("unchecked", async (args) => {
	const response = await apiFetch(`/api/v1/recruiting/candidate/${args.id}/actions/transition`, {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ target_state: args.to })
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
		state: json.data ?? json
	};
});
var getDistributionGuidelinesChildren = /* @__PURE__ */ query("unchecked", async (id) => {
	const response = await apiFetch(`/api/v1/recruiting/distribution-guidelines?parent_id=${id}`);
	if (!response.ok) return [];
	const data = await response.json();
	return Array.isArray(data) ? data : data.data ?? [];
});
var deleteDistributionGuidelinesChild = /* @__PURE__ */ command("unchecked", async (childId) => {
	const response = await apiFetch(`/api/v1/recruiting/distribution-guidelines/${childId}`, { method: "DELETE" });
	if (!response.ok) {
		const body = await response.json().catch(() => null);
		return {
			ok: false,
			message: body?.error ?? body?.message ?? `Request failed (${response.status})`
		};
	}
	return { ok: true };
});
var getQualificationChildren = /* @__PURE__ */ query("unchecked", async (id) => {
	const response = await apiFetch(`/api/v1/recruiting/qualification?parent_id=${id}`);
	if (!response.ok) return [];
	const data = await response.json();
	return Array.isArray(data) ? data : data.data ?? [];
});
var deleteQualificationChild = /* @__PURE__ */ command("unchecked", async (childId) => {
	const response = await apiFetch(`/api/v1/recruiting/qualification/${childId}`, { method: "DELETE" });
	if (!response.ok) {
		const body = await response.json().catch(() => null);
		return {
			ok: false,
			message: body?.error ?? body?.message ?? `Request failed (${response.status})`
		};
	}
	return { ok: true };
});
init_remote_functions(data_remote_exports, "src/routes/(app)/recruiting/candidate/[candidate_id]/data.remote.ts", "nkeois");
for (const [name, fn] of Object.entries(data_remote_exports)) {
	fn.__.id = "nkeois/" + name;
	fn.__.name = name;
}
export { getItem as a, getDistributionGuidelinesChildren as i, deleteDistributionGuidelinesChild as n, getQualificationChildren as o, deleteQualificationChild as r, transitionItem as s, data_remote_exports as t };

//# sourceMappingURL=data78.remote.js.map