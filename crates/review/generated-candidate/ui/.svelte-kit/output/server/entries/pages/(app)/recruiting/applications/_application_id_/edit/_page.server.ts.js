import { n as getItem } from "../../../../../../../chunks/data75.remote.js";
// src/routes/(app)/recruiting/applications/[application_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.application_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map