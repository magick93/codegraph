import { n as getItem } from "../../../../../../../chunks/data55.remote.js";
// src/routes/(app)/common/process-history/[process_history_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.process_history_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map