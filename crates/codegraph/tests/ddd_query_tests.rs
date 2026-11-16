//! DD9 typed repository queries (issue #659) — the pipeline gate.
//!
//! Two contracts are pinned here, both through `driver::run` (the production
//! front door):
//!
//! 1. The golden `library.ddd` design's `query`-carrying operations lower
//!    onto the repository plane of BOTH providers as parameterized queries:
//!    the predicate's source text never reaches the generated code, the
//!    sort keys become the ORDER BY with the identity tiebreaker, and a
//!    declared `page` becomes the bounded window.
//! 2. A query whose predicate is valid pherolang but outside the
//!    SQL-lowerable subset FAILS the run with a named rejection before any
//!    entity is generated — never a broader query.
//!
//! Byte identity for query-less models is pinned by the standing canaries
//! (`ux_rules_byte_identity_tests`, `forms_byte_identity_tests`,
//! `policy_rls_snapshot_tests`) and by `ddd_pipeline_tests`'
//! design-vs-config equivalence gate; this file's last test guards that no
//! query surface leaks into a query-less model.

use std::fs;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ddd")
        .join(name)
}

const LIBRARY_DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.library]
label = "Library"
schema_dir = "library"
postgres_schema = "library"
"#;

/// The equivalence fixture's domain: one entity, no `.ddd` queries.
const EQUIVALENCE_DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.equivalence]
label = "Equivalence"
schema_dir = "equivalence"
postgres_schema = "equivalence"
"#;

/// One `driver::run` in the no-plan (all generators) backward-compat mode.
async fn run_pipeline(
    config: &Path,
    output: &Path,
    mox: &Path,
    ddd_files: &[PathBuf],
) -> codegraph::error::Result<()> {
    let mox_files = vec![mox.to_path_buf()];
    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: config,
        output,
        extension_points_path: None,
        integration_manifests: &[],
        profile_name: "default",
        variant: None,
        // Nonexistent path ⇒ the no-plan path (the `ddd_pipeline_tests`
        // recipe).
        profiles_config_path: Some(PathBuf::from("definitely-absent-profiles.toml")),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &mox_files,
        rosetta_files: &[],
        ddd_files,
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await
}

fn read(root: &Path, rel: &str) -> String {
    let path = root.join(rel);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{rel} must be generated ({e}): {}", path.display()))
}

#[tokio::test]
async fn library_query_operations_lower_to_parameterized_repository_methods() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("domains.toml"), LIBRARY_DOMAINS_TOML).unwrap();
    let output = root.path().join("generated");
    run_pipeline(
        &root.path().join("domains.toml"),
        &output,
        &fixture("library.mox"),
        &[fixture("library.ddd")],
    )
    .await
    .expect("the library design generates");

    let trait_src = read(&output, "src/domain/library/movie/repository.rs");
    assert!(
        trait_src.contains("/// Design query `recentByGenre`"),
        "the many-valued query is declared on the trait: {trait_src}"
    );
    assert!(
        trait_src.contains("/// Read capability: ReadMovies."),
        "the query's read capability is documented: {trait_src}"
    );
    assert!(
        trait_src.contains("async fn recent_by_genre(")
            && trait_src.contains("target_genre: String,")
            && trait_src.contains("minimum_runtime: i64,"),
        "the declared parameters keep their mapped types: {trait_src}"
    );
    assert!(
        trait_src.contains("limit: Option<u64>,") && trait_src.contains("offset: Option<u64>,"),
        "the declared `page` adds the runtime page parameters: {trait_src}"
    );
    assert!(
        trait_src.contains(
            "async fn first_by_runtime(\n        &self,\n        db: &C,\n        minimum_runtime: i64,"
        ) || trait_src.contains("async fn longest_runtime("),
        "the 0..1 query is declared: {trait_src}"
    );

    let impl_src = read(&output, "src/domain/library/movie/repository_impl.rs");
    // The predicate is parameterized: values are bound arguments, and the
    // verbatim predicate source text appears nowhere.
    assert!(
        impl_src.contains("crate::entity::library_movie::Column::Genre.eq(target_genre.clone())"),
        "the genre comparison binds the parameter: {impl_src}"
    );
    assert!(
        impl_src
            .contains("crate::entity::library_movie::Column::RuntimeMinutes.gte(minimum_runtime)"),
        "the runtime comparison binds the parameter: {impl_src}"
    );
    for verbatim in [
        "self.genre == targetGenre",
        "targetGenre &&",
        ">= minimumRuntime",
    ] {
        assert!(
            !impl_src.contains(verbatim),
            "the predicate must never be interpolated into the generated code ({verbatim}): \
             {impl_src}"
        );
    }
    assert!(
        impl_src.contains("order_by_asc(crate::entity::library_movie::Column::Title)"),
        "the declared sort key orders the query: {impl_src}"
    );
    assert!(
        impl_src.contains("order_by_asc(crate::entity::library_movie::Column::Id)"),
        "the identity tiebreaker keeps the page deterministic: {impl_src}"
    );
    assert!(
        impl_src.contains(
            "let page_size = limit.or(Some(20u64)).map(|page_size| page_size.min(100u64));"
        ),
        "`page limit 20 max 100` becomes the bounded window: {impl_src}"
    );
    assert!(
        impl_src.contains("query.offset(offset.unwrap_or(0))"),
        "{impl_src}"
    );

    // The 0..1 query returns an optional and never throws "not found".
    let single = impl_src
        .split("async fn first_by_runtime(")
        .nth(1)
        .or_else(|| impl_src.split("async fn longest_runtime(").nth(1))
        .expect("the single-row query is emitted");
    assert!(
        single.contains("Result<Option<MovieResponse>, Box<dyn std::error::Error>>"),
        "0..1 returns an optional: {single}"
    );
    assert!(single.contains("query.one(db)"), "{single}");
    assert!(
        single.contains("None => return Ok(None),"),
        "a missing row is Ok(None), never an error: {single}"
    );

    // The equality-finder path is RETAINED for query-less declared ops.
    assert!(
        impl_src.contains("async fn find_by_title("),
        "query-less declared operations keep the equality finder: {impl_src}"
    );
    assert!(
        impl_src.contains("Column::Title.eq(title.clone())"),
        "the equality finder still binds its parameter: {impl_src}"
    );
}

