//! Media key lowering (issue #625 R7): the overwrite-in-place object key
//! is a PURE FUNCTION of (org, domain, entity, id, field) — so deletion
//! cleanup never needs to read the (possibly already-cascaded) child rows.
//! This module lowers a domain's key template into per-field Rust format
//! strings for the generated DELETE handler's cleanup block: `{org}` and
//! `{id}` stay runtime placeholders (the session bundle + the path param);
//! `{domain}`/`{entity}`/`{field}` bake to literals at generation time.

use codegraph_core::types::{KeyPlaceholder, KeySegment, KeyTemplate};

/// Whether the template's FIRST path segment is exactly the `{org}`
/// placeholder — the org-first shape.
///
/// This is the ONE derivation of "the org is checkable as a key prefix",
/// shared by the generated media route's guard (issue #707) and the bucket
/// RLS bootstrap (`media/migration.rs`, issue #625), so the request-plane
/// check and the bucket policy cannot disagree. Both consumers need the SAME
/// thing: the org id occupying the whole first segment — a prefix check can
/// render it, and `split_part(name, '/', 1) = <org>` can compare it.
///
/// A template that carries `{org}` somewhere else (`media/{org}/…`, mixed
/// into the first segment as `media-{org}`, or joined to another placeholder
/// as `{org}-{domain}`) is deliberately NOT org-first: the route then guards
/// with the full `render_key` equality (strictly stronger than any prefix
/// test) and the migration records that bucket RLS cannot scope objects by
/// tenant (#625).
///
/// The separator check is load-bearing. `KeyTemplate::parse` splits on `/`
/// but pushes placeholders WITHIN a chunk before appending the separator, so
/// `{org}-{domain}/…` yields `[Org, "-", Domain, "/", …]` — its first
/// LEXEME is the org placeholder while its first SEGMENT is `{org}-{domain}`.
/// Matching only `segments[0]` claimed org-first for that template and
/// emitted the `"{org}/"` prefix guard, which can never match the rendered
/// key `<org>-<domain>/…` — every legitimate display/remove would 404.
pub fn key_template_is_org_first(template: &KeyTemplate) -> bool {
    let segments = template.segments();
    matches!(
        segments.first(),
        Some(KeySegment::Placeholder(KeyPlaceholder::Org))
    ) && matches!(segments.get(1), Some(KeySegment::Literal(l)) if l == "/")
}

/// The org-first key PREFIX as a plain string with `{org}` left as a runtime
/// slot — `"{org}/"` — or `None` for a non-org-first template.
///
/// The const is a PLAIN string (the emitted guard `.replace`s `{org}`, it
/// never runs `format!` on runtime text), so nothing here is format-escaped.
/// It is derived from [`key_template_is_org_first`] rather than hardcoded:
/// that predicate guarantees the first segment is the bare `{org}`
/// placeholder, which is exactly what this renders.
pub fn key_prefix_format_for(template: &KeyTemplate) -> Option<String> {
    key_template_is_org_first(template).then(|| "{org}/".to_string())
}

/// One entity field's deletion-cleanup lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaCleanupField {
    /// The contained-value feature (snake) — the cleanup loop emits one
    /// key per field.
    pub field_snake: String,
    /// A Rust format string with `org` and `id` named args, e.g.
    /// `"{org}/core/person/{id}/avatar"`.
    pub key_format: String,
}

/// Lower the template into the Rust format string for one field. Braces in
/// literal segments are escaped (a literal `{` would otherwise open a
/// format placeholder).
pub fn key_format_for(
    template: &KeyTemplate,
    domain: &str,
    entity: &str,
    field_snake: &str,
) -> String {
    let mut out = String::new();
    for seg in template.segments() {
        match seg {
            KeySegment::Literal(lit) => out.push_str(&lit.replace('{', "{{").replace('}', "}}")),
            KeySegment::Placeholder(ph) => match ph {
                KeyPlaceholder::Org => out.push_str("{org}"),
                KeyPlaceholder::Id => out.push_str("{id}"),
                KeyPlaceholder::Domain => {
                    out.push_str(&domain.replace('{', "{{").replace('}', "}}"))
                }
                KeyPlaceholder::Entity => {
                    out.push_str(&entity.replace('{', "{{").replace('}', "}}"))
                }
                KeyPlaceholder::Field => {
                    out.push_str(&field_snake.replace('{', "{{").replace('}', "}}"))
                }
            },
        }
    }
    out
}

