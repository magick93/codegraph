import { t as createSupabaseClient } from "../../../chunks/supabase.js";
import { redirect } from "@sveltejs/kit";
// src/routes/(app)/+layout.server.ts
var load = async ({ fetch, cookies }) => {
	const supabase = createSupabaseClient({
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
	});
	const { data: { session } } = await supabase.auth.getSession();
	if (!session) redirect(303, "/login");
	let accounts = null;
	try {
		const { data, error } = await supabase.rpc("get_accounts");
		if (error) console.error("[layout] get_accounts failed:", error.message ?? String(error));
		else accounts = data;
	} catch (e) {
		console.error("[layout] get_accounts threw:", e);
	}
	const org = accounts?.find((a) => !a.personal_account);
	return {
		session,
		organization: org ? {
			id: org.account_id,
			name: org.name,
			slug: org.slug,
			role: org.account_role
		} : null
	};
};
export { load };

//# sourceMappingURL=_layout.server.ts.js.map