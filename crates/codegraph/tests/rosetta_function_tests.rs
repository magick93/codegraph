//! Function node family + func codegen tests (issue #263).
//!
//! The rosetta bridge lands sigil `Function` elements as structured
//! `FunctionNode`s (dispatch head, typed inputs/output, aliases,
//! `set`/`add` operations with `->` paths, post-conditions, transform
//! annotations) instead of needs_review; the `functions` generator (gated
//! behind `rosetta_backend`) emits per-domain Rust free functions with
//! transpiled bodies, aliases as let-bindings, dispatch heads as `match`,
//! and `debug_assert!` post-conditions under the `function_postconditions`
//! feature.

use std::path::Path;

use codegraph_backend::{BackendConfig, create_backend};
use codegraph_config::config::{DomainConfig, parse_domain_config};
use codegraph_core::traits::GraphQuerier;

// ── Fixture (the shared rosetta_bridge store model + its functions) ────

fn store_model() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rosetta_bridge/model/store.rosetta")
}

const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.partners]
label = "Partners"
schema_dir = "partners"
postgres_schema = "partners"

[domains.store]
label = "Store"
schema_dir = "store"
postgres_schema = "store"
"#;

async fn bridge_store() -> (
    codegraph_backend::Backend,
    codegraph::ingest::rosetta_ingest::RosettaIngestOutcome,
    tempfile::TempDir,
) {
    let dir = tempfile::tempdir().unwrap();
    let domains_path = dir.path().join("domains.toml");
    std::fs::write(&domains_path, DOMAINS_TOML).unwrap();
    let domain_config: DomainConfig = parse_domain_config(&domains_path).unwrap();

    let model_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rosetta_bridge/model");
    let backend = create_backend(&BackendConfig::default()).await.unwrap();
    let outcome = codegraph::ingest::rosetta_ingest::ingest_rosetta_files(
        backend.ingestor(),
        backend.querier(),
        &[model_dir.join("partners.rosetta"), store_model()],
        &domain_config,
        "Type",
    )
    .await
    .unwrap();
    (backend, outcome, dir)
}

// ── Bridge stats + needs_review shrink ─────────────────────────────────

#[tokio::test]
async fn function_nodes_ingest_with_stats_and_needs_review_shrinks() {
    let (_backend, outcome, _dir) = bridge_store().await;
    let stats = &outcome.stats;
    // OrderScore, ValueForStatus, ShippedValue, ApprovedValue.
    assert_eq!(stats.functions_ingested, 4, "stats: {stats}");
    assert!(
        !stats
            .needs_review_names
            .iter()
            .any(|n| n.starts_with("func ")),
        "functions left needs_review in #263: {:?}",
        stats.needs_review_names
    );
}

#[tokio::test]
async fn functions_never_land_in_bridged_titles() {
    // Equivalence-of-concerns: functions are computation plane — they
    // create NO SchemaNodes, so classification and DDL stay untouched.
    let (backend, outcome, _dir) = bridge_store().await;
    for name in [
        "OrderScore",
        "ValueForStatus",
        "ShippedValue",
        "ApprovedValue",
    ] {
        assert!(
            !outcome.stats.bridged_titles.iter().any(|t| t == name),
            "{name} must not bridge as a schema"
        );
    }
    let schemas = backend.querier().list_schemas(None).await.unwrap();
    for name in [
        "OrderScore",
        "ValueForStatus",
        "ShippedValue",
        "ApprovedValue",
    ] {
        assert!(
            !schemas.iter().any(|s| s.title == name),
            "{name} must not become a schema node"
        );
    }
}

// ── Node read-back: structured payloads ────────────────────────────────

#[tokio::test]
async fn function_nodes_read_back_with_structured_payloads() {
    let (backend, _outcome, _dir) = bridge_store().await;
    let functions = backend.querier().list_functions().await.unwrap();
    // list_functions orders by (domain, name) — both fixture functions
    // live in `store`.
    let by_name = |name: &str| {
        functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{name} missing: {functions:?}"))
    };

    // Simple derived value: alias, two ops, post-condition, [enrich].
    let score = by_name("OrderScore");
    assert_eq!(score.domain.as_deref(), Some("store"));
    assert_eq!(
        score.definition.as_deref(),
        Some("a simple derived value with an alias and a post-condition")
    );
    assert!(score.dispatch.is_none());
    assert_eq!(score.inputs.len(), 1);
    assert_eq!(score.inputs[0].name, "order");
    assert_eq!(score.inputs[0].type_ref, "OrderType");
    assert!(!score.inputs[0].is_array);
    assert!(!score.inputs[0].is_optional);
    let output = score.output.as_ref().expect("OrderScore output");
    assert_eq!(
        (output.name.as_str(), output.type_ref.as_str()),
        ("result", "number")
    );
    assert_eq!(score.aliases.len(), 1);
    assert_eq!(score.aliases[0].name, "base");
    assert!(
        score.aliases[0].expr_json.contains("FeatureCall"),
        "alias payload is the Expr::to_json shape: {}",
        score.aliases[0].expr_json
    );
    assert_eq!(score.operations.len(), 2);
    assert!(!score.operations[0].is_add);
    assert_eq!(score.operations[0].assign_root, "result");
    assert!(score.operations[0].path.is_empty());
    assert!(score.operations[1].is_add, "the count op is an `add`");
    assert_eq!(score.post_conditions.len(), 1);
    assert_eq!(score.post_conditions[0].name, "ResultPresent");
    assert_eq!(
        score.post_conditions[0].definition.as_deref(),
        Some("the result is present")
    );
    assert_eq!(score.transform_annotations.len(), 1);
    assert_eq!(
        score.transform_annotations[0].kind,
        codegraph_core::types::FunctionTransformKind::Enrich
    );
    assert_eq!(score.transform_annotations[0].reference, None);
    assert_eq!(
        score.properties.get("origin").and_then(|v| v.as_str()),
        Some("rosetta")
    );

    // Dispatch head: (status: OrderStatus->Draft).
    let dispatch_parent = by_name("ValueForStatus");
    let dispatch = dispatch_parent.dispatch.as_ref().expect("dispatch head");
    assert_eq!(
        (
            dispatch.attribute.as_str(),
            dispatch.enumeration.as_str(),
            dispatch.value.as_str()
        ),
        ("status", "OrderStatus", "Draft")
    );
    assert_eq!(dispatch_parent.inputs.len(), 2);
    assert_eq!(dispatch_parent.inputs[1].type_ref, "OrderStatus");
    assert!(dispatch_parent.extends.is_none());

    // Extends resolution: children carry the parent name.
    let shipped = by_name("ShippedValue");
    assert_eq!(shipped.extends.as_deref(), Some("ValueForStatus"));
    assert_eq!(shipped.aliases.len(), 1);
    assert_eq!(shipped.aliases[0].name, "net");
    let approved = by_name("ApprovedValue");
    assert_eq!(approved.extends.as_deref(), Some("ValueForStatus"));
    assert_eq!(
        approved.dispatch.as_ref().map(|d| d.value.as_str()),
        Some("Approved")
    );
}

