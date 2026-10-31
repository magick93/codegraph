import { t as PUBLIC_API_BASE_URL } from "../../../chunks/public.js";
import { json } from "@sveltejs/kit";
// src/routes/version/+server.ts
var GET = async () => {
	const ui = {
		version: "0.1.0",
		build_time: "2026-10-31T10:48:22.751Z"
	};
	let api = null;
	let error = null;
	try {
		const resp = await fetch(`${PUBLIC_API_BASE_URL || "http://localhost:3000"}/version`);
		if (resp.ok) api = await resp.json();
		else error = `API returned HTTP ${resp.status}`;
	} catch {
		error = "API unreachable";
	}
	return json({
		ui,
		api,
		error
	});
};
export { GET };

//# sourceMappingURL=_server.ts.js.map