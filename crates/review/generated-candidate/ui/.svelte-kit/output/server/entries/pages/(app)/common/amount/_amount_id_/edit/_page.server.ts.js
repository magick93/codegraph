import { n as getItem } from "../../../../../../../chunks/data3.remote.js";
// src/routes/(app)/common/amount/[amount_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.amount_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map