#[tokio::test]
async fn function_extends_edges_read_back() {
    let (backend, _outcome, _dir) = bridge_store().await;
    let edges = backend.querier().list_function_extends().await.unwrap();
    assert_eq!(
        edges,
        vec![
            ("ApprovedValue".to_string(), "ValueForStatus".to_string()),
            ("ShippedValue".to_string(), "ValueForStatus".to_string()),
        ],
        "both children link to their parent, child-ordered"
    );
}

#[tokio::test]
async fn mock_engine_lands_the_same_function_surface() {
    let dir = tempfile::tempdir().unwrap();
    let domains_path = dir.path().join("domains.toml");
    std::fs::write(&domains_path, DOMAINS_TOML).unwrap();
    let domain_config: DomainConfig = parse_domain_config(&domains_path).unwrap();

    let engine = codegraph_core::mock::MockEngine::new();
    let model_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rosetta_bridge/model");
    let outcome = codegraph::ingest::rosetta_ingest::ingest_rosetta_files(
        &engine,
        &engine,
        &[model_dir.join("partners.rosetta"), store_model()],
        &domain_config,
        "Type",
    )
    .await
    .unwrap();
    assert_eq!(outcome.stats.functions_ingested, 4);
    assert_eq!(engine.list_functions().await.unwrap().len(), 4);
    let edges = engine.list_function_extends().await.unwrap();
    assert_eq!(edges.len(), 2);
}

// ── Extends cycle = hard error naming the cycle ─────────────────────────

const CYCLE_MODEL: &str = r#"
namespace cycle.dom

type Thing:
	id string (1..1)

func Alpha extends Beta:
	output:
		result number (1..1)

	set result:
		1.0

func Beta extends Alpha:
	output:
		result number (1..1)

	set result:
		2.0
"#;

#[tokio::test]
async fn function_extends_cycle_is_a_hard_error_naming_the_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let model_path = dir.path().join("cycle.rosetta");
    std::fs::write(&model_path, CYCLE_MODEL).unwrap();
    let domains_path = dir.path().join("domains.toml");
    std::fs::write(
        &domains_path,
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.cycle]
label = "Cycle"
schema_dir = "cycle"
postgres_schema = "cycle"
"#,
    )
    .unwrap();
    let backend = create_backend(&BackendConfig::default()).await.unwrap();
    let domain_config: DomainConfig = parse_domain_config(&domains_path).unwrap();
    let err = codegraph::ingest::rosetta_ingest::ingest_rosetta_files(
        backend.ingestor(),
        backend.querier(),
        &[model_path],
        &domain_config,
        "Type",
    )
    .await
    .expect_err("an extends cycle must hard-error");
    let msg = err.to_string();
    assert!(msg.contains("cycle"), "error names the failure: {msg}");
    assert!(
        msg.contains("Alpha -> Beta -> Alpha"),
        "error describes the cycle path: {msg}"
    );
}

// ── Function docReference → RegulatoryReference edges ───────────────────

const DOCREF_MODEL: &str = r#"
namespace docref.dom

body RegElement RegBody <"the body">

type DocType:
	id string (1..1)

func Documented:
	[docReference RegBody]
	inputs:
		item DocType (1..1)
	output:
		result number (1..1)

	set result:
		1.0
"#;

