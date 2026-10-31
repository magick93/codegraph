import { r as getItem } from "../../../../../../chunks/data82.remote.js";
// src/routes/(app)/recruiting/grant-application/[grant_application_id]/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.grant_application_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map