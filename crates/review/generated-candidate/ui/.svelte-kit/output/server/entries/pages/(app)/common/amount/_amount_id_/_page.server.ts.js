import { r as getItem } from "../../../../../../chunks/data2.remote.js";
// src/routes/(app)/common/amount/[amount_id]/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.amount_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map