#[tokio::test]
async fn function_doc_references_link_to_regulatory_nodes() {
    let dir = tempfile::tempdir().unwrap();
    let model_path = dir.path().join("docref.rosetta");
    std::fs::write(&model_path, DOCREF_MODEL).unwrap();
    let domains_path = dir.path().join("domains.toml");
    std::fs::write(
        &domains_path,
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.docref]
label = "Docref"
schema_dir = "docref"
postgres_schema = "docref"
"#,
    )
    .unwrap();
    let backend = create_backend(&BackendConfig::default()).await.unwrap();
    let domain_config: DomainConfig = parse_domain_config(&domains_path).unwrap();
    let outcome = codegraph::ingest::rosetta_ingest::ingest_rosetta_files(
        backend.ingestor(),
        backend.querier(),
        &[model_path],
        &domain_config,
        "Type",
    )
    .await
    .unwrap();
    assert_eq!(outcome.stats.functions_ingested, 1);
    assert_eq!(outcome.stats.regulatory_edges, 1, "the func docReference");

    let refs = backend
        .querier()
        .list_regulatory_references()
        .await
        .unwrap();
    assert!(
        refs.iter().any(|r| r.owner == "Documented"
            && r.owner_label == "Function"
            && r.target == "RegBody"
            && r.edge_kind == codegraph_core::types::RegulatoryEdgeKind::Reference),
        "RegulatoryReference edge FROM the FunctionNode: {refs:?}"
    );
}

// ── functions generator (rosetta_backend gate) ──────────────────────────

const GENERATOR_MODEL: &str = r#"
namespace gen.store

enum OrderStatus:
	Draft
	Shipped

type OrderType:
	total number (1..1)
	tags string (0..*)

func OrderScore: <"a simple derived value">
	[enrich]
	inputs:
		order OrderType (1..1)
	output:
		result number (1..1)

	alias base: order -> total

	set result:
		base + order -> total

	add result:
		order -> tags count

	post-condition ResultPresent: <"the result is present">
		result exists

func ValueForStatus(status: OrderStatus->Draft):
	inputs:
		order OrderType (1..1)
		status OrderStatus (1..1)
	output:
		result number (1..1)

	set result:
		1.0

func ShippedValue(status: OrderStatus->Shipped) extends ValueForStatus:
	inputs:
		order OrderType (1..1)
		status OrderStatus (1..1)
	output:
		result number (1..1)

	alias net: order -> total

	set result:
		net * 2.0
"#;

fn profiles_toml(functions: bool, postconditions: bool) -> String {
    let mut generators = "[\"dto\"".to_string();
    if functions {
        generators.push_str(", \"functions\"");
    }
    generators.push(']');
    format!(
        r#"
[profiles.default.meta]
name = "rosetta-func"
version = "1.0.0"
description = "functions gate test"

[profiles.default.features]
rosetta_backend = true
function_postconditions = {postconditions}

[profiles.default.api]
generators = {generators}
"#
    )
}

async fn run_generator_pipeline(
    dir: &Path,
    model_text: &str,
    functions: bool,
    postconditions: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let model = dir.join("gen.rosetta");
    std::fs::write(&model, model_text)?;
    let domains = dir.join("domains.toml");
    std::fs::write(&domains, DOMAINS_TOML)?;
    let profiles = dir.join("profiles.toml");
    std::fs::write(&profiles, profiles_toml(functions, postconditions))?;
    let output = dir.join("generated");

    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &domains,
        output: &output,
        extension_points_path: None,
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(profiles),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &[],
        rosetta_files: &[model],
        ddd_files: &[],
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await?;
    Ok(())
}

fn read_functions_rs(dir: &Path) -> Option<String> {
    std::fs::read_to_string(dir.join("generated/src/domain/store/functions.rs")).ok()
}

#[tokio::test]
async fn functions_generator_emits_golden_rust_when_enabled() {
    let dir = tempfile::tempdir().unwrap();
    run_generator_pipeline(dir.path(), GENERATOR_MODEL, true, false)
        .await
        .expect("pipeline with rosetta_backend + functions succeeds");

    let content = read_functions_rs(dir.path()).expect("functions.rs emitted");
    // Signatures: builtin number → f64, snake_case fn names.
    assert!(
        content.contains("pub fn order_score(order: OrderType) -> f64 {"),
        "{content}"
    );
    assert!(
        content.contains("pub fn value_for_status(order: OrderType, status: OrderStatus) -> f64 {"),
        "{content}"
    );
    // Transform annotation metadata.
    assert!(content.contains("/// Transform: [enrich]"), "{content}");
    // Alias as a let-binding with a transpiled `->` expression.
    assert!(content.contains("let base = order.total;"), "{content}");
    // Dispatch head → match with the own arm, the extending child arm,
    // and the default arm.
    assert!(content.contains("match status {"), "{content}");
    assert!(content.contains("OrderStatus::Draft => {"), "{content}");
    assert!(
        content.contains("OrderStatus::Shipped => shipped_value(order, status),"),
        "{content}"
    );
    assert!(
        content.contains(
            "_ => todo!(\"function 'ValueForStatus': unhandled OrderStatus dispatch value\"),"
        ),
        "{content}"
    );
    // Output accumulates through a default-initialized mutable local.
    assert!(
        content.contains("let mut result = f64::default();"),
        "{content}"
    );
    // Post-conditions are ABSENT while function_postconditions is off —
    // but the marker convention keeps the record for the untranspilable
    // Count op (no collection knowledge for function bodies).
    assert!(
        !content.contains("debug_assert!"),
        "post-conditions are config-gated: {content}"
    );
    assert!(
        content.contains(
            "// TODO(#263): transpile operation 'add result' (unsupported: Count: count requires a collection field)"
        ),
        "{content}"
    );
    // The child's own emitted fn keeps its alias + extends provenance.
    assert!(content.contains("let net = order.total;"), "{content}");
    assert!(
        content.contains("pub fn shipped_value(order: OrderType, status: OrderStatus) -> f64 {"),
        "{content}"
    );
}

