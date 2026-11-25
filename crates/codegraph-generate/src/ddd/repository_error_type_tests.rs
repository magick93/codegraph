//! Issue #813 — the repository plane's error type is
//! `Box<dyn std::error::Error + Send + Sync>` in BOTH persistence providers.
//!
//! The mechanism. The `{Entity}Repository<C>` trait is provider-neutral, so
//! its error type is one spelling shared by the SeaORM impl
//! (`repository_impl.rs`) and the cornucopia adapter
//! (`cornucopia_repository_impl.rs`), both under `#[async_trait]`, which
//! requires every method future to be `Send`. A `?` that sits in the
//! scrutinee of an `if let` (or in the head of a `for`) holds its `Result`
//! temporary until the end of the whole statement — so when the success block
//! goes on to `.await` something else, the error type is live across that
//! await. PR #811 replaced the cornucopia adapter's `|e| e.to_string()`
//! mapping with `db_err`, returning `Box<dyn std::error::Error>` (no
//! `Send + Sync`): a `String` crossing the await was fine, the non-Send box
//! was not. The shape exists exactly for an entity with a contained
//! value-object collection, whose `find_by_id` reads the child rows inside
//! the load's success block (and whose `list` / finders read them inside the
//! row loop), and it turned every such repository future `!Send` in a
//! consumer's generated workers workspace.
//!
//! The fix is a whole-plane sweep, and that is what these pins hold:
//!
//! - [`repository_plane_error_boxes_are_send_sync`] — every occurrence of the
//!   error trait object in the seven repository-plane sources is followed by
//!   ` + Send + Sync` (the trait template, both provider emitters, and the
//!   include adapter). A non-Send box anywhere in the plane re-opens the hole
//!   for the shape that holds a `?` temporary across an await.
//! - [`db_err_only_maps_cornucopia_query_results`] — `db_err` takes
//!   `tokio_postgres::Error`, so it may only be attached to a query call.
//!   PR #811's blanket replacement also attached it to `serde_json::to_value`
//!   (a `serde_json::Error`) in the as-json `find_tree` arm: a type error no
//!   in-repo cornucopia fixture reached.
//! - [`as_json_find_tree_maps_the_serialization_error_through_its_own_closure`]
//!   — that arm's serialization error gets its own Send + Sync mapping.
//!
//! `templates/ddd/errors.tera` deliberately keeps BOTH
//! `From<Box<dyn std::error::Error>>` impls; it is not part of the sweep.

use crate::code_writer::CodeWriter;
use crate::ddd::cornucopia_repo::emit_adapter_tree;
use crate::ddd::repository_emitter::{EntityTree, TreeIncludeResolved};

/// The error trait object every repository-plane signature and ascription
/// spells.
const ERROR_TRAIT_OBJECT: &str = "dyn std::error::Error";

/// The bound that must immediately follow it.
const SEND_SYNC_BOUND: &str = " + Send + Sync";

/// The repository plane: the provider-neutral trait template plus the six
/// Rust emitters that spell the error box. `include_str!` (not a filesystem
/// read) so a path rename is a compile error, never a silently empty pin.
const REPOSITORY_PLANE_SOURCES: &[(&str, &str)] = &[
    (
        "templates/ddd/repository.tera",
        include_str!("../../templates/ddd/repository.tera"),
    ),
    (
        "src/ddd/cornucopia_repo.rs",
        include_str!("cornucopia_repo.rs"),
    ),
    (
        "src/ddd/cornucopia_repo_include.rs",
        include_str!("cornucopia_repo_include.rs"),
    ),
    (
        "src/ddd/repository_emitter/crud.rs",
        include_str!("repository_emitter/crud.rs"),
    ),
    (
        "src/ddd/repository_emitter/finders.rs",
        include_str!("repository_emitter/finders.rs"),
    ),
    (
        "src/ddd/repository_emitter/query_search.rs",
        include_str!("repository_emitter/query_search.rs"),
    ),
    (
        "src/ddd/repository_emitter/tree.rs",
        include_str!("repository_emitter/tree.rs"),
    ),
];

