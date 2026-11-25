//! Issue #824 — the machine read model exists only where it has a LOADER,
//! and it addresses the DTO's own field name.
//!
//! The #817 read-model route (`api/machine_read.tera`, PR #819) rendered
//! for every machine-bearing design. Two ordinary shapes did not compile,
//! and a consumer's generated workers workspace failed `cargo check`:
//!
//! 1. **A machine on a design without a repository.** Its op set is empty,
//!    so its query handler has no `find_by_id` — the method the route
//!    module loads the row through (`E0599`).
//! 2. **A machine bound to a `…Code`-typed status field.** Ingest strips
//!    the suffix from the Rust field (`approvalStatusCode` →
//!    `approval_status`; the column keeps it), and the module addressed the
//!    model name's bare snake_case (`E0609`: no field
//!    `approval_status_code`).
//!
//! Why no gate saw it: the route is monolith-only, so on workers it is
//! unrouted code that merely has to compile, and every fixture put its
//! machines on root scaffold entities with plain `*_status` fields.
//!
//! Node-free pins over the REAL driver (`driver::run`, the
//! `phantom_machine_tests` shape) on `tests/fixtures/machine_read_shapes`:
//! both shapes in one recruiting module, a monolith profile (the topology
//! that mounts the route) and a workers profile (the consumer's).
//!
//! The contract pinned here:
//!
//! - **One loader rule, every consumer.** The read model is withheld —
//!   module, `/workflow` merge, router, OpenAPI, API contract, UI — for a
//!   machine-bearing entity whose query plane exposes no unscoped
//!   `find_by_id`; the kernel (registry embed, seed, guards) is untouched.
//! - **The withholding is visible**: the run-path validation pass emits a
//!   Warning naming the class and the reason.
//! - **The status accessor is the DTO's field name**, read from the graph
//!   property the DTO emitter names its field by.
//!
//! ```bash
//! cargo test -p codegraph --test machine_read_shape_tests
//! ```
//!
//! The compile half lives in `service_ops_compile_gate`
//! (`generated_app_with_machine_read_shapes_compiles`): the whole emitted
//! app over this fixture must `cargo check` clean.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/machine_read_shapes")
}

struct Fixture {
    root: TempDir,
    config: PathBuf,
    mox: PathBuf,
    ddd: PathBuf,
    profiles: PathBuf,
}

/// Copy the committed fixture into a temp dir so generated output never
/// lands inside the checkout (the `phantom_machine_tests` shape).
fn write_fixture() -> Fixture {
    let src = fixture_dir();
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("model")).unwrap();
    fs::create_dir_all(root.path().join("design")).unwrap();
    let mox = root.path().join("model/recruiting.mox");
    let ddd = root.path().join("design/recruiting.ddd");
    let config = root.path().join("domains.toml");
    let profiles = root.path().join("profiles.toml");
    fs::copy(src.join("model/recruiting.mox"), &mox).unwrap();
    fs::copy(src.join("design/recruiting.ddd"), &ddd).unwrap();
    fs::copy(src.join("domains.toml"), &config).unwrap();
    fs::copy(src.join("profiles.toml"), &profiles).unwrap();
    Fixture {
        root,
        config,
        mox,
        ddd,
        profiles,
    }
}

/// Run the real driver over the fixture under one of its profiles and
/// return the output directory.
async fn generate(fixture: &Fixture, profile: &str) -> PathBuf {
    let output = fixture.root.path().join(format!("out-{profile}"));
    generate_into(fixture, profile, &output).await;
    output
}

