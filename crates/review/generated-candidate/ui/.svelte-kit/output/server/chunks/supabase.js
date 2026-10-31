import { i as PUBLIC_SUPABASE_URL, r as PUBLIC_SUPABASE_ANON_KEY } from "./public.js";
import { createBrowserClient, createServerClient, isBrowser } from "@supabase/ssr";
// src/lib/supabase.ts
var SUPABASE_URL = PUBLIC_SUPABASE_URL ?? "http://localhost:54321";
var SUPABASE_ANON_KEY = PUBLIC_SUPABASE_ANON_KEY ?? "";
function createSupabaseClient(opts) {
	if (isBrowser()) return createBrowserClient(SUPABASE_URL, SUPABASE_ANON_KEY);
	return createServerClient(SUPABASE_URL, SUPABASE_ANON_KEY, {
		global: { fetch: opts?.fetch },
		cookies: {
			getAll() {
				return opts?.cookies?.getAll() ?? [];
			},
			setAll(cookiesToSet) {
				opts?.cookies?.setAll?.(cookiesToSet);
			}
		}
	});
}
export { createSupabaseClient as t };

//# sourceMappingURL=supabase.js.map