/// The cornucopia adapter emitters — the only sources that spell `db_err`.
const CORNUCOPIA_ADAPTER_SOURCES: &[(&str, &str)] = &[
    (
        "src/ddd/cornucopia_repo.rs",
        include_str!("cornucopia_repo.rs"),
    ),
    (
        "src/ddd/cornucopia_repo_include.rs",
        include_str!("cornucopia_repo_include.rs"),
    ),
];

/// Every line of `source` that spells the error trait object WITHOUT the
/// `Send + Sync` bound. Comment lines are skipped: prose may name the
/// non-Send box (this module's own doc comment does). A string literal that
/// must carry the old spelling — a negative assertion, say — has to build it
/// by concatenation, because a literal IS an occurrence.
fn non_send_spelling_lines(source: &str) -> Vec<(usize, &str)> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .filter_map(|(index, line)| {
            let mut search_from = 0;
            while let Some(offset) = line[search_from..].find(ERROR_TRAIT_OBJECT) {
                let after = &line[search_from + offset + ERROR_TRAIT_OBJECT.len()..];
                if !after.starts_with(SEND_SYNC_BOUND) {
                    return Some((index + 1, line));
                }
                search_from += offset + ERROR_TRAIT_OBJECT.len();
            }
            None
        })
        .collect()
}

