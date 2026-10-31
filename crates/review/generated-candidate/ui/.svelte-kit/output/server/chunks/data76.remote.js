import { t as __exportAll } from "./rolldown-runtime.js";
import { a as command } from "./remote.js";
import { t as apiFetch } from "./api.js";
import { init_remote_functions } from "@sveltejs/kit/internal/server";
// src/routes/(app)/recruiting/applications/new/data.remote.ts
var data_remote_exports = /* @__PURE__ */ __exportAll({ createItem: () => createItem });
var createItem = /* @__PURE__ */ command("unchecked", async (args) => {
	const response = await apiFetch(`/api/v1/recruiting/applications`, {
		method: "POST",
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
	const json = await response.json();
	return {
		ok: true,
		id: (json.data ?? json).id
	};
});
init_remote_functions(data_remote_exports, "src/routes/(app)/recruiting/applications/new/data.remote.ts", "1estt8l");
for (const [name, fn] of Object.entries(data_remote_exports)) {
	fn.__.id = "1estt8l/" + name;
	fn.__.name = name;
}
export { data_remote_exports as n, createItem as t };

//# sourceMappingURL=data76.remote.js.map