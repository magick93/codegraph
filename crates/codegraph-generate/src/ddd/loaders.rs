//! The by-id loaders every plane must agree on (issue #869).
//!
//! The generated handlers read a row back through the query handler's
//! `find_by_id` / `find_by_id_scoped` for more than the `read` op: the
//! create and update re-reads, the child ownership pre-checks on update and
//! delete, and the guarded root delete all call them. The query plane used to
//! emit both loaders only under `has_read`, so a design exposing `save` (or
//! `delete`) without `findById` generated handlers calling methods that did
//! not exist (E0599).
//!
//! This module is the ONE derivation. The query handler (`query.tera`), the
//! repository trait (`repository.tera`), both repository impls (SeaORM
//! `repository_emitter`, cornucopia `cornucopia_repo`), the scaffold's
//! `QueryHandler::new` arity, and the child-op parent load all read it.

/// Which by-id loaders the query handler exposes for one entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QueryLoaders {
    /// `find_by_id(id, …)` — the unscoped load.
    pub find_by_id: bool,
    /// `find_by_id_scoped(id, parent_id, …)` — the parent-scoped load.
    pub find_by_id_scoped: bool,
}

impl QueryLoaders {
    /// The loaders the handler plane calls, from the entity's resolved op set
    /// and whether it carries a parent reference.
    ///
    /// - `find_by_id`: the create re-read and the update re-read (both
    ///   roles), the root read, and the guarded root delete.
    /// - `find_by_id_scoped`: the child read, and the child update/delete
    ///   ownership pre-checks.
    #[must_use]
    pub const fn for_ops(
        has_create: bool,
        has_read: bool,
        has_update: bool,
        has_delete: bool,
        has_parent_ref: bool,
    ) -> Self {
        Self {
            find_by_id: has_create || has_update || (!has_parent_ref && (has_read || has_delete)),
            find_by_id_scoped: has_parent_ref && (has_read || has_update || has_delete),
        }
    }

    /// [`Self::for_ops`] over an operation-name list (`"create"`, `"read"`,
    /// …), the shape every context builder already holds.
    #[must_use]
    pub fn for_operations<S: AsRef<str>>(operations: &[S], has_parent_ref: bool) -> Self {
        let has = |op: &str| operations.iter().any(|o| o.as_ref() == op);
        Self::for_ops(
            has("create"),
            has("read"),
            has("update"),
            has("delete"),
            has_parent_ref,
        )
    }

    /// Whether the REPOSITORY plane (trait + impls) needs `find_by_id`: the
    /// query handler's need, plus the FTS / embedding search hydration, which
    /// reads every hit back through the repository directly.
    #[must_use]
    pub const fn repository_find_by_id(self, has_fts: bool, has_embeddings: bool) -> bool {
        self.find_by_id || has_fts || has_embeddings
    }
}

#[cfg(test)]
mod tests {
    use super::QueryLoaders;

    #[test]
    fn root_with_read_has_only_the_unscoped_loader() {
        let l = QueryLoaders::for_ops(false, true, false, false, false);
        assert_eq!(
            l,
            QueryLoaders {
                find_by_id: true,
                find_by_id_scoped: false
            }
        );
    }

    #[test]
    fn save_without_find_by_id_still_gets_the_re_read_loader() {
        // `save` = create + update, no `findById` (#869).
        let root = QueryLoaders::for_ops(true, false, true, false, false);
        assert!(root.find_by_id);
        assert!(!root.find_by_id_scoped);
        let child = QueryLoaders::for_ops(true, false, true, false, true);
        assert!(child.find_by_id, "the create/update re-read is unscoped");
        assert!(child.find_by_id_scoped, "the child update ownership check");
    }

    #[test]
    fn guarded_root_delete_loads_unscoped_and_child_delete_loads_scoped() {
        assert!(QueryLoaders::for_ops(false, false, false, true, false).find_by_id);
        let child = QueryLoaders::for_ops(false, false, false, true, true);
        assert!(!child.find_by_id);
        assert!(child.find_by_id_scoped);
    }

    #[test]
    fn child_read_is_scoped_only() {
        let l = QueryLoaders::for_ops(false, true, false, false, true);
        assert!(!l.find_by_id);
        assert!(l.find_by_id_scoped);
    }

    #[test]
    fn an_empty_op_set_has_no_loader() {
        for parent in [false, true] {
            assert_eq!(
                QueryLoaders::for_ops(false, false, false, false, parent),
                QueryLoaders::default()
            );
        }
    }

    #[test]
    fn list_only_designs_need_no_by_id_loader() {
        let l = QueryLoaders::for_operations(&["list"], false);
        assert_eq!(l, QueryLoaders::default());
    }

    #[test]
    fn repository_plane_adds_search_hydration() {
        let none = QueryLoaders::default();
        assert!(!none.repository_find_by_id(false, false));
        assert!(none.repository_find_by_id(true, false));
        assert!(none.repository_find_by_id(false, true));
    }

    /// The pre-#869 query plane emitted `find_by_id` for
    /// `read && (create || !parent_ref)` and `find_by_id_scoped` for
    /// `read && parent_ref`. Every op set that carries `read` keeps at least
    /// those loaders, so a design with `read` never loses a method.
    #[test]
    fn every_read_bearing_op_set_keeps_its_pre_869_loaders() {
        for bits in 0u8..16 {
            let (c, u, d, p) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0, bits & 8 != 0);
            let l = QueryLoaders::for_ops(c, true, u, d, p);
            if c || !p {
                assert!(l.find_by_id, "c={c} u={u} d={d} p={p}");
            }
            if p {
                assert!(l.find_by_id_scoped, "c={c} u={u} d={d} p={p}");
            }
        }
    }
}