/// The whole repository plane spells its error box `Send + Sync`.
#[test]
fn repository_plane_error_boxes_are_send_sync() {
    let mut offenders = Vec::new();
    for (path, source) in REPOSITORY_PLANE_SOURCES {
        for (line, text) in non_send_spelling_lines(source) {
            offenders.push(format!("{path}:{line}: {text}"));
        }
    }
    assert!(
        offenders.is_empty(),
        "repository-plane error boxes must be `{ERROR_TRAIT_OBJECT}{SEND_SYNC_BOUND}` \
         (issue #813 — a non-Send box makes a repository future `!Send` \
         whenever a `?` temporary crosses an `.await`):\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_checker_rejects_a_non_send_box() {
    let source = "    ) -> Result<Uuid, Box<dyn std::error::Error>> {{\n";
    assert_eq!(
        non_send_spelling_lines(source),
        vec![(1, source.trim_end())]
    );
}

#[test]
fn the_checker_rejects_a_non_send_turbofish_ascription() {
    let source = ".ok_or_else(|| Box::<dyn std::error::Error>::from(\"gone\"))?;";
    assert_eq!(non_send_spelling_lines(source), vec![(1, source)]);
}

#[test]
fn the_checker_accepts_the_send_sync_box() {
    let source = "    ) -> Result<Uuid, Box<dyn std::error::Error + Send + Sync>> {{\n\
                  \x20        fn db_err(e: tokio_postgres::Error) -> Box<dyn std::error::Error + Send + Sync> {{\n";
    assert!(non_send_spelling_lines(source).is_empty(), "{source}");
}

#[test]
fn the_checker_checks_every_occurrence_on_a_line() {
    let source = "Box<dyn std::error::Error + Send + Sync> then Box<dyn std::error::Error>";
    assert_eq!(non_send_spelling_lines(source), vec![(1, source)]);
}

/// Prose may name the non-Send box; only code is pinned.
#[test]
fn the_checker_skips_comment_lines() {
    let source = "// the pre-#813 spelling was Box<dyn std::error::Error>\n";
    assert!(non_send_spelling_lines(source).is_empty(), "{source}");
}

/// `db_err` takes `tokio_postgres::Error`, so it may only be attached to a
/// cornucopia query call — every such emitted line carries the `.bind(..)`
/// whose result it maps. PR #811's blanket replacement also attached it to
/// `serde_json::to_value` in the as-json `find_tree` arm, which cannot
/// compile (issue #813's sibling defect).
#[test]
fn db_err_only_maps_cornucopia_query_results() {
    let mut offenders = Vec::new();
    for (path, source) in CORNUCOPIA_ADAPTER_SOURCES {
        for (index, line) in source.lines().enumerate() {
            if line.contains("map_err(db_err)") && !line.contains(".bind(") {
                offenders.push(format!("{path}:{}: {line}", index + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "`db_err` maps `tokio_postgres::Error` — attach it to a query call only:\n{}",
        offenders.join("\n")
    );
}

/// The as-json `find_tree` arm serializes the response with
/// `serde_json::to_value`, whose error is NOT `tokio_postgres::Error`: it
/// gets its own mapping closure, still a Send + Sync box holding the typed
/// error (the #810 intent).
#[test]
fn as_json_find_tree_maps_the_serialization_error_through_its_own_closure() {
    let mut tree = minimal_tree("PayRun", "pay_run");
    tree.hierarchy_field = Some("parent_id".to_string());
    tree.tree_include.push(TreeIncludeResolved {
        alias: "deployed_worker".to_string(),
        via_table: "common.deployment".to_string(),
        via_fk_column: "position_id".to_string(),
        parent_table: "common.worker".to_string(),
        parent_ref_column: "worker_id".to_string(),
        worker_detail_joins: Vec::new(),
    });
    let mut code = CodeWriter::new();
    emit_adapter_tree(&tree, &mut code);
    let out = code.as_str();

    // The sibling defect first: PR #811 attached the DATABASE error mapping
    // to a `serde_json::Error`.
    let serialization = out
        .lines()
        .find(|line| line.contains("serde_json::to_value(resp)"))
        .unwrap_or_else(|| panic!("the arm serializes the response:\n{out}"));
    assert!(
        !serialization.contains("db_err"),
        "`db_err` takes the database error type — never the serialization error:\n{serialization}"
    );
    assert_eq!(
        serialization.trim(),
        "let mut val = serde_json::to_value(resp).map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;",
        "the serialization error maps through its own Send + Sync closure"
    );
    assert!(
        out.contains(&format!(
            ") -> Result<Vec<serde_json::Value>, Box<{ERROR_TRAIT_OBJECT}{SEND_SYNC_BOUND}>> {{"
        )),
        "the as-json tree read returns the Send + Sync box:\n{out}"
    );
}

/// A typed (non-as-json) tree read keeps the typed error mapping untouched.
#[test]
fn typed_find_tree_never_serializes() {
    let mut tree = minimal_tree("PayRun", "pay_run");
    tree.hierarchy_field = Some("parent_id".to_string());
    let mut code = CodeWriter::new();
    emit_adapter_tree(&tree, &mut code);
    let out = code.as_str();

    assert!(
        out.contains(&format!(
            ") -> Result<Vec<PayRunResponse>, Box<{ERROR_TRAIT_OBJECT}{SEND_SYNC_BOUND}>> {{"
        )),
        "{out}"
    );
    assert!(
        !out.contains("serde_json::to_value(resp)"),
        "a typed tree read has nothing to serialize:\n{out}"
    );
}

/// The smallest `EntityTree` that renders a tree read: no columns beyond the
/// synthetic timestamps, no children.
fn minimal_tree(entity: &str, table: &str) -> EntityTree {
    EntityTree {
        entity_name: entity.to_string(),
        module_name: table.to_string(),
        schema_name: "common".to_string(),
        table_name: table.to_string(),
        entity_module: format!("common_{table}"),
        namespace: None,
        direct_columns: Vec::new(),
        child_tables: Vec::new(),
        junction_tables: Vec::new(),
        has_create: false,
        has_read: true,
        has_update: false,
        has_delete: false,
        has_list: false,
        has_workflow: false,
        has_fts: false,
        has_embeddings: false,
        fts_language: "english".to_string(),
        is_auditable: true,
        soft_delete_visibility: "exclude_by_default".to_string(),
        soft_delete_column: Some("deleted_at".to_string()),
        soft_delete_cascade: "restrict".to_string(),
        track_updated_user: false,
        track_deleted_user: false,
        append_only: false,
        filter_fields: Vec::new(),
        nested_filter_fields: Vec::new(),
        parent_ref: None,
        hierarchy_field: None,
        tree_include: Vec::new(),
        design_finders: Vec::new(),
    }
}
