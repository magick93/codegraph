import { r as getEndpointData } from "../../../../../../chunks/data102.remote.js";
// src/routes/(app)/settings/webhooks/[id]/+page.server.ts
var load = async ({ params }) => {
	const { endpoint, subscriptions, deliveries } = await getEndpointData(params.id);
	return {
		endpoint,
		subscriptions,
		deliveries
	};
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map