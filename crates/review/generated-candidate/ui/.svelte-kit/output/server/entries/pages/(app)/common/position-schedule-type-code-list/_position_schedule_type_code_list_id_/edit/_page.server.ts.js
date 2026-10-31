import { n as getItem } from "../../../../../../../chunks/data51.remote.js";
// src/routes/(app)/common/position-schedule-type-code-list/[position_schedule_type_code_list_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.position_schedule_type_code_list_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map