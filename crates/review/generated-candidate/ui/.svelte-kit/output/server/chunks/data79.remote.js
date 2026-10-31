import { t as __exportAll } from "./rolldown-runtime.js";
import { a as command, n as query } from "./remote.js";
import { t as apiFetch } from "./api.js";
import { error } from "@sveltejs/kit";
import { init_remote_functions } from "@sveltejs/kit/internal/server";
// src/routes/(app)/recruiting/candidate/[candidate_id]/edit/data.remote.ts
var data_remote_exports = /* @__PURE__ */ __exportAll({
	getItem: () => getItem,
	updateItem: () => updateItem
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
var updateItem = /* @__PURE__ */ command("unchecked", async (args) => {
	const response = await apiFetch(`/api/v1/recruiting/candidate/${args.id}`, {
		method: "PUT",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(args.data)
	});
	if (!response.ok) {
		const body = await response.json().catch(() => null);
		const fieldErrors = {};
		const details = body?.error?.details;
		if (Array.isArray(details)) {
			for (const d of details) if (d && typeof d.field === "string" && typeof d.message === "string") fieldErrors[d.field] = d.message;
		}
		return {
			ok: false,
			message: body?.error?.message ?? body?.message ?? `Request failed (${response.status})`,
			fieldErrors
		};
	}
	return { ok: true };
});
init_remote_functions(data_remote_exports, "src/routes/(app)/recruiting/candidate/[candidate_id]/edit/data.remote.ts", "omnmd3");
for (const [name, fn] of Object.entries(data_remote_exports)) {
	fn.__.id = "omnmd3/" + name;
	fn.__.name = name;
}
export { getItem as n, updateItem as r, data_remote_exports as t };

//# sourceMappingURL=data79.remote.js.map