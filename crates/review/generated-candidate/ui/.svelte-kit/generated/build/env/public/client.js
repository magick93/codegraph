import { payload } from "../../../../../node_modules/@sveltejs/kit/src/runtime/client/payload.js";
const env = payload.env;

export const PUBLIC_API_BASE_URL = env.PUBLIC_API_BASE_URL;
export const PUBLIC_API_URL = env.PUBLIC_API_URL;
export const PUBLIC_SUPABASE_ANON_KEY = env.PUBLIC_SUPABASE_ANON_KEY;
export const PUBLIC_SUPABASE_URL = env.PUBLIC_SUPABASE_URL;
