//! Unit KATs for the cornucopia `?include=` SQL emitters (issue #781 P1).
//!
//! Split out of `cornucopia_queries.rs` (issue #751 file budget, the
//! `db/ddl/query/` precedent): the emitters stay in the sibling module; these
//! KATs drive them through the same `super::*` surface they always used.

use super::*;

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

fn tree(
    entity: &str,
    table: &str,
    columns: Vec<TreeColumn>,
    soft_delete: Option<&str>,
    append_only: bool,
) -> EntityTree {
    // Production truth: `is_auditable` is the read plane's decision
    // (`(...) && !append_only`) and the DDL emits `deleted_at` exactly for it —
    // an append-only table carries no audit columns at all, so a tree cannot
    // be auditable and append-only at once.
    let is_auditable = soft_delete.is_some() && !append_only;
    EntityTree {
        entity_name: entity.to_string(),
        module_name: table.to_string(),
        schema_name: "common".to_string(),
        table_name: table.to_string(),
        entity_module: format!("common_{table}"),
        namespace: None,
        direct_columns: columns,
        child_tables: vec![],
        junction_tables: vec![],
        has_create: true,
        has_read: true,
        has_update: false,
        has_delete: false,
        has_list: true,
        has_workflow: false,
        has_fts: false,
        has_embeddings: false,
        fts_language: "english".to_string(),
        is_auditable,
        soft_delete_visibility: "exclude_by_default".to_string(),
        soft_delete_column: soft_delete.map(str::to_string),
        soft_delete_cascade: "restrict".to_string(),
        track_updated_user: false,
        track_deleted_user: false,
        append_only,
        filter_fields: vec![],
        nested_filter_fields: vec![],
        parent_ref: None,
        hierarchy_field: None,
        tree_include: vec![],
        design_finders: vec![],
    }
}

fn facts<'a>(
    alias: &'a str,
    fk: &'a str,
    target: &'a EntityTree,
    pg_types: &'a std::collections::HashMap<String, String>,
) -> IncludeQueryFacts<'a> {
    IncludeQueryFacts {
        alias,
        shape: crate::api::include_plan::CornucopiaIncludeShape::SingleScalar,
        fk_column: fk,
        reverse_fk_column: fk,
        reverse_fk_is_required: false,
        target,
        target_pg_types: pg_types,
        leaf: None,
        leaf_fk_column: "",
        leaf_pg_types: None,
    }
}

fn array_facts<'a>(
    alias: &'a str,
    reverse_fk: &'a str,
    reverse_fk_is_required: bool,
    target: &'a EntityTree,
    pg_types: &'a std::collections::HashMap<String, String>,
) -> IncludeQueryFacts<'a> {
    IncludeQueryFacts {
        alias,
        shape: crate::api::include_plan::CornucopiaIncludeShape::SingleArray,
        fk_column: "unused_for_the_array_shape",
        reverse_fk_column: reverse_fk,
        reverse_fk_is_required,
        target,
        target_pg_types: pg_types,
        leaf: None,
        leaf_fk_column: "",
        leaf_pg_types: None,
    }
}

fn dot_facts<'a>(
    alias: &'a str,
    fk: &'a str,
    leaf_fk: &'a str,
    target: &'a EntityTree,
    leaf: &'a EntityTree,
    pg_types: &'a std::collections::HashMap<String, String>,
) -> IncludeQueryFacts<'a> {
    IncludeQueryFacts {
        alias,
        shape: crate::api::include_plan::CornucopiaIncludeShape::Dot,
        fk_column: fk,
        reverse_fk_column: fk,
        reverse_fk_is_required: false,
        target,
        target_pg_types: pg_types,
        leaf: Some(leaf),
        leaf_fk_column: leaf_fk,
        leaf_pg_types: Some(pg_types),
    }
}

fn worker() -> EntityTree {
    tree("Worker", "worker", vec![], Some("deleted_at"), false)
}

