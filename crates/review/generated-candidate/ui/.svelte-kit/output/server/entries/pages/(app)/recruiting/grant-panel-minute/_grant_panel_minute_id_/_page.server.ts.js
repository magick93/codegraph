import { r as getItem } from "../../../../../../chunks/data90.remote.js";
// src/routes/(app)/recruiting/grant-panel-minute/[grant_panel_minute_id]/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.grant_panel_minute_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map