import { n as getList } from "../../../../../chunks/data93.remote.js";
// src/routes/(app)/recruiting/grant-review-note/+page.server.ts
var load = async ({ url }) => {
	const page = Number(url.searchParams.get("page") ?? "0");
	const pageSize = Number(url.searchParams.get("page_size") ?? "20");
	return {
		items: await getList({
			page: String(page),
			pageSize: String(pageSize)
		}),
		page,
		pageSize
	};
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map