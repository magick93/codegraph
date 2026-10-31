import { r as getItem } from "../../../../../../chunks/data42.remote.js";
// src/routes/(app)/common/name/[name_id]/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.name_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map