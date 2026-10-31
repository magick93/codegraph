import { r as getItem } from "../../../../../../chunks/data6.remote.js";
// src/routes/(app)/common/code/[code_id]/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.code_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map