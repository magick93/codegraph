import { _ as escape_html } from "../../chunks/server2.js";
import { t as page } from "../../chunks/state.js";
import "../../chunks/button.js";
// src/routes/+error.svelte
function _error($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		$$renderer.push(`<div class="flex min-h-[50vh] flex-col items-center justify-center text-center"><h1 class="text-6xl font-bold text-gray-300">${escape_html(page.status)}</h1> <p class="mt-4 text-lg text-gray-600">`);
		if (page.status === 404) $$renderer.push(`<!--[0-->The page you're looking for doesn't exist.`);
		else if (page.status === 500) $$renderer.push(`<!--[1-->Something went wrong on our end.`);
		else $$renderer.push(`<!--[-1-->${escape_html(page.error?.message ?? "An unexpected error occurred.")}`);
		$$renderer.push(`<!--]--></p> <div class="mt-6 flex gap-3">`);
		$$renderer.push(`<!----> `);
		$$renderer.push(`<!----></div></div>`);
	});
}
export { _error as default };

//# sourceMappingURL=_error.svelte.js.map