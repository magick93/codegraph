import { n as getItem } from "../../../../../../../chunks/data15.remote.js";
// src/routes/(app)/common/date/[date_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.date_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map