use crate::api::include_path::{IncludeSegment, ResolvedIncludePath};
use crate::code_writer::CodeWriter;

use super::query_search::emit_sort_ordering;
use super::*;

fn make_column(
    field: &str,
    rust_type: &str,
    nullable: bool,
    dto_rust_type: Option<&str>,
    is_array: bool,
    is_structured_wrapper: bool,
) -> TreeColumn {
    TreeColumn {
        field_name: field.to_string(),
        pg_column_name: field.to_string(),
        dto_field_name: None,
        rust_type: rust_type.to_string(),
        is_nullable: nullable,
        is_entity_ref: false,
        dto_rust_type: dto_rust_type.map(|s| s.to_string()),
        is_workflow_managed: false,
        is_array,
        pg_cast: None,
        is_composite_range: false,
        is_structured_wrapper,
        is_media: false,
    }
}

// --- emit_entity_to_dto_field tests ---

#[test]
fn entity_to_dto_plain_required() {
    let col = make_column("given_name", "String", false, None, false, false);
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert_eq!(code.as_str(), "    given_name: row.given_name,\n");
}

#[test]
fn entity_to_dto_codelist_required() {
    let col = make_column(
        "gender_code",
        "String",
        false,
        Some("GenderCodeList"),
        false,
        false,
    );
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert!(code.as_str().contains(".parse().unwrap_or_default()"));
}

#[test]
fn entity_to_dto_codelist_nullable() {
    let col = make_column(
        "currency_code",
        "String",
        true,
        Some("CurrencyCodeList"),
        false,
        false,
    );
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert!(code.as_str().contains(".and_then(|v| v.parse().ok())"));
}

#[test]
fn entity_to_dto_codelist_array_required() {
    let col = make_column(
        "codes",
        "Vec<String>",
        false,
        Some("StatusEnum"),
        true,
        false,
    );
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert!(code.as_str().contains("filter_map"));
    assert!(!code.as_str().contains(".map(|v|"));
}

#[test]
fn entity_to_dto_codelist_array_nullable() {
    let col = make_column(
        "codes",
        "Vec<String>",
        true,
        Some("StatusEnum"),
        true,
        false,
    );
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert!(code.as_str().contains(".map(|v| v.into_iter()"));
}

#[test]
fn entity_to_dto_jsonb_required() {
    let col = make_column("address", "serde_json::Value", false, None, false, true);
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert!(
        code.as_str()
            .contains("serde_json::from_value(row.address).unwrap_or_default()")
    );
}

#[test]
fn entity_to_dto_jsonb_nullable() {
    let col = make_column("metadata", "serde_json::Value", true, None, false, true);
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert!(
        code.as_str()
            .contains(".and_then(|v| serde_json::from_value(v).ok())")
    );
}

#[test]
fn entity_to_dto_jsonb_array_required() {
    let col = make_column("tags", "serde_json::Value", false, None, true, true);
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert!(
        code.as_str()
            .contains("serde_json::from_value(row.tags).unwrap_or_default()")
    );
}

#[test]
fn entity_to_dto_jsonb_array_nullable() {
    let col = make_column("prefs", "serde_json::Value", true, None, true, true);
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert!(
        code.as_str()
            .contains(".and_then(|v| serde_json::from_value(v).ok())")
    );
}

// --- issue #459: the MAIN hydrator adapts uuid↔String, mirroring the
// include-fetch emitter's `emit_field_assignments_typed` arms (tree.rs) ---

/// A Uuid entity column hydrating a String DTO field (the mox unformatted
/// datatype drift shape) must adapt with `.to_string()` — the old fallback
/// emitted `.parse()` (no such method on Uuid) or a bare E0308 copy.
#[test]
fn entity_to_dto_converts_uuid_column_to_string_dto() {
    let col = make_column("org_id", "Uuid", false, Some("String"), false, false);
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert_eq!(
        code.as_str(),
        "    org_id: row.org_id.to_string(),\n",
        "uuid column → String DTO must emit .to_string(). Got:\n{}",
        code.as_str()
    );
}

