import { r as getItem } from "../../../../../../chunks/data34.remote.js";
// src/routes/(app)/common/gender-code-list/[gender_code_list_id]/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.gender_code_list_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map