#[tokio::test]
async fn post_conditions_emit_debug_assert_when_the_feature_is_on() {
    let dir = tempfile::tempdir().unwrap();
    run_generator_pipeline(dir.path(), GENERATOR_MODEL, true, true)
        .await
        .expect("pipeline with function_postconditions succeeds");

    let content = read_functions_rs(dir.path()).expect("functions.rs emitted");
    assert!(
        content.contains("debug_assert!(result.is_some(), \"ResultPresent\");"),
        "{content}"
    );
}

#[tokio::test]
async fn functions_generator_off_emits_nothing() {
    let dir = tempfile::tempdir().unwrap();
    run_generator_pipeline(dir.path(), GENERATOR_MODEL, false, false)
        .await
        .expect("pipeline without functions listed succeeds");
    assert!(
        read_functions_rs(dir.path()).is_none(),
        "functions.rs must not exist when the generator is not listed"
    );
}

#[tokio::test]
async fn functions_listed_while_rosetta_backend_off_is_a_configuration_error() {
    // Byte-identity protection: the capability gate hard-errors when the
    // generator is listed but its feature is off.
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("gen.rosetta");
    std::fs::write(&model, GENERATOR_MODEL).unwrap();
    let domains = dir.path().join("domains.toml");
    std::fs::write(&domains, DOMAINS_TOML).unwrap();
    let profiles = dir.path().join("profiles.toml");
    std::fs::write(
        &profiles,
        r#"
[profiles.default.meta]
name = "rosetta-func-misconfig"
version = "1.0.0"
description = "functions misconfiguration gate"

[profiles.default.features]
rosetta_backend = false

[profiles.default.api]
generators = ["dto", "functions"]
"#,
    )
    .unwrap();

    let result = codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &domains,
        output: &dir.path().join("generated"),
        extension_points_path: None,
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(profiles),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &[],
        rosetta_files: &[model],
        ddd_files: &[],
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await;

    assert!(
        result.is_err(),
        "listing functions with rosetta_backend off must fail plan validation"
    );
    assert!(
        read_functions_rs(dir.path()).is_none(),
        "no output may be written when plan validation fails"
    );
}

// ── Equivalence of concerns: functions alter no other output ────────────

/// The generator model minus every func — the same data model the
/// equivalence run bridges.
const GENERATOR_MODEL_NO_FUNCS: &str = r#"
namespace gen.store

enum OrderStatus:
	Draft
	Shipped

type OrderType:
	total number (1..1)
	tags string (0..*)
"#;

#[tokio::test]
async fn functions_alter_no_output_beyond_their_own_module() {
    // Two runs with the SAME profiles (functions listed): one bridges the
    // model with functions, one without. Every generated file must be
    // byte-identical except the functions module itself — pinning that
    // function ingestion does not perturb classification, DDL, entities,
    // or any other generator's view of the graph.
    let dir_with = tempfile::tempdir().unwrap();
    let dir_without = tempfile::tempdir().unwrap();
    run_generator_pipeline(dir_with.path(), GENERATOR_MODEL, true, false)
        .await
        .expect("with-functions run");
    run_generator_pipeline(dir_without.path(), GENERATOR_MODEL_NO_FUNCS, true, false)
        .await
        .expect("without-functions run");

    let out_with = dir_with.path().join("generated");
    let out_without = dir_without.path().join("generated");
    assert!(
        read_functions_rs(dir_with.path()).is_some(),
        "the with-functions run emits the module"
    );
    assert!(
        read_functions_rs(dir_without.path()).is_none(),
        "a model without funcs emits no module"
    );

    let collect = |root: &Path| -> std::collections::BTreeMap<String, String> {
        let mut files = std::collections::BTreeMap::new();
        for path in walk(root) {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            files.insert(rel, content);
        }
        files
    };
    let with = collect(&out_with);
    let without = collect(&out_without);

    let mut diff = Vec::new();
    for (rel, content) in &with {
        match without.get(rel) {
            None => diff.push(format!("only in with-functions run: {rel}")),
            Some(other) => {
                if other != content {
                    diff.push(format!("content drift: {rel}"));
                }
            }
        }
    }
    for rel in without.keys() {
        if !with.contains_key(rel) {
            diff.push(format!("only in without-functions run: {rel}"));
        }
    }
    // The ONLY permitted differences: the functions module itself plus the
    // two mechanical registries that index emitted files — the domain
    // mod.rs (declares the new module) and the run manifest (inventories
    // it). Everything classification/DDL/entity-shaped must be identical.
    let permitted: std::collections::BTreeSet<String> = [
        "src/domain/store/functions.rs".to_string(),
        "src/domain/store/mod.rs".to_string(),
        ".codegraph-manifest.json".to_string(),
    ]
    .into_iter()
    .collect();
    let unexpected: Vec<String> = diff
        .iter()
        .filter(|entry| {
            !permitted.contains(
                entry
                    .rsplit_once(": ")
                    .map(|(_, file)| file)
                    .unwrap_or(entry),
            )
        })
        .cloned()
        .collect();
    assert!(
        unexpected.is_empty(),
        "functions must not alter any other generated file: {unexpected:?} (full diff: {diff:?})"
    );
}