/// [`generate`] into a caller-chosen directory (the regeneration shape).
async fn generate_into(fixture: &Fixture, profile: &str, output: &Path) {
    let mox_files = vec![fixture.mox.clone()];
    let ddd_files = vec![fixture.ddd.clone()];
    codegraph::driver::run(codegraph::driver::RunArgs {
        integration_manifests: &[],
        schemas: None,
        classifier: None,
        config_path: &fixture.config,
        output,
        extension_points_path: None,
        profile_name: profile,
        variant: None,
        profiles_config_path: Some(fixture.profiles.clone()),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &mox_files,
        rosetta_files: &[],
        ddd_files: &ddd_files,
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
    .unwrap_or_else(|e| panic!("generation under `{profile}` must succeed: {e}"));
}

/// `relative path -> content` for every file under `root`.
fn tree(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    fn walk(dir: &Path, base: &Path, out: &mut BTreeMap<String, String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                let rel = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.insert(rel, fs::read_to_string(&path).unwrap_or_default());
            }
        }
    }
    walk(root, root, &mut out);
    out
}

fn read<'a>(tree: &'a BTreeMap<String, String>, rel: &str) -> &'a str {
    tree.get(rel)
        .unwrap_or_else(|| panic!("expected generated file `{rel}`; have:\n{}", keys(tree)))
}

fn keys(tree: &BTreeMap<String, String>) -> String {
    tree.keys().cloned().collect::<Vec<_>>().join("\n")
}

/// Every file under `tree` whose content mentions `needle`.
fn files_mentioning<'a>(tree: &'a BTreeMap<String, String>, needle: &str) -> Vec<&'a str> {
    tree.iter()
        .filter(|(_, content)| content.contains(needle))
        .map(|(rel, _)| rel.as_str())
        .collect()
}

/// Ingest the fixture (mox, then the design plane) into one engine, for
/// the pins that read the graph directly.
async fn ingested() -> codegraph_grafeo::GrafeoEngine {
    let fixture = write_fixture();
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let engine = codegraph_grafeo::GrafeoEngine::in_memory().unwrap();
    let mox_files = vec![fixture.mox.clone()];
    codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        &mox_files,
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap();
    let ddd_files = vec![fixture.ddd.clone()];
    codegraph::ingest::ddd_ingest::ingest_ddd_files(&engine, &engine, &ddd_files, &config)
        .await
        .unwrap();
    engine
}

// ── 1. The loader rule, read off the graph ─────────────────────────────────

/// Both designs are machine-bearing; only the one with a repository has a
/// read model. The kernel surface is untouched by the rule.
#[tokio::test]
async fn only_the_machine_with_a_loader_has_a_read_model() {
    let engine = ingested().await;
    let config = codegraph_config::config::parse_domain_config(&write_fixture().config).unwrap();

    let mut bearing: Vec<String> = codegraph_generate::machines::machine_bearing_titles(&engine)
        .await
        .into_iter()
        .collect();
    bearing.sort();
    assert_eq!(bearing, ["Application", "PositionOpeningType"]);

    let mut with_read_model: Vec<String> =
        codegraph_generate::machines::machine_read_model_titles(&engine, &config)
            .await
            .into_iter()
            .collect();
    with_read_model.sort();
    assert_eq!(
        with_read_model,
        ["PositionOpeningType"],
        "a machine-bearing design without a loader keeps its machine, not a read model"
    );

    // The kernel surface still resolves for BOTH: the registry embed, the
    // seed and the guards key on it.
    assert!(
        codegraph_generate::machines::machine_surface_for_entity(&engine, "Application", None)
            .await
            .is_some(),
        "the loader-less design is still machine-bearing"
    );
    assert!(
        codegraph_generate::machines::machine_read_model_surface_for_entity(
            &engine,
            &config,
            "recruiting",
            "Application",
            None,
        )
        .await
        .is_none(),
        "…but its read model is withheld"
    );
}

/// The loader predicate itself — the two facts `ddd/query.tera` reads.
#[test]
fn loader_predicate_is_read_without_a_parent_reference() {
    use codegraph_generate::machines::machine_read_model_has_loader;
    assert!(
        machine_read_model_has_loader(true, false),
        "a root with a read op"
    );
    assert!(
        !machine_read_model_has_loader(false, false),
        "no read op: a design without a repository (an empty op set) has no `find_by_id`"
    );
    assert!(
        !machine_read_model_has_loader(false, true),
        "no read op, parent-scoped"
    );
    assert!(
        !machine_read_model_has_loader(true, true),
        "a parent-scoped member loads through `find_by_id_scoped`, which the read model does not use"
    );
}

