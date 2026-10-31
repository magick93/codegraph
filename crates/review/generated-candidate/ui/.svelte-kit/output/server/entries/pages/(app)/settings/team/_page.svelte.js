import "../../../../../chunks/server2.js";
import "../../../../../chunks/ui.js";
import { t as createSupabaseClient } from "../../../../../chunks/supabase.js";
// src/routes/(app)/settings/team/+page.svelte
function _page($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { data } = $$props;
		createSupabaseClient();
		data.organization?.id, data.organization?.name;
	});
}
export { _page as default };

//# sourceMappingURL=_page.svelte.js.map