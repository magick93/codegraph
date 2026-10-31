import { o as head } from "../../../chunks/server2.js";
import "../../../chunks/index-server2.js";
// src/routes/version/+page.svelte
function _page($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		head("f0swhj", $$renderer, ($$renderer) => {
			$$renderer.title(($$renderer) => {
				$$renderer.push(`<title>Version Information</title>`);
			});
		});
		$$renderer.push(`<div class="container mx-auto max-w-2xl p-6 space-y-6"><h1 class="text-2xl font-bold">Version Information</h1> `);
		$$renderer.push(`<!--[0--><p class="text-muted-foreground">Loading...</p>`);
		$$renderer.push(`<!--]--></div>`);
	});
}
export { _page as default };

//# sourceMappingURL=_page.svelte.js.map