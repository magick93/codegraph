//! Issue #625 PR-A — the media plan builder: `[domains.X.media]` + graph →
//! `MediaPlan` resolution, the provider capability matrix, and the strict
//! validation contract (unresolved classes, refers-wins targets, array
//! media fields, and missing columns are config errors naming the
//! offender).
//!
//! Node-free PR-CI pins: in-memory Grafeo — no node, no DB, no protoc.

use codegraph_config::DomainConfig;
use codegraph_config::config::parse_domain_config_str;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{EdgeType, MediaRouteParent, PropertyNode, SchemaNode};
use codegraph_generate::error::Error;
use codegraph_generate::media::build_media_plans;
use codegraph_grafeo::GrafeoEngine;
use codegraph_type_contracts::RefClassificationKind;

// ── fixture ─────────────────────────────────────────────────────────────

const BASE_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]
type_suffix = "Type"

[domains.core]
label = "Core"
schema_dir = "core"
postgres_schema = "core"
"#;

fn media_toml(provider: &str, mode: &str) -> String {
    format!(
        "{BASE_TOML}\n[domains.core.media]\nprovider = \"{provider}\"\nupload_mode = \"{mode}\"\nbucket = \"person-media\"\nmax_bytes = 5242880\naccept = [\"image/png\", \"image/jpeg\"]\n"
    )
}

fn person_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "PersonType".into(),
        title: "PersonType".into(),
        description: None,
        schema_type: "object".into(),
        classification: "entity".into(),
        domain: Some("core".into()),
        namespace: None,
        rel_path: "core/person.json".into(),
        pg_type: "uuid".into(),
        rust_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        rust_type_name: "Person".into(),
        pg_table_name: "person".into(),
        api_path_segment: "person".into(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    }
}

fn media_reference_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "MediaReferenceType".into(),
        title: "MediaReferenceType".into(),
        description: None,
        schema_type: "object".into(),
        classification: "value_object".into(),
        domain: Some("core".into()),
        namespace: None,
        rel_path: "core/media_reference.json".into(),
        pg_type: "text".into(),
        rust_type: "String".into(),
        sea_orm_type: "String".into(),
        rust_type_name: "MediaReference".into(),
        pg_table_name: "media_reference".into(),
        api_path_segment: "media-reference".into(),
        parent_schema: None,
        is_entity: false,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    }
}

fn person_name_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "PersonNameType".into(),
        title: "PersonNameType".into(),
        description: None,
        schema_type: "object".into(),
        classification: "value_object".into(),
        domain: Some("core".into()),
        namespace: None,
        rel_path: "core/person_name.json".into(),
        pg_type: "text".into(),
        rust_type: "String".into(),
        sea_orm_type: "String".into(),
        rust_type_name: "PersonName".into(),
        pg_table_name: "person_name".into(),
        api_path_segment: "person-name".into(),
        parent_schema: None,
        is_entity: false,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    }
}

fn prop(
    name: &str,
    kind: RefClassificationKind,
    ref_target: Option<&str>,
    is_array: bool,
) -> PropertyNode {
    PropertyNode {
        name: name.into(),
        prop_type: if ref_target.is_some() {
            "object".into()
        } else {
            "string".into()
        },
        description: None,
        format: None,
        is_required: false,
        is_nullable: true,
        is_id: name == "id",
        is_array,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: codegraph_naming::to_snake_case(name),
        pg_column_type: "TEXT".into(),
        rust_field_name: codegraph_naming::to_snake_case(name),
        rust_field_type: "String".into(),
        sea_orm_type: "String".into(),
        render_strategy: "flat".into(),
        ref_target: ref_target.map(|t| t.to_string()),
        classification: None,
        classification_kind: Some(kind),
        projection: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    }
}

