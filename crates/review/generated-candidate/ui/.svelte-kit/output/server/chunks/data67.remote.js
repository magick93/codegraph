import { t as __exportAll } from "./rolldown-runtime.js";
import { a as command, n as query } from "./remote.js";
import { t as apiFetch } from "./api.js";
import { error } from "@sveltejs/kit";
import { init_remote_functions } from "@sveltejs/kit/internal/server";
// src/routes/(app)/compensation/pay-runs/[pay_run_id]/edit/data.remote.ts
var data_remote_exports = /* @__PURE__ */ __exportAll({
	getItem: () => getItem,
	updateItem: () => updateItem
});
var getItem = /* @__PURE__ */ query("unchecked", async (id) => {
	const response = await apiFetch(`/api/v1/compensation/pay-runs/${id}`);
	if (!response.ok) error(response.status, "Failed to load payrun");
	const json = await response.json();
	return json.data ?? json;
});
var updateItem = /* @__PURE__ */ command("unchecked", async (args) => {
	const response = await apiFetch(`/api/v1/compensation/pay-runs/${args.id}`, {
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
init_remote_functions(data_remote_exports, "src/routes/(app)/compensation/pay-runs/[pay_run_id]/edit/data.remote.ts", "1rxjx4c");
for (const [name, fn] of Object.entries(data_remote_exports)) {
	fn.__.id = "1rxjx4c/" + name;
	fn.__.name = name;
}
export { getItem as n, updateItem as r, data_remote_exports as t };

//# sourceMappingURL=data67.remote.js.map