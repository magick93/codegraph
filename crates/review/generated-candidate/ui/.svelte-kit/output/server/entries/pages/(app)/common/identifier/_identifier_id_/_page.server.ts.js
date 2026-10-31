import { r as getItem } from "../../../../../../chunks/data38.remote.js";
// src/routes/(app)/common/identifier/[identifier_id]/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.identifier_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map