async fn ingest_model(db: &(impl GraphIngestor + GraphQuerier)) {
    db.ingest_schema(&person_schema())
        .await
        .expect("person schema");
    db.ingest_schema(&media_reference_schema())
        .await
        .expect("media reference schema");
    db.ingest_schema(&person_name_schema())
        .await
        .expect("person name schema");
    // Person: id, displayName (plain), avatar (VO → MediaReferenceType),
    // names (VO → PersonNameType, NOT media — the negative control).
    for p in [
        prop("id", RefClassificationKind::EntityReference, None, false),
        prop(
            "displayName",
            RefClassificationKind::EntityReference,
            None,
            false,
        ),
        prop(
            "avatar",
            RefClassificationKind::ValueObject,
            Some("MediaReferenceType"),
            false,
        ),
        prop(
            "names",
            RefClassificationKind::ValueObject,
            Some("PersonNameType"),
            true,
        ),
    ] {
        db.ingest_property("PersonType", "PersonType", &p)
            .await
            .expect("person property");
    }
    // MediaReference: url + mimeType (the HR Open reference shape).
    for p in [
        prop("url", RefClassificationKind::EntityReference, None, false),
        prop(
            "mimeType",
            RefClassificationKind::EntityReference,
            None,
            false,
        ),
    ] {
        db.ingest_property("MediaReferenceType", "MediaReferenceType", &p)
            .await
            .expect("media reference property");
    }
    // PersonName: given/family (no url column — the missing-column case's
    // target when media_classes is overridden).
    for p in [
        prop(
            "givenName",
            RefClassificationKind::EntityReference,
            None,
            false,
        ),
        prop(
            "familyName",
            RefClassificationKind::EntityReference,
            None,
            false,
        ),
    ] {
        db.ingest_property("PersonNameType", "PersonNameType", &p)
            .await
            .expect("person name property");
    }
    // Ref edges: get_property_ref_target resolves through
    // `(:Property)-[:ReferencesSchema]->(:Schema)` — the property node's
    // ref_target field alone does not resolve.
    for (from, to) in [
        ("avatar::PersonType", "MediaReferenceType"),
        ("names::PersonType", "PersonNameType"),
    ] {
        db.ingest_edge(from, to, EdgeType::ReferencesSchema, None)
            .await
            .expect("references edge");
    }
}

/// The array-media fixture's item edge: `get_array_item_schema` resolves
/// through `[:ItemsOf]`.
async fn ingest_gallery_items_edge(db: &(impl GraphIngestor + GraphQuerier)) {
    db.ingest_edge(
        "gallery::PersonType",
        "MediaReferenceType",
        EdgeType::ItemsOf,
        None,
    )
    .await
    .expect("items edge");
}

fn parse(toml: &str) -> DomainConfig {
    parse_domain_config_str(toml).expect("domains.toml")
}

// ── resolution ──────────────────────────────────────────────────────────

#[tokio::test]
async fn plan_resolves_child_table_and_columns() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse(&media_toml("supabase", "client-direct"));

    let plans = build_media_plans(&engine, &config).await.expect("plans");
    let plan = plans.get("core").expect("core plan");
    assert_eq!(plan.domain, "core");
    assert_eq!(
        plan.provider,
        codegraph_core::types::MediaProvider::Supabase
    );
    assert_eq!(
        plan.upload_mode,
        codegraph_core::types::UploadMode::ClientDirect
    );
    assert_eq!(
        plan.visibility,
        codegraph_core::types::MediaVisibility::Signed
    );
    assert_eq!(plan.media_classes, vec!["MediaReferenceType".to_string()]);
    assert_eq!(
        plan.fields.len(),
        1,
        "names is not media: {:?}",
        plan.fields
    );
    let f = &plan.fields[0];
    assert_eq!(f.entity_title, "PersonType");
    assert_eq!(f.entity_table, "person");
    assert_eq!(f.field_name, "avatar");
    assert_eq!(f.field_snake, "avatar");
    assert_eq!(f.child_table, "person_avatar");
    assert_eq!(f.parent_fk_column, "person_id");
    assert_eq!(f.url_column, "url");
    assert_eq!(f.mime_column, "mime_type");
    // Issue #690: the owning table's tenancy rides the DDL's own rule
    // (`is_global_entity`), never a property scan — mox entity schemas never
    // declare `platform_organization_id` (the DDL injects it), so a property
    // scan would return None for every real model and silently emit
    // unscoped SQL.
    assert_eq!(
        f.parent_tenant_column,
        Some("platform_organization_id".to_string()),
        "a tenant-scoped parent must carry the tenant column"
    );
}

