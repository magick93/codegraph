import { r as getItem } from "../../../../../../chunks/data26.remote.js";
// src/routes/(app)/common/event-base/[event_base_id]/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.event_base_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map