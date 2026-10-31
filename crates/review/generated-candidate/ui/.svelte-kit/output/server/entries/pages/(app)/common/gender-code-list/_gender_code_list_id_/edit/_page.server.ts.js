import { n as getItem } from "../../../../../../../chunks/data35.remote.js";
// src/routes/(app)/common/gender-code-list/[gender_code_list_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.gender_code_list_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map