/// Issue #699: a soft-deleted parent must not serve or destroy its media.
/// The `deleted_at` column exists exactly when the table DDL calls the entity
/// auditable — the domain's `auditable` flag (default ON) and NOT
/// append-only (`db/ddl/query.rs`) — so the plan carries that ONE decision
/// and the emitted gate filters on it only when the column exists. Mutating
/// the resolver to a constant `true` fails the append-only leg; a constant
/// `false` fails the default leg; ignoring `auditable` fails the middle leg.
#[tokio::test]
async fn parent_soft_delete_follows_the_ddl_auditable_decision() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;

    // Default: `auditable` defaults ON in the DDL, so the column exists.
    let config = parse(&media_toml("supabase", "client-direct"));
    let plans = build_media_plans(&engine, &config).await.expect("plans");
    assert!(
        plans
            .get("core")
            .expect("core plan")
            .fields
            .first()
            .expect("one media field")
            .parent_soft_delete,
        "an auditable parent carries `deleted_at`, so the gate must filter it"
    );

    // `auditable = false`: the DDL emits NO `deleted_at`, so the gate must
    // not reference it (the reference would 500 every media request).
    let not_auditable = media_toml("supabase", "client-direct")
        .replace("[domains.core]\n", "[domains.core]\nauditable = false\n");
    assert_ne!(not_auditable, media_toml("supabase", "client-direct"));
    let config = parse(&not_auditable);
    let plans = build_media_plans(&engine, &config).await.expect("plans");
    assert!(
        !plans
            .get("core")
            .expect("core plan")
            .fields
            .first()
            .expect("one media field")
            .parent_soft_delete,
        "a non-auditable parent has no deleted_at column"
    );

    // Append-only — the `operations` inference the DDL uses (no update AND
    // no delete): the row is never soft-deleted, and the DDL drops the
    // audit block, so there is nothing to filter on.
    let append_only = media_toml("supabase", "client-direct").replace(
        r#"operations = ["create", "read", "update", "delete", "list"]"#,
        r#"operations = ["create", "read", "list"]"#,
    );
    assert_ne!(append_only, media_toml("supabase", "client-direct"));
    let config = parse(&append_only);
    let plans = build_media_plans(&engine, &config).await.expect("plans");
    assert!(
        !plans
            .get("core")
            .expect("core plan")
            .fields
            .first()
            .expect("one media field")
            .parent_soft_delete,
        "an append-only parent is never soft-deleted"
    );
}

#[tokio::test]
async fn plans_are_deterministic_across_runs() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse(&media_toml("supabase", "client-direct"));
    let a = build_media_plans(&engine, &config).await.expect("plans a");
    let b = build_media_plans(&engine, &config).await.expect("plans b");
    assert_eq!(a, b);
}

#[tokio::test]
async fn no_media_config_builds_no_plans() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse(BASE_TOML);
    let plans = build_media_plans(&engine, &config).await.expect("plans");
    assert!(plans.is_empty());
}

#[tokio::test]
async fn bare_media_class_resolves_via_type_suffix() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse(&media_toml("supabase", "client-direct"));
    let plans = build_media_plans(&engine, &config).await.expect("plans");
    assert_eq!(
        plans["core"].media_classes,
        vec!["MediaReferenceType".to_string()],
        "MediaReference resolves to MediaReferenceType via the Type suffix"
    );
}

// ── #713: the route path is carried on the plan ─────────────────────────

fn contact_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "ContactType".into(),
        title: "ContactType".into(),
        description: None,
        schema_type: "object".into(),
        classification: "entity".into(),
        domain: Some("core".into()),
        namespace: None,
        rel_path: "core/contact.json".into(),
        pg_type: "uuid".into(),
        rust_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        rust_type_name: "Contact".into(),
        pg_table_name: "contact".into(),
        api_path_segment: "contact".into(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    }
}