fn walk(root: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

// ── function bindings (issues #450 B0 + #337 B1) ────────────────────────

async fn run_generator_pipeline_with_functions(
    dir: &Path,
    model_text: &str,
    functions: bool,
    postconditions: bool,
    functions_toml: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let model = dir.join("gen.rosetta");
    std::fs::write(&model, model_text)?;
    let mut domains = String::from(DOMAINS_TOML);
    if let Some(fragment) = functions_toml {
        domains.push('\n');
        domains.push_str(fragment);
    }
    let domains_path = dir.join("domains.toml");
    std::fs::write(&domains_path, domains)?;
    let profiles = dir.join("profiles.toml");
    std::fs::write(&profiles, profiles_toml(functions, postconditions))?;
    let output = dir.join("generated");

    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &domains_path,
        output: &output,
        extension_points_path: None,
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(profiles),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &[],
        rosetta_files: &[model],
        ddd_files: &[],
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await?;
    Ok(())
}

fn read_split_file(dir: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(format!("generated/src/domain/store/functions/{name}"))).ok()
}

/// Issue #450 TDD mandate: a `[functions."Name"]` entry in domains.toml
/// resolves and reaches the functions generator — a `manual` implementation
/// moves the function into the write-once `manual.rs` stub and
/// `generated.rs` never claims it; unknown function names and unknown
/// values are hard errors naming the offender.
#[tokio::test]
async fn function_binding_resolution_from_domains_toml() {
    // (a) The binding materializes: OrderScore becomes a hand-owned stub
    // in the split layout; the unbound functions stay transpiled.
    let dir = tempfile::tempdir().unwrap();
    run_generator_pipeline_with_functions(
        dir.path(),
        GENERATOR_MODEL,
        true,
        false,
        Some("[functions.\"OrderScore\"]\nimplementation = \"manual\"\n"),
    )
    .await
    .expect("bound pipeline succeeds");

    assert!(
        !dir.path()
            .join("generated/src/domain/store/functions.rs")
            .exists(),
        "a bound domain emits the split layout, not the single file"
    );
    let manual =
        read_split_file(dir.path(), "manual.rs").expect("manual.rs emitted for the bound fn");
    assert!(
        manual.contains("fn order_score(&self, order: OrderType) -> f64"),
        "{manual}"
    );
    assert!(manual.contains("todo!"), "{manual}");
    let generated = read_split_file(dir.path(), "generated.rs").expect("generated.rs emitted");
    assert!(
        !generated.contains("pub fn order_score"),
        "generated.rs never claims a manual-bound function: {generated}"
    );
    assert!(
        generated.contains("pub fn value_for_status"),
        "unbound functions keep today's transpiled emission: {generated}"
    );
    let module = read_split_file(dir.path(), "mod.rs").expect("mod.rs emitted");
    assert!(
        module.contains("pub trait ManualImpls"),
        "the manual seam trait is emitted: {module}"
    );
    assert!(module.contains("fn order_score"), "{module}");

    // (b) An unknown function name is a hard error naming the offender.
    let dir = tempfile::tempdir().unwrap();
    let err = run_generator_pipeline_with_functions(
        dir.path(),
        GENERATOR_MODEL,
        true,
        false,
        Some("[functions.\"NoSuchFunc\"]\n"),
    )
    .await
    .expect_err("unknown function name must fail the run");
    assert!(err.to_string().contains("NoSuchFunc"), "{err}");

    // (c) An unknown enum value is a hard error naming the value.
    let dir = tempfile::tempdir().unwrap();
    let err = run_generator_pipeline_with_functions(
        dir.path(),
        GENERATOR_MODEL,
        true,
        false,
        Some("[functions.\"OrderScore\"]\nimplementation = \"serverless\"\n"),
    )
    .await
    .expect_err("unknown implementation value must fail the run");
    assert!(err.to_string().contains("serverless"), "{err}");
}

/// Issue #450 TDD mandate: with no `[functions]` table — or only
/// all-default entries — output is byte-identical to the unbound run.
#[tokio::test]
async fn function_binding_absent_is_byte_identical() {
    let collect = |root: &Path| -> std::collections::BTreeMap<String, String> {
        let mut files = std::collections::BTreeMap::new();
        for path in walk(root) {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            files.insert(rel, content);
        }
        files
    };

    let dir_absent = tempfile::tempdir().unwrap();
    run_generator_pipeline_with_functions(dir_absent.path(), GENERATOR_MODEL, true, false, None)
        .await
        .expect("absent-table run");
    assert!(
        read_functions_rs(dir_absent.path()).is_some(),
        "the absent-table run keeps today's single-file layout"
    );

    let dir_default = tempfile::tempdir().unwrap();
    run_generator_pipeline_with_functions(
        dir_default.path(),
        GENERATOR_MODEL,
        true,
        false,
        Some("[functions.\"OrderScore\"]\n"),
    )
    .await
    .expect("all-default-entry run");

    let absent = collect(&dir_absent.path().join("generated"));
    let default = collect(&dir_default.path().join("generated"));
    let mut diff = Vec::new();
    for (rel, content) in &absent {
        match default.get(rel) {
            None => diff.push(format!("only in absent run: {rel}")),
            Some(other) => {
                if other != content {
                    diff.push(format!("content drift: {rel}"));
                }
            }
        }
    }
    for rel in default.keys() {
        if !absent.contains_key(rel) {
            diff.push(format!("only in default run: {rel}"));
        }
    }
    assert!(
        diff.is_empty(),
        "an all-default functions entry must be byte-identical to no table: {diff:?}"
    );
}

