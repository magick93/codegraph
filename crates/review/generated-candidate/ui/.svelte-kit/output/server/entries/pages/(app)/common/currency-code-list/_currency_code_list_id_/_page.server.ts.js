import { r as getItem } from "../../../../../../chunks/data10.remote.js";
// src/routes/(app)/common/currency-code-list/[currency_code_list_id]/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.currency_code_list_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map