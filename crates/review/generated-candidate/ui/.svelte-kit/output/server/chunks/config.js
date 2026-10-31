import { defineEnvVars } from "@sveltejs/kit/env";
import { handle_issues, validate } from "@sveltejs/kit/internal/env";
// src/env.ts
var variables = defineEnvVars({
	PUBLIC_API_BASE_URL: {
		public: true,
		schema: (value) => value
	},
	PUBLIC_API_URL: {
		public: true,
		schema: (value) => value
	},
	PUBLIC_SUPABASE_ANON_KEY: {
		public: true,
		schema: (value) => value
	},
	PUBLIC_SUPABASE_URL: {
		public: true,
		schema: (value) => value
	}
});
// .svelte-kit/generated/build/env/config.js
var issues = {};
var dynamic_private_env = {};
var explicit_public_env = {};
var rendered_env = {};
handle_issues(issues);
function set_env(env) {
	const issues = {};
	const PUBLIC_API_BASE_URL = validate(variables, env.PUBLIC_API_BASE_URL, "PUBLIC_API_BASE_URL", issues);
	explicit_public_env.PUBLIC_API_BASE_URL = PUBLIC_API_BASE_URL;
	rendered_env.PUBLIC_API_BASE_URL = PUBLIC_API_BASE_URL;
	const PUBLIC_API_URL = validate(variables, env.PUBLIC_API_URL, "PUBLIC_API_URL", issues);
	explicit_public_env.PUBLIC_API_URL = PUBLIC_API_URL;
	rendered_env.PUBLIC_API_URL = PUBLIC_API_URL;
	const PUBLIC_SUPABASE_ANON_KEY = validate(variables, env.PUBLIC_SUPABASE_ANON_KEY, "PUBLIC_SUPABASE_ANON_KEY", issues);
	explicit_public_env.PUBLIC_SUPABASE_ANON_KEY = PUBLIC_SUPABASE_ANON_KEY;
	rendered_env.PUBLIC_SUPABASE_ANON_KEY = PUBLIC_SUPABASE_ANON_KEY;
	const PUBLIC_SUPABASE_URL = validate(variables, env.PUBLIC_SUPABASE_URL, "PUBLIC_SUPABASE_URL", issues);
	explicit_public_env.PUBLIC_SUPABASE_URL = PUBLIC_SUPABASE_URL;
	rendered_env.PUBLIC_SUPABASE_URL = PUBLIC_SUPABASE_URL;
	handle_issues(issues);
}
export { variables as a, set_env as i, explicit_public_env as n, rendered_env as r, dynamic_private_env as t };

//# sourceMappingURL=config.js.map