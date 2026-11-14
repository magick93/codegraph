//! Media storage migration builder (issue #625 PR-B): the Supabase bucket +
//! defense-in-depth RLS bootstrap, `migrations/023000_media_storage.sql`
//! (after the functions band — the static-band convention of
//! `022000_functions.sql`; no allocator coupling).
//!
//! Scope rules (issue #625 §2 + PR decisions):
//! - Supabase ONLY (the `storage_side_rls` capability). R2 buckets are
//!   platform-side — no migration analogue (the doctor/wrangler side owns
//!   visibility there).
//! - The app plane authorizes every upload BEFORE any provider call; these
//!   policies mirror the expressible subset only (org membership via the
//!   first key segment — the `{org}` placeholder position — never the full
//!   capability vocabulary).
//! - Storage-presence guard: plain-Postgres dev environments without the
//!   `storage` schema get a WARNING, not a migration failure (the pgmq-shim
//!   philosophy).
//! - sqlite targets are refused upstream (no Supabase Storage there).

use std::collections::BTreeMap;

use codegraph_core::types::{KeyPlaceholder, KeySegment, MediaPlan, MediaVisibility};

/// The static migration band (after `022000_functions.sql`).
pub const MEDIA_MIGRATION_BAND: u32 = 23_000;

/// The generated SQL. Pure over the plans — unit-KAT-tested in this module.
pub fn emit_media_storage_migration(plans: &BTreeMap<String, MediaPlan>) -> Option<String> {
    // One provider per app (validated upstream); supabase carries the SQL
    // surface. Any other provider ⇒ no migration file at all.
    let supabase = plans.values().find(|p| {
        matches!(p.provider, codegraph_core::types::MediaProvider::Supabase)
    })?;
    Some(render_sql(supabase))
}