#[test]
fn entity_to_dto_converts_nullable_uuid_column_to_string_dto() {
    let col = make_column("org_id", "Uuid", true, Some("String"), false, false);
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert_eq!(
        code.as_str(),
        "    org_id: row.org_id.map(|v| v.to_string()),\n",
        "nullable uuid column → String DTO must emit .map(|v| v.to_string()). Got:\n{}",
        code.as_str()
    );
}

/// A String entity column hydrating a non-String DTO field keeps the
/// `.parse()` conversion the include-fetch emitter spells.
#[test]
fn entity_to_dto_parses_string_column_to_uuid_dto() {
    let col = make_column("duty_id", "String", false, Some("Uuid"), false, false);
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert_eq!(
        code.as_str(),
        "    duty_id: row.duty_id.parse().unwrap_or_default(),\n",
        "String column → non-String DTO must emit .parse().unwrap_or_default(). Got:\n{}",
        code.as_str()
    );
}

#[test]
fn entity_to_dto_parses_nullable_string_column_to_uuid_dto() {
    let col = make_column("duty_id", "String", true, Some("Uuid"), false, false);
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert_eq!(
        code.as_str(),
        "    duty_id: row.duty_id.and_then(|v| v.parse().ok()),\n",
        "nullable String column → non-String DTO must emit .and_then(parse). Got:\n{}",
        code.as_str()
    );
}

/// Matching types (the entity-ref shape: Uuid column, no DTO override)
/// keep the byte-identical plain assignment.
#[test]
fn entity_to_dto_keeps_plain_assignment_for_matching_types() {
    let col = make_column("duty_id", "Uuid", false, None, false, false);
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert_eq!(code.as_str(), "    duty_id: row.duty_id,\n");
    assert!(!code.as_str().contains(".to_string()"), "{}", code.as_str());
}

// --- emit_child_field_population tests ---

#[test]
fn child_population_array() {
    let children = vec![ChildTableInfo {
        field_name: "addresses".to_string(),
        struct_name: "CandidateAddress".to_string(),
        sql_table_name: "candidate_address".to_string(),
        sql_schema_name: "recruiting".to_string(),
        parent_fk_column: "candidate_id".to_string(),
        is_array: true,
        columns: vec![],
        child_tables: vec![],
        is_back_ref: false,
        is_media: false,
    }];
    let mut code = CodeWriter::new();
    emit_child_field_population(&mut code, &children, "    ");
    assert_eq!(code.as_str(), "    addresses: addresses_rows,\n");
}

#[test]
fn child_population_single() {
    let children = vec![ChildTableInfo {
        field_name: "profile".to_string(),
        struct_name: "CandidateProfile".to_string(),
        sql_table_name: "candidate_profile".to_string(),
        sql_schema_name: "recruiting".to_string(),
        parent_fk_column: "candidate_id".to_string(),
        is_array: false,
        columns: vec![],
        child_tables: vec![],
        is_back_ref: false,
        is_media: false,
    }];
    let mut code = CodeWriter::new();
    emit_child_field_population(&mut code, &children, "    ");
    assert_eq!(
        code.as_str(),
        "    profile: profile_rows.into_iter().next(),\n"
    );
}

#[test]
fn child_population_empty() {
    let mut code = CodeWriter::new();
    emit_child_field_population(&mut code, &[], "    ");
    assert_eq!(code.as_str(), "");
}

// --- emit_sort_ordering tests (issue #306) ---