/// A ROOT (`PersonType`) + a CHILD (`ContactType`, title sorting FIRST) with
/// a media field on each — the #713 shape.
async fn ingest_parent_and_child(db: &(impl GraphIngestor + GraphQuerier)) {
    ingest_model(db).await;
    db.ingest_schema(&contact_schema())
        .await
        .expect("contact schema");
    for p in [
        prop("id", RefClassificationKind::EntityReference, None, false),
        prop(
            "avatar",
            RefClassificationKind::ValueObject,
            Some("MediaReferenceType"),
            false,
        ),
    ] {
        db.ingest_property("ContactType", "ContactType", &p)
            .await
            .expect("contact property");
    }
    db.ingest_edge(
        "avatar::ContactType",
        "MediaReferenceType",
        EdgeType::ReferencesSchema,
        None,
    )
    .await
    .expect("contact references edge");
}

#[tokio::test]
async fn media_fields_carry_the_route_path_they_are_mounted_under() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_parent_and_child(&engine).await;
    let config = parse(&format!(
        "{}\n[domains.core.entity_config.PersonType]\npath_segment = \"persons\"\n\n\
         [domains.core.entity_config.ContactType]\nrole = \"child\"\nparent = \"PersonType\"\n\
         parent_ref = \"person_id\"\npath_segment = \"contacts\"\n",
        media_toml("supabase", "through-app")
    ));
    let plans = build_media_plans(&engine, &config).await.expect("plans");
    let fields = &plans["core"].fields;
    assert_eq!(fields.len(), 2, "{fields:?}");

    // `ContactType` sorts before `PersonType` — the contract anchors here.
    let child = &fields[0];
    assert_eq!(child.entity_title, "ContactType");
    assert_eq!(child.entity_path_segment, "contacts");
    assert_eq!(
        child.route_parents,
        vec![MediaRouteParent {
            entity_title: "PersonType".into(),
            path_segment: "persons".into(),
            param_name: "person_id".into(),
        }],
        "the child's hop is the router's `.nest` shape"
    );
    assert_eq!(
        child.route_prefix("v2", "core"),
        "/api/v2/core/persons/{person_id}/contacts"
    );
    assert_eq!(
        child.route_prefix_with_ids("v2", "core", "abc"),
        "/api/v2/core/persons/abc/contacts"
    );
    assert_eq!(
        child.parent_link_column.as_deref(),
        Some("person_id"),
        "issue #722: the nested route's ownership predicate column, from config `parent_ref`"
    );

    let root = &fields[1];
    assert_eq!(root.entity_title, "PersonType");
    assert_eq!(root.entity_path_segment, "persons");
    assert!(root.route_parents.is_empty(), "a root has no parent hops");
    assert_eq!(root.route_prefix("v2", "core"), "/api/v2/core/persons");
    assert!(
        root.parent_link_column.is_none(),
        "a root media field carries no ownership link column"
    );
}

/// A modeled scalar ref from the media-owning child to a candidate parent —
/// the router's Source-2 trigger
/// (`(child:Schema)-[:HasProperty]->(p)-[:ReferencesSchema]->(parent)`).
async fn ingest_contact_ref_candidate(
    db: &(impl GraphIngestor + GraphQuerier),
    property: &str,
    parent_title: &str,
) {
    let p = prop(
        property,
        RefClassificationKind::EntityReference,
        Some(parent_title),
        false,
    );
    db.ingest_property("ContactType", "ContactType", &p)
        .await
        .expect("candidate property");
    db.ingest_edge(
        &format!("{property}::ContactType"),
        parent_title,
        EdgeType::ReferencesSchema,
        None,
    )
    .await
    .expect("candidate references edge");
}

