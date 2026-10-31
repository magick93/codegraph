import { n as getItem } from "../../../../../../../chunks/data71.remote.js";
// src/routes/(app)/events/public-event/[public_event_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.public_event_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map