/// The single-fetch query addresses the FK through a correlated subquery
/// (unambiguous columns, RLS on the source) and filters the TARGET's
/// soft-delete marker.
#[test]
fn single_scalar_include_sql_shape() {
    let target = tree(
        "Person",
        "person",
        vec![
            col("given_name", "given_name", "String", false),
            col("nickname", "nickname", "String", true),
        ],
        Some("deleted_at"),
        false,
    );
    let pg = std::collections::HashMap::new();
    let sql = render_include_sql(&worker(), &facts("person", "person_id", &target, &pg));

    assert!(
            sql.contains(
                "--! include_person_for_worker (source_id) : (id, given_name, nickname?, created_at, updated_at)"
            ),
            "{sql}"
        );
    assert!(
            sql.contains(
                "FROM \"common\".\"person\"\nWHERE \"id\" = (SELECT \"person_id\" FROM \"common\".\"worker\" WHERE \"id\" = :source_id)\n  AND \"deleted_at\" IS NULL\nLIMIT 1;"
            ),
            "{sql}"
        );
}

/// The batch query keys each target row by the source id and qualifies
/// every target column (the JOIN makes unqualified names ambiguous).
#[test]
fn batch_scalar_include_sql_is_keyed_and_qualified() {
    let target = tree(
        "Person",
        "person",
        vec![col("given_name", "given_name", "String", false)],
        None,
        false,
    );
    let pg = std::collections::HashMap::new();
    let sql = render_include_sql(&worker(), &facts("person", "person_id", &target, &pg));

    assert!(
            sql.contains("--! include_person_batch_for_worker (source_ids) : (source_id, id, given_name, created_at, updated_at)"),
            "{sql}"
        );
    assert!(
            sql.contains("SELECT \"src\".\"source_id\" AS \"source_id\", \"t\".\"id\", \"t\".\"given_name\", \"t\".\"created_at\", \"t\".\"updated_at\""),
            "{sql}"
        );
    assert!(
        sql.contains("WHERE \"id\" = ANY(:source_ids::uuid[])"),
        "{sql}"
    );
    assert!(
        sql.contains(
            "JOIN \"common\".\"person\" AS \"t\" ON \"t\".\"id\" = \"src\".\"target_id\";"
        ),
        "{sql}"
    );
    // No soft-delete filter when the target has no marker.
    assert!(!sql.contains("IS NULL"), "{sql}");
}

/// Append-only targets carry no `updated_at` (issue #284) — and no
/// `deleted_at` either: the DDL emits no audit column for an append-only
/// table (`is_auditable = auditable && !append_only`), so the include SQL
/// must not invent the marker (issue #784 — inventing it 500s every request).
#[test]
fn append_only_target_omits_updated_at_and_invents_no_marker() {
    let target = tree(
        "Person",
        "person",
        vec![col("gender", "gender_code", "String", false)],
        Some("deleted_at"),
        true,
    );
    let pg = std::collections::HashMap::new();
    let sql = render_include_sql(&worker(), &facts("person", "person_id", &target, &pg));

    assert!(
        sql.contains("AS \"gender\""),
        "physical column keeps its field alias:\n{sql}"
    );
    assert!(
        sql.contains("(id, gender, created_at)"),
        "append-only target has no updated_at hint:\n{sql}"
    );
    assert!(!sql.contains("updated_at"), "{sql}");
    assert!(
        !sql.contains("IS NULL"),
        "an append-only target has no deleted_at column — the marker must not \
         be invented:\n{sql}"
    );
}

/// Columns whose Postgres type maps to `String` in Rust (numeric, ranges,
/// geometry) cast to text in both the plain and the prefixed list.
#[test]
fn text_cast_applies_to_both_select_forms() {
    let target = tree(
        "Person",
        "person",
        vec![col("salary", "salary", "String", false)],
        None,
        false,
    );
    let mut pg = std::collections::HashMap::new();
    pg.insert("salary".to_string(), "NUMERIC".to_string());
    let sql = render_include_sql(&worker(), &facts("person", "person_id", &target, &pg));

    assert!(sql.contains("\"salary\"::text"), "{sql}");
    assert!(sql.contains("\"t\".\"salary\"::text"), "{sql}");
}

