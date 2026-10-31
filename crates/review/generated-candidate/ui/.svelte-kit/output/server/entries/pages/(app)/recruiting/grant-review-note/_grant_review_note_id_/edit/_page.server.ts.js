import { n as getItem } from "../../../../../../../chunks/data95.remote.js";
// src/routes/(app)/recruiting/grant-review-note/[grant_review_note_id]/edit/+page.server.ts
var load = async ({ params }) => {
	return { item: await getItem(params.grant_review_note_id) };
};
export { load };

//# sourceMappingURL=_page.server.ts.js.map