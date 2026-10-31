import { r as getItem } from "../../../../../../chunks/data58.remote.js";
// src/routes/(app)/common/process-history-item/[process_history_item_id]/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.process_history_item_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map