fn render_sql(plan: &MediaPlan) -> String {
    let mime_array = plan
        .accept
        .iter()
        .map(|m| format!("'{m}'"))
        .collect::<Vec<_>>()
        .join(", ");
    // Org-scoped policy only when the template's FIRST segment is {org} —
    // the expressible-subset rule. Otherwise the app gate stands alone and
    // the migration records that decision.
    let org_first = plan
        .key_template
        .segments()
        .first()
        .is_some_and(|s| matches!(s, KeySegment::Placeholder(KeyPlaceholder::Org)));

    let mut sql = String::new();
    sql.push_str(&format!(
        "-- Media storage bootstrap (issue #625 PR-B): private bucket +\n\
         -- defense-in-depth storage RLS for domain \"{domain}\". The app plane\n\
         -- authorizes every upload BEFORE any provider call — these policies\n\
         -- mirror the expressible subset only (issue #625 §2).\n\
         -- Static band 023000 (after the functions band). Idempotent.\n\
         DO $$\n\
         BEGIN\n\
         \x20 IF NOT EXISTS (SELECT 1 FROM information_schema.schemata WHERE schema_name = 'storage') THEN\n\
         \x20   RAISE WARNING 'media migration: no storage schema (plain Postgres?) — skipping bucket bootstrap';\n\
         \x20   RETURN;\n\
         \x20 END IF;\n",
        domain = plan.domain,
    ));
    sql.push_str(&format!(
        "  INSERT INTO storage.buckets (id, name, public, file_size_limit, allowed_mime_types)\n\
         \x20 VALUES ('{bucket}', '{bucket}', {public}, {max_bytes}, ARRAY[{mimes}]::text[])\n\
         \x20 ON CONFLICT (id) DO UPDATE SET\n\
         \x20   public = EXCLUDED.public,\n\
         \x20   file_size_limit = EXCLUDED.file_size_limit,\n\
         \x20   allowed_mime_types = EXCLUDED.allowed_mime_types;\n",
        bucket = plan.bucket,
        public = if plan.visibility == MediaVisibility::Public {
            "true"
        } else {
            "false"
        },
        max_bytes = plan.max_bytes,
        mimes = mime_array,
    ));

    if org_first {
        let predicate = format!(
            "bucket_id = '{bucket}' AND split_part(name, '/', 1) = COALESCE(auth.jwt() -> 'app_metadata' ->> 'organization_id', '')",
            bucket = plan.bucket,
        );
        sql.push_str(&format!(
            "  -- Org-scoped: the default key template puts the org id in the\n\
             \x20 -- first path segment; the JWT's app_metadata.organization_id must\n\
             \x20 -- match it. The app-plane gate stays authoritative for everything\n\
             \x20 -- bucket RLS cannot express (scopes, roles, SoD).\n\
             \x20 IF NOT EXISTS (\n\
             \x20   SELECT 1 FROM pg_policies\n\
             \x20   WHERE schemaname = 'storage' AND tablename = 'objects'\n\
             \x20     AND policyname = 'media_{domain}_org_scoped'\n\
             \x20 ) THEN\n\
             \x20   CREATE POLICY media_{domain}_org_scoped ON storage.objects\n\
             \x20     FOR ALL TO authenticated\n\
             \x20     USING ({predicate})\n\
             \x20     WITH CHECK ({predicate});\n\
             \x20 END IF;\n",
            domain = plan.domain,
            predicate = predicate,
        ));
    } else {
        sql.push_str(&format!(
            "  -- NOTE: the key template's first segment is not {{org}} — bucket RLS\n\
             \x20 -- cannot scope objects by tenant for domain \"{domain}\". The app-plane\n\
             \x20 -- capability/RLS gate is the sole enforcement (recorded in #625).\n",
            domain = plan.domain,
        ));
    }
    sql.push_str("END $$;\n");
    sql
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_core::types::{
        KeySegment, KeyTemplate, MediaProvider, MediaProviderSettings, MediaVisibility, UploadMode,
    };

    fn plan(org_first: bool) -> MediaPlan {
        let segments = if org_first {
            vec![
                KeySegment::Placeholder(KeyPlaceholder::Org),
                KeySegment::Literal("/".to_string()),
                KeySegment::Placeholder(KeyPlaceholder::Id),
                KeySegment::Literal("/".to_string()),
                KeySegment::Placeholder(KeyPlaceholder::Field),
            ]
        } else {
            vec![
                KeySegment::Placeholder(KeyPlaceholder::Id),
                KeySegment::Literal("/".to_string()),
                KeySegment::Placeholder(KeyPlaceholder::Field),
            ]
        };
        MediaPlan {
            domain: "core".to_string(),
            provider: MediaProvider::Supabase,
            upload_mode: UploadMode::ClientDirect,
            bucket: "person-media".to_string(),
            key_template: KeyTemplate::from_segments(segments),
            visibility: MediaVisibility::Signed,
            max_bytes: 5_242_880,
            accept: vec!["image/png".to_string(), "image/jpeg".to_string()],
            display_ttl_secs: 300,
            media_classes: vec!["MediaReferenceType".to_string()],
            fields: vec![],
            provider_settings: MediaProviderSettings::Supabase,
        }
    }

    #[test]
    fn supabase_plan_renders_bucket_and_org_scoped_policy() {
        let mut plans = BTreeMap::new();
        plans.insert("core".to_string(), plan(true));
        let sql = emit_media_storage_migration(&plans).expect("supabase ⇒ migration");
        assert!(sql.contains("INSERT INTO storage.buckets"), "{sql}");
        assert!(sql.contains("'person-media'"), "{sql}");
        assert!(sql.contains("false, 5242880"), "{sql}");
        assert!(
            sql.contains("ARRAY['image/png', 'image/jpeg']::text[]"),
            "{sql}"
        );
        assert!(
            sql.contains("CREATE POLICY media_core_org_scoped ON storage.objects"),
            "{sql}"
        );
        assert!(
            sql.contains("split_part(name, '/', 1) = COALESCE(auth.jwt() -> 'app_metadata' ->> 'organization_id', '')"),
            "{sql}"
        );
        // Fail-soft on plain Postgres (no storage schema) + idempotency.
        assert!(
            sql.contains("RAISE WARNING 'media migration: no storage schema"),
            "{sql}"
        );
        assert!(sql.contains("IF NOT EXISTS"), "{sql}");
    }

    #[test]
    fn non_org_first_template_records_the_app_gate_decision() {
        let mut plans = BTreeMap::new();
        plans.insert("core".to_string(), plan(false));
        let sql = emit_media_storage_migration(&plans).expect("supabase ⇒ migration");
        assert!(
            !sql.contains("CREATE POLICY"),
            "no policy when the template cannot express org scoping: {sql}"
        );
        assert!(
            sql.contains("first segment is not {org}"),
            "the decision is recorded: {sql}"
        );
    }

    #[test]
    fn r2_plan_emits_no_migration() {
        let mut plans = BTreeMap::new();
        let mut p = plan(true);
        p.provider = MediaProvider::R2;
        p.provider_settings = MediaProviderSettings::R2 {
            binding: "MEDIA_BUCKET".to_string(),
            presign_ttl_secs: 900,
        };
        plans.insert("core".to_string(), p);
        assert!(
            emit_media_storage_migration(&plans).is_none(),
            "R2 buckets are platform-side — no migration analogue"
        );
    }

    #[test]
    fn migration_band_is_after_functions() {
        assert_eq!(MEDIA_MIGRATION_BAND, 23_000);
    }
}
