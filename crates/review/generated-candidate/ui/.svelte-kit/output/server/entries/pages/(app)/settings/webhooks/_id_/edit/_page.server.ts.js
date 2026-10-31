import { r as getEndpoint } from "../../../../../../../chunks/data103.remote.js";
// src/routes/(app)/settings/webhooks/[id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { endpoint: await getEndpoint(params.id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map