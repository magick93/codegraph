import { n as getItem } from "../../../../../../../chunks/data7.remote.js";
// src/routes/(app)/common/code/[code_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.code_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map