/// A same-domain schema with NO table and no routes — a parent the router can
/// never nest under.
fn billing_addr_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "BillingAddrType".into(),
        title: "BillingAddrType".into(),
        description: None,
        schema_type: "object".into(),
        classification: "value_object".into(),
        domain: Some("core".into()),
        namespace: None,
        rel_path: "core/billing_addr.json".into(),
        pg_type: "text".into(),
        rust_type: "String".into(),
        sea_orm_type: "String".into(),
        rust_type_name: "BillingAddr".into(),
        pg_table_name: "billing_addr".into(),
        api_path_segment: "billing_addr".into(),
        parent_schema: None,
        is_entity: false,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    }
}

/// An entity in ANOTHER domain — a candidate the router skips (its
/// `title_to_entity_idx` only holds this domain's generated entities).
fn foreign_entity_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "AaaType".into(),
        title: "AaaType".into(),
        description: None,
        schema_type: "object".into(),
        classification: "entity".into(),
        domain: Some("other".into()),
        namespace: None,
        rel_path: "other/aaa.json".into(),
        pg_type: "uuid".into(),
        rust_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        rust_type_name: "Aaa".into(),
        pg_table_name: "aaa".into(),
        api_path_segment: "aaa".into(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    }
}

fn nested_child_config(extra: &str) -> DomainConfig {
    // The parent's config-free segment is its `api_path_segment` ("person"),
    // and the param derives from it (`person_id`).
    parse(&format!(
        "{}\n[domains.core.entity_config.ContactType]\npath_segment = \"contacts\"\n{extra}",
        media_toml("supabase", "through-app")
    ))
}

#[tokio::test]
async fn graph_parent_candidate_is_the_nesting_fallback() {
    // The router's Source 2: `role = "child"` with no configured `parent`
    // resolves the anchor from a modeled scalar ref to the parent entity, so
    // the media path must honour it rather than emit a flat route.
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_parent_and_child(&engine).await;
    ingest_contact_ref_candidate(&engine, "person", "PersonType").await;

    let config = nested_child_config("role = \"child\"\nparent_ref = \"person_id\"\n");
    let plans = build_media_plans(&engine, &config).await.expect("plans");
    let child = plans["core"]
        .fields
        .iter()
        .find(|f| f.entity_title == "ContactType")
        .expect("the child's media field");
    assert_eq!(
        child.route_parents,
        vec![MediaRouteParent {
            entity_title: "PersonType".into(),
            path_segment: "person".into(),
            param_name: "person_id".into(),
        }]
    );
}

#[tokio::test]
async fn unresolvable_config_parent_falls_through_to_the_graph_candidate() {
    // B1: a `parent` naming a class that no longer exists is the realistic
    // upgrade shape. The router's Source 1 requires the parent in
    // `title_to_entity_idx` and FALLS THROUGH to Source 2 when it is not —
    // so the media plan must too, instead of aborting the whole run.
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_parent_and_child(&engine).await;
    ingest_contact_ref_candidate(&engine, "person", "PersonType").await;

    let config = nested_child_config(
        "role = \"child\"\nparent = \"GhostType\"\nparent_ref = \"person_id\"\n",
    );
    let plans = build_media_plans(&engine, &config)
        .await
        .expect("a stale configured parent must fall through to the graph candidate");
    let child = plans["core"]
        .fields
        .iter()
        .find(|f| f.entity_title == "ContactType")
        .expect("the child's media field");
    assert_eq!(
        child.route_parents,
        vec![MediaRouteParent {
            entity_title: "PersonType".into(),
            path_segment: "person".into(),
            param_name: "person_id".into(),
        }],
        "the graph candidate is the anchor the router mounts"
    );
}

#[tokio::test]
async fn a_non_routable_parent_never_yields_a_nested_path() {
    // B3: `parent` naming a value object (no table, no routes) is not
    // mountable — the router nests nothing for the child, so the plan must
    // fail closed rather than document `/billing_addr/{id}/contacts/...`.
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_parent_and_child(&engine).await;
    engine
        .ingest_schema(&billing_addr_schema())
        .await
        .expect("billing addr schema");

    let config = nested_child_config(
        "role = \"child\"\nparent = \"BillingAddrType\"\nparent_ref = \"person_id\"\n",
    );
    let err = match build_media_plans(&engine, &config).await {
        Err(Error::Config(msg)) => msg,
        Err(other) => panic!("expected Error::Config, got {other:?}"),
        Ok(plans) => panic!(
            "a non-routable parent must not yield a nested path: {:?}",
            plans["core"].fields
        ),
    };
    assert!(err.contains("BillingAddrType"), "{err}");
    assert!(err.contains("not a routable entity"), "{err}");
    assert!(err.contains("ContactType"), "{err}");
}

