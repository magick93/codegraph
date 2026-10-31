
import { set_building, set_prerendering } from '$app/env/server';
import { set_assets } from '$app/paths/internal/server';
import { set_fix_stack_trace, set_manifest, set_read_implementation, log_response } from '../../../node_modules/@sveltejs/kit/src/runtime/server/internal.js';
import error from './shared/error-template.js';

export const options = {
	app_template_contains_nonce: false,
	csp: {"mode":"auto","directives":{"upgrade-insecure-requests":false,"block-all-mixed-content":false},"reportOnly":{"upgrade-insecure-requests":false,"block-all-mixed-content":false}},
	csrf_check_origin: true,
	csrf_trusted_origins: [],
	embedded: false,
	hash_routing: false,
	hooks: null, // added lazily, via `get_hooks`
	link_header_preload: false,
	paths_origin: undefined,
	service_worker: false,
	service_worker_options: undefined,
	templates: {
		app: ({ head, body, assets, nonce, env }) => "<!doctype html>\n<html lang=\"%lang%\" dir=\"%dir%\">\n\t<head>\n\t\t<meta charset=\"utf-8\" />\n\t\t<link rel=\"icon\" type=\"image/png\" sizes=\"32x32\" href=\"" + assets + "/brand/favicon-32x32.png\" />\n\t\t<link rel=\"icon\" type=\"image/png\" sizes=\"16x16\" href=\"" + assets + "/brand/favicon-16x16.png\" />\n\t\t<link rel=\"apple-touch-icon\" sizes=\"128x128\" href=\"" + assets + "/brand/crewbase-icon-128x128.png\" />\n\t\t<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n\t\t<link rel=\"preconnect\" href=\"https://fonts.googleapis.com\" />\n\t\t<link rel=\"preconnect\" href=\"https://fonts.gstatic.com\" crossorigin />\n\t\t<link href=\"https://fonts.googleapis.com/css2?family=DM+Sans:ital,opsz,wght@0,9..40,300;0,9..40,400;0,9..40,500;0,9..40,600;1,9..40,400&family=Fraunces:ital,opsz,wght@0,9..144,400;0,9..144,500;0,9..144,600;1,9..144,400&display=swap\" rel=\"stylesheet\" />\n\t\t" + head + "\n\t</head>\n\t<body data-sveltekit-preload-data=\"hover\" class=\"min-h-screen antialiased\">\n\t\t<div style=\"display: contents\">" + body + "</div>\n\t</body>\n</html>\n",
		error
	},
	version: "1793443709793",
	version_hash: "o0oy9l"
};

export async function get_hooks() {
	let handle;
	let handleFetch;
	let handleError;
	let init;
	({ handle, handleFetch, handleError, init } = await import("../../../src/hooks.server.ts"));

	let reroute;
	let transport;
	({ reroute, transport } = await import("../../../src/hooks.ts"));

	return {
		handle,
		handleFetch,
		handleError,
		init,
		reroute,
		transport
	};
}

export { set_assets, set_building, set_fix_stack_trace, set_manifest, set_prerendering, set_read_implementation, log_response };
