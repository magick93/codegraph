import { a as getItem, i as getDistributionGuidelinesChildren, o as getQualificationChildren } from "../../../../../../chunks/data78.remote.js";
// src/routes/(app)/recruiting/candidate/[candidate_id]/+page.server.ts
var load = async ({ params }) => {
	const item = await getItem(params.candidate_id);
	return {
		item,
		children: {
			distribution_guidelines: await getDistributionGuidelinesChildren(item.id),
			qualification: await getQualificationChildren(item.id)
		}
	};
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map