// ── 2. Monolith: the loader-less machine is withheld on every surface ──────

#[tokio::test]
async fn loaderless_machine_emits_no_read_model_on_any_surface() {
    let fixture = write_fixture();
    let output = generate(&fixture, "default").await;
    let tree = tree(&output);

    // The module, and the `pub mod` line the filesystem-scanning mod pass
    // would otherwise re-declare it by.
    assert!(
        !tree.contains_key("src/api/recruiting/application_machine.rs"),
        "the read-model module must not be emitted without a loader"
    );
    let api_mod = read(&tree, "src/api/recruiting/mod.rs");
    assert!(
        !api_mod.contains("application_machine"),
        "no `pub mod application_machine;`:\n{api_mod}"
    );
    // The router, the OpenAPI document and — transitively, through the
    // router entity — the API contract.
    let router = read(&tree, "src/api/recruiting/router.rs");
    assert!(!router.contains("application_machine"), "{router}");
    let openapi = read(&tree, "src/api/openapi/recruiting.rs");
    assert!(!openapi.contains("application_machine"), "{openapi}");
    assert!(
        files_mentioning(&tree, "application_machine").is_empty(),
        "no surface may reference the withheld module: {:?}",
        files_mentioning(&tree, "application_machine")
    );

    // Why: the query handler has no loader at all.
    let query = read(&tree, "src/domain/recruiting/application/query.rs");
    assert!(
        !query.contains("fn find_by_id"),
        "a design without a repository has no `find_by_id`:\n{query}"
    );

    // The kernel is untouched: the machine is still in the registry.
    let registry = read(&tree, "domain-types/src/machines/mod.rs");
    assert!(
        registry.contains("disposition"),
        "the withheld read model must not take the kernel embed with it:\n{registry}"
    );
}

// ── 3. Monolith: the `…Code` status field is addressed by its DTO name ─────

#[tokio::test]
async fn code_typed_status_field_is_addressed_by_its_dto_name() {
    let fixture = write_fixture();
    let output = generate(&fixture, "default").await;
    let tree = tree(&output);

    // The DTO's own spelling: the Rust field drops the suffix. (The
    // module stem is the table name, `position_opening`; the title stays
    // `PositionOpeningType`. The struct lives in the domain-types crate —
    // the app-level file only re-exports it.)
    let dto: String = tree
        .iter()
        .filter(|(rel, _)| rel.ends_with("position_opening/dto_response.rs"))
        .map(|(_, content)| content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!dto.is_empty(), "no Response DTO for position_opening");
    assert!(
        dto.contains("pub approval_status:") && !dto.contains("pub approval_status_code:"),
        "the Response DTO names the field `approval_status`:\n{dto}"
    );

    // The read model reads THAT field.
    let module = read(&tree, "src/api/recruiting/position_opening_machine.rs");
    assert!(
        module.contains("entity.approval_status.to_string()"),
        "the read model must address the DTO field:\n{module}"
    );
    assert!(
        !module.contains("entity.approval_status_code"),
        "the model name's bare snake_case is a field the DTO does not have (E0609). (The \
         MACHINE_ID const may still spell `approval_status_code`: it is derived from the \
         machine's NAME, which keeps the suffix.)\n{module}"
    );

    // And it is a real read model: routed, registered, loading through the
    // query handler's unscoped `find_by_id`.
    assert!(module.contains("_queries.find_by_id("), "{module}");
    let router = read(&tree, "src/api/recruiting/router.rs");
    assert!(
        router.contains("position_opening_machine::get_machine") && router.contains("/machine"),
        "{router}"
    );
    let openapi = read(&tree, "src/api/openapi/recruiting.rs");
    assert!(
        openapi.contains("position_opening_machine::get_machine"),
        "{openapi}"
    );

    // The column keeps the suffix — the strip is the Rust field's alone.
    let columns: Vec<&str> = files_mentioning(&tree, "approval_status_code");
    assert!(
        columns.iter().any(|rel| rel.contains("migrations")),
        "the column keeps its suffix: {columns:?}"
    );
}

