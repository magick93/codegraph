import "./config.js";
import "./server.js";
import "./remote.js";
import { getRequestEvent } from "@sveltejs/kit/internal/server";
// src/lib/server/api.ts
var API_ORIGIN = "";
var API_KEY = "";
function forwardedAuth() {
	try {
		const event = getRequestEvent();
		const header = event.request.headers.get("authorization");
		if (header) return header;
		return event.locals?.apiKey ?? null;
	} catch {
		return null;
	}
}
async function apiFetch(path, init = {}) {
	const headers = new Headers(init.headers);
	if (!headers.has("Authorization")) {
		const forwarded = forwardedAuth();
		if (forwarded) headers.set("Authorization", forwarded);
		else if (API_KEY) headers.set("Authorization", `Bearer ${API_KEY}`);
	}
	if (!headers.has("Accept")) headers.set("Accept", "application/json");
	return fetch(`${API_ORIGIN}${path}`, {
		...init,
		headers
	});
}
export { apiFetch as t };

//# sourceMappingURL=api.js.map