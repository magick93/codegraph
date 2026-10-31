import { r as getItem } from "../../../../../../chunks/data62.remote.js";
// src/routes/(app)/common/string-type-array/[string_type_array_id]/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.string_type_array_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map