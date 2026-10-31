import { r as getItem } from "../../../../../../chunks/data30.remote.js";
// src/routes/(app)/common/formatted-date-time/[formatted_date_time_id]/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.formatted_date_time_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map