fn sort_test_tree(is_auditable: bool) -> EntityTree {
    EntityTree {
        entity_name: "Task".into(),
        module_name: "task".into(),
        schema_name: "common".into(),
        table_name: "task".into(),
        entity_module: "common_task".into(),
        namespace: None,
        direct_columns: vec![],
        child_tables: vec![],
        junction_tables: vec![],
        has_create: false,
        has_read: true,
        has_update: false,
        has_delete: false,
        has_list: true,
        has_workflow: false,
        has_fts: false,
        has_embeddings: false,
        fts_language: "english".into(),
        is_auditable,
        soft_delete_visibility: "exclude_by_default".into(),
        soft_delete_column: None,
        soft_delete_cascade: "restrict".into(),
        track_updated_user: false,
        track_deleted_user: false,
        append_only: false,
        filter_fields: vec![],
        nested_filter_fields: vec![],
        parent_ref: None,
        hierarchy_field: None,
        tree_include: vec![],
        design_finders: vec![],
    }
}

/// The sort-aware ordering names the validated column (fully quoted
/// through sea_query aliases) with the requested direction and the `id`
/// tiebreaker; the else arm keeps `created_at DESC`.
#[test]
fn sort_ordering_quotes_columns_and_appends_id_tiebreaker() {
    let tree = sort_test_tree(false);
    let sort_columns = vec![
        UxSortColumn {
            key: "name".into(),
            column: "name".into(),
        },
        UxSortColumn {
            key: "total_amount".into(),
            column: "total_amount".into(),
        },
    ];
    let mut code = CodeWriter::new();
    emit_sort_ordering(&tree, &sort_columns, &mut code);
    let out = code.as_str();

    assert!(
        out.contains("let query = if let Some((sort_field, sort_desc)) = sort {"),
        "{out}"
    );
    assert!(
        out.contains("let dir = if sort_desc {\n                sea_orm::sea_query::Order::Desc\n            } else {\n                sea_orm::sea_query::Order::Asc\n            };"),
        "{out}"
    );
    // Schema/table/column fully qualified and quoted by sea_query Alias.
    assert!(
        out.contains("Alias::new(\"common\"), sea_orm::sea_query::Alias::new(\"task\"), sea_orm::sea_query::Alias::new(\"name\")"),
        "{out}"
    );
    assert!(out.contains("Alias::new(\"total_amount\")"), "{out}");
    // Deterministic `, id ASC` tiebreaker for stable pagination.
    assert!(
        out.contains("ordered.order_by_asc(sea_orm::sea_query::Expr::col((sea_orm::sea_query::Alias::new(\"common\"), sea_orm::sea_query::Alias::new(\"task\"), sea_orm::sea_query::Alias::new(\"id\"))))"),
        "{out}"
    );
    // Default arm keeps today's ordering.
    assert!(
        out.contains("query.order_by_desc(crate::entity::common_task::Column::CreatedAt)"),
        "{out}"
    );
}

/// The `id` tiebreaker applies in both directions (the match arm runs
/// before the shared tiebreaker line).
#[test]
fn sort_ordering_tiebreaker_is_direction_independent() {
    let tree = sort_test_tree(true);
    let sort_columns = vec![UxSortColumn {
        key: "due_date".into(),
        column: "due_date".into(),
    }];
    let mut code = CodeWriter::new();
    emit_sort_ordering(&tree, &sort_columns, &mut code);
    let out = code.as_str();
    assert_eq!(out.matches("order_by_asc").count(), 1, "{out}");
    assert!(out.contains("\"due_date\""), "{out}");
}

// --- include-fetch hydrator type adaptation (issue #459) ---

