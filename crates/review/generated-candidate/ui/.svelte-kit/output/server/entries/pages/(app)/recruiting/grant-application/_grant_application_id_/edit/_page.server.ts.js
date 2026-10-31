import { n as getItem } from "../../../../../../../chunks/data83.remote.js";
// src/routes/(app)/recruiting/grant-application/[grant_application_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.grant_application_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map