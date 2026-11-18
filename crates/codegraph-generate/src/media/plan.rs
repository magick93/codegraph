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
    KeyTemplate, MediaFieldPlan, MediaPlan, MediaProvider, MediaProviderSettings, MediaRouteParent,
    MediaVisibility, ParentCandidate, SchemaNode, UploadMode, provider_capabilities,
};
use codegraph_naming::{child_parent_fk_column, child_table_name, to_snake_case};
use codegraph_type_contracts::RefClassificationKind;

use crate::api::api_model::resolve_path_segment_with_config;
use crate::api::router::param_name_from_path_segment;
use crate::db::entity::is_global_entity;
use crate::error::Error;

/// The tenant column the DDL injects on every non-global table (and on its
/// child tables). Mirrors `db/ddl/query.rs` + `persistence.rs`.
const TENANT_COLUMN: &str = "platform_organization_id";

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
            build_media_plan_for_domain(
                db,
                config,
                name,
                media,
                &config.defaults.type_suffix,
                &titles,
            )
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

/// The media fields that emit HTTP routes for one entity (issue #625 PR-C):
/// the domain plan's media fields when the upload mode needs an app-plane
/// route (`through-app` | `presigned`). Client-direct emits NO routes (R5 —
/// the browser speaks to the provider directly with its session; the app
/// plane authorizes through bucket RLS + the normal update path). Sorted by
/// field snake — deterministic router emission.
pub async fn media_route_fields(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain_name: &str,
    entity_title: &str,
) -> Result<Vec<String>, Error> {
    Ok(media_route_surface(db, config, domain_name, entity_title)
        .await?
        .fields)
}

/// The router-facing media surface for ONE entity (issue #710): the
/// plan-driven fields PLUS the domain's configured `max_bytes`, resolved
/// from the SAME `MediaPlan` the route generator renders the handler from
/// — so the axum `DefaultBodyLimit` ceiling and the handler's
/// `data.len() > max_bytes` check cannot drift. Empty/`None` when the
/// domain declares no `[media]`, or is `client-direct` (no app-plane route).
#[derive(Debug, Clone, Default)]
pub struct MediaRouteSurface {
    pub fields: Vec<String>,
    /// `Some` exactly when `fields` is non-empty: an entity with no
    /// app-plane media route carries no body limit.
    pub max_bytes: Option<u64>,
}

pub async fn media_route_surface(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain_name: &str,
    entity_title: &str,
) -> Result<MediaRouteSurface, Error> {
    let Some(media) = config
        .domains
        .get(domain_name)
        .and_then(|d| d.media.as_ref())
    else {
        return Ok(MediaRouteSurface::default());
    };
    if media.upload_mode == codegraph_config::MediaUploadMode::ClientDirect {
        return Ok(MediaRouteSurface::default());
    }
    let titles = [entity_title.to_string()];
    let plan = build_media_plan_for_domain(
        db,
        config,
        domain_name,
        media,
        &config.defaults.type_suffix,
        &titles,
    )
    .await?;
    let fields: Vec<String> = plan.fields.iter().map(|f| f.field_snake.clone()).collect();
    Ok(MediaRouteSurface {
        max_bytes: (!fields.is_empty()).then_some(plan.max_bytes),
        fields,
    })
}

