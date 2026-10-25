//! L1 driver-level graph cache (issue #275): with `--graph-cache`, a second
//! consecutive `run` with unchanged inputs reopens the persisted graph
//! instead of re-ingesting; changed inputs invalidate the cache.

use std::fs;
use std::path::{Path, PathBuf};

// ── L1: driver-level graph cache ─────────────────────────────────────────

const INVENTORY_MOX: &str = r#"
package inventory

class WidgetType {
    String [1] name
    refers WidgetType [0..1] replacement
}
"#;

const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.inventory]
label = "Inventory"
schema_dir = "inventory"
postgres_schema = "inventory"
"#;

struct CacheFixture {
    root: tempfile::TempDir,
    cache: tempfile::TempDir,
    config: PathBuf,
    mox: PathBuf,
    mox_files: Vec<PathBuf>,
}

fn write_cache_fixture() -> CacheFixture {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("domains.toml"), DOMAINS_TOML).unwrap();
    fs::create_dir_all(root.path().join("model")).unwrap();
    fs::write(root.path().join("model/inventory.mox"), INVENTORY_MOX).unwrap();
    CacheFixture {
        config: root.path().join("domains.toml"),
        mox: root.path().join("model/inventory.mox"),
        mox_files: vec![root.path().join("model/inventory.mox")],
        root,
        cache: tempfile::tempdir().unwrap(),
    }
}

fn cache_run_args<'a>(fx: &'a CacheFixture, output: &'a Path) -> codegraph::driver::RunArgs<'a> {
    codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &fx.config,
        output,
        extension_points_path: None,
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(fx.root.path().join("profiles.toml")),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &fx.mox_files,
        rosetta_files: &[],
        ddd_files: &[],
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        ux_rules: None,
        codegraph_rev: None,
        check: false,
    }
}

#[tokio::test]
async fn second_run_with_unchanged_inputs_skips_reingestion() {
    let fx = write_cache_fixture();
    let output = fx.root.path().join("generated");

    let first = codegraph::driver::run_with_graph_cache(
        cache_run_args(&fx, &output),
        Some(fx.cache.path()),
    )
    .await
    .unwrap();
    assert!(!first.graph_cache_reused, "first run must ingest fresh");
    let hash = first
        .inputs_hash
        .expect("inputs hash recorded on first run");
    assert_eq!(hash.len(), 64);
    assert!(fx.cache.path().join("inputs.sha256").exists());
    assert!(fx.cache.path().join("graph.grafeo").exists());

    let second = codegraph::driver::run_with_graph_cache(
        cache_run_args(&fx, &output),
        Some(fx.cache.path()),
    )
    .await
    .unwrap();
    assert!(second.graph_cache_reused, "second run must reuse the graph");
    assert_eq!(second.inputs_hash.as_deref(), Some(hash.as_str()));
}

/// Issue #538: a cache-miss run (changed model inputs) must REBUILD the
/// persisted graph, not reopen the stale checkpoint and append every node a
/// second time. Counted via the artifact export: trait queries dedupe by
/// title and mask the duplication. Fails first (duplicate node identities).
#[tokio::test]
async fn reingest_on_changed_inputs_does_not_duplicate_nodes() {
    let fx = write_cache_fixture();
    let output = fx.root.path().join("generated");

    let first = codegraph::driver::run_with_graph_cache(
        cache_run_args(&fx, &output),
        Some(fx.cache.path()),
    )
    .await
    .unwrap();
    assert!(!first.graph_cache_reused);
    let hash = first.inputs_hash.clone().expect("hash recorded");
    let nodes_after_first = exported_node_count(fx.cache.path(), &hash);

    fs::write(
        &fx.mox,
        "package inventory\n\nclass WidgetType {\n    String [1] name\n    String [0..1] color\n}\n",
    )
    .unwrap();

    let second = codegraph::driver::run_with_graph_cache(
        cache_run_args(&fx, &output),
        Some(fx.cache.path()),
    )
    .await
    .unwrap();
    assert!(!second.graph_cache_reused, "changed inputs must re-ingest");
    let hash2 = second.inputs_hash.clone().expect("hash recorded");
    let nodes_after_second = exported_node_count(fx.cache.path(), &hash2);

    assert_eq!(
        nodes_after_second, nodes_after_first,
        "re-ingest on changed inputs must rebuild the graph, not append onto the checkpoint"
    );
}