#[tokio::test]
async fn a_non_lowerable_query_fails_the_run_before_any_entity_is_generated() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("domains.toml"), LIBRARY_DOMAINS_TOML).unwrap();
    let output = root.path().join("generated");
    let error = run_pipeline(
        &root.path().join("domains.toml"),
        &output,
        &fixture("library.mox"),
        &[fixture("query_refusal.ddd")],
    )
    .await
    .expect_err("the run must fail");
    let message = error.to_string();
    assert!(
        message.contains("validation errors found"),
        "the validation pass reports the count: {message}"
    );
    // No domain tree was written — the refusal precedes generation.
    assert!(
        !output.join("src").exists(),
        "a refused query must not half-generate the app: {:?}",
        output.join("src")
    );
}

#[tokio::test]
async fn the_refusal_message_names_the_repository_operation_and_construct() {
    // The same seam the run path uses (`ValidationPass` →
    // `query_refusals`), asserted directly so the wording is pinned.
    let root = tempfile::tempdir().unwrap();
    let config_path = root.path().join("domains.toml");
    fs::write(&config_path, LIBRARY_DOMAINS_TOML).unwrap();
    let config = codegraph_config::config::parse_domain_config(&config_path)
        .expect("fixture domains.toml parses");

    let engine = codegraph_core::mock::MockEngine::builder().build();
    codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        &[fixture("library.mox")],
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .expect("the mox fixture ingests");
    codegraph::ingest::ddd_ingest::ingest_ddd_files(
        &engine,
        &engine,
        &[fixture("query_refusal.ddd")],
        &config,
    )
    .await
    .expect("the refusal design is a VALID pherolang model — it compiles");

    let refusals = codegraph_generate::ddd::design::query_refusals(&engine)
        .await
        .expect("the refusal seam runs");
    assert_eq!(
        refusals.len(),
        1,
        "exactly one operation is out of subset: {refusals:?}"
    );
    let message = &refusals[0];
    assert_eq!(
        message,
        "query `MovieRepository.longerThan`: arithmetic (`+`) inside a comparison operand \
         cannot be lowered to a SQL predicate; the supported subset is stored scalar feature \
         refs, operation parameters, string/number/boolean/date literals, == != < <= > >=, \
         && || !, parentheses, and null comparisons"
    );
}

#[tokio::test]
async fn a_query_less_design_emits_no_query_surface() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("domains.toml"), EQUIVALENCE_DOMAINS_TOML).unwrap();
    let output = root.path().join("generated");
    run_pipeline(
        &root.path().join("domains.toml"),
        &output,
        &fixture("equivalence.mox"),
        &[fixture("equivalence.ddd")],
    )
    .await
    .expect("the query-less design generates");

    let mut checked = 0usize;
    for dir in ["src/domain/equivalence/widget", "src/entity", "migrations"] {
        let path = output.join(dir);
        if !path.exists() {
            continue;
        }
        for entry in fs::read_dir(&path).expect("dir readable") {
            let entry = entry.expect("entry");
            if !entry.path().is_file() {
                continue;
            }
            let content = fs::read_to_string(entry.path()).unwrap_or_default();
            assert!(
                !content.contains("Design query")
                    && !content.contains("design_query")
                    && !content.contains("order_by_asc(crate::entity"),
                "a query-less design must emit no query surface: {}",
                entry.path().display()
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "the fixture must emit files to check");
}
