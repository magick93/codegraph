-- Media storage bootstrap (issue #625 PR-B): private bucket +
-- defense-in-depth storage RLS for domain "refunds". The app plane
-- authorizes every upload BEFORE any provider call — these policies
-- mirror the expressible subset only (issue #625 §2).
-- Static band 023000 (after the functions band). Idempotent.
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM information_schema.schemata WHERE schema_name = 'storage') THEN
    RAISE WARNING 'media migration: no storage schema (plain Postgres?) — skipping bucket bootstrap';
    RETURN;
  END IF;
  INSERT INTO storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
  VALUES ('refund-receipts', 'refund-receipts', false, 5242880, ARRAY['image/png', 'image/jpeg', 'application/pdf']::text[])
  ON CONFLICT (id) DO UPDATE SET
    public = EXCLUDED.public,
    file_size_limit = EXCLUDED.file_size_limit,
    allowed_mime_types = EXCLUDED.allowed_mime_types;
  -- Org-scoped: the default key template puts the org id in the
  -- first path segment; the JWT's app_metadata.organization_id must
  -- match it. The app-plane gate stays authoritative for everything
  -- bucket RLS cannot express (scopes, roles, SoD).
  IF NOT EXISTS (
    SELECT 1 FROM pg_policies
    WHERE schemaname = 'storage' AND tablename = 'objects'
      AND policyname = 'media_refunds_org_scoped'
  ) THEN
    CREATE POLICY media_refunds_org_scoped ON storage.objects
      FOR ALL TO authenticated
      USING (bucket_id = 'refund-receipts' AND split_part(name, '/', 1) = COALESCE(auth.jwt() -> 'app_metadata' ->> 'organization_id', ''))
      WITH CHECK (bucket_id = 'refund-receipts' AND split_part(name, '/', 1) = COALESCE(auth.jwt() -> 'app_metadata' ->> 'organization_id', ''));
  END IF;
END $$;