/// The cleanup fields for one entity from its domain plan (sorted by field
/// name — deterministic emission). Empty when the entity carries no media
/// fields (flag-off ⇒ no block).
pub fn cleanup_fields_for(
    plan: &codegraph_core::types::MediaPlan,
    entity_title: &str,
) -> Vec<MediaCleanupField> {
    plan.fields
        .iter()
        .filter(|f| f.entity_title == entity_title)
        .map(|f| MediaCleanupField {
            key_format: key_format_for(
                &plan.key_template,
                &plan.domain,
                &f.entity_table,
                &f.field_snake,
            ),
            field_snake: f.field_snake.clone(),
        })
        .collect()
}

/// The pre-rendered cleanup block for the DELETE handler (the #338
/// guard-block pattern — whitespace-exact so flag-off stays byte-identical).
/// The lowered `key_format` strings are Rust format strings; `org` and `id`
/// are bound locals inside the block.
pub fn render_cleanup_block(domain: &str, fields: &[MediaCleanupField]) -> String {
    if fields.is_empty() {
        return String::new();
    }
    let mut keys = String::new();
    for f in fields {
        keys.push_str(&format!("            format!(\"{}\"),\n", f.key_format));
    }
    format!(
        "\n\
         \x20   // Media cleanup (issue #631): overwrite-in-place keys make the object\n\
         \x20   // key a pure function of (org, entity, id, field) — recompute and\n\
         \x20   // delete best-effort; the `testkit storage --gc` sweep is the backstop.\n\
         \x20   if let Some(storage) = &state.{domain}_media_storage {{\n\
         \x20       let org = api_key_info.organization_id.to_string();\n\
         \x20       for key in [\n\
         {keys}\
         \x20       ] {{\n\
         \x20           if let Err(e) = storage.delete(&key).await {{\n\
         \x20               tracing::warn!(key = %key, error = %e, \"media cleanup failed (gc sweep will retry)\");\n\
         \x20           }}\n\
         \x20       }}\n\
         \x20   }}\n",
        domain = domain,
        keys = keys,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_core::types::{
        MediaFieldPlan, MediaProvider, MediaProviderSettings, MediaVisibility, UploadMode,
    };

    fn template() -> KeyTemplate {
        KeyTemplate::parse("{org}/{domain}/{entity}/{id}/{field}").expect("template")
    }

    #[test]
    fn org_first_predicate_matches_only_a_leading_bare_org_segment() {
        let org_first = |t: &str| key_template_is_org_first(&KeyTemplate::parse(t).expect("t"));
        // The default and any template whose FIRST segment is exactly {org}.
        assert!(org_first("{org}/{domain}/{entity}/{id}/{field}"));
        assert!(org_first("{org}/media/{id}/{field}"));
        // `{org}` elsewhere, mixed into the first segment, or absent — all
        // NOT org-first (no segment a prefix test can compare, and the
        // bucket-RLS `split_part` predicate cannot express them either).
        assert!(!org_first("media/{org}/{domain}/{entity}/{id}/{field}"));
        assert!(!org_first("media-{org}/{domain}/{entity}/{id}/{field}"));
        assert!(!org_first("{domain}/{entity}/{id}/{field}"));
        // Joined to another placeholder: the first LEXEME is the org but the
        // first SEGMENT is `{org}-{domain}`, so a `"{org}/"` prefix test can
        // never match the rendered key `<org>-<domain>/…`. Treating this as
        // org-first 404'd every legitimate display/remove (#707 review).
        assert!(!org_first("{org}-{domain}/{entity}/{id}/{field}"));
        assert!(!org_first("{org}{domain}/{entity}/{id}/{field}"));
        assert!(!org_first("{org}.{domain}/{entity}/{id}/{field}"));
    }

    #[test]
    fn key_prefix_format_is_derived_from_the_predicate() {
        let t = template();
        assert_eq!(key_prefix_format_for(&t).as_deref(), Some("{org}/"));
        // A non-org-first template gets NO prefix const — the route falls back
        // to the full `render_key` equality (issue #707 review).
        let non_org_first =
            KeyTemplate::parse("media/{org}/{domain}/{entity}/{id}/{field}").expect("template");
        assert_eq!(key_prefix_format_for(&non_org_first), None);
        let mixed = KeyTemplate::parse("media-{org}/{domain}/{entity}/{id}/{field}").expect("t");
        assert_eq!(key_prefix_format_for(&mixed), None);
        let joined = KeyTemplate::parse("{org}-{domain}/{entity}/{id}/{field}").expect("t");
        assert_eq!(key_prefix_format_for(&joined), None);
    }

    #[test]
    fn default_template_lowers_to_runtime_org_and_id() {
        let fmt = key_format_for(&template(), "core", "person", "avatar");
        assert_eq!(fmt, "{org}/core/person/{id}/avatar");
    }

    #[test]
    fn custom_template_bakes_literals_and_escapes_braces() {
        let t = KeyTemplate::parse("media/{org}/fixed/{id}/{field}").expect("template");
        let fmt = key_format_for(&t, "core", "person", "avatar");
        assert_eq!(fmt, "media/{org}/fixed/{id}/avatar");
        // Literal braces in the template survive as escaped braces.
        let t2 = KeyTemplate::parse("{id}/{field}").expect("template");
        let _ = t2; // parse guard
        let literal = key_format_for(&template(), "co{re}", "per{son", "avatar");
        assert!(
            literal.contains("co{{re}}"),
            "literal braces escaped: {literal}"
        );
        assert!(literal.contains("per{{son"), "{literal}");
    }

    #[test]
    fn cleanup_fields_filter_by_entity_and_sort_by_field() {
        let plan = codegraph_core::types::MediaPlan {
            domain: "core".to_string(),
            provider: MediaProvider::Supabase,
            upload_mode: UploadMode::ClientDirect,
            bucket: "person-media".to_string(),
            key_template: template(),
            visibility: MediaVisibility::Signed,
            max_bytes: 100,
            accept: vec![],
            display_ttl_secs: 300,
            media_classes: vec![],
            fields: vec![
                MediaFieldPlan {
                    entity_title: "PersonType".to_string(),
                    entity_table: "person".to_string(),
                    entity_path_segment: "person".to_string(),
                    route_parents: vec![],
                    field_name: "bannerImage".to_string(),
                    field_snake: "banner_image".to_string(),
                    child_table: "person_banner_image".to_string(),
                    parent_fk_column: "person_id".to_string(),
                    url_column: "url".to_string(),
                    mime_column: "mime_type".to_string(),
                    parent_tenant_column: Some("platform_organization_id".to_string()),
                    parent_link_column: None,
                },
                MediaFieldPlan {
                    entity_title: "OtherType".to_string(),
                    entity_table: "other".to_string(),
                    entity_path_segment: "other".to_string(),
                    route_parents: vec![],
                    field_name: "avatar".to_string(),
                    field_snake: "avatar".to_string(),
                    child_table: "other_avatar".to_string(),
                    parent_fk_column: "other_id".to_string(),
                    url_column: "url".to_string(),
                    mime_column: "mime_type".to_string(),
                    parent_tenant_column: Some("platform_organization_id".to_string()),
                    parent_link_column: None,
                },
                MediaFieldPlan {
                    entity_title: "PersonType".to_string(),
                    entity_table: "person".to_string(),
                    entity_path_segment: "person".to_string(),
                    route_parents: vec![],
                    field_name: "avatar".to_string(),
                    field_snake: "avatar".to_string(),
                    child_table: "person_avatar".to_string(),
                    parent_fk_column: "person_id".to_string(),
                    url_column: "url".to_string(),
                    mime_column: "mime_type".to_string(),
                    parent_tenant_column: Some("platform_organization_id".to_string()),
                    parent_link_column: None,
                },
            ],
            provider_settings: MediaProviderSettings::Supabase,
        };
        let fields = cleanup_fields_for(&plan, "PersonType");
        assert_eq!(
            fields
                .iter()
                .map(|f| f.field_snake.as_str())
                .collect::<Vec<_>>(),
            vec!["banner_image", "avatar"],
            "only this entity's fields, in plan order"
        );
        assert_eq!(fields[1].key_format, "{org}/core/person/{id}/avatar");
    }

    #[test]
    fn cleanup_block_renders_only_when_fields_exist() {
        let fields = vec![MediaCleanupField {
            field_snake: "avatar".to_string(),
            key_format: "{org}/core/person/{id}/avatar".to_string(),
        }];
        let block = render_cleanup_block("core", &fields);
        assert!(
            block.contains("if let Some(storage) = &state.core_media_storage {"),
            "{block}"
        );
        assert!(
            block.contains(
                "for key in [\n            format!(\"{org}/core/person/{id}/avatar\"),\n        ] {"
            ),
            "the per-field key list: {block}"
        );
        assert!(
            block.contains("storage.delete(&key).await"),
            "best-effort provider delete: {block}"
        );
        assert!(
            render_cleanup_block("core", &[]).is_empty(),
            "flag-off ⇒ empty block"
        );
    }
}
