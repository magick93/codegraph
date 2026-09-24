//! Issue #267 — namespaces as first-class citizens.
//!
//! Covers the core model + graph plumbing (mock + grafeo parity),
//! determinism guarantees, config grammar, validation checks, and the
//! namespace-less back-compat contract. The atproto `NamespaceNode`
//! collision was resolved by renaming the AT-Protocol type to
//! `AtprotoNamespaceNode` (see `types/namespace.rs` docs); the tests here
//! pin both families coexisting.

use codegraph::validate::ValidationPass;
use codegraph_config::config::parse_domain_config_str;
use codegraph_core::mock::MockEngine;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{
    qualified_schema_id, topological_namespace_order, AtprotoNamespaceNode, EdgeProperties,
    EdgeType, LexiconNode, NamespaceImport, NamespaceNode, SchemaNode,
};
use codegraph_grafeo::GrafeoEngine;

// ── fixtures ───────────────────────────────────────────────────────────

fn ns(fqn: &str, parent: Option<&str>, source: Option<&str>) -> NamespaceNode {
    NamespaceNode {
        fqn: fqn.to_string(),
        parent: parent.map(|p| p.to_string()),
        source: source.map(|s| s.to_string()),
    }
}

fn schema(id: &str, title: &str, domain: &str) -> SchemaNode {
    SchemaNode {
        schema_id: id.to_string(),
        title: title.to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: "entity".to_string(),
        domain: Some(domain.to_string()),
        namespace: None,
        rel_path: format!("{domain}/json/{title}.json"),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: title.to_string(),
        pg_table_name: title.to_lowercase(),
        api_path_segment: title.to_lowercase(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
    }
}

fn import(from: &str, to: &str, wildcard: bool, alias: Option<&str>) -> NamespaceImport {
    NamespaceImport {
        from_ns: from.to_string(),
        to_ns: to.to_string(),
        wildcard,
        alias: alias.map(|a| a.to_string()),
    }
}

/// The domains.toml used by the validation tests: common + products with
/// depends_on, and the cdm namespace tree declared under products.
fn validation_config_toml(depends_on: bool) -> String {
    let dep_line = if depends_on {
        "depends_on = [\"common\"]\n"
    } else {
        ""
    };
    format!(
        "[defaults]\n\
         operations = [\"create\", \"read\", \"update\", \"delete\", \"list\"]\n\n\
         [domains.common]\n\
         label = \"Common\"\n\
         schema_dir = \"common\"\n\
         postgres_schema = \"common\"\n\n\
         [domains.products]\n\
         label = \"Products\"\n\
         schema_dir = \"products\"\n\
         postgres_schema = \"products\"\n\
         {dep_line}\n\
         [namespaces.\"cdm.base\"]\n\
         domain = \"products\"\n"
    )
}

/// A two-namespace scenario with a parent/child hierarchy, imports, and
/// schemas linked via InNamespace — run against EVERY backend for parity.
///
/// Graph:
/// ```text
/// cdm.base ──NamespaceParent──► cdm          (child → parent)
/// cdm.base.datetime ──NamespaceParent──► cdm.base
/// billing ──NamespaceImports {wildcard}──► cdm.base
/// billing ──NamespaceImports {alias}─────► cdm.base.datetime
/// Invoice ──InNamespace──► billing
/// Party   ──InNamespace──► cdm.base
/// Date    ──InNamespace──► cdm.base.datetime
/// ```
async fn namespace_scenario(db: &(impl GraphIngestor + GraphQuerier)) {
    for node in [
        ns("cdm", None, Some("config")),
        ns("cdm.base", Some("cdm"), Some("config")),
        ns("cdm.base.datetime", Some("cdm.base"), Some("discovered")),
        ns("billing", None, None),
    ] {
        db.ingest_namespace(&node).await.unwrap();
    }
    db.ingest_namespace_import(&import("billing", "cdm.base", true, None))
        .await
        .unwrap();
    db.ingest_namespace_import(&import("billing", "cdm.base.datetime", false, Some("dt")))
        .await
        .unwrap();

    db.ingest_schema(&schema("billing/Invoice", "Invoice", "billing"))
        .await
        .unwrap();
    db.ingest_schema(&schema("cdm/Party", "Party", "common"))
        .await
        .unwrap();
    db.ingest_schema(&schema("cdm/Date", "Date", "common"))
        .await
        .unwrap();
    for (schema_id, target) in [
        ("billing/Invoice", "billing"),
        ("cdm/Party", "cdm.base"),
        ("cdm/Date", "cdm.base.datetime"),
    ] {
        db.ingest_edge(schema_id, target, EdgeType::InNamespace, None)
            .await
            .unwrap();
    }
}

// ── node + edge round-trips (mock + grafeo parity) ─────────────────────

#[tokio::test]
async fn namespace_nodes_roundtrip_on_mock() {
    let engine = MockEngine::new();
    namespace_scenario(&engine).await;
    assert_namespaces_roundtrip(&engine).await;
    assert_imports_roundtrip(&engine).await;
}

#[tokio::test]
async fn namespace_nodes_roundtrip_on_grafeo() {
    let engine = GrafeoEngine::in_memory().unwrap();
    namespace_scenario(&engine).await;
    assert_namespaces_roundtrip(&engine).await;
    assert_imports_roundtrip(&engine).await;
}

async fn assert_namespaces_roundtrip(db: &dyn GraphQuerier) {
    let namespaces = db.list_namespaces().await.unwrap();
    assert_eq!(
        namespaces
            .iter()
            .map(|n| n.fqn.as_str())
            .collect::<Vec<_>>(),
        // Stable ORDER BY fqn — determinism contract.
        vec!["billing", "cdm", "cdm.base", "cdm.base.datetime"]
    );
    let datetime = namespaces
        .iter()
        .find(|n| n.fqn == "cdm.base.datetime")
        .unwrap();
    assert_eq!(datetime.parent.as_deref(), Some("cdm.base"));
    assert_eq!(datetime.source.as_deref(), Some("discovered"));
    let billing = namespaces.iter().find(|n| n.fqn == "billing").unwrap();
    assert_eq!(billing.parent, None);
    assert_eq!(billing.source, None);
}

async fn assert_imports_roundtrip(db: &dyn GraphQuerier) {
    let imports = db.get_namespace_imports("billing").await.unwrap();
    // Ordered by (to_ns, alias) — determinism contract.
    assert_eq!(imports.len(), 2, "payload read-back: {imports:?}");
    assert_eq!(imports[0].to_ns, "cdm.base");
    assert!(imports[0].wildcard, "wildcard flag must round-trip");
    assert_eq!(imports[0].alias, None);
    assert_eq!(imports[1].to_ns, "cdm.base.datetime");
    assert!(!imports[1].wildcard);
    assert_eq!(imports[1].alias.as_deref(), Some("dt"));
    // Non-importing namespaces return empty, not error.
    assert!(db.get_namespace_imports("cdm").await.unwrap().is_empty());
}

// ── list_schemas_by_namespace (recursive + flat) ───────────────────────

#[tokio::test]
async fn list_schemas_by_namespace_recursive_on_mock() {
    let engine = MockEngine::new();
    namespace_scenario(&engine).await;
    assert_schemas_by_namespace(&engine).await;
}

#[tokio::test]
async fn list_schemas_by_namespace_recursive_on_grafeo() {
    let engine = GrafeoEngine::in_memory().unwrap();
    namespace_scenario(&engine).await;
    assert_schemas_by_namespace(&engine).await;
}

async fn assert_schemas_by_namespace(db: &dyn GraphQuerier) {
    // Flat: only schemas directly in cdm.base.
    let flat = db
        .list_schemas_by_namespace("cdm.base", false)
        .await
        .unwrap();
    assert_eq!(
        flat.iter()
            .map(|s| s.schema_id.as_str())
            .collect::<Vec<_>>(),
        vec!["cdm/Party"],
        "flat lookup must not include child namespaces"
    );
    // Recursive: cdm.base + cdm.base.datetime descendants.
    let recursive = db
        .list_schemas_by_namespace("cdm.base", true)
        .await
        .unwrap();
    assert_eq!(
        recursive
            .iter()
            .map(|s| s.schema_id.as_str())
            .collect::<Vec<_>>(),
        vec!["cdm/Date", "cdm/Party"],
        "recursive lookup must include descendants, ordered by schema_id"
    );
    // Leaf namespace: own schema only.
    let leaf = db
        .list_schemas_by_namespace("cdm.base.datetime", false)
        .await
        .unwrap();
    assert_eq!(leaf.len(), 1);
    assert_eq!(leaf[0].title, "Date");
    // Unknown namespace: empty, not error.
    assert!(db
        .list_schemas_by_namespace("nope", true)
        .await
        .unwrap()
        .is_empty());
}

// ── namespace_generation_order: determinism + cycle ────────────────────

#[tokio::test]
async fn namespace_generation_order_is_deterministic_on_mock() {
    let engine = MockEngine::new();
    assert_generation_order_deterministic(&engine).await;
}

#[tokio::test]
async fn namespace_generation_order_is_deterministic_on_grafeo() {
    let engine = GrafeoEngine::in_memory().unwrap();
    assert_generation_order_deterministic(&engine).await;
}

async fn assert_generation_order_deterministic(db: &(impl GraphIngestor + GraphQuerier)) {
    // Ingest in NON-topological, NON-alphabetical order to prove the sort
    // is computed, not incidental.
    for fqn in ["zeta", "billing", "mid", "alpha"] {
        db.ingest_namespace(&ns(fqn, None, None)).await.unwrap();
    }
    // billing imports alpha; mid imports alpha (tie between billing/mid).
    db.ingest_namespace_import(&import("billing", "alpha", false, None))
        .await
        .unwrap();
    db.ingest_namespace_import(&import("mid", "alpha", false, None))
        .await
        .unwrap();
    // Duplicate import must not affect order (deduped).
    db.ingest_namespace_import(&import("mid", "alpha", false, None))
        .await
        .unwrap();

    let first = db.namespace_generation_order().await.unwrap();
    let second = db.namespace_generation_order().await.unwrap();
    assert_eq!(
        first,
        vec!["alpha", "billing", "mid", "zeta"],
        "imported-before-importer with lexicographic tie-break; stable across runs ({first:?} vs {second:?})"
    );
    assert_eq!(first, second, "repeat runs must be identical");
}

#[tokio::test]
async fn namespace_generation_order_cycle_is_error_on_mock() {
    let engine = MockEngine::new();
    assert_generation_order_cycle(&engine).await;
}

#[tokio::test]
async fn namespace_generation_order_cycle_is_error_on_grafeo() {
    let engine = GrafeoEngine::in_memory().unwrap();
    assert_generation_order_cycle(&engine).await;
}

async fn assert_generation_order_cycle(db: &(impl GraphIngestor + GraphQuerier)) {
    for fqn in ["a", "b", "c"] {
        db.ingest_namespace(&ns(fqn, None, None)).await.unwrap();
    }
    db.ingest_namespace_import(&import("a", "b", false, None))
        .await
        .unwrap();
    db.ingest_namespace_import(&import("b", "c", false, None))
        .await
        .unwrap();
    db.ingest_namespace_import(&import("c", "a", false, None))
        .await
        .unwrap();
    let err = db
        .namespace_generation_order()
        .await
        .expect_err("cycle must be an error naming the members");
    let msg = err.to_string();
    assert!(msg.contains("cycle"), "{msg}");
    for member in ["a", "b", "c"] {
        assert!(
            msg.contains(member),
            "cycle error must name {member}: {msg}"
        );
    }
}

// ── core pure-function determinism (unit-level pins) ────────────────────

#[test]
fn topological_order_stable_across_runs_and_input_orders() {
    let a = ["z".to_string(), "m".to_string(), "a".to_string()].to_vec();
    let b = ["a".to_string(), "z".to_string(), "m".to_string()].to_vec();
    let o1 = topological_namespace_order(&a, &[]).unwrap();
    let o2 = topological_namespace_order(&b, &[]).unwrap();
    assert_eq!(o1, o2);
    assert_eq!(o1, vec!["a", "m", "z"]);
}

#[test]
fn qualified_schema_id_back_compat() {
    // Namespaced: `<ns>::<Name>`; namespace-less: legacy id verbatim.
    assert_eq!(
        qualified_schema_id(Some("cdm.base"), "old/id.json", "Party"),
        "cdm.base::Party"
    );
    assert_eq!(
        qualified_schema_id(None, "old/id.json", "Party"),
        "old/id.json"
    );
}

// ── atproto coexistence (collision resolution pins) ─────────────────────

#[tokio::test]
async fn atproto_and_namespace_families_coexist_on_mock() {
    let engine = MockEngine::new();
    // AT-Protocol family (renamed).
    engine
        .ingest_atproto_namespace(&AtprotoNamespaceNode {
            authority: "nz.gravy".into(),
            segment: "grants".into(),
            domain: "atproto".into(),
        })
        .await
        .unwrap();
    engine
        .ingest_lexicon(&LexiconNode {
            nsid: "nz.gravy.grants.grant".into(),
            lex_type: "record".into(),
            key_strategy: "tid".into(),
            revision: Some(1),
            description: None,
            domain: "grants".into(),
        })
        .await
        .unwrap();
    engine
        .ingest_edge(
            "nz.gravy.grants.grant",
            "nz.gravy",
            EdgeType::InNamespace,
            None,
        )
        .await
        .unwrap();
    // Graph-wide namespace family — same edge type, different node family.
    engine
        .ingest_namespace(&ns("cdm.base", None, Some("config")))
        .await
        .unwrap();
    engine
        .ingest_schema(&schema("cdm/Party", "Party", "common"))
        .await
        .unwrap();
    engine
        .ingest_edge("cdm/Party", "cdm.base", EdgeType::InNamespace, None)
        .await
        .unwrap();

    let atproto = engine.get_atproto_namespaces().await.unwrap();
    assert_eq!(atproto.len(), 1);
    assert_eq!(atproto[0].authority, "nz.gravy");
    let namespaces = engine.list_namespaces().await.unwrap();
    assert_eq!(namespaces, vec![ns("cdm.base", None, Some("config"))]);
    // Each family's InNamespace edges resolve to its own node family.
    assert_eq!(
        engine
            .list_schemas_by_namespace("cdm.base", false)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn atproto_and_namespace_families_coexist_on_grafeo() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine
        .ingest_atproto_namespace(&AtprotoNamespaceNode {
            authority: "nz.gravy".into(),
            segment: "grants".into(),
            domain: "atproto".into(),
        })
        .await
        .unwrap();
    engine
        .ingest_namespace(&ns("cdm.base", None, Some("config")))
        .await
        .unwrap();
    // Shared InNamespace edge type: Lexicon nsid → authority, Schema id → fqn.
    let atproto = engine.get_atproto_namespaces().await.unwrap();
    assert_eq!(atproto.len(), 1);
    assert_eq!(atproto[0].authority, "nz.gravy");
    let namespaces = engine.list_namespaces().await.unwrap();
    assert_eq!(namespaces.len(), 1);
    assert_eq!(namespaces[0].fqn, "cdm.base");
    assert_eq!(namespaces[0].source.as_deref(), Some("config"));
}

// ── SchemaNode.namespace grafeo column round-trip ───────────────────────

#[tokio::test]
async fn schema_namespace_roundtrip_on_grafeo() {
    let engine = GrafeoEngine::in_memory().unwrap();

    // Namespace-less schema: namespace stays None (back-compat).
    let plain = schema("common/PersonType", "PersonType", "common");
    engine.ingest_schema(&plain).await.unwrap();
    let read_back = engine.get_schema("PersonType").await.unwrap().unwrap();
    assert_eq!(read_back.namespace, None);
    assert_eq!(read_back.schema_id, "common/PersonType");

    // Namespaced schema: namespace round-trips.
    let mut namespaced = schema("cdm/Party", "Party", "common");
    namespaced.namespace = Some("cdm.base".to_string());
    engine.ingest_schema(&namespaced).await.unwrap();
    let read_back = engine.get_schema("Party").await.unwrap().unwrap();
    assert_eq!(read_back.namespace.as_deref(), Some("cdm.base"));
    // And the classification-data path carries it too.
    let classified = engine.get_classification_data().await.unwrap();
    let party = classified.iter().find(|c| c.title == "Party").unwrap();
    assert_eq!(party.namespace.as_deref(), Some("cdm.base"));
    let person = classified.iter().find(|c| c.title == "PersonType").unwrap();
    assert_eq!(person.namespace, None);
}

// ── EdgeType payload write path (ingest_edge with props) ────────────────

#[tokio::test]
async fn namespace_imports_via_ingest_edge_payload_on_grafeo() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine.ingest_namespace(&ns("a", None, None)).await.unwrap();
    engine.ingest_namespace(&ns("b", None, None)).await.unwrap();
    let props = EdgeProperties {
        import_wildcard: Some(true),
        import_alias: Some("bee".to_string()),
        ..Default::default()
    };
    engine
        .ingest_edge("a", "b", EdgeType::NamespaceImports, Some(&props))
        .await
        .unwrap();
    let imports = engine.get_namespace_imports("a").await.unwrap();
    assert_eq!(imports.len(), 1);
    assert!(imports[0].wildcard);
    assert_eq!(imports[0].alias.as_deref(), Some("bee"));
}

// ── validation checks ───────────────────────────────────────────────────

async fn validation_issues(
    db: &(impl GraphIngestor + GraphQuerier),
    toml: &str,
) -> Vec<(String, String)> {
    let config = parse_domain_config_str(toml).unwrap();
    ValidationPass::run(db, &config)
        .await
        .into_iter()
        .map(|i| (i.check.to_string(), i.message))
        .collect()
}

#[tokio::test]
async fn namespaceless_graph_has_no_namespace_issues() {
    // Back-compat: a namespace-less project produces ZERO namespace checks.
    let engine = MockEngine::new();
    engine
        .ingest_schema(&schema("common/PersonType", "PersonType", "common"))
        .await
        .unwrap();
    let issues = validation_issues(&engine, &validation_config_toml(false)).await;
    assert!(
        issues
            .iter()
            .all(|(check, _)| !check.starts_with("namespace_")),
        "namespace-less graph must not trigger namespace checks: {issues:?}"
    );
}

#[tokio::test]
async fn undeclared_import_is_error() {
    let engine = MockEngine::new();
    engine
        .ingest_namespace(&ns("billing", None, None))
        .await
        .unwrap();
    engine
        .ingest_namespace(&ns("cdm.base", None, Some("config")))
        .await
        .unwrap();
    // 'ghost' namespace: neither in graph nor declared in domains.toml.
    engine
        .ingest_namespace_import(&import("billing", "ghost", false, None))
        .await
        .unwrap();
    let issues = validation_issues(&engine, &validation_config_toml(false)).await;
    let undeclared: Vec<_> = issues
        .iter()
        .filter(|(c, _)| c == "namespace_import_undeclared")
        .collect();
    assert_eq!(undeclared.len(), 1, "{issues:?}");
    assert!(undeclared[0].1.contains("ghost"), "{issues:?}");

    // Declared in the config allowlist → the same import is fine.
    let engine = MockEngine::new();
    engine
        .ingest_namespace(&ns("billing", None, None))
        .await
        .unwrap();
    engine
        .ingest_namespace_import(&import("billing", "cdm.base", false, None))
        .await
        .unwrap();
    let issues = validation_issues(&engine, &validation_config_toml(false)).await;
    assert!(
        issues
            .iter()
            .all(|(c, _)| c != "namespace_import_undeclared"),
        "{issues:?}"
    );
}

#[tokio::test]
async fn cross_domain_import_requires_depends_on() {
    // Same-domain import (cdm.base assigned to products; billing's import
    // target domain resolves to products via config): NO depends_on → the
    // cross-domain gate only fires when domains actually differ. Build the
    // differing case: products' namespace imports common's namespace.
    let engine = MockEngine::new();
    engine
        .ingest_namespace(&ns("cdm.base", None, Some("config")))
        .await
        .unwrap();
    engine
        .ingest_namespace(&ns("shared.kernel", None, None))
        .await
        .unwrap();
    engine
        .ingest_schema(&schema("shared/KernelType", "KernelType", "common"))
        .await
        .unwrap();
    engine
        .ingest_edge(
            "shared/KernelType",
            "shared.kernel",
            EdgeType::InNamespace,
            None,
        )
        .await
        .unwrap();
    // shared.kernel resolves to domain 'common' (schema-derived fallback).
    engine
        .ingest_namespace_import(&import("cdm.base", "shared.kernel", false, None))
        .await
        .unwrap();

    // products (cdm.base's declared domain) has NO depends_on common → Error.
    let issues = validation_issues(&engine, &validation_config_toml(false)).await;
    let missing: Vec<_> = issues
        .iter()
        .filter(|(c, _)| c == "namespace_import_undeclared_dependency")
        .collect();
    assert_eq!(missing.len(), 1, "{issues:?}");
    assert!(
        missing[0].1.contains("products") && missing[0].1.contains("common"),
        "message must name both domains: {issues:?}"
    );

    // With depends_on declared → clean.
    let issues = validation_issues(&engine, &validation_config_toml(true)).await;
    assert!(
        issues
            .iter()
            .all(|(c, _)| c != "namespace_import_undeclared_dependency"),
        "{issues:?}"
    );
}

#[tokio::test]
async fn namespace_domain_conflict_is_warning() {
    // cdm.base declared under products, but its member schema is
    // dir-derived from common → one Warning, no Errors.
    let engine = MockEngine::new();
    engine
        .ingest_namespace(&ns("cdm.base", None, Some("config")))
        .await
        .unwrap();
    engine
        .ingest_schema(&schema("common/Party", "Party", "common"))
        .await
        .unwrap();
    engine
        .ingest_edge("common/Party", "cdm.base", EdgeType::InNamespace, None)
        .await
        .unwrap();
    let issues = validation_issues(&engine, &validation_config_toml(true)).await;
    let conflicts: Vec<_> = issues
        .iter()
        .filter(|(c, _)| c == "namespace_domain_conflict")
        .collect();
    assert_eq!(conflicts.len(), 1, "{issues:?}");
    assert!(conflicts[0].1.contains("cdm.base"), "{issues:?}");

    // Assigned consistently (schema in products) → no conflict.
    let engine = MockEngine::new();
    engine
        .ingest_namespace(&ns("cdm.base", None, Some("config")))
        .await
        .unwrap();
    engine
        .ingest_schema(&schema("products/Party", "Party", "products"))
        .await
        .unwrap();
    engine
        .ingest_edge("products/Party", "cdm.base", EdgeType::InNamespace, None)
        .await
        .unwrap();
    let issues = validation_issues(&engine, &validation_config_toml(true)).await;
    assert!(
        issues.iter().all(|(c, _)| c != "namespace_domain_conflict"),
        "{issues:?}"
    );
}

// ── config grammar round-trip (dotted fqns as TOML keys) ────────────────

#[test]
fn config_namespace_grammar_roundtrip() {
    let toml = "[domains.products]\n\
                label = \"Products\"\n\
                schema_dir = \"products\"\n\
                postgres_schema = \"products\"\n\n\
                [namespaces.\"cdm.base.datetime\"]\n\
                domain = \"products\"\n\n\
                [namespaces.cdm.base]\n\
                domain = \"products\"\n";
    let config = parse_domain_config_str(toml).unwrap();
    // Quoted dotted key AND nested unquoted spelling both normalize.
    assert_eq!(
        config.namespaces["cdm.base.datetime"].domain.as_deref(),
        Some("products")
    );
    assert_eq!(
        config.namespaces["cdm.base"].domain.as_deref(),
        Some("products")
    );
    // The parent of a declared intermediate level (`cdm`) is NOT implied.
    assert!(!config.namespaces.contains_key("cdm"));
}

// ── derived NamespaceDepends ────────────────────────────────────────────

#[test]
fn namespace_depends_derivation_through_domain_plane() {
    use codegraph_core::types::derive_namespace_depends;
    use std::collections::HashMap;
    let mut ns_domain = HashMap::new();
    ns_domain.insert("billing".to_string(), "billing".to_string());
    ns_domain.insert("cdm.base".to_string(), "common".to_string());
    let mut domain_depends = HashMap::new();
    domain_depends.insert("billing".to_string(), vec!["common".to_string()]);
    let pairs = derive_namespace_depends(&ns_domain, &domain_depends);
    assert_eq!(
        pairs,
        vec![("billing".to_string(), "cdm.base".to_string())],
        "billing's domain depends on cdm.base's domain ⇒ NamespaceDepends(billing, cdm.base)"
    );
}