/// The media fields the APP writes the reference row for (issue #707):
/// `through-app` mode emits an upload handler that INSERTs the child row on
/// its own route, so that handler is the reference's ONLY writer and the
/// request DTOs (create/update) must omit it — otherwise a client can name
/// any object key it likes, and `display`/`remove` would sign or delete
/// another organization's object with the service credential.
///
/// Deliberately NOT `media_route_fields`: that set also covers `presigned`,
/// whose route only MINTS a PUT URL — its reference is "persisted afterwards
/// through the normal update path" (`media_route.tera`), i.e. the client IS
/// the writer there (presigned overwrite semantics are #701's), and
/// client-direct has no app-plane route at all. Stripping the DTO in those
/// modes would make the field unwritable; they keep it, byte-identical.
pub async fn media_upload_owned_fields(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain_name: &str,
    entity_title: &str,
) -> Result<Vec<String>, Error> {
    let Some(media) = config
        .domains
        .get(domain_name)
        .and_then(|d| d.media.as_ref())
    else {
        return Ok(Vec::new());
    };
    if media.upload_mode != codegraph_config::MediaUploadMode::ThroughApp {
        return Ok(Vec::new());
    }
    let titles = [entity_title.to_string()];
    let plan = build_media_plan_for_domain(
        db,
        config,
        domain_name,
        media,
        &config.defaults.type_suffix,
        &titles,
    )
    .await?;
    // BOTH spellings, because the consumers match against different ones: the
    // DTO/repository trees carry the INGESTED `rust_field_name` (mox strips
    // `@`/`-`, JSON escapes keywords) while the plan keys on the authored
    // name and its snake form. Returning only the snake form made a media
    // feature named `avatar-image` (or `type`) silently skip the exclusion —
    // still guarded on the read path, but the defense-in-depth would be off.
    let mut out: Vec<String> = Vec::with_capacity(plan.fields.len() * 2);
    for field in &plan.fields {
        out.push(field.field_snake.clone());
        if field.field_name != field.field_snake {
            out.push(field.field_name.clone());
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// The domain's configured upload mode when it needs routes (`Some`).
/// The router template branches on it (presign issuance vs multipart).
pub fn media_upload_mode(config: &DomainConfig, domain_name: &str) -> Option<UploadMode> {
    config.domains.get(domain_name).and_then(|d| {
        d.media
            .as_ref()
            .filter(|m| m.upload_mode != codegraph_config::MediaUploadMode::ClientDirect)
            .map(|m| match m.upload_mode {
                codegraph_config::MediaUploadMode::Presigned => UploadMode::Presigned,
                _ => UploadMode::ThroughApp,
            })
    })
}

/// Build one domain's plan from explicit entity titles (the generator
/// passes `generation_order` titles; [`build_media_plans`] derives them
/// from the graph).
pub async fn build_media_plan_for_domain(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
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
    // The graph parent candidates are the router's Source-2 nesting fallback;
    // loaded ONCE per plan and only when a `role = "child"` entity declares no
    // configured `parent` (the common shapes never pay for the query).
    let mut parent_candidates: Option<Vec<ParentCandidate>> = None;
    for title in entity_titles {
        let schema = db.get_schema(title).await?.ok_or_else(|| {
            Error::Config(format!(
                "domain \"{domain_name}\" media: entity \"{title}\" resolves to no schema"
            ))
        })?;
        // Resolved lazily: only an entity that actually carries a media field
        // needs its route path, so an unrelated child with broken nesting
        // cannot fail this domain's plan.
        let mut route_path: Option<ResolvedRoutePath> = None;
        // Tenancy of the owning table follows the DDL's own rule — NEVER a
        // property scan: mox entity schemas never declare the tenant column
        // (the DDL injects it), so a scan would return `None` for every real
        // model and silently emit unscoped media SQL (issue #690).
        // `is_global_entity` is a constant-false stub today; when it grows
        // real logic the media plane follows it. The DDL
        // (`db/ddl/query.rs`) and `persistence.rs` carry the same function;
        // all three must move together.
        let parent_tenant_column = if is_global_entity(&schema.pg_table_name, config) {
            None
        } else {
            Some(TENANT_COLUMN.to_string())
        };
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
                        let resolved = match &route_path {
                            Some(path) => path.clone(),
                            None => {
                                let (entity_path_segment, route_parents) =
                                    resolve_media_route_path(
                                        db,
                                        config,
                                        domain_name,
                                        title,
                                        &schema,
                                        &mut parent_candidates,
                                    )
                                    .await?;
                                // Issue #722: a nested route carries the parent
                                // id, so the parent gate must also prove the
                                // addressed row belongs to that parent. The
                                // link column comes from the canonical FK
                                // resolver chain; a nested field with none is
                                // a hard error (fail closed — never a wider
                                // gate).
                                let parent_link_column = resolve_media_parent_link_column(
                                    db,
                                    config,
                                    domain_name,
                                    title,
                                    &route_parents,
                                    &mut parent_candidates,
                                )
                                .await?;
                                let resolved = ResolvedRoutePath {
                                    entity_path_segment,
                                    route_parents,
                                    parent_link_column,
                                };
                                route_path = Some(resolved.clone());
                                resolved
                            }
                        };
                        // Issue #699: the table DDL emits `deleted_at` when it
                        // calls the entity auditable — the domain's `auditable`
                        // flag (default ON) and NOT append-only
                        // (`db/ddl/query.rs`). Append-only is the explicit
                        // entity flag OR the same operations inference the DDL
                        // uses (effective operations exclude update AND
                        // delete). A soft-deleted parent must not serve or
                        // destroy its media, but a gate naming a column the
                        // table lacks would 500 every request — so both halves
                        // ride this ONE value. Computed HERE (not per entity)
                        // because `resolve_entity_operations` is not cached
                        // and this builder runs once per entity handler too:
                        // an entity with no media field must cost no query.
                        let parent_soft_delete = {
                            let domain_cfg = config.domains.get(domain_name);
                            let auditable = domain_cfg.and_then(|d| d.auditable).unwrap_or(true);
                            let append_only = domain_cfg
                                .and_then(|d| d.get_entity_config(title))
                                .is_some_and(|ec| ec.is_append_only())
                                || {
                                    let ops = crate::api::api_model::resolve_entity_operations(
                                        db,
                                        config,
                                        domain_name,
                                        title,
                                    )
                                    .await;
                                    !ops.iter().any(|op| op == "update" || op == "delete")
                                };
                            auditable && !append_only
                        };
                        fields.push(MediaFieldPlan {
                            entity_title: title.clone(),
                            entity_table: schema.pg_table_name.clone(),
                            entity_path_segment: resolved.entity_path_segment,
                            route_parents: resolved.route_parents,
                            field_name: prop.name.clone(),
                            field_snake,
                            child_table,
                            parent_fk_column: child_parent_fk_column(&schema.pg_table_name),
                            url_column,
                            mime_column,
                            parent_tenant_column: parent_tenant_column.clone(),
                            parent_link_column: resolved.parent_link_column,
                            parent_soft_delete,
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

/// The route path a media field is addressed under, resolved once per owning
/// entity (issue #713 + #722): the entity's own segment plus the nesting
/// chain, and — for a nested entity — the column that links its rows to the
/// URL parent.
#[derive(Debug, Clone)]
struct ResolvedRoutePath {
    entity_path_segment: String,
    route_parents: Vec<MediaRouteParent>,
    parent_link_column: Option<String>,
}

/// The link column a NESTED media route's parent gate filters on (issue
/// #722): the column on the OWNING table that points at the ancestor the
/// route is mounted under (e.g. `person_id` on `contact`).
///
/// The derivation is the canonical resolver, never a new rule
/// ([`crate::fk::resolve_parent_fk_column_same_domain`]: config `parent_ref` →
/// config child+parent (`{parent}_id`) → graph parent candidate, with the
/// router's same-domain check). A nested field with no resolvable column is a
/// HARD config error: the gate must prove the addressed row belongs to the URL
/// parent, and skipping that proof is an IDOR.
///
/// Public so the node-free PR-CI gates (`media_pipeline_tests`) can pin the
/// fail-closed branch directly: the branch is defensive (the nesting resolver's
/// parent predicate is narrower than the FK resolver's), so no graph shape
/// reaches it through `build_media_plan_for_domain` alone.
pub async fn resolve_media_parent_link_column(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain_name: &str,
    title: &str,
    route_parents: &[MediaRouteParent],
    parent_candidates: &mut Option<Vec<ParentCandidate>>,
) -> Result<Option<String>, Error> {
    if route_parents.is_empty() {
        return Ok(None);
    }
    // Depth cap (issue #722 review): the emitted extractor binds at most
    // grandparent → parent → id — the SAME limit the CRUD plane's
    // `ancestor_path_params` arm has. `router.tera` nests recursively, so a
    // deeper entity WOULD be mounted with more url params and every media
    // request would answer 400 (axum rejects a tuple whose arity differs from
    // the route's param count). Fail closed at generation time instead of
    // advertising a route the handler can never serve.
    if route_parents.len() > 2 {
        let chain: Vec<&str> = route_parents
            .iter()
            .map(|hop| hop.entity_title.as_str())
            .collect();
        return Err(Error::Config(format!(
            "domain \"{domain_name}\" media: \"{title}\" is mounted {} levels deep ({}), but a \
             media handler binds at most grandparent → parent → id (the CRUD plane's \
             `ancestor_path_params` limit) — a deeper route would answer 400 on every request. \
             Flatten the nesting or extend both planes together.",
            route_parents.len(),
            chain.join(" > "),
        )));
    }
    // Loaded lazily by the route-path resolver for Source-2 nesting; a
    // config-declared nesting may not have needed it yet, so ensure it exists
    // (the resolver's graph fallback reads it).
    if parent_candidates.is_none() {
        *parent_candidates = Some(db.get_parent_candidates().await?);
    }
    let candidates = parent_candidates.as_ref().cloned().unwrap_or_default();
    let entity_cfg = config
        .domains
        .get(domain_name)
        .and_then(|d| d.get_entity_config(title));
    crate::fk::resolve_parent_fk_column_same_domain(
        title,
        &candidates,
        entity_cfg,
        domain_name,
        config,
        db,
    )
    .await
    .ok_or_else(|| {
        Error::Config(format!(
            "domain \"{domain_name}\" media: nested media field on \"{title}\" has no resolvable \
             link column to its parent route — the parent gate must prove the row belongs to the \
             URL parent (without it the route is an IDOR). Declare \
             `parent_ref = \"<parent_fk_column>\"` under \
             [domains.{domain_name}.entity_config.{title}]"
        ))
    })
    .map(Some)
}

/// The route path a media field is addressed under (issue #713): the owning
/// entity's segment plus the nesting chain, resolved ONCE here so the hurl
/// contract and the utoipa path read one value and neither re-resolves the
/// schema.
///
/// The nesting rule is `api/router.rs`'s, hop by hop (`router.tera` mounts a
/// child as `.nest("/{parent.param_name}/{child.path_segment}", …)`):
///
/// - **Source 1** — a `role = "child"` entity nests under its CONFIGURED
///   `parent`, but only when that parent is a routable entity of THIS domain
///   (the router's `title_to_entity_idx` membership); otherwise the router
///   falls through to Source 2, and so does this resolver.
/// - **Source 2** — every other EXPLICIT non-root role (the router's gate is
///   `role != "root"`, not `role == "child"`) nests under the first graph
///   parent candidate whose parent is routable in this domain; the router
///   iterates candidates in order and skips the unmountable ones.
/// - **`role = "root"`** (or no role) mounts at the domain level.
///
/// A non-root entity that resolves through NEITHER source is a MODELING error:
/// the router mounts no route for it at all (its routes fn is nested nowhere),
/// so a flat contract would address a path that does not exist. The diagnostic
/// names the configured parent and/or the candidate that was considered.
async fn resolve_media_route_path(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain_name: &str,
    title: &str,
    schema: &SchemaNode,
    parent_candidates: &mut Option<Vec<ParentCandidate>>,
) -> Result<(String, Vec<MediaRouteParent>), Error> {
    let domain_cfg = config.domains.get(domain_name);
    let entity_cfg = domain_cfg.and_then(|d| d.get_entity_config(title));
    let entity_path_segment = resolve_path_segment_with_config(entity_cfg, schema, config);

    let mut route_parents: Vec<MediaRouteParent> = Vec::new();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    visited.insert(title.to_string());
    let mut current_title = title.to_string();
    let mut current_cfg = entity_cfg;
    loop {
        let role = current_cfg
            .and_then(|c| c.role.as_deref())
            .unwrap_or("root");
        let configured_parent = current_cfg.and_then(|c| c.parent.clone());
        match resolve_parent_hop(
            db,
            config,
            domain_name,
            &current_title,
            role,
            configured_parent.as_deref(),
            parent_candidates,
        )
        .await?
        {
            ParentHop::Root => break,
            ParentHop::Hop {
                title: parent_title,
                schema: parent_schema,
            } => {
                if !visited.insert(parent_title.clone()) {
                    return Err(Error::Config(format!(
                        "domain \"{domain_name}\" media: the parent chain of \"{title}\" \
                         cycles at \"{parent_title}\""
                    )));
                }
                let parent_cfg = domain_cfg.and_then(|d| d.get_entity_config(&parent_title));
                let parent_segment =
                    resolve_path_segment_with_config(parent_cfg, &parent_schema, config);
                route_parents.insert(
                    0,
                    MediaRouteParent {
                        entity_title: parent_title.clone(),
                        param_name: param_name_from_path_segment(&parent_segment),
                        path_segment: parent_segment,
                    },
                );
                current_title = parent_title;
                current_cfg = parent_cfg;
            }
            ParentHop::Unresolvable {
                role,
                configured,
                candidate,
            } => {
                let subject = if current_title == title {
                    format!("entity \"{current_title}\"")
                } else {
                    format!("ancestor \"{current_title}\" of \"{title}\"")
                };
                return Err(Error::Config(format!(
                    "domain \"{domain_name}\" media: {subject} is role = \"{role}\" but has no \
                     mountable parent — the router mounts no route for it, so a media contract \
                     would address a path that does not exist. {}",
                    unresolvable_parent_reason(
                        configured.as_deref(),
                        candidate.as_deref(),
                        domain_name
                    )
                )));
            }
        }
    }
    Ok((entity_path_segment, route_parents))
}

/// One entity's parent resolution — the router's two sources, per hop.
enum ParentHop {
    /// The effective role is `"root"`: the entity mounts at the domain level.
    Root,
    /// A mountable ancestor (a routable entity of this domain). The schema is
    /// boxed — it dwarfs the diagnostic variants and is consumed immediately.
    Hop {
        title: String,
        schema: Box<SchemaNode>,
    },
    /// An explicit non-root role with no mountable ancestor: the router mounts
    /// no route for the entity, nor for anything nested inside it.
    Unresolvable {
        role: String,
        configured: Option<String>,
        candidate: Option<String>,
    },
}

/// Resolve ONE hop of the nesting chain, exactly as `api/router.rs` does.
async fn resolve_parent_hop(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain_name: &str,
    title: &str,
    role: &str,
    configured_parent: Option<&str>,
    parent_candidates: &mut Option<Vec<ParentCandidate>>,
) -> Result<ParentHop, Error> {
    // A root mounts at the domain level. Every EXPLICIT non-root role nests —
    // the router's Source-2 gate is `role != "root"` (`api/router.rs`), and
    // UI-plane parity encodes the same rule (`ui/page.rs`).
    if role == "root" {
        return Ok(ParentHop::Root);
    }

    // Source 1: a `role = "child"` entity's CONFIGURED parent. The router only
    // honours it when the title is in `title_to_entity_idx` (a routable entity
    // of this domain) and otherwise FALLS THROUGH to Source 2 — a stale
    // `parent` naming a renamed/absent class must not abort generation.
    if role == "child"
        && let Some(parent) = configured_parent
        && let Some(schema) = routable_parent_schema(db, config, domain_name, parent).await?
    {
        return Ok(ParentHop::Hop {
            title: parent.to_string(),
            schema: Box::new(schema),
        });
    }

    // Source 2: the graph parent candidates, tried IN ORDER (the router skips
    // candidates whose parent it cannot mount). The first candidate seen is
    // remembered so the diagnostic can name it when nothing resolves.
    if parent_candidates.is_none() {
        *parent_candidates = Some(db.get_parent_candidates().await?);
    }
    let candidates = parent_candidates.as_deref().unwrap_or_default();
    let mut named_candidate: Option<String> = None;
    for candidate in candidates.iter().filter(|c| c.child_title == title) {
        let parent_title = candidate.parent_title.clone();
        if let Some(schema) = routable_parent_schema(db, config, domain_name, &parent_title).await?
        {
            return Ok(ParentHop::Hop {
                title: parent_title,
                schema: Box::new(schema),
            });
        }
        named_candidate.get_or_insert(parent_title);
    }

    Ok(ParentHop::Unresolvable {
        role: role.to_string(),
        configured: configured_parent.map(str::to_string),
        candidate: named_candidate,
    })
}

/// A parent the router can actually mount: a schema of THIS domain that emits
/// a table (`is_entity` + a `pg_table_name`) and is not generation-skipped —
/// the router's `title_to_entity_idx` membership (`api/router.rs` pass 1).
async fn routable_parent_schema(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain_name: &str,
    title: &str,
) -> Result<Option<SchemaNode>, Error> {
    let Some(schema) = db.get_schema_in_domain(title, domain_name).await? else {
        return Ok(None);
    };
    if !schema.is_entity || schema.pg_table_name.is_empty() {
        return Ok(None);
    }
    let generation_mode = config
        .domains
        .get(domain_name)
        .and_then(|d| d.get_entity_config(title))
        .and_then(|ec| ec.generation_mode.as_deref())
        .unwrap_or(&config.defaults.generation_mode);
    if generation_mode == "handler_only" || generation_mode == "ddd_only" {
        return Ok(None);
    }
    Ok(Some(schema))
}

/// Why no mountable ancestor was found, naming whatever the model declared.
fn unresolvable_parent_reason(
    configured: Option<&str>,
    candidate: Option<&str>,
    domain_name: &str,
) -> String {
    match (configured, candidate) {
        (Some(parent), Some(candidate)) => format!(
            "Configured parent \"{parent}\" is not a routable entity in domain \
             \"{domain_name}\" (a parent must be a generated entity with a table), and the \
             graph parent candidate \"{candidate}\" is not routable either — point \
             `parent`/`parent_ref` at a generated entity."
        ),
        (Some(parent), None) => format!(
            "Configured parent \"{parent}\" is not a routable entity in domain \
             \"{domain_name}\" (a parent must be a generated entity with a table) and no graph \
             parent candidate exists — point `parent`/`parent_ref` at a generated entity."
        ),
        (None, Some(candidate)) => format!(
            "The graph parent candidate \"{candidate}\" is not a routable entity in domain \
             \"{domain_name}\" (a parent must be a generated entity with a table) — point \
             `parent`/`parent_ref` at a generated entity."
        ),
        (None, None) => "Declare `parent`/`parent_ref` naming a generated entity of this domain \
                         (or model a parent candidate)."
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use codegraph_config::config::parse_domain_config_str;
    use codegraph_core::mock::MockEngine;

    /// Issue #722: a nested media field whose link column cannot be resolved
    /// must fail closed. Shipping the ancestor-tuple extractor without the
    /// ownership predicate turns a nested media route into an IDOR, so a
    /// missing link column is a hard `Error::Config` naming the entity, the
    /// domain, and the fix — never a silently wider gate.
    #[tokio::test]
    async fn nested_media_without_a_resolvable_link_column_is_config_error() {
        let config = parse_domain_config_str(
            "[defaults]\n\
             type_suffix = \"Type\"\n\n\
             [domains.core]\n\
             label = \"Core\"\n\
             schema_dir = \"core\"\n\
             postgres_schema = \"core\"\n\n\
             [domains.core.entity_config.ContactType]\n\
             role = \"value_object\"\n\
             path_segment = \"contacts\"\n\n\
             [domains.core.media]\n\
             provider = \"supabase\"\n\
             upload_mode = \"through-app\"\n\
             bucket = \"person-media\"\n\
             max_bytes = 100\n\
             accept = [\"image/png\"]\n",
        )
        .expect("domains.toml");

        // No `parent_ref`, no config child+parent arm (role is not "child"),
        // and no graph parent candidate ⇒ the canonical FK resolver has
        // nothing to resolve.
        let db = MockEngine::new();
        let route_parents = vec![MediaRouteParent {
            entity_title: "PersonType".into(),
            path_segment: "persons".into(),
            param_name: "person_id".into(),
        }];
        let mut candidates = None;
        let err = resolve_media_parent_link_column(
            &db,
            &config,
            "core",
            "ContactType",
            &route_parents,
            &mut candidates,
        )
        .await
        .expect_err("a nested media field with no link column must fail closed");

        let Error::Config(msg) = err else {
            panic!("expected Error::Config, got {err:?}");
        };
        assert!(msg.contains("ContactType"), "{msg}");
        assert!(msg.contains("core"), "{msg}");
        assert!(
            msg.contains("parent_ref"),
            "the diagnostic names the fix: {msg}"
        );
        assert!(msg.contains("IDOR"), "{msg}");
    }

    /// A ROOT media field carries no link column — the resolver is only
    /// consulted for a nested route (and root output must stay byte-identical).
    #[tokio::test]
    async fn root_media_field_has_no_link_column() {
        let config = parse_domain_config_str(
            "[defaults]\n\
             type_suffix = \"Type\"\n\n\
             [domains.core]\n\
             label = \"Core\"\n\
             schema_dir = \"core\"\n\
             postgres_schema = \"core\"\n\n\
             [domains.core.media]\n\
             provider = \"supabase\"\n\
             upload_mode = \"through-app\"\n\
             bucket = \"person-media\"\n\
             max_bytes = 100\n\
             accept = [\"image/png\"]\n",
        )
        .expect("domains.toml");
        let db = MockEngine::new();
        let mut candidates = None;
        let link = resolve_media_parent_link_column(
            &db,
            &config,
            "core",
            "PersonType",
            &[],
            &mut candidates,
        )
        .await
        .expect("a root field needs no link column");
        assert!(link.is_none());
    }

    /// Issue #722 review: `router.tera` nests recursively, but the emitted
    /// extractor (like the CRUD plane's `ancestor_path_params`) binds at most
    /// three ids. A depth-3 media route would carry four url params and the
    /// handler could never serve it (axum rejects an arity mismatch with 400),
    /// so it must fail at generation time — never as an advertised dead route.
    #[tokio::test]
    async fn depth_three_media_nesting_is_config_error() {
        let config = parse_domain_config_str(
            "[defaults]\n\
             type_suffix = \"Type\"\n\n\
             [domains.core]\n\
             label = \"Core\"\n\
             schema_dir = \"core\"\n\
             postgres_schema = \"core\"\n\n\
             [domains.core.entity_config.AddressType]\n\
             role = \"child\"\n\
             parent = \"ContactType\"\n\
             parent_ref = \"contact_id\"\n\n\
             [domains.core.media]\n\
             provider = \"supabase\"\n\
             upload_mode = \"through-app\"\n\
             bucket = \"person-media\"\n\
             max_bytes = 100\n\
             accept = [\"image/png\"]\n",
        )
        .expect("domains.toml");
        let db = MockEngine::new();
        let route_parents = [
            ("CountryType", "countries", "country_id"),
            ("PersonType", "persons", "person_id"),
            ("ContactType", "contacts", "contact_id"),
        ]
        .into_iter()
        .map(|(title, segment, param)| MediaRouteParent {
            entity_title: title.into(),
            path_segment: segment.into(),
            param_name: param.into(),
        })
        .collect::<Vec<_>>();
        let mut candidates = None;
        let err = resolve_media_parent_link_column(
            &db,
            &config,
            "core",
            "AddressType",
            &route_parents,
            &mut candidates,
        )
        .await
        .expect_err("depth 3 cannot be served by the emitted extractor");

        let Error::Config(msg) = err else {
            panic!("expected Error::Config, got {err:?}");
        };
        assert!(msg.contains("AddressType"), "{msg}");
        assert!(msg.contains("3 levels deep"), "{msg}");
        assert!(msg.contains("grandparent"), "{msg}");
        assert!(
            msg.contains("CountryType > PersonType > ContactType"),
            "the diagnostic names the chain: {msg}"
        );
    }
}