#[tokio::test]
async fn graph_parent_candidates_skip_unmountable_parents() {
    // The router iterates candidates IN ORDER and skips the ones it cannot
    // mount. `AaaType` (another domain) sorts FIRST here — the exact shape
    // that used to win and abort the run.
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_parent_and_child(&engine).await;
    engine
        .ingest_schema(&foreign_entity_schema())
        .await
        .expect("foreign entity schema");
    ingest_contact_ref_candidate(&engine, "aaa", "AaaType").await;
    ingest_contact_ref_candidate(&engine, "person", "PersonType").await;

    // Precondition: the unmountable candidate really is tried first.
    let candidates = engine.get_parent_candidates().await.expect("candidates");
    let contact_candidates: Vec<&str> = candidates
        .iter()
        .filter(|c| c.child_title == "ContactType")
        .map(|c| c.parent_title.as_str())
        .collect();
    assert_eq!(
        contact_candidates,
        vec!["AaaType", "PersonType"],
        "candidate order is (child, parent, field)"
    );

    let config = nested_child_config("role = \"child\"\nparent_ref = \"person_id\"\n");
    let plans = build_media_plans(&engine, &config).await.expect("plans");
    let child = plans["core"]
        .fields
        .iter()
        .find(|f| f.entity_title == "ContactType")
        .expect("the child's media field");
    assert_eq!(
        child.route_parents,
        vec![MediaRouteParent {
            entity_title: "PersonType".into(),
            path_segment: "person".into(),
            param_name: "person_id".into(),
        }],
        "the first MOUNTABLE candidate wins, as in the router"
    );
}

#[tokio::test]
async fn child_media_field_without_a_parent_is_config_error() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_parent_and_child(&engine).await;
    // `role = "child"` with neither a configured `parent` nor a graph parent
    // candidate: the route is unmountable, so a flat contract would be a lie.
    let config = nested_child_config("role = \"child\"\n");
    let err = match build_media_plans(&engine, &config).await {
        Err(Error::Config(msg)) => msg,
        other => panic!("expected Error::Config, got {other:?}"),
    };
    assert!(err.contains("ContactType"), "{err}");
    assert!(err.contains("parent"), "{err}");
    assert!(err.contains("child"), "{err}");
}

// ── validation contract ─────────────────────────────────────────────────

#[tokio::test]
async fn unresolved_media_class_is_config_error_naming_it() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse(&media_toml("supabase", "client-direct").replace(
        "accept = [\"image/png\", \"image/jpeg\"]",
        "accept = [\"image/png\"]\nmedia_classes = [\"DoesNotExist\"]",
    ));
    let err = match build_media_plans(&engine, &config).await {
        Err(Error::Config(msg)) => msg,
        other => panic!("expected Error::Config, got {other:?}"),
    };
    assert!(err.contains("DoesNotExist"), "{err}");
    assert!(err.contains("core"), "{err}");
}

#[tokio::test]
async fn refers_wins_media_target_is_config_error() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    // The avatar property lowered as an entity reference (refers wins).
    db_refers_avatar(&engine).await;
    let config = parse(&media_toml("supabase", "client-direct"));
    let err = match build_media_plans(&engine, &config).await {
        Err(Error::Config(msg)) => msg,
        other => panic!("expected Error::Config, got {other:?}"),
    };
    assert!(err.contains("PersonType"), "{err}");
    assert!(err.contains("avatar"), "{err}");
}

async fn db_refers_avatar(db: &(impl GraphIngestor + GraphQuerier)) {
    let p = prop(
        "avatar",
        RefClassificationKind::EntityReference,
        Some("MediaReferenceType"),
        false,
    );
    db.ingest_property("PersonType", "PersonType", &p)
        .await
        .expect("avatar re-ingest");
}

