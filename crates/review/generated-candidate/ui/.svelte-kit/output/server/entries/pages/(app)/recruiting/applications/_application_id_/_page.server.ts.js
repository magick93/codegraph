import { r as getItem } from "../../../../../../chunks/data74.remote.js";
// src/routes/(app)/recruiting/applications/[application_id]/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.application_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map