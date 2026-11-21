//! The one soft-delete derivation behind `?include=` (issue #784).
//!
//! The include fetch planes (SeaORM emitter, cornucopia SQL) must agree on
//! WHEN a related row's soft-delete marker filters the include — and the
//! marker column must exist on the target's table AND on its entity model
//! when it does. Both providers consume [`include_soft_delete_marker`], so
//! the two planes cannot drift (pinned by
//! `both_providers_derive_the_include_marker_from_one_predicate`).

use super::types::EntityTree;

/// The soft-delete marker an `?include=` fetch filters on: `deleted_at`
/// exactly when the read plane calls the target auditable.
///
/// The predicate is [`EntityTree::is_auditable`] — the SAME decision the
/// query/repository planes use to gate their `include_deleted` parameters
/// and their `Column::DeletedAt.is_null()` filters (`ddd/query.rs`,
/// `repository_emitter/crud.rs`), and the decision the emitted handlers pass
/// (`false`) against. Includes therefore agree with the CRUD read contract by
/// construction: a related row the primary resource would have hidden is
/// hidden from the include too.
///
/// Why the read plane's decision and not the DDL context's own auditable
/// decision: the entity model carries the `DeletedAt` column exactly when the
/// read plane filters on it (`db/entity.rs`'s auditable push — "the
/// repository emitter's auditable soft-delete filter compiles only when the
/// entity model has the column"), so a filter derived HERE compiles wherever
/// the read plane does. The DDL emits `deleted_at` for the same default
/// domain decision, so the column exists for every auditable tree; the
/// exotic divergences (a `.ddd` design's `auditable = false` over an
/// auditable domain, an `Audit` policy with `track_deleted = false`) are
/// pre-existing tensions shared with the read plane, not new risk introduced
/// by includes.
///
/// Why NOT the policy-derived [`EntityTree::soft_delete_column`]: that field
/// comes from a `SoftDelete` policy node and
/// `ingest_policies_and_relationships` has no caller on the mox-first driver
/// path, so it is `None` for every mox-first model — the #784 root cause.
/// The include planes stopped reading it; its non-include consumers (the
/// policy-driven query SQL shapes) are untouched.
///
/// `None` means "no marker on this target": the include hydrates unfiltered,
/// exactly like a tree the emitter could not resolve. No column is ever
/// invented — a marker naming a column the table lacks would 500 every
/// include request.
pub(crate) fn include_soft_delete_marker(tree: &EntityTree) -> Option<&'static str> {
    tree.is_auditable.then_some("deleted_at")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The derivation for the three entity shapes that matter: auditable by
    /// default (the mox-first shape — the marker exists even though no
    /// policy ever reached the graph), explicitly non-auditable, and
    /// append-only (whose DDL carries no audit column at all).
    #[test]
    fn marker_follows_the_read_plane_auditable_decision() {
        // Auditable default: marker present.
        let mut tree = test_tree();
        tree.is_auditable = true;
        assert_eq!(include_soft_delete_marker(&tree), Some("deleted_at"));

        // Explicit `auditable = false`: no marker, whatever a stale policy
        // field claims.
        tree.is_auditable = false;
        tree.soft_delete_column = Some("deleted_at".to_string());
        assert_eq!(include_soft_delete_marker(&tree), None);

        // Append-only: `is_auditable` is already false (`(...) &&
        // !append_only`), so the marker is absent and the table's missing
        // audit columns are never referenced.
        let tree = test_tree();
        assert_eq!(include_soft_delete_marker(&tree), None);
    }

    fn test_tree() -> EntityTree {
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
            is_auditable: false,
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
}
