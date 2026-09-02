--- Bootstrap: required PostgreSQL extensions for basejump / pg_tle
CREATE EXTENSION IF NOT EXISTS http WITH SCHEMA extensions;
CREATE EXTENSION IF NOT EXISTS pg_tle;
--- Core crypto helpers (gen_random_bytes for API keys, pgcrypto functions)
CREATE EXTENSION IF NOT EXISTS pgcrypto;
