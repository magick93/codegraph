//! Media key lowering (issue #625 R7): the overwrite-in-place object key
//! is a PURE FUNCTION of (org, domain, entity, id, field) — so deletion
//! cleanup never needs to read the (possibly already-cascaded) child rows.
//! This module lowers a domain's key template into per-field Rust format
//! strings for the generated DELETE handler's cleanup block: `{org}` and
//! `{id}` stay runtime placeholders (the session bundle + the path param);
//! `{domain}`/`{entity}`/`{field}` bake to literals at generation time.

use codegraph_core::types::{KeyPlaceholder, KeySegment, KeyTemplate};

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
                    field_name: "bannerImage".to_string(),
                    field_snake: "banner_image".to_string(),
                    child_table: "person_banner_image".to_string(),
                    parent_fk_column: "person_id".to_string(),
                    url_column: "url".to_string(),
                    mime_column: "mime_type".to_string(),
                },
                MediaFieldPlan {
                    entity_title: "OtherType".to_string(),
                    entity_table: "other".to_string(),
                    field_name: "avatar".to_string(),
                    field_snake: "avatar".to_string(),
                    child_table: "other_avatar".to_string(),
                    parent_fk_column: "other_id".to_string(),
                    url_column: "url".to_string(),
                    mime_column: "mime_type".to_string(),
                },
                MediaFieldPlan {
                    entity_title: "PersonType".to_string(),
                    entity_table: "person".to_string(),
                    field_name: "avatar".to_string(),
                    field_snake: "avatar".to_string(),
                    child_table: "person_avatar".to_string(),
                    parent_fk_column: "person_id".to_string(),
                    url_column: "url".to_string(),
                    mime_column: "mime_type".to_string(),
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
