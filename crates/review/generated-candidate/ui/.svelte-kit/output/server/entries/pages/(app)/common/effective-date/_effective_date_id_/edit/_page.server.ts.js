import { n as getItem } from "../../../../../../../chunks/data23.remote.js";
// src/routes/(app)/common/effective-date/[effective_date_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.effective_date_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map