/// A reverse-FK target: the child rows live on the target table, and the
/// predicate is the target's own FK column — no source subquery, no JOIN.
fn deployment() -> EntityTree {
    tree(
        "Deployment",
        "deployment",
        vec![
            col("reference", "reference", "String", false),
            col("customer_id", "customer_id", "Uuid", false),
        ],
        Some("deleted_at"),
        false,
    )
}

/// Array single: the target's reverse FK is the predicate, the target's
/// soft-delete marker ANDs on, and there is no scalar `LIMIT 1`.
#[test]
fn single_array_include_sql_scans_the_reverse_fk() {
    let pg = std::collections::HashMap::new();
    let sql = render_include_sql(
        &tree("Customer", "customer", vec![], None, false),
        &array_facts("deployment", "customer_id", false, &deployment(), &pg),
    );

    assert!(
            sql.contains(
                "--! include_deployment_for_customer (source_id) : (id, reference, customer_id, created_at, updated_at)"
            ),
            "{sql}"
        );
    assert!(
            sql.contains(
                "SELECT \"id\", \"reference\", \"customer_id\", \"created_at\", \"updated_at\"\nFROM \"common\".\"deployment\"\nWHERE \"customer_id\" = :source_id\n  AND \"deleted_at\" IS NULL;"
            ),
            "{sql}"
        );
    assert!(!sql.contains("LIMIT 1"), "{sql}");
    assert!(
        !sql.contains("SELECT \"customer_id\" FROM \"common\".\"customer\""),
        "no source subquery for the reverse shape:\n{sql}"
    );
}

/// Array batch: one query for many sources, the reverse FK aliased to the
/// fixed `source_id` key the adapter groups by, the `uuid[]` cast the
/// other array-parameter query pins, and the target's soft-delete marker.
/// An OPTIONAL reverse FK carries the `source_id?` hint (`Option<Uuid>`),
/// matching the adapter's `None => continue` arm.
#[test]
fn batch_array_include_sql_keys_the_reverse_fk() {
    let pg = std::collections::HashMap::new();
    let sql = render_include_sql(
        &tree("Customer", "customer", vec![], None, false),
        &array_facts("deployment", "customer_id", false, &deployment(), &pg),
    );

    assert!(
            sql.contains(
                "--! include_deployment_batch_for_customer (source_ids) : (source_id?, id, reference, customer_id, created_at, updated_at)"
            ),
            "{sql}"
        );
    assert!(
            sql.contains(
                "SELECT \"customer_id\" AS \"source_id\", \"id\", \"reference\", \"customer_id\", \"created_at\", \"updated_at\"\nFROM \"common\".\"deployment\"\nWHERE \"customer_id\" = ANY(:source_ids::uuid[])\n  AND \"deleted_at\" IS NULL;"
            ),
            "{sql}"
        );
    // Single-table shape — no JOIN derived table needed.
    assert!(!sql.contains("JOIN"), "{sql}");
    assert!(!sql.contains("target_id"), "{sql}");
}

/// A REQUIRED reverse FK hints `source_id` (plain `Uuid`), the row type the
/// adapter's `let key = row.source_id;` arm matches.
#[test]
fn required_reverse_fk_batch_hint_is_non_nullable() {
    let pg = std::collections::HashMap::new();
    let sql = render_include_sql(
        &tree("Customer", "customer", vec![], None, false),
        &array_facts("invoice", "customer_id", true, &deployment(), &pg),
    );
    assert!(
            sql.contains(
                "--! include_invoice_batch_for_customer (source_ids) : (source_id, id, reference, customer_id, created_at, updated_at)"
            ),
            "{sql}"
        );
    assert!(!sql.contains("source_id?"), "{sql}");
}

