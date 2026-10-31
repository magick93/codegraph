import { n as getItem } from "../../../../../../../chunks/data19.remote.js";
// src/routes/(app)/common/distribution-base/[distribution_base_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.distribution_base_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map