fn single_include_path() -> ResolvedIncludePath {
    ResolvedIncludePath {
        alias: "job".into(),
        segments: vec![IncludeSegment {
            entity_name: "Job".into(),
            schema_title: "JobType".into(),
            module_name: "job".into(),
            domain: "hr".into(),
            table: "\"hr\".\"job\"".into(),
            fk_column: "job_id".into(),
            reverse_fk_column: "job_id".into(),
            fk_is_required: false,
            reverse_fk_is_required: false,
            is_array: false,
            child_table_override: None,
        }],
        response_rust_type: "JobResponse".into(),
        fetch_method: "fetch_job_for_task".into(),
        batch_fetch_method: "fetch_job_batch_for_task".into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_single_include(dto_type: Option<&str>, col_type: &str, nullable: bool) -> String {
    let tree = sort_test_tree(false);
    let path = single_include_path();
    let mut code = CodeWriter::new();
    RepositoryImplEmitter.emit_include_fetch_methods(
        &tree,
        &mut code,
        std::slice::from_ref(&path),
        &[None],
        &[None],
        &[vec![vec!["organization_id".to_string()]]],
        &[vec![vec!["organization_id".to_string()]]],
        &[vec![vec![false]]],
        &[vec![vec![false]]],
        &[vec![vec![dto_type.map(str::to_string)]]],
        &[vec![vec![nullable]]],
        &[vec![vec![col_type.to_string()]]],
    );
    code.into_string()
}

/// A Uuid entity column hydrating a String DTO field (the mox unformatted
/// datatype shape: the refers-FK column is Uuid while the TEXT-mapped DTO
/// field is String) must adapt with `.to_string()` — a plain assignment
/// fails E0308 in the generated app (issue #459).
#[test]
fn include_fetch_converts_uuid_column_to_string_dto() {
    let out = emit_single_include(Some("String"), "Uuid", false);
    assert!(
        out.contains("organization_id: target.organization_id.to_string(),"),
        "uuid column → String DTO must emit .to_string(). Got:\n{out}"
    );
}

#[test]
fn include_fetch_converts_nullable_uuid_column_to_string_dto() {
    let out = emit_single_include(Some("String"), "Uuid", true);
    assert!(
        out.contains("organization_id: target.organization_id.map(|v| v.to_string()),"),
        "nullable uuid column → String DTO must emit .map(|v| v.to_string()). Got:\n{out}"
    );
}

/// A String entity column hydrating a non-String DTO field mirrors the main
/// hydrator's `.parse()` conversion.
#[test]
fn include_fetch_parses_string_column_to_non_string_dto() {
    let out = emit_single_include(Some("CurrencyCodeList"), "String", false);
    assert!(
        out.contains("organization_id: target.organization_id.parse().unwrap_or_default(),"),
        "String column → non-String DTO must emit .parse().unwrap_or_default(). Got:\n{out}"
    );
}

#[test]
fn include_fetch_parses_nullable_string_column_to_non_string_dto() {
    let out = emit_single_include(Some("CurrencyCodeList"), "String", true);
    assert!(
        out.contains("organization_id: target.organization_id.and_then(|v| v.parse().ok()),"),
        "nullable String column → non-String DTO must emit .and_then(parse). Got:\n{out}"
    );
}

/// Matching types keep the byte-identical plain assignment.
#[test]
fn include_fetch_keeps_plain_assignment_for_matching_types() {
    let out = emit_single_include(Some("Uuid"), "Uuid", false);
    assert!(
        out.contains("organization_id: target.organization_id,"),
        "matching types must keep the plain assignment. Got:\n{out}"
    );
    assert!(!out.contains(".to_string()"), "{out}");
}

// --- include-fetch soft-delete filtering (issue #784) ---

/// One path, emitted with an explicit target tree — the soft-delete marker's
/// derivation input.
fn emit_include_with_target(path: &ResolvedIncludePath, target_tree: Option<EntityTree>) -> String {
    let tree = sort_test_tree(false);
    let per_seg_dto: Vec<Vec<String>> = path
        .segments
        .iter()
        .map(|_| vec!["organization_id".to_string()])
        .collect();
    let per_seg_col: Vec<Vec<String>> = per_seg_dto.clone();
    let per_seg_structured: Vec<Vec<bool>> = path.segments.iter().map(|_| vec![false]).collect();
    let per_seg_codelist: Vec<Vec<bool>> = per_seg_structured.clone();
    let per_seg_dto_types: Vec<Vec<Option<String>>> =
        path.segments.iter().map(|_| vec![None]).collect();
    let per_seg_nullable: Vec<Vec<bool>> = per_seg_structured.clone();
    let per_seg_col_types: Vec<Vec<String>> = per_seg_dto.clone();
    let mut code = CodeWriter::new();
    RepositoryImplEmitter.emit_include_fetch_methods(
        &tree,
        &mut code,
        std::slice::from_ref(path),
        &[target_tree],
        // The dot intermediate tree — resolved by the emitter loop for
        // multi-segment paths; the KATs for that hop pin it via
        // `dot_include_fetch_filters_the_intermediate_when_auditable`
        // (built with its own tree below).
        &[None],
        &[per_seg_dto],
        &[per_seg_col],
        &[per_seg_structured],
        &[per_seg_codelist],
        &[per_seg_dto_types],
        &[per_seg_nullable],
        &[per_seg_col_types],
    );
    code.into_string()
}

/// The reverse-FK (collection) include path.
fn array_include_path() -> ResolvedIncludePath {
    ResolvedIncludePath {
        alias: "work_order".into(),
        segments: vec![IncludeSegment {
            entity_name: "WorkOrder".into(),
            schema_title: "WorkOrderType".into(),
            module_name: "work_order".into(),
            domain: "hr".into(),
            table: "\"hr\".\"work_order\"".into(),
            fk_column: "unused_for_the_array_shape".into(),
            reverse_fk_column: "customer_id".into(),
            fk_is_required: false,
            reverse_fk_is_required: false,
            is_array: true,
            child_table_override: None,
        }],
        response_rust_type: "WorkOrderResponse".into(),
        fetch_method: "fetch_work_order_for_task".into(),
        batch_fetch_method: "fetch_work_order_batch_for_task".into(),
    }
}

/// The one-hop dot path (`job.position` — the target's own FK hop).
fn dot_include_path() -> ResolvedIncludePath {
    let seg = |entity: &str, module: &str, fk: &str| IncludeSegment {
        entity_name: entity.to_string(),
        schema_title: format!("{entity}Type"),
        module_name: module.to_string(),
        domain: "hr".into(),
        table: format!("\"hr\".\"{module}\""),
        fk_column: fk.to_string(),
        reverse_fk_column: fk.to_string(),
        fk_is_required: false,
        reverse_fk_is_required: false,
        is_array: false,
        child_table_override: None,
    };
    ResolvedIncludePath {
        alias: "job.position".into(),
        segments: vec![
            seg("Job", "job", "job_id"),
            seg("Position", "position", "position_id"),
        ],
        response_rust_type: "JobResponse".into(),
        fetch_method: "fetch_job_position_for_task".into(),
        batch_fetch_method: "fetch_job_position_batch_for_task".into(),
    }
}

/// An auditable target's include fetch EXCLUDES soft-deleted rows (issue
/// #784): the scalar arm's target load ANDs the `DeletedAt.is_null()` filter
/// the CRUD read plane applies, in BOTH the single and the batch variant.
#[test]
fn scalar_include_fetch_filters_soft_deleted_target_when_auditable() {
    let out = emit_include_with_target(&single_include_path(), Some(sort_test_tree(true)));
    assert_eq!(
        out.matches("crate::entity::hr_job::Column::DeletedAt.is_null()")
            .count(),
        2,
        "single + batch scalar arms must both filter the auditable target:\n{out}"
    );
}

/// A non-auditable target invents no filter — the DDL gives the table no
/// `deleted_at` column to filter on.
#[test]
fn scalar_include_fetch_invents_no_filter_when_target_not_auditable() {
    let out = emit_include_with_target(&single_include_path(), Some(sort_test_tree(false)));
    assert!(
        !out.contains("DeletedAt"),
        "a non-auditable target must carry no soft-delete filter:\n{out}"
    );
}

/// A target tree the emitter could not resolve hydrates unfiltered (today's
/// shape, preserved: no tree, no invented marker).
#[test]
fn include_fetch_without_a_target_tree_invents_no_filter() {
    let out = emit_include_with_target(&single_include_path(), None);
    assert!(!out.contains("DeletedAt"), "{out}");
}

/// The reverse-FK (collection) include excludes soft-deleted children in
/// both the single and the batch variant (issue #784). Excluding every row
/// yields the empty collection — the handler's `Some(vec![])` shape.
#[test]
fn reverse_fk_include_fetch_filters_soft_deleted_target_when_auditable() {
    let out = emit_include_with_target(&array_include_path(), Some(sort_test_tree(true)));
    assert_eq!(
        out.matches("crate::entity::hr_work_order::Column::DeletedAt.is_null()")
            .count(),
        2,
        "single + batch reverse-FK arms must both filter the auditable target:\n{out}"
    );
}

/// The dot path's LEAF load excludes soft-deleted leaf rows when the leaf is
/// auditable (issue #784): the leaf field stays `None` on the combined
/// response rather than surfacing a deleted row.
#[test]
fn dot_include_fetch_filters_the_leaf_when_auditable() {
    let out = emit_include_with_target(&dot_include_path(), Some(sort_test_tree(true)));
    assert!(
        out.contains(".filter(crate::entity::hr_position::Column::DeletedAt.is_null())"),
        "the dot leaf load must filter the auditable leaf:\n{out}"
    );
}

/// The dot path invents no leaf filter when the leaf is not auditable.
#[test]
fn dot_include_fetch_invents_no_leaf_filter_when_leaf_not_auditable() {
    let out = emit_include_with_target(&dot_include_path(), Some(sort_test_tree(false)));
    assert!(
        !out.contains("DeletedAt"),
        "a non-auditable leaf must carry no soft-delete filter:\n{out}"
    );
}

/// The dot path's INTERMEDIATE load excludes soft-deleted intermediate rows
/// when the intermediate is auditable (issue #784): a deleted intermediate
/// hides its leaf — `Ok(None)` for the whole combined response, mirroring the
/// cornucopia dot SQL's `"i"."deleted_at" IS NULL` hop.
#[test]
fn dot_include_fetch_filters_the_intermediate_when_auditable() {
    let tree = sort_test_tree(false);
    let path = dot_include_path();
    let per_seg_dto: Vec<Vec<String>> = path
        .segments
        .iter()
        .map(|_| vec!["organization_id".to_string()])
        .collect();
    let mut code = CodeWriter::new();
    RepositoryImplEmitter.emit_include_fetch_methods(
        &tree,
        &mut code,
        std::slice::from_ref(&path),
        // Leaf tree (last segment): auditable — filtered.
        &[Some(sort_test_tree(true))],
        // Intermediate tree (segments[0]): auditable — filtered.
        &[Some(sort_test_tree(true))],
        std::slice::from_ref(&per_seg_dto),
        std::slice::from_ref(&per_seg_dto),
        &[vec![vec![false], vec![false]]],
        &[vec![vec![false], vec![false]]],
        &[vec![vec![None], vec![None]]],
        &[vec![vec![false], vec![false]]],
        std::slice::from_ref(&per_seg_dto),
    );
    let out = code.into_string();
    assert!(
        out.contains(".filter(crate::entity::hr_job::Column::DeletedAt.is_null())"),
        "the dot intermediate load must filter the auditable intermediate:\n{out}"
    );
    assert!(
        out.contains(".filter(crate::entity::hr_position::Column::DeletedAt.is_null())"),
        "the dot leaf load must filter the auditable leaf:\n{out}"
    );
}

// --- dot-intermediate addressing (issue #789, the #784 TDD §8 finding) ---
//
// The old `emit_dot_fetch_method` loaded the INTERMEDIATE with
// `Column::Id.eq(source_id)` — the SOURCE row's primary key — so
// `intermediate.id == source_pk` never matched across entities and every
// SeaORM dot include answered `Ok(None)`. The cornucopia twin is correct
// (`WHERE "i"."id" = (SELECT "fk" FROM source WHERE "id" = :source_id)`);
// the SeaORM arm must mirror it through the scalar include arm's own
// idiom: load the source by PK, read its FK, load the intermediate by
// that value.

/// The dot fetch addresses the INTERMEDIATE through the SOURCE's FK: the
/// source row loads by its own PK first, its FK is read, and the
/// intermediate loads by that value — never by the source's PK.
#[test]
fn dot_fetch_addresses_the_intermediate_through_the_source_fk() {
    let out = emit_include_with_target(&dot_include_path(), None);
    assert!(
        out.contains(".filter(crate::entity::common_task::Column::Id.eq(source_id))"),
        "the source row must load by its own PK first:\n{out}"
    );
    assert!(
        out.contains("source.job_id"),
        "the source's FK to the intermediate must be read:\n{out}"
    );
    assert!(
        out.contains("crate::entity::hr_job::Column::Id.eq(fk_value)"),
        "the intermediate must load by the source's FK value:\n{out}"
    );
    assert!(
        !out.contains("hr_job::Column::Id.eq(source_id)"),
        "the intermediate must not be addressed by the source's PK:\n{out}"
    );
}

/// A source row that does not exist answers `Ok(None)` — the fetch must
/// match the source load (and early-return on `None`) BEFORE touching the
/// intermediate.
#[test]
fn dot_fetch_returns_none_when_the_source_row_is_missing() {
    let out = emit_include_with_target(&dot_include_path(), None);
    let source_match = out
        .find("let source = match source {")
        .unwrap_or_else(|| panic!("the source load must be matched:\n{out}"));
    let intermediate_load = out
        .find("let intermediate = crate::entity::hr_job::Entity::find()")
        .unwrap_or_else(|| panic!("the intermediate load must exist:\n{out}"));
    assert!(
        source_match < intermediate_load,
        "the source match must precede the intermediate load:\n{out}"
    );
    let arms = &out[source_match..out.find("};").unwrap_or(out.len())];
    assert!(
        arms.contains("None => return Ok(None),"),
        "a missing source row must early-return Ok(None):\n{out}"
    );
}

/// An OPTIONAL first-hop FK reads through the Option idiom —
/// `Some(v) => v, None => return Ok(None)` (the scalar arm's shape).
#[test]
fn dot_fetch_optional_fk_uses_the_option_idiom() {
    let out = emit_include_with_target(&dot_include_path(), None);
    assert!(
        out.contains("let fk_value = match source.job_id {"),
        "an optional first-hop FK must unwrap through a match:\n{out}"
    );
    let fk_read = out
        .find("let fk_value = match source.job_id {")
        .unwrap_or_else(|| panic!("the optional FK match must exist:\n{out}"));
    let arms = &out[fk_read
        ..out[fk_read..]
            .find("};")
            .map(|i| fk_read + i)
            .unwrap_or(out.len())];
    assert!(
        arms.contains("Some(v) => v,") && arms.contains("None => return Ok(None),"),
        "an optional FK with no value must early-return Ok(None):\n{out}"
    );
}

/// A REQUIRED first-hop FK reads the plain `Uuid` directly — no Option
/// machinery on a non-optional column.
#[test]
fn dot_fetch_required_fk_reads_the_plain_uuid() {
    let mut path = dot_include_path();
    path.segments[0].fk_is_required = true;
    let out = emit_include_with_target(&path, None);
    assert!(
        out.contains("let fk_value = source.job_id;"),
        "a required first-hop FK must read the plain Uuid:\n{out}"
    );
    assert!(
        !out.contains("match source.job_id"),
        "a required FK must carry no Option unwrapping:\n{out}"
    );
}
