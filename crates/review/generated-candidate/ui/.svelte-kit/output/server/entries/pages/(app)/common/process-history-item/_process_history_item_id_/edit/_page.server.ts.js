import { n as getItem } from "../../../../../../../chunks/data59.remote.js";
// src/routes/(app)/common/process-history-item/[process_history_item_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.process_history_item_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map