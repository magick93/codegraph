import { r as getItem } from "../../../../../../chunks/data22.remote.js";
// src/routes/(app)/common/effective-date/[effective_date_id]/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.effective_date_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map