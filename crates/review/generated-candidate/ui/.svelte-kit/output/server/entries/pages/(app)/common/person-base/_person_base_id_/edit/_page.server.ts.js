import { n as getItem } from "../../../../../../../chunks/data47.remote.js";
// src/routes/(app)/common/person-base/[person_base_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.person_base_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map