//! Unit KATs for the cornucopia CRUD soft-delete contract (issue #788).
//!
//! `render_entity_sql` derives the ONE `soft_delete_col` local that feeds
//! every CRUD writer in `cornucopia_queries.rs`. The derivation must be
//! [`include_soft_delete_marker`] — the read plane's auditable decision — not
//! the policy-derived `EntityTree.soft_delete_column`, which the mox-first
//! driver path never populates (`ingest_policies_and_relationships` has no
//! caller), so it is `None` for every project: reads returned soft-deleted
//! rows, DELETE hard-deleted auditable tables, and the `include_deleted`
//! toggle was dead on this plane.
//!
//! The KATs drive `render_entity_sql` itself (not the leaf writers) so the
//! derivation site is what is pinned; the sibling
//! `cornucopia_queries_tests.rs` covers the include SQL half.

use super::*;

/// A fixture tree with the PRODUCTION truth spelled explicitly:
/// `is_auditable` is the read plane's decision (`(...) && !append_only`) and
/// `soft_delete_column` stays `None` — the mox-first shape, where no
/// `SoftDelete` policy node ever reaches the graph.
fn tree(entity: &str, table: &str, columns: Vec<TreeColumn>, is_auditable: bool) -> EntityTree {
    EntityTree {
        entity_name: entity.to_string(),
        module_name: table.to_string(),
        schema_name: "shop".to_string(),
        table_name: table.to_string(),
        entity_module: format!("shop_{table}"),
        namespace: None,
        direct_columns: columns,
        child_tables: vec![],
        junction_tables: vec![],
        has_create: true,
        has_read: true,
        has_update: true,
        has_delete: true,
        has_list: true,
        has_workflow: false,
        has_fts: false,
        has_embeddings: false,
        fts_language: "english".to_string(),
        is_auditable,
        soft_delete_visibility: "exclude_by_default".to_string(),
        soft_delete_column: None,
        soft_delete_cascade: "restrict".to_string(),
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

fn col(field: &str, pg_column: &str, rust_type: &str, is_nullable: bool) -> TreeColumn {
    TreeColumn {
        field_name: field.to_string(),
        pg_column_name: pg_column.to_string(),
        dto_field_name: None,
        rust_type: rust_type.to_string(),
        is_nullable,
        is_entity_ref: false,
        dto_rust_type: None,
        is_workflow_managed: false,
        is_array: false,
        pg_cast: None,
        is_composite_range: false,
        is_structured_wrapper: false,
        is_media: false,
    }
}

fn customer(auditable: bool) -> EntityTree {
    tree(
        "Customer",
        "customer",
        vec![col("name", "name", "String", false)],
        auditable,
    )
}

/// Minimal domains.toml — `render_entity_sql` reads only the embedding
/// entity config, which these fixtures carry none of.
fn config() -> DomainConfig {
    toml::from_str(
        r#"
[defaults]
operations = []

[domains.shop]
label = "Shop"
schema_dir = "schemas/shop"
postgres_schema = "shop"
"#,
    )
    .unwrap()
}

fn pg_types() -> std::collections::HashMap<String, String> {
    std::collections::HashMap::new()
}

/// Render the fixture entity's full CRUD SQL through the ONE entry point the
/// generator uses — the function that owns the `soft_delete_col` local.
fn render(tree: &EntityTree) -> String {
    render_entity_sql("CustomerType", "shop", &config(), tree, &pg_types())
}

/// Extract one `--! {name}` statement block (up to the next annotation).
fn statement_block(sql: &str, name: &str) -> String {
    let start = sql
        .find(&format!("--! {name} "))
        .unwrap_or_else(|| panic!("no `--! {name}` statement:\n{sql}"));
    let rest = &sql[start..];
    let end = rest[1..]
        .find("\n--! ")
        .map(|i| i + 1)
        .unwrap_or(rest.len());
    rest[..end].to_string()
}

/// The auditable CRUD list/count filter soft-deleted rows, and the
/// `_including_deleted` twins stay unfiltered (issue #788 defect 1 + 4: the
/// toggle the repository plane drives from `is_auditable` was dead because
/// both variants were byte-identical).
#[test]
fn list_and_count_sql_filters_soft_deleted_rows_when_auditable() {
    let sql = render(&customer(true));

    let list = statement_block(&sql, "list_customer");
    assert!(
        list.contains("FROM \"shop\".\"customer\"\n  WHERE \"deleted_at\" IS NULL\nORDER BY \"created_at\" DESC"),
        "the default list must exclude soft-deleted rows:\n{list}"
    );
    let count = statement_block(&sql, "count_customer");
    assert!(
        count.contains("FROM \"shop\".\"customer\"\n  WHERE \"deleted_at\" IS NULL;"),
        "the default count must exclude soft-deleted rows:\n{count}"
    );

    // The escape hatch: the `_including_deleted` twins carry NO marker.
    let list_all = statement_block(&sql, "list_customer_including_deleted");
    assert!(
        !list_all.contains("deleted_at"),
        "`list_customer_including_deleted` must stay unfiltered:\n{list_all}"
    );
    let count_all = statement_block(&sql, "count_customer_including_deleted");
    assert!(
        !count_all.contains("deleted_at"),
        "`count_customer_including_deleted` must stay unfiltered:\n{count_all}"
    );
    // And the two arms must NOT be byte-identical any more (the toggle lives).
    assert_ne!(list, list_all, "the include_deleted toggle must branch");
}

/// `get_customer` filters soft-deleted rows; `get_customer_including_deleted`
/// stays unfiltered (issue #788 defect 1).
#[test]
fn get_sql_filters_and_including_deleted_stays_unfiltered() {
    let sql = render(&customer(true));

    let get = statement_block(&sql, "get_customer");
    assert!(
        get.contains("WHERE \"id\" = :id\n    AND \"deleted_at\" IS NULL;"),
        "the default get must exclude soft-deleted rows:\n{get}"
    );
    let get_all = statement_block(&sql, "get_customer_including_deleted");
    assert!(
        get_all.contains("WHERE \"id\" = :id;"),
        "`get_customer_including_deleted` must stay unfiltered:\n{get_all}"
    );
    assert!(!get_all.contains("deleted_at"), "{get_all}");
}

/// The parent-scoped get (`find_by_id_scoped`'s statement) filters too
/// (issue #788 defect 1 — nested reads returned deleted children).
#[test]
fn scoped_get_sql_filters_when_auditable() {
    let mut tree = customer(true);
    tree.parent_ref = Some("customer_id".to_string());
    let sql = render(&tree);

    let scoped = statement_block(&sql, "get_customer_scoped");
    assert!(
        scoped.contains(
            "WHERE \"id\" = :id\n    AND \"customer_id\" = :parent_id\n    AND \"deleted_at\" IS NULL;"
        ),
        "the scoped get must exclude soft-deleted rows:\n{scoped}"
    );
    let scoped_all = statement_block(&sql, "get_customer_scoped_including_deleted");
    assert!(!scoped_all.contains("deleted_at"), "{scoped_all}");
}

/// FTS search excludes soft-deleted rows on the default arm only
/// (issue #788 defect 1).
#[test]
fn search_sql_filters_when_auditable() {
    let mut tree = customer(true);
    tree.has_fts = true;
    let sql = render(&tree);

    let search = statement_block(&sql, "search_customer");
    assert!(
        search.contains(
            "WHERE search_tsv @@ websearch_to_tsquery('english', :q) AND \"deleted_at\" IS NULL"
        ),
        "the default search must exclude soft-deleted rows:\n{search}"
    );
    let search_all = statement_block(&sql, "search_customer_including_deleted");
    assert!(
        !search_all.contains("deleted_at"),
        "`search_customer_including_deleted` must stay unfiltered:\n{search_all}"
    );
}

/// DELETE on an auditable table is a SOFT delete (issue #788 defect 2 — the
/// data-loss class): `UPDATE ... SET "deleted_at" = NOW()` with the same
/// row guard. A tree the read plane calls non-auditable keeps the hard
/// `DELETE FROM` (its DDL has no column to set).
#[test]
fn delete_sql_soft_deletes_when_auditable_and_hard_deletes_only_without_a_column() {
    let sql = render(&customer(true));
    let delete = statement_block(&sql, "delete_customer");
    assert!(
        delete.contains(
            "UPDATE \"shop\".\"customer\"\nSET \"deleted_at\" = NOW()\nWHERE \"id\" = :id\n  AND \"deleted_at\" IS NULL;"
        ),
        "an auditable entity's DELETE must soft-delete:\n{delete}"
    );
    assert!(
        !delete.contains("DELETE FROM"),
        "no hard delete on an auditable table:\n{delete}"
    );

    let sql = render(&customer(false));
    let delete = statement_block(&sql, "delete_customer");
    assert!(
        delete.contains("DELETE FROM \"shop\".\"customer\"\nWHERE \"id\" = :id;"),
        "a non-auditable entity keeps the hard delete:\n{delete}"
    );
}

/// UPDATE refuses to touch soft-deleted rows (issue #788 defect 3 — the
/// where-guard was dead on this plane while the SeaORM twin enforced it).
#[test]
fn update_sql_guards_against_deleted_rows_when_auditable() {
    let sql = render(&customer(true));
    let update = statement_block(&sql, "update_customer");
    assert!(
        update.contains("WHERE \"id\" = :id\n  AND \"deleted_at\" IS NULL;"),
        "the update must guard against soft-deleted rows:\n{update}"
    );

    let sql = render(&customer(false));
    let update = statement_block(&sql, "update_customer");
    assert!(
        update.contains("WHERE \"id\" = :id;"),
        "a non-auditable entity needs no guard:\n{update}"
    );
}

/// No marker is EVER invented for a tree the read plane calls non-auditable —
/// even with a STALE policy field claiming `deleted_at` (a table without the
/// column would 500 every request, the #784 posture carried to the CRUD
/// plane). Append-only trees (`is_auditable = auditable && !append_only`)
/// carry no audit columns at all: byte-absence of `deleted_at` across the
/// whole file.
#[test]
fn non_auditable_and_append_only_sql_invents_no_marker() {
    // Non-auditable WITH a stale policy field — the stale field must not
    // resurrect the marker (RED before the fix: the policy field fed the
    // writers directly).
    let mut tree = customer(false);
    tree.soft_delete_column = Some("deleted_at".to_string());
    let sql = render(&tree);
    assert!(
        !sql.contains("deleted_at"),
        "a stale policy field must not invent soft-delete SQL on a \
         non-auditable tree:\n{sql}"
    );

    // Append-only: no audit column exists on the table, so the whole file is
    // `deleted_at`-free (and carries no UPDATE — append-only entities have
    // no update path, issue #284).
    let mut append_only = customer(false);
    append_only.append_only = true;
    append_only.soft_delete_column = Some("deleted_at".to_string());
    let sql = render(&append_only);
    assert!(!sql.contains("deleted_at"), "{sql}");
    assert!(!sql.contains("--! update_customer"), "{sql}");
    assert!(
        sql.contains("DELETE FROM \"shop\".\"customer\""),
        "append-only keeps the hard delete:\n{sql}"
    );
}

/// ONE derivation feeds BOTH planes (issue #788): for the same tree, the
/// CRUD list SQL carries the soft-delete marker exactly when the include SQL
/// does — both read [`include_soft_delete_marker`]. Neither plane can drift
/// from the derivation or from each other.
#[test]
fn crud_and_include_sql_share_one_marker() {
    use crate::api::include_plan::CornucopiaIncludeShape;

    let source = tree("WorkOrder", "work_order", vec![], true);
    for auditable in [true, false] {
        let target = customer(auditable);
        let pg = pg_types();

        // The CRUD plane, through the derivation site.
        let crud = render(&target);
        let crud_filtered =
            statement_block(&crud, "list_customer").contains("\"deleted_at\" IS NULL");

        // The include plane, through its own call of the same predicate.
        let facts = IncludeQueryFacts {
            alias: "customer",
            shape: CornucopiaIncludeShape::SingleScalar,
            fk_column: "customer_id",
            reverse_fk_column: "customer_id",
            reverse_fk_is_required: false,
            target: &target,
            target_pg_types: &pg,
            leaf: None,
            leaf_fk_column: "",
            leaf_pg_types: None,
        };
        let include = render_include_sql(&source, &facts);
        let include_filtered = include.contains("\"deleted_at\" IS NULL");

        assert_eq!(
            crud_filtered, auditable,
            "the CRUD list marker must track the auditable decision:\n{crud}"
        );
        assert_eq!(
            include_filtered, auditable,
            "the include marker must track the auditable decision:\n{include}"
        );
        assert_eq!(
            crud_filtered, include_filtered,
            "one derivation feeds both planes (issue #788)"
        );
    }
}
