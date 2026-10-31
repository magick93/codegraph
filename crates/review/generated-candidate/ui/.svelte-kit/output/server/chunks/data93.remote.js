import { t as __exportAll } from "./rolldown-runtime.js";
import { n as query } from "./remote.js";
import { t as apiFetch } from "./api.js";
import { error } from "@sveltejs/kit";
import { init_remote_functions } from "@sveltejs/kit/internal/server";
// src/routes/(app)/recruiting/grant-review-note/data.remote.ts
var data_remote_exports = /* @__PURE__ */ __exportAll({ getList: () => getList });
var getList = /* @__PURE__ */ query("unchecked", async (args) => {
	const params = new URLSearchParams({
		page: args.page,
		page_size: args.pageSize
	});
	const response = await apiFetch(`/api/v1/recruiting/grant-review-note?${params.toString()}`);
	if (!response.ok) error(response.status, "Failed to load grantreviewnote list");
	const data = await response.json();
	return Array.isArray(data) ? data : data.data ?? [];
});
init_remote_functions(data_remote_exports, "src/routes/(app)/recruiting/grant-review-note/data.remote.ts", "14gm9p7");
for (const [name, fn] of Object.entries(data_remote_exports)) {
	fn.__.id = "14gm9p7/" + name;
	fn.__.name = name;
}
export { getList as n, data_remote_exports as t };

//# sourceMappingURL=data93.remote.js.map