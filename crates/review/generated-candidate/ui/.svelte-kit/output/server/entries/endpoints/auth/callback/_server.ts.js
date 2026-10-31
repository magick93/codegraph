import { t as createSupabaseClient } from "../../../../chunks/supabase.js";
import { redirect } from "@sveltejs/kit";
// src/routes/auth/callback/+server.ts
var GET = async ({ url, fetch, cookies }) => {
	const code = url.searchParams.get("code");
	if (code) await createSupabaseClient({
		fetch,
		cookies: {
			getAll() {
				return cookies.getAll().map(({ name, value }) => ({
					name,
					value
				}));
			},
			setAll(cookiesToSet) {
				for (const { name, value, options } of cookiesToSet) cookies.set(name, value, {
					path: "/",
					...options
				});
			}
		}
	}).auth.exchangeCodeForSession(code);
	const next = url.searchParams.get("next") ?? "/dashboard";
	redirect(303, next);
};
export { GET };

//# sourceMappingURL=_server.ts.js.map