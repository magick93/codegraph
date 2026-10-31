import { n as getItem } from "../../../../../../../chunks/data43.remote.js";
// src/routes/(app)/common/name/[name_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.name_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map