// ── 4. Workers: the consumer's exact failure is gone ───────────────────────

/// The reporting consumer's topology. The route is monolith-only, but the
/// loader rule is not: the loader-less machine emits no module here either
/// — the exact `application_machine.rs` that failed its `cargo check`.
#[tokio::test]
async fn workers_topology_emits_no_module_for_the_loaderless_machine() {
    let fixture = write_fixture();
    let output = generate(&fixture, "workers").await;
    let tree = tree(&output);

    assert!(
        !tree.contains_key("workers/recruiting/src/api/recruiting/application_machine.rs"),
        "no unrouted module for a machine without a loader; have:\n{}",
        keys(&tree)
    );
    assert!(
        files_mentioning(&tree, "application_machine").is_empty(),
        "{:?}",
        files_mentioning(&tree, "application_machine")
    );
    // The `…Code` twin keeps its module (the `/workflow` merge derives from
    // it on both topologies), spelled by the DTO field.
    let module = read(
        &tree,
        "workers/recruiting/src/api/recruiting/position_opening_machine.rs",
    );
    assert!(
        module.contains("entity.approval_status.to_string()"),
        "{module}"
    );
}

// ── 5. The withholding is visible ──────────────────────────────────────────

#[tokio::test]
async fn validation_names_the_withheld_read_model() {
    let engine = ingested().await;
    let config = codegraph_config::config::parse_domain_config(&write_fixture().config).unwrap();

    let warnings =
        codegraph_generate::machines::machine_read_model_withheld_warnings(&engine, &config).await;
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("`Application`") && warnings[0].contains("no `read` op"),
        "the Warning names the class and the reason: {warnings:?}"
    );

    let issues = codegraph::validate::ValidationPass::run(&engine, &config).await;
    let named: Vec<&codegraph::validate::ValidationIssue> = issues
        .iter()
        .filter(|issue| issue.check == "machine_read_model")
        .collect();
    assert_eq!(named.len(), 1, "one finding, for `Application` only");
    assert!(matches!(
        named[0].severity,
        codegraph::validate::Severity::Warning
    ));
    assert!(
        named[0].message.contains("`Application`"),
        "{}",
        named[0].message
    );
}

// ── 6. Regeneration sweeps a withheld read model ───────────────────────────

/// The stale-artifact half of #824 — the #807 class, again. The
/// pre-generation sweep keys per ENTITY, so a per-generator artifact that
/// stops being emitted for a still-expected entity survives it, and the
/// filesystem-scanning `generate_mod_files` pass re-declares every `.rs`
/// file it finds: the leftover `application_machine.rs` was compiled again
/// and the consumer's `E0599` returned on the first regeneration after the
/// fix. The pipeline now sweeps machine read-model modules the run did not
/// emit, gated on the plan carrying the emitting generator.
#[tokio::test]
async fn stale_read_model_module_is_swept_on_regeneration() {
    for profile in ["default", "workers"] {
        let fixture = write_fixture();
        let output = fixture.root.path().join(format!("out-stale-{profile}"));
        let api_dir = match profile {
            "workers" => output.join("workers/recruiting/src/api/recruiting"),
            _ => output.join("src/api/recruiting"),
        };
        let stale = api_dir.join("application_machine.rs");
        fs::create_dir_all(&api_dir).unwrap();
        fs::write(&stale, "//! stale read model from a pre-#824 generator\n").unwrap();

        generate_into(&fixture, profile, &output).await;

        assert!(
            !stale.exists(),
            "{profile}: the withheld read model must be swept"
        );
        let api_mod = fs::read_to_string(api_dir.join("mod.rs")).expect("api mod");
        assert!(
            !api_mod.contains("application_machine"),
            "{profile}: the mod pass must not re-declare the swept module:\n{api_mod}"
        );
    }
}
