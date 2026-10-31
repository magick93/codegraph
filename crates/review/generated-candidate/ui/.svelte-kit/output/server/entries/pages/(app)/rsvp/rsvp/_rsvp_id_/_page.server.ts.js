import { r as getItem } from "../../../../../../chunks/data98.remote.js";
// src/routes/(app)/rsvp/rsvp/[rsvp_id]/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.rsvp_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map