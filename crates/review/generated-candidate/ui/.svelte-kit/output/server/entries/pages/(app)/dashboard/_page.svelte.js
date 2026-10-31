import "../../../../chunks/server2.js";
import "../../../../chunks/ui.js";
// src/routes/(app)/dashboard/+page.svelte
function _page($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { data } = $$props;
		data.organization?.name;
	});
}
export { _page as default };

//# sourceMappingURL=_page.svelte.js.map