/// The write-once contract: a hand-edited `manual.rs` survives
/// regeneration; the generated modules re-emit around it.
#[tokio::test]
async fn manual_rs_is_write_once_across_regeneration() {
    let dir = tempfile::tempdir().unwrap();
    run_generator_pipeline_with_functions(
        dir.path(),
        GENERATOR_MODEL,
        true,
        false,
        Some("[functions.\"OrderScore\"]\nimplementation = \"manual\"\n"),
    )
    .await
    .expect("first run");

    // The consumer fills in the hand-owned stub.
    let manual_path = dir
        .path()
        .join("generated/src/domain/store/functions/manual.rs");
    let hand_owned = std::fs::read_to_string(&manual_path).unwrap()
        + "\n// hand-edited: the real implementation lives here\n";
    std::fs::write(&manual_path, &hand_owned).unwrap();

    run_generator_pipeline_with_functions(
        dir.path(),
        GENERATOR_MODEL,
        true,
        false,
        Some("[functions.\"OrderScore\"]\nimplementation = \"manual\"\n"),
    )
    .await
    .expect("second run");

    let after = std::fs::read_to_string(&manual_path).unwrap();
    assert!(
        after.contains("hand-edited: the real implementation lives here"),
        "manual.rs is write-once — regeneration must not clobber it: {after}"
    );
    let generated = read_split_file(dir.path(), "generated.rs").unwrap();
    assert!(
        generated.contains("pub fn value_for_status"),
        "the generated module still re-emits: {generated}"
    );
}

// ── B1: exposure = "http" (issue #337) ──────────────────────────────────

/// One fully-transpilable func (primitive + codelist inputs), one partial
/// (dispatch default arm → `todo!`), one hand-owned, one taking a
/// value-object-typed input (unresolvable on the API plane). `WidgetType`
/// is config-forced into the entity plane so `WidgetTypeResponse` exists.
const BINDING_API_MODEL: &str = r#"
namespace gen.store

enum OrderStatus:
	Draft
	Shipped

type OrderType:
	total number (1..1)

type WidgetType:
	label string (1..1)

func SurchargeFor: <"price surcharge for a status">
	inputs:
		status OrderStatus (1..1)
		amount number (1..1)
	output:
		result number (1..1)

	set result:
		amount + 1.0

func BrokenAnswer(status: OrderStatus->Draft):
	inputs:
		status OrderStatus (1..1)
		amount number (1..1)
	output:
		result number (1..1)

	set result:
		amount

func HandSet: <"hand-owned amount">
	inputs:
		amount number (1..1)
	output:
		result number (1..1)

	set result:
		amount

func VoTaker:
	inputs:
		order OrderType (1..1)
	output:
		result number (1..1)

	set result:
		1.0
"#;

const BINDING_API_DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.store]
label = "Store"
schema_dir = "store"
postgres_schema = "store"
entities = ["WidgetType"]

[functions."SurchargeFor"]
exposure = "http"

[functions."BrokenAnswer"]
exposure = "http"

[functions."HandSet"]
implementation = "manual"
exposure = "http"
"#;

/// T9's variant: VoTaker (a value-object-typed function) is HTTP-exposed.
const BINDING_API_VO_DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.store]
label = "Store"
schema_dir = "store"
postgres_schema = "store"
entities = ["WidgetType"]

[functions."VoTaker"]
exposure = "http"
"#;

const BINDING_API_PROFILES_TOML: &str = r#"
[profiles.default.meta]
name = "rosetta-func-http"
version = "1.0.0"
description = "functions http exposure gate"

[profiles.default.features]
rosetta_backend = true

[profiles.default.api]
generators = ["dto", "sea_orm_entity", "ddl", "functions", "router", "openapi", "errors"]
"#;

async fn run_binding_api_pipeline_with(
    dir: &Path,
    model_text: &str,
    domains_content: &str,
    profiles: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let model = dir.join("gen.rosetta");
    std::fs::write(&model, model_text)?;
    let domains = dir.join("domains.toml");
    std::fs::write(&domains, domains_content)?;
    let profiles_path = dir.join("profiles.toml");
    std::fs::write(&profiles_path, profiles)?;
    let output = dir.join("generated");

    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &domains,
        output: &output,
        extension_points_path: None,
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(profiles_path),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &[],
        rosetta_files: &[model],
        ddd_files: &[],
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await?;
    Ok(())
}

