import { r as getEndpoints } from "../../../../../chunks/data101.remote.js";
// src/routes/(app)/settings/webhooks/+page.server.ts
var load = async () => {
	return { endpoints: await getEndpoints() };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map