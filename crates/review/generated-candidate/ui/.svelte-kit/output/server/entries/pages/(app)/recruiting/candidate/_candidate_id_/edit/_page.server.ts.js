import { n as getItem } from "../../../../../../../chunks/data79.remote.js";
// src/routes/(app)/recruiting/candidate/[candidate_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.candidate_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map