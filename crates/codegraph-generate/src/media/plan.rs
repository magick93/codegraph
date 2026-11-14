//! `MediaPlan` resolution: `[domains.X.media]` + graph → per-domain plans.
//!
//! Resolution is strict (the charts-binding precedent): unresolved media
//! classes, refers-wins targets, array media fields, and missing url/mime
//! columns are hard `Error::Config`s naming the domain/entity/field — never
//! bare names in templates.

use std::collections::{BTreeMap, BTreeSet};

use codegraph_config::{DomainConfig, MediaConfig};
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PropertyNode;
use codegraph_core::types::{
    KeyTemplate, MediaFieldPlan, MediaPlan, MediaProvider, MediaProviderSettings, MediaVisibility,
    UploadMode, provider_capabilities,
};
use codegraph_naming::{child_parent_fk_column, child_table_name, to_snake_case};
use codegraph_type_contracts::RefClassificationKind;

use crate::error::Error;

/// Build the media plan for every domain that declares `[media]`, keyed by
/// domain name (BTreeMap — deterministic iteration, the charts precedent).
/// Entity titles derive from the graph (`list_schemas`, entities only,
/// sorted) — the validation entry point; the generator passes
/// `generation_order` titles instead via
/// [`build_media_plan_for_domain`].
pub async fn build_media_plans(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<BTreeMap<String, MediaPlan>, Error> {
    validate_media_provider_consistency(config)?;
    let mut plans = BTreeMap::new();
    for (name, entry) in &config.domains {
        let Some(media) = &entry.media else {
            continue;
        };
        let mut titles: Vec<String> = db
            .list_schemas(Some(name))
            .await?
            .into_iter()
            .filter(|s| s.is_entity)
            .map(|s| s.title)
            .collect();
        titles.sort();
        plans.insert(
            name.clone(),
            build_media_plan_for_domain(db, name, media, &config.defaults.type_suffix, &titles)
                .await?,
        );
    }
    Ok(plans)
}

/// Graph-free validation (doctor parity): the provider capability matrix
/// and the key-template syntax — the checks that need no graph. Doctor
/// runs this per `[media]` domain (hard failure, the function-bindings
/// precedent); the plan builder runs it first too, so the cheap config
/// errors fail before any graph read.
pub fn validate_media_matrix_and_template(
    domain_name: &str,
    media: &MediaConfig,
) -> Result<(), Error> {
    let provider = MediaProvider::parse(media.provider.as_str()).ok_or_else(|| {
        Error::Config(format!(
            "domain \"{domain_name}\" media: provider \"{}\" missing from the core mirror",
            media.provider.as_str()
        ))
    })?;
    let caps = provider_capabilities(provider);
    let upload_mode = UploadMode::parse(media.upload_mode.as_str()).ok_or_else(|| {
        Error::Config(format!(
            "domain \"{domain_name}\" media: upload mode \"{}\" missing from the core mirror",
            media.upload_mode.as_str()
        ))
    })?;
    let required = upload_mode.as_capability();
    if !caps.contains(&required) {
        return Err(Error::Config(format!(
            "domain \"{domain_name}\" media: provider \"{}\" does not support upload_mode \"{}\" \
             (missing capability {:?}); pick a supported mode or provider",
            media.provider.as_str(),
            media.upload_mode.as_str(),
            required
        )));
    }
    KeyTemplate::parse(&media.key_template)
        .map(|_| ())
        .map_err(|e| Error::Config(format!("domain \"{domain_name}\" media key_template: {e}")))
}

/// One provider per app in v1 (issue #625 PR-B decision): the AppState
/// media surface is single-provider — a mixed declaration is a config error
/// naming both providers and the domains, never a silently half-configured
/// plane. Graph-free: doctor runs the SAME function, so doctor and the run
/// path cannot drift.
pub fn validate_media_provider_consistency(config: &DomainConfig) -> Result<(), Error> {
    let mut first: Option<(MediaProvider, String)> = None;
    for (name, entry) in &config.domains {
        let Some(media) = &entry.media else {
            continue;
        };
        let provider = MediaProvider::parse(media.provider.as_str()).ok_or_else(|| {
            Error::Config(format!(
                "domain \"{name}\" media: provider \"{}\" missing from the core mirror",
                media.provider.as_str()
            ))
        })?;
        match &first {
            None => first = Some((provider, name.clone())),
            Some((first_provider, first_domain)) if provider != *first_provider => {
                return Err(Error::Config(format!(
                    "media: domains \"{first_domain}\" and \"{name}\" declare different \
                     providers (\"{}\" vs \"{}\") — one provider per app in v1; split the \
                     providers across apps or align the declarations",
                    first_provider.as_str(),
                    provider.as_str()
                )));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Build one domain's plan from explicit entity titles (the generator
/// passes `generation_order` titles; [`build_media_plans`] derives them
/// from the graph).
pub async fn build_media_plan_for_domain(
    db: &dyn GraphQuerier,
    domain_name: &str,
    media: &MediaConfig,
    type_suffix: &str,
    entity_titles: &[String],
) -> Result<MediaPlan, Error> {
    validate_media_matrix_and_template(domain_name, media)?;
    let provider = MediaProvider::parse(media.provider.as_str()).ok_or_else(|| {
        Error::Config(format!(
            "domain \"{domain_name}\" media: provider \"{}\" missing from the core mirror",
            media.provider.as_str()
        ))
    })?;
    // The capability matrix: an upload_mode the provider cannot serve is a
    // config error naming both — never runtime branching (issue #625 §2).
    let caps = provider_capabilities(provider);
    let upload_mode = UploadMode::parse(media.upload_mode.as_str()).ok_or_else(|| {
        Error::Config(format!(
            "domain \"{domain_name}\" media: upload mode \"{}\" missing from the core mirror",
            media.upload_mode.as_str()
        ))
    })?;
    let required = upload_mode.as_capability();
    if !caps.contains(&required) {
        return Err(Error::Config(format!(
            "domain \"{domain_name}\" media: provider \"{}\" does not support upload_mode \"{}\" \
             (missing capability {:?}); pick a supported mode or provider",
            media.provider.as_str(),
            media.upload_mode.as_str(),
            required
        )));
    }
    let key_template = KeyTemplate::parse(&media.key_template)
        .map_err(|e| Error::Config(format!("domain \"{domain_name}\" media key_template: {e}")))?;

    // Media classes resolve exact → +type_suffix (the wire_alias_refs chain).
    let mut media_classes = Vec::new();
    let mut media_titles = BTreeSet::new();
    for class in &media.media_classes {
        let resolved = match db.get_schema(class).await? {
            Some(s) => s,
            None => {
                let suffixed = format!("{class}{type_suffix}");
                db.get_schema(&suffixed).await?.ok_or_else(|| {
                    Error::Config(format!(
                        "domain \"{domain_name}\" media: class \"{class}\" resolves to no schema \
                         (tried \"{class}\" and \"{suffixed}\")"
                    ))
                })?
            }
        };
        if media_titles.insert(resolved.title.clone()) {
            media_classes.push(resolved.title);
        }
    }

    // Fields: every non-array ValueObject property whose resolved target is
    // a declared media class. EntityReference targets are the refers-wins
    // case — the target has no child table, so media cannot lower there.
    let mut fields = Vec::new();
    let mut seen = BTreeSet::new();
    for title in entity_titles {
        let schema = db.get_schema(title).await?.ok_or_else(|| {
            Error::Config(format!(
                "domain \"{domain_name}\" media: entity \"{title}\" resolves to no schema"
            ))
        })?;
        for prop in db.get_properties(title).await? {
            match prop.effective_kind() {
                Some(RefClassificationKind::ValueObject) => {
                    let target = if prop.is_array {
                        db.get_array_item_schema(&prop.name, title).await?
                    } else {
                        db.get_property_ref_target(&prop.name, title).await?
                    };
                    let Some(target) = target else {
                        continue;
                    };
                    if !media_titles.contains(&target.title) {
                        continue;
                    }
                    if prop.is_array {
                        return Err(Error::Config(format!(
                            "domain \"{domain_name}\" media: array media field {title}.{} is a \
                             recorded deferral (issue #625 PR-A) — media references must be \
                             contained values with [0..1] multiplicity",
                            prop.name
                        )));
                    }
                    let url_column = find_column(
                        &db.get_properties(&target.title).await?,
                        &media.url_field,
                        &target.title,
                    )?;
                    let mime_column = find_column(
                        &db.get_properties(&target.title).await?,
                        &media.mime_field,
                        &target.title,
                    )?;
                    let field_snake = to_snake_case(&prop.name);
                    let child_table = child_table_name(&schema.pg_table_name, &field_snake);
                    if seen.insert((title.clone(), field_snake.clone())) {
                        fields.push(MediaFieldPlan {
                            entity_title: title.clone(),
                            entity_table: schema.pg_table_name.clone(),
                            field_name: prop.name.clone(),
                            field_snake,
                            child_table,
                            parent_fk_column: child_parent_fk_column(&schema.pg_table_name),
                            url_column,
                            mime_column,
                        });
                    }
                }
                Some(RefClassificationKind::EntityReference) => {
                    let target = db.get_property_ref_target(&prop.name, title).await?;
                    if let Some(target) = target
                        && media_titles.contains(&target.title)
                    {
                        return Err(Error::Config(format!(
                            "domain \"{domain_name}\" media: {title}.{} lowers as an entity \
                             reference (refers wins) — media requires contains-only targets",
                            prop.name
                        )));
                    }
                }
                _ => {}
            }
        }
    }
    fields
        .sort_by(|a, b| (&a.entity_title, &a.field_snake).cmp(&(&b.entity_title, &b.field_snake)));

    let provider_settings = match provider {
        MediaProvider::Supabase => MediaProviderSettings::Supabase,
        MediaProvider::R2 => {
            let r2 = media.r2.as_ref().ok_or_else(|| {
                Error::Config(format!(
                    "domain \"{domain_name}\" media: provider \"r2\" requires the media.r2 table"
                ))
            })?;
            MediaProviderSettings::R2 {
                binding: r2.binding.clone(),
                presign_ttl_secs: r2.presign_ttl_secs,
            }
        }
        MediaProvider::S3 => MediaProviderSettings::S3 {
            region: media.s3.as_ref().and_then(|s| s.region.clone()),
            endpoint: media.s3.as_ref().and_then(|s| s.endpoint.clone()),
        },
    };

    Ok(MediaPlan {
        domain: domain_name.to_string(),
        provider,
        upload_mode,
        bucket: media.bucket.clone(),
        key_template,
        visibility: if media.public_read {
            MediaVisibility::Public
        } else {
            MediaVisibility::Signed
        },
        max_bytes: media.max_bytes,
        accept: media.accept.clone(),
        display_ttl_secs: media.display_ttl_secs,
        media_classes,
        fields,
        provider_settings,
    })
}

/// The url/mime column on the media class, matched snake-lowered against
/// the configured field name; a miss is a config error naming the class
/// and the expected column (the model must carry the reference shape).
fn find_column(props: &[PropertyNode], expected: &str, class_title: &str) -> Result<String, Error> {
    props
        .iter()
        .find(|p| to_snake_case(&p.name) == expected)
        .map(|p| p.pg_column_name.clone())
        .ok_or_else(|| {
            Error::Config(format!(
                "media class \"{class_title}\" has no \"{expected}\" column — add it to the \
                 model or point url_field/mime_field at the column the class carries"
            ))
        })
}