#[tokio::test]
async fn array_media_field_is_config_error() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let p = prop(
        "gallery",
        RefClassificationKind::ValueObject,
        Some("MediaReferenceType"),
        true,
    );
    db_ingest_prop(&engine, "PersonType", &p).await;
    ingest_gallery_items_edge(&engine).await;
    let config = parse(&media_toml("supabase", "client-direct"));
    let err = match build_media_plans(&engine, &config).await {
        Err(Error::Config(msg)) => msg,
        other => panic!("expected Error::Config, got {other:?}"),
    };
    assert!(err.contains("gallery"), "{err}");
}

async fn db_ingest_prop(db: &(impl GraphIngestor + GraphQuerier), title: &str, p: &PropertyNode) {
    db.ingest_property(title, title, p).await.expect("property");
}

#[tokio::test]
async fn missing_mime_column_is_config_error_naming_class() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    // Point mime_field at a column the class lacks.
    let config = parse(&media_toml("supabase", "client-direct").replace(
        "accept = [\"image/png\", \"image/jpeg\"]",
        "accept = [\"image/png\"]\nmime_field = \"mime_kind\"",
    ));
    let err = match build_media_plans(&engine, &config).await {
        Err(Error::Config(msg)) => msg,
        other => panic!("expected Error::Config, got {other:?}"),
    };
    assert!(err.contains("MediaReferenceType"), "{err}");
    assert!(err.contains("mime_kind"), "{err}");
}

// ── the capability matrix (generation-side) ─────────────────────────────

#[tokio::test]
async fn r2_client_direct_is_matrix_error_naming_capability() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse(&format!(
        "{BASE_TOML}\n[domains.core.media]\nprovider = \"r2\"\nupload_mode = \"client-direct\"\nbucket = \"b\"\nmax_bytes = 1000\naccept = [\"image/png\"]\n\n[domains.core.media.r2]\nbinding = \"MEDIA\"\n"
    ));
    let err = match build_media_plans(&engine, &config).await {
        Err(Error::Config(msg)) => msg,
        other => panic!("expected Error::Config, got {other:?}"),
    };
    assert!(err.contains("r2"), "{err}");
    assert!(err.contains("client-direct"), "{err}");
}

#[tokio::test]
async fn bad_key_template_is_config_error_naming_template() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse(&media_toml("supabase", "client-direct").replace(
        "accept = [\"image/png\", \"image/jpeg\"]",
        "accept = [\"image/png\"]\nkey_template = \"{org}/{bucket}\"",
    ));
    let err = match build_media_plans(&engine, &config).await {
        Err(Error::Config(msg)) => msg,
        other => panic!("expected Error::Config, got {other:?}"),
    };
    assert!(err.contains("bucket"), "{err}");
}

// ── PR-B: one provider per app (issue #625) ─────────────────────────────

#[tokio::test]
async fn mixed_providers_across_domains_are_config_error_naming_both() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    // Domain `core` is supabase (the BASE_TOML fixture in this file), a
    // second domain declares r2.
    let config = parse(&format!(
        "{}\n[domains.billing]\nlabel = \"Billing\"\nschema_dir = \"billing\"\npostgres_schema = \"billing\"\n\n[domains.billing.media]\nprovider = \"r2\"\nupload_mode = \"through-app\"\nbucket = \"billing-media\"\nmax_bytes = 1000\naccept = [\"image/png\"]\n\n[domains.billing.media.r2]\nbinding = \"MEDIA\"\n",
        media_toml("supabase", "client-direct")
    ));
    let err = match build_media_plans(&engine, &config).await {
        Err(Error::Config(msg)) => msg,
        other => panic!("expected Error::Config, got {other:?}"),
    };
    assert!(err.contains("core"), "{err}");
    assert!(err.contains("billing"), "{err}");
    assert!(err.contains("supabase"), "{err}");
    assert!(err.contains("r2"), "{err}");
    assert!(err.contains("one provider per app"), "{err}");
}
