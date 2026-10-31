import { n as getItem } from "../../../../../../../chunks/data67.remote.js";
// src/routes/(app)/compensation/pay-runs/[pay_run_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.pay_run_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map