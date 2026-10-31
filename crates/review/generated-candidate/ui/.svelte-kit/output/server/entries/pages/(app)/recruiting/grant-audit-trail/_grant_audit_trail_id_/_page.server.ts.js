import { r as getItem } from "../../../../../../chunks/data86.remote.js";
// src/routes/(app)/recruiting/grant-audit-trail/[grant_audit_trail_id]/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.grant_audit_trail_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map