/// Export the checkpointed graph and count its nodes. The exporter is
/// content-keyed: an append-on-top re-ingest yields duplicate identities
/// and FAILS HERE with `DuplicateNodeIdentity` instead of passing silently.
fn exported_node_count(cache_dir: &Path, hash: &str) -> usize {
    let engine = codegraph::artifact::open_reusable_engine(cache_dir, hash)
        .unwrap()
        .expect("checkpointed graph must reopen for the recorded hash");
    let dump = std::env::temp_dir().join(format!("graph-cache-count-{hash}.json"));
    codegraph::artifact::export_ir(&engine, &dump).unwrap_or_else(|e| {
        panic!(
            "checkpointed graph must export cleanly (duplicate nodes = append-on-top re-ingest, issue #538): {e}"
        )
    });
    let bytes = fs::read(&dump).unwrap();
    let _ = fs::remove_file(&dump);
    codegraph::artifact::parse_document(&bytes)
        .unwrap()
        .nodes
        .len()
}

/// Issue #538 defense-in-depth: a marker whose recorded node count
/// disagrees with the reopened database is a NAMED hard error — never
/// silently consumed as a valid cache.
#[tokio::test]
async fn reopened_cache_with_stale_node_count_is_a_named_hard_error() {
    let fx = write_cache_fixture();
    let output = fx.root.path().join("generated");

    let first = codegraph::driver::run_with_graph_cache(
        cache_run_args(&fx, &output),
        Some(fx.cache.path()),
    )
    .await
    .unwrap();
    let hash = first.inputs_hash.clone().expect("hash recorded");

    // Tamper: keep the hash, corrupt the recorded node count. A duplicated
    // graph (the #538 corruption) would read exactly like this.
    fs::write(
        fx.cache.path().join("inputs.sha256"),
        format!("{hash} 999999\n"),
    )
    .unwrap();

    let second = codegraph::driver::run_with_graph_cache(
        cache_run_args(&fx, &output),
        Some(fx.cache.path()),
    )
    .await;
    let err = second.expect_err("mismatched node count must hard-fail the run");
    let msg = err.to_string();
    assert!(
        msg.contains("node count mismatch"),
        "error must name the node-count mismatch, got: {msg}"
    );
}

/// Back-compat: a legacy hash-only marker (written before node counts were
/// recorded) still reuses the cache — a mid-flight consumer must not eat a
/// surprise cold re-ingest. The next successful ingest upgrades the format.
#[tokio::test]
async fn legacy_hash_only_marker_still_reuses_the_cache() {
    let fx = write_cache_fixture();
    let output = fx.root.path().join("generated");

    let first = codegraph::driver::run_with_graph_cache(
        cache_run_args(&fx, &output),
        Some(fx.cache.path()),
    )
    .await
    .unwrap();
    let hash = first.inputs_hash.clone().expect("hash recorded");
    fs::write(fx.cache.path().join("inputs.sha256"), format!("{hash}\n")).unwrap();

    let second = codegraph::driver::run_with_graph_cache(
        cache_run_args(&fx, &output),
        Some(fx.cache.path()),
    )
    .await
    .unwrap();
    assert!(
        second.graph_cache_reused,
        "legacy hash-only marker must reuse (lenient parse)"
    );
}

#[tokio::test]
async fn changed_inputs_invalidates_the_graph_cache() {
    let fx = write_cache_fixture();
    let output = fx.root.path().join("generated");

    let first = codegraph::driver::run_with_graph_cache(
        cache_run_args(&fx, &output),
        Some(fx.cache.path()),
    )
    .await
    .unwrap();
    assert!(!first.graph_cache_reused);

    fs::write(
        &fx.mox,
        "package inventory\n\nclass WidgetType {\n    String [1] name\n    String [0..1] color\n}\n",
    )
    .unwrap();

    let second = codegraph::driver::run_with_graph_cache(
        cache_run_args(&fx, &output),
        Some(fx.cache.path()),
    )
    .await
    .unwrap();
    assert!(!second.graph_cache_reused, "changed inputs must re-ingest");
    assert_ne!(second.inputs_hash.as_deref(), first.inputs_hash.as_deref());
}
