import { variables } from "../../../../src/env.ts";
import { validate, handle_issues } from '@sveltejs/kit/internal/env';

const issues = {};

export { variables }

export const dynamic_private_env = {};

export const explicit_public_env = {};

export const rendered_env = {};

handle_issues(issues);

export function set_env(env) {
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