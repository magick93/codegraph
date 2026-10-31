import { n as getItem } from "../../../../../../../chunks/data39.remote.js";
// src/routes/(app)/common/identifier/[identifier_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.identifier_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map