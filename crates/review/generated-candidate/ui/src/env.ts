import { defineEnvVars } from '@sveltejs/kit/env';

export const variables = defineEnvVars({
	PUBLIC_API_BASE_URL: {
		public: true,
		schema: (value) => value
	},
	PUBLIC_API_KEY: {
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