/// The array shape keeps the append-only (`updated_at` suppression) and
/// marker-free arms identical to the scalar shape: no invented filter.
#[test]
fn array_include_omits_updated_at_and_invents_no_marker() {
    let target = tree(
        "Deployment",
        "deployment",
        vec![col("customer_id", "customer_id", "Uuid", false)],
        None,
        true,
    );
    let pg = std::collections::HashMap::new();
    let sql = render_include_sql(
        &tree("Customer", "customer", vec![], None, false),
        &array_facts("deployment", "customer_id", false, &target, &pg),
    );

    assert!(
        sql.contains("(source_id) : (id, customer_id, created_at)"),
        "append-only target has no updated_at hint:\n{sql}"
    );
    assert!(!sql.contains("updated_at"), "{sql}");
    assert!(!sql.contains("IS NULL"), "no marker is invented:\n{sql}");
}

/// The dot shape's single query selects the LEAF's columns from the leaf
/// table, joins the intermediate on the intermediate's own FK to the leaf, and
/// addresses the intermediate through the SOURCE's FK — two hops, one query.
/// Both hops' soft-delete markers apply (target here; leaf in the twin below).
#[test]
fn dot_include_sql_joins_through_the_intermediate_fk() {
    let intermediate = tree(
        "Customer",
        "customer",
        vec![col("org_id", "org_id", "Uuid", true)],
        Some("deleted_at"),
        false,
    );
    let leaf = tree(
        "Organization",
        "organization",
        vec![col("name", "name", "String", false)],
        None,
        false,
    );
    let pg = std::collections::HashMap::new();
    let sql = render_include_sql(
        &worker(),
        &dot_facts(
            "customer.org",
            "customer_id",
            "org_id",
            &intermediate,
            &leaf,
            &pg,
        ),
    );

    assert!(
        sql.contains(
            "--! include_customer_org_for_worker (source_id) : (id, name, created_at, updated_at)"
        ),
        "{sql}"
    );
    assert!(
        sql.contains(
            "SELECT \"l\".\"id\", \"l\".\"name\", \"l\".\"created_at\", \"l\".\"updated_at\"\n\
             FROM \"common\".\"organization\" AS \"l\"\n\
             JOIN \"common\".\"customer\" AS \"i\" ON \"i\".\"org_id\" = \"l\".\"id\"\n\
             WHERE \"i\".\"id\" = (SELECT \"customer_id\" FROM \"common\".\"worker\" WHERE \"id\" = :source_id)\n  \
             AND \"i\".\"deleted_at\" IS NULL\n\
             LIMIT 1;"
        ),
        "{sql}"
    );
    // The leaf columns are qualified by the leaf alias — no ambiguity with the
    // joined intermediate's `id`.
    assert!(!sql.contains("SELECT \"id\", \"name\""), "{sql}");
}

/// The dot batch keys each row by SOURCE id (the `included.<first>` map key),
/// carries the same leaf column list, and filters both hops' soft delete.
#[test]
fn dot_include_batch_keys_the_source_and_applies_both_markers() {
    let intermediate = tree(
        "Customer",
        "customer",
        vec![col("org_id", "org_id", "Uuid", true)],
        Some("deleted_at"),
        false,
    );
    let leaf = tree(
        "Organization",
        "organization",
        vec![col("name", "name", "String", false)],
        Some("deleted_at"),
        false,
    );
    let pg = std::collections::HashMap::new();
    let sql = render_include_sql(
        &worker(),
        &dot_facts(
            "customer.org",
            "customer_id",
            "org_id",
            &intermediate,
            &leaf,
            &pg,
        ),
    );

    assert!(
        sql.contains(
            "--! include_customer_org_batch_for_worker (source_ids) : (source_id, id, name, created_at, updated_at)"
        ),
        "{sql}"
    );
    assert!(
        sql.contains("SELECT \"src\".\"source_id\" AS \"source_id\", \"l\".\"id\", \"l\".\"name\""),
        "{sql}"
    );
    assert!(
        sql.contains(
            "SELECT \"id\" AS \"source_id\", \"customer_id\" AS \"target_id\"\n\
             \x20   FROM \"common\".\"worker\"\n\
             \x20   WHERE \"id\" = ANY(:source_ids::uuid[])"
        ),
        "{sql}"
    );
    assert!(
        sql.contains(
            "JOIN \"common\".\"customer\" AS \"i\" ON \"i\".\"id\" = \"src\".\"target_id\"\n\
             JOIN \"common\".\"organization\" AS \"l\" ON \"l\".\"id\" = \"i\".\"org_id\"\n\
             WHERE \"i\".\"deleted_at\" IS NULL\n  AND \"l\".\"deleted_at\" IS NULL;"
        ),
        "{sql}"
    );
}