/// Issue #337: HTTP-exposed functions ship as authenticated POST endpoints
/// — request/response DTOs, `#[utoipa::path]` handlers, a
/// `functions_routes()` merge in the domain router, and OpenAPI paths +
/// schemas. Complete `dsl` functions call the generated body; partial ones
/// answer 501 naming the gap; `manual` ones dispatch through the
/// `ManualImpls` provider slot.
#[tokio::test]
async fn http_exposed_functions_emit_routes_openapi_and_501_for_partial() {
    let dir = tempfile::tempdir().unwrap();
    run_binding_api_pipeline_with(
        dir.path(),
        BINDING_API_MODEL,
        BINDING_API_DOMAINS_TOML,
        BINDING_API_PROFILES_TOML,
    )
    .await
    .expect("binding api pipeline succeeds");

    // The functions plane resolved the exposed signatures: the codelist
    // input imports the generated enum, and generated.rs compiles against it.
    let generated = read_split_file(dir.path(), "generated.rs").expect("generated.rs");
    assert!(
        generated.contains("pub fn surcharge_for(status: OrderStatus, amount: f64) -> f64"),
        "{generated}"
    );
    assert!(
        generated.contains("use crate::codelist::OrderStatus;"),
        "{generated}"
    );

    let api = std::fs::read_to_string(dir.path().join("generated/src/api/store/functions_api.rs"))
        .expect("functions_api.rs emitted");

    // Request DTO from typed inputs; response carries the declared output.
    assert!(api.contains("pub struct SurchargeForRequest"), "{api}");
    assert!(api.contains("pub status: OrderStatus,"), "{api}");
    assert!(api.contains("pub amount: f64,"), "{api}");
    assert!(api.contains("pub struct SurchargeForResponse"), "{api}");
    assert!(api.contains("pub result: f64,"), "{api}");

    // utoipa registration.
    assert!(api.contains("#[utoipa::path("), "{api}");
    assert!(
        api.contains("operation_id = \"store_functions_surcharge_for\""),
        "{api}"
    );
    assert!(api.contains("(status = 501,"), "{api}");

    // Complete dsl function dispatches into the generated body.
    assert!(
        api.contains(
            "let result = crate::domain::store::functions::surcharge_for(body.status, body.amount);"
        ),
        "{api}"
    );

    // Partial function answers 501 naming the function and the gap.
    assert!(
        api.contains("Err(not_implemented("),
        "partial handler must 501: {api}"
    );
    assert!(api.contains("\"BrokenAnswer\""), "{api}");
    assert!(api.contains("not fully transpilable"), "{api}");

    // Manual function dispatches through the AppState provider slot.
    assert!(api.contains("state.store_manual_impls.as_ref()"), "{api}");
    assert!(api.contains("provider.hand_set(body.amount)"), "{api}");
    assert!(api.contains("ManualImpls"), "{api}");

    // Route registration.
    assert!(api.contains("pub fn functions_routes()"), "{api}");
    assert!(
        api.contains(".route(\"/surcharge_for\", post(surcharge_for))"),
        "{api}"
    );
    assert!(
        api.contains(".route(\"/broken_answer\", post(broken_answer))"),
        "{api}"
    );
    assert!(
        api.contains(".route(\"/hand_set\", post(hand_set))"),
        "{api}"
    );

    // The domain router merges the functions routes.
    let router = std::fs::read_to_string(dir.path().join("generated/src/api/store/router.rs"))
        .expect("router.rs");
    assert!(
        router.contains(".merge(crate::api::store::functions_api::functions_routes())"),
        "{router}"
    );

    // OpenAPI registers the paths and schemas.
    let openapi = std::fs::read_to_string(dir.path().join("generated/src/api/openapi/store.rs"))
        .expect("openapi store.rs");
    assert!(
        openapi.contains("crate::api::store::functions_api::surcharge_for"),
        "{openapi}"
    );
    assert!(
        openapi.contains("crate::api::store::functions_api::SurchargeForRequest"),
        "{openapi}"
    );
    assert!(
        openapi.contains("crate::api::store::functions_api::HandSetResponse"),
        "{openapi}"
    );

    // The manual.rs stub signature agrees with the trait + handler.
    let manual = read_split_file(dir.path(), "manual.rs").expect("manual.rs");
    assert!(
        manual.contains("fn hand_set(&self, amount: f64) -> f64"),
        "{manual}"
    );
}

/// An HTTP-exposed function whose input carries no generated Rust type
/// (a value object) is a hard generation error naming the function and
/// the input — never a bare name in the emitted tree (#517 contract).
#[tokio::test]
async fn unresolvable_input_type_is_a_generation_error_naming_the_function() {
    let dir = tempfile::tempdir().unwrap();
    let err = run_binding_api_pipeline_with(
        dir.path(),
        BINDING_API_MODEL,
        BINDING_API_VO_DOMAINS_TOML,
        BINDING_API_PROFILES_TOML,
    )
    .await
    .expect_err("VoTaker's OrderType input is a value object — must fail");
    let msg = err.to_string();
    assert!(msg.contains("VoTaker"), "{msg}");
    assert!(msg.contains("OrderType"), "{msg}");
}

/// The AppState carries a `{domain}_manual_impls` provider slot for
/// domains with hand-owned AND HTTP-exposed functions, constructed `None`
/// by the scaffold — consumers register their `ManualImpls` impl there.
#[tokio::test]
async fn manual_exposed_function_gets_an_app_state_provider_slot() {
    let dir = tempfile::tempdir().unwrap();
    let profiles = r#"
[profiles.default.meta]
name = "rosetta-func-slot"
version = "1.0.0"
description = "functions AppState slot gate"

[profiles.default.features]
rosetta_backend = true

[profiles.default.api]
generators = ["functions", "scaffold"]
"#;
    run_binding_api_pipeline_with(
        dir.path(),
        BINDING_API_MODEL,
        BINDING_API_DOMAINS_TOML,
        profiles,
    )
    .await
    .expect("scaffold pipeline succeeds");

    let app_state = std::fs::read_to_string(dir.path().join("generated/src/app_state.rs"))
        .expect("app_state.rs");
    assert!(
        app_state.contains(
            "pub store_manual_impls: Option<std::sync::Arc<dyn crate::domain::store::functions::ManualImpls>>"
        ),
        "{app_state}"
    );

    let server =
        std::fs::read_to_string(dir.path().join("generated/src/server.rs")).expect("server.rs");
    assert!(server.contains("store_manual_impls: None,"), "{server}");
}

