use crate::api;

/// Returns the lowercased PG cast string for range/geometry types, or `None` for standard types.
/// Used by entity and repository generators to emit explicit casts (e.g. `$N::tstzrange`,
/// `column_type = "custom(\"geometry\")"` with `select_as = "text"`).
pub fn pg_cast_for_type(pg_column_type: &str) -> Option<String> {
    let upper = pg_column_type.to_uppercase();
    if upper.starts_with("GEOMETRY") || upper.starts_with("GEOGRAPHY") {
        return Some("geometry".to_string());
    }
    match upper.as_str() {
        "TSTZRANGE" | "DATERANGE" | "INT4RANGE" | "INT8RANGE" => {
            Some(pg_column_type.to_lowercase())
        }
        _ => None,
    }
}

/// Returns true if the pg_cast value represents a geometry/geography type
/// that needs ST_AsGeoJSON/ST_GeomFromGeoJSON in queries.
pub fn is_geometry_cast(cast: &str) -> bool {
    cast == "geometry"
}

/// Compute the FK column name for a child entity given a `ParentCandidate`.
///
/// This is the single source of truth for FK naming; entity, DDL, repository,
/// command, query, handler, and router generators must all call this helper
/// (or the bulk wrapper `resolve_parent_fk_column`) to stay in sync.
pub fn fk_column_for_candidate(
    pc: &codegraph_core::types::ParentCandidate,
    suffix: &str,
) -> String {
    let parent_name = api::router::strip_suffix(&pc.parent_title, suffix);
    match pc.source {
        codegraph_core::types::DetectionSource::ArrayItems => {
            codegraph_naming::to_snake_case(parent_name) + "_id"
        }
        // ScalarRef: the FK column already exists on the child — it is the
        // child's own reference property (e.g. `tenantId` → `tenant_id`).
        // Apply ensure_id_suffix instead of unconditionally appending `_id`
        // so Id-suffixed schema properties don't produce `tenant_id_id`.
        _ => codegraph_core::types::ensure_id_suffix(&codegraph_naming::to_snake_case(
            &pc.field_name,
        )),
    }
}

/// Find the first `ParentCandidate` whose child matches `schema_title` and
/// return the FK column name.  Falls back to `entity_cfg.parent_ref` if set.
///
/// This is the sync version — it does NOT check whether the parent is in the
/// same domain.  Use it for DB-level generators (entity, DDL) where the FK
/// column should exist regardless of routing/nesting.
pub fn resolve_parent_fk_column(
    schema_title: &str,
    parent_candidates: &[codegraph_core::types::ParentCandidate],
    entity_cfg: Option<&codegraph_config::EntityConfig>,
    suffix: &str,
) -> Option<String> {
    // 1. Manual config always wins
    if let Some(fk) = entity_cfg.and_then(|ec| ec.parent_ref.clone()) {
        return Some(fk);
    }
    // If manual config says role=child with a parent, derive FK from parent name
    if let Some(ec) = entity_cfg {
        if ec.role.as_deref() == Some("child") {
            if let Some(ref parent_title) = ec.parent {
                let parent_name = api::router::strip_suffix(parent_title, suffix);
                return Some(format!(
                    "{}_id",
                    codegraph_naming::to_snake_case(parent_name)
                ));
            }
        }
    }
    // 2. Graph fallback (no domain check — FK column always needed)
    let stripped = api::router::strip_suffix(schema_title, suffix);
    parent_candidates.iter().find_map(|pc| {
        let child_name = api::router::strip_suffix(&pc.child_title, suffix);
        if child_name == stripped {
            Some(fk_column_for_candidate(pc, suffix))
        } else {
            None
        }
    })
}

/// Async version of [`resolve_parent_fk_column`] that checks whether the
/// parent is in the same domain before treating the entity as a child.
/// Use this for API-level generators (handler, command, query, repository)
/// where `parent_ref` drives route nesting and parent_id parameters.
pub async fn resolve_parent_fk_column_same_domain(
    schema_title: &str,
    parent_candidates: &[codegraph_core::types::ParentCandidate],
    entity_cfg: Option<&codegraph_config::EntityConfig>,
    domain: &str,
    config: &codegraph_config::DomainConfig,
    db: &dyn codegraph_core::traits::GraphQuerier,
) -> Option<String> {
    // 1. Manual config always wins
    if let Some(fk) = entity_cfg.and_then(|ec| ec.parent_ref.clone()) {
        return Some(fk);
    }
    if let Some(ec) = entity_cfg {
        if ec.role.as_deref() == Some("child") {
            if let Some(ref parent_title) = ec.parent {
                let parent_name =
                    api::router::strip_suffix(parent_title, &config.defaults.type_suffix);
                return Some(format!(
                    "{}_id",
                    codegraph_naming::to_snake_case(parent_name)
                ));
            }
        }
    }
    // 2. Graph fallback with same-domain check (only for non-root entities)
    let effective_role = entity_cfg
        .and_then(|ec| ec.role.as_deref())
        .unwrap_or("root");
    if effective_role == "root" {
        return None;
    }
    let stripped = api::router::strip_suffix(schema_title, &config.defaults.type_suffix);
    for pc in parent_candidates {
        let child_name = api::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix);
        if child_name == stripped {
            // Check if parent is in same domain: explicitly listed OR schema domain matches
            let in_explicit_list = config
                .domains
                .get(domain)
                .map(|d| d.entities.contains(&pc.parent_title))
                .unwrap_or(false);
            let in_same_domain = in_explicit_list
                || db
                    .get_schema_in_domain(&pc.parent_title, domain)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|s| s.domain.as_ref().map(|d| d == domain))
                    .unwrap_or(false);
            if in_same_domain {
                return Some(fk_column_for_candidate(pc, &config.defaults.type_suffix));
            }
            return None; // Parent in different domain — no parent_ref for nesting
        }
    }
    None
}