/// The include marker derives from the read plane's auditable decision, NOT
/// from the policy-derived `soft_delete_column` (issue #784): the mox-first
/// driver path never ingests `SoftDelete` policies, so a marker keyed on that
/// field is unreachable exactly where the tables ARE auditable. Both arms:
///
/// * an auditable tree with NO policy field still carries the marker (the
///   mox-first shape — RED before the fix, the #784 root cause);
/// * a stale policy field over a non-auditable tree invents NO marker (a
///   table without the column would 500 every include request).
#[test]
fn include_sql_marker_follows_the_auditable_decision_not_the_policy_field() {
    let pg = std::collections::HashMap::new();

    // The mox-first auditable shape: no `SoftDelete` policy, auditable table.
    let mut auditable = tree("Person", "person", vec![], None, false);
    assert!(auditable.is_auditable);
    let sql = render_include_sql(&worker(), &facts("person", "person_id", &auditable, &pg));
    assert!(
        sql.contains("\n  AND \"deleted_at\" IS NULL\nLIMIT 1;"),
        "an auditable target must carry the marker without any policy field:\n{sql}"
    );

    // A stale policy field over a non-auditable tree must NOT resurrect the
    // marker (explicit `auditable = false` — the DDL gives no column).
    auditable.is_auditable = false;
    auditable.soft_delete_column = Some("deleted_at".to_string());
    let sql = render_include_sql(&worker(), &facts("person", "person_id", &auditable, &pg));
    assert!(
        !sql.contains("IS NULL"),
        "a policy field must not invent a marker on a non-auditable tree:\n{sql}"
    );
}

/// The dot shape filters BOTH hops through the auditable decision: a live
/// intermediate over a soft-deleted leaf still excludes the row, and the
/// batch applies the same two-hop marker.
#[test]
fn dot_include_sql_markers_follow_the_auditable_decision_per_hop() {
    let pg = std::collections::HashMap::new();

    // Live intermediate, soft-deleted leaf: only the leaf marker applies.
    let intermediate = tree("Customer", "customer", vec![], None, false);
    let leaf = tree("Organization", "organization", vec![], Some("deleted_at"), false);
    let sql = render_include_sql(
        &worker(),
        &dot_facts("customer.org", "customer_id", "org_id", &intermediate, &leaf, &pg),
    );
    assert!(
        sql.contains("WHERE \"i\".\"id\" = (SELECT \"customer_id\" FROM \"common\".\"worker\" WHERE \"id\" = :source_id)\n  AND \"l\".\"deleted_at\" IS NULL\nLIMIT 1;"),
        "the leaf hop carries the marker while the live intermediate carries none:\n{sql}"
    );

    // The mirror: soft-deleted intermediate, live leaf.
    let intermediate = tree("Customer", "customer", vec![], Some("deleted_at"), false);
    let leaf = tree("Organization", "organization", vec![], None, false);
    let sql = render_include_sql(
        &worker(),
        &dot_facts("customer.org", "customer_id", "org_id", &intermediate, &leaf, &pg),
    );
    assert!(
        sql.contains("AND \"i\".\"deleted_at\" IS NULL\nLIMIT 1;"),
        "the intermediate hop carries the marker:\n{sql}"
    );
    assert!(
        !sql.contains("\"l\".\"deleted_at\""),
        "the live leaf carries no marker:\n{sql}"
    );
}