// ── Issue #450 remaining slices: sql / external / event ────────────────

/// Issue #450: the previously reserved `sql`/`external` implementations and
/// the `event` exposure are accepted from domains.toml — parsing and
/// validation no longer refuse them (their generation slices are B3/B4).
/// `sql` on a `database_target = "sqlite"` profile stays a hard error
/// naming the function: a Postgres function has no sqlite counterpart.
#[tokio::test]
async fn sql_external_event_bindings_resolve_from_domains_toml() {
    // (a) implementation = "sql" parses + validates on the postgres target.
    let dir = tempfile::tempdir().unwrap();
    run_generator_pipeline_with_functions(
        dir.path(),
        GENERATOR_MODEL,
        true,
        false,
        Some("[functions.\"OrderScore\"]\nimplementation = \"sql\"\n"),
    )
    .await
    .expect("sql implementation is accepted on the default postgres target");

    // (b) implementation = "external" parses + validates.
    let dir = tempfile::tempdir().unwrap();
    run_generator_pipeline_with_functions(
        dir.path(),
        GENERATOR_MODEL,
        true,
        false,
        Some("[functions.\"OrderScore\"]\nimplementation = \"external\"\n"),
    )
    .await
    .expect("external implementation is accepted");

    // (c) exposure = "event" parses + validates.
    let dir = tempfile::tempdir().unwrap();
    run_generator_pipeline_with_functions(
        dir.path(),
        GENERATOR_MODEL,
        true,
        false,
        Some("[functions.\"OrderScore\"]\nexposure = \"event\"\n"),
    )
    .await
    .expect("event exposure is accepted");

    // (d) sql + sqlite is a hard config error naming the function.
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("gen.rosetta");
    std::fs::write(&model, GENERATOR_MODEL).unwrap();
    let domains = dir.path().join("domains.toml");
    std::fs::write(
        &domains,
        format!("{DOMAINS_TOML}\n[functions.\"OrderScore\"]\nimplementation = \"sql\"\n"),
    )
    .unwrap();
    let profiles = dir.path().join("profiles.toml");
    std::fs::write(
        &profiles,
        r#"
[profiles.default.meta]
name = "rosetta-func-sqlite"
version = "1.0.0"
description = "sql binding on sqlite refusal"

[profiles.default.features]
rosetta_backend = true
database_target = "sqlite"

[profiles.default.api]
generators = ["dto", "functions"]
"#,
    )
    .unwrap();
    let err = codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &domains,
        output: &dir.path().join("generated"),
        extension_points_path: None,
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(profiles),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &[],
        rosetta_files: &[model],
        ddd_files: &[],
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
    .expect_err("sql binding on sqlite must fail the run");
    let msg = err.to_string();
    assert!(msg.contains("OrderScore"), "{msg}");
    assert!(msg.contains("sqlite"), "{msg}");
}

/// The E2 compile gate (issue #337): the emitted crate `cargo check`s
/// cleanly with an exposed-function surface present. Ignored — it needs
/// the full dependency stack; run it in the ops-integration lane or by
/// hand: `cargo test -p codegraph --test rosetta_function_tests -- --ignored`.
#[tokio::test]
#[ignore = "compiles the emitted crate (minutes); ops-integration lane"]
async fn emitted_crate_compiles_with_exposed_functions() {
    let dir = tempfile::tempdir().unwrap();
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap();
    let profiles = format!(
        r#"
[profiles.default.meta]
name = "rosetta-func-compile"
version = "1.0.0"
description = "functions compile gate"
type_contracts_base = "{}"
codegraph_workflow_base = "{}"

[profiles.default.features]
rosetta_backend = true

[profiles.default.api]
generators = ["ddl", "sea_orm_entity", "codelist", "dto", "repository", "command", "query", "handler", "router", "errors", "links", "domain_types_dto", "domain_types_query_service", "domain_types_scaffold", "openapi", "scaffold", "functions"]
"#,
        workspace_root
            .join("crates/codegraph-type-contracts")
            .display(),
        workspace_root.join("crates/codegraph-workflow").display(),
    );
    let profiles = profiles.replace(
        "description = \"functions compile gate\"",
        &format!(
            "description = \"functions compile gate\"\ndomain_types_base = \"{}\"",
            "crates/domain-types"
        ),
    );
    // The compile gate excludes the unbound VO-typed function: unexposed
    // functions keep verbatim signatures (the documented caveat — model
    // types like `OrderType` have no generated Rust yet), which is not
    // what this gate exercises.
    let marker = "func VoTaker:";
    let model = match BINDING_API_MODEL.split_once(marker) {
        Some((head, _)) => head.to_string(),
        None => BINDING_API_MODEL.to_string(),
    };
    run_binding_api_pipeline_with(dir.path(), &model, BINDING_API_DOMAINS_TOML, &profiles)
        .await
        .expect("compile-gate pipeline succeeds");

    let output = tokio::process::Command::new("cargo")
        .args(["check"])
        .current_dir(dir.path().join("generated"))
        .output()
        .await
        .expect("cargo check spawned");
    assert!(
        output.status.success(),
        "emitted crate must compile with the functions HTTP surface:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
