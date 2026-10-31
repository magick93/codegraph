import "../../../../chunks/server2.js";
import "../../../../chunks/ui.js";
import { t as createSupabaseClient } from "../../../../chunks/supabase.js";
// src/routes/(auth)/signup/+page.svelte
function _page($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		createSupabaseClient();
	});
}
export { _page as default };

//# sourceMappingURL=_page.svelte.js.map