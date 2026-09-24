use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

/// A first-class namespace node (issue #267).
///
/// A namespace is WHERE a type lives and WHAT it can see: hierarchical
/// (dotted FQNs), import-based visibility. It is deliberately distinct from
/// a domain (bounded context / deploy boundary): the namespace→domain
/// assignment is optional, many-to-one, and declared in config
/// (`[namespaces."<fqn>"]` with `domain = "..."`).
///
/// # Naming-collision note
///
/// The AT-Protocol line previously owned the `NamespaceNode` name with
/// unrelated repo-namespace semantics (`authority`/`segment`/`domain`); it
/// was renamed to [`crate::types::AtprotoNamespaceNode`] (and its grafeo
/// label `Namespace` → `AtprotoNamespace`, trait methods
/// `ingest_namespace`→`ingest_atproto_namespace`,
/// `get_namespaces`→`get_atproto_namespaces`) so this graph-wide namespace
/// concept could take the canonical names. [`crate::types::EdgeType::InNamespace`]
/// is shared by both features (the "X lives in namespace Y" semantics are
/// compatible).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamespaceNode {
    /// Dotted fully-qualified name, e.g. `"cdm.base.datetime"`. Unique key
    /// of the node (returned by `ingest_namespace`).
    pub fqn: String,
    /// Parent namespace FQN, when this namespace is nested (written as a
    /// `NamespaceParent` edge child → parent; persisted flat too so
    /// read-back is one query).
    pub parent: Option<String>,
    /// Provenance of the declaration: `"config"` (domains.toml),
    /// `"discovered"` (found in source but not declared), `"mox"`,
    /// `"rosetta"`, ... Open-ended; `None` when unknown.
    pub source: Option<String>,
}

/// One `NamespaceImports` edge (issue #267): `from_ns` imports `to_ns`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamespaceImport {
    /// The importing namespace FQN (edge source).
    pub from_ns: String,
    /// The imported (visible) namespace FQN (edge target).
    pub to_ns: String,
    /// Wildcard import (`import <ns>.*`) — every type in `to_ns` visible.
    pub wildcard: bool,
    /// Optional alias for the imported namespace (`import <ns> as alias`).
    pub alias: Option<String>,
}

/// The schema_id form for a namespaced type: `<ns>::<Name>`.
///
/// Namespace-less schemas keep the legacy id verbatim (`uri.to_string()`
/// from the JSON/mox ingest paths) — this is the back-compat contract: the
/// function is inert when `namespace` is `None`.
pub fn qualified_schema_id(namespace: Option<&str>, legacy_schema_id: &str, title: &str) -> String {
    match namespace {
        Some(ns) if !ns.is_empty() => format!("{ns}::{title}"),
        _ => legacy_schema_id.to_string(),
    }
}

/// Title-uniqueness collision policy (issue #267).
///
/// Uniqueness is scoped per namespace: the key is `(namespace, title)`.
/// When two entries collide inside the SAME namespace the first (in the
/// given order) keeps the plain/legacy id and later ones are disambiguated:
/// first by the namespace-qualified form (`<ns>::<Title>`), then by a
/// numeric suffix (`<ns>::<Title>::2`, `::3`, ...). Returns the ids in
/// input order. `#268` wires this into generation-order title claiming.
pub fn disambiguate_schema_ids(entries: &[(Option<String>, String, String)]) -> Vec<String> {
    let mut taken: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(entries.len());
    for (namespace, title, legacy_id) in entries {
        let base = qualified_schema_id(namespace.as_deref(), legacy_id, title);
        let mut candidate = base.clone();
        let mut n = 1;
        while !taken.insert(candidate.clone()) {
            n += 1;
            candidate = match namespace.as_deref() {
                Some(ns) if !ns.is_empty() => format!("{ns}::{title}::{n}"),
                _ => format!("{legacy_id}::{n}"),
            };
        }
        out.push(candidate);
    }
    out
}

/// Deterministic topological order over namespaces by their imports
/// (issue #267, `GraphQuerier::namespace_generation_order`).
///
/// An import edge `(a, b)` means `a` imports `b`, so `b` must be ordered
/// BEFORE `a`. Kahn's algorithm with a **lexicographic tie-break on FQN**
/// makes the output fully deterministic for the same input set. A cycle is
/// an error naming the cycle members (sorted, so the message is stable
/// too).
pub fn topological_namespace_order(
    fqns: &[String],
    imports: &[(String, String)],
) -> Result<Vec<String>, String> {
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    for fqn in fqns {
        in_degree.entry(fqn.as_str()).or_insert(0);
    }
    // Deduplicate edges: a repeated import must not double-count in-degree.
    let mut edges: BTreeSet<(&str, &str)> = BTreeSet::new();
    for (from, to) in imports {
        if from == to {
            continue;
        }
        if !fqns.iter().any(|f| f.as_str() == from.as_str())
            || !fqns.iter().any(|f| f.as_str() == to.as_str())
        {
            continue;
        }
        edges.insert((from.as_str(), to.as_str()));
    }
    for (from, to) in &edges {
        // `from` imports `to` ⇒ `to` first; `from`'s in-degree grows.
        *in_degree.entry(from).or_insert(0) += 1;
        dependents.entry(to).or_default().insert(from);
    }

    // BTreeSet gives the lexicographic tie-break for free.
    let mut ready: BTreeSet<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(fqn, _)| *fqn)
        .collect();
    let mut order = Vec::with_capacity(fqns.len());
    while let Some(current) = ready.pop_first() {
        order.push(current.to_string());
        if let Some(deps) = dependents.get(current) {
            for dep in deps.clone() {
                if let Some(deg) = in_degree.get_mut(dep) {
                    *deg -= 1;
                    if *deg == 0 {
                        ready.insert(dep);
                    }
                }
            }
        }
    }

    if order.len() != in_degree.len() {
        let ordered: HashSet<&str> = order.iter().map(|s| s.as_str()).collect();
        let mut cycle: Vec<String> = in_degree
            .keys()
            .filter(|fqn| !ordered.contains(**fqn))
            .map(|fqn| fqn.to_string())
            .collect();
        cycle.sort();
        return Err(format!(
            "namespace import cycle detected involving: {}",
            cycle.join(", ")
        ));
    }
    Ok(order)
}

/// Derive `NamespaceDepends` pairs (issue #267): namespace A's assigned
/// domain depends (per domains.toml `depends_on`) on namespace B's assigned
/// domain ⇒ `(A, B)` namespace dependency. Output is deduplicated and
/// sorted — deterministic.
///
/// Namespaces without a domain assignment contribute nothing (the
/// dependency is defined only through the domain plane).
pub fn derive_namespace_depends(
    ns_domain: &HashMap<String, String>,
    domain_depends: &HashMap<String, Vec<String>>,
) -> Vec<(String, String)> {
    let mut out: BTreeSet<(String, String)> = BTreeSet::new();
    // Sort namespace pairs for deterministic iteration.
    let mut assignments: Vec<(&String, &String)> = ns_domain.iter().collect();
    assignments.sort();
    for (ns_a, domain_a) in &assignments {
        if let Some(depends) = domain_depends.get(*domain_a) {
            for (ns_b, domain_b) in &assignments {
                if ns_b == ns_a {
                    continue;
                }
                if depends.contains(*domain_b) {
                    out.insert(((*ns_a).clone(), (*ns_b).clone()));
                }
            }
        }
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fqn(s: &str) -> String {
        s.to_string()
    }

    #[test]
    fn namespace_node_serde_roundtrip() {
        let node = NamespaceNode {
            fqn: "cdm.base.datetime".into(),
            parent: Some("cdm.base".into()),
            source: Some("config".into()),
        };
        let json = serde_json::to_string(&node).unwrap();
        let back: NamespaceNode = serde_json::from_str(&json).unwrap();
        assert_eq!(node, back);
    }

    #[test]
    fn namespace_import_serde_roundtrip() {
        let imp = NamespaceImport {
            from_ns: "billing".into(),
            to_ns: "cdm.base".into(),
            wildcard: true,
            alias: Some("base".into()),
        };
        let json = serde_json::to_string(&imp).unwrap();
        let back: NamespaceImport = serde_json::from_str(&json).unwrap();
        assert_eq!(imp, back);
    }

    #[test]
    fn qualified_schema_id_uses_ns_title_when_namespaced() {
        assert_eq!(
            qualified_schema_id(
                Some("cdm.base"),
                "common/json/PersonType.json",
                "PersonType"
            ),
            "cdm.base::PersonType"
        );
    }

    #[test]
    fn qualified_schema_id_inert_when_namespaceless() {
        // Back-compat: None (or empty) keeps the legacy id verbatim.
        assert_eq!(
            qualified_schema_id(None, "common/json/PersonType.json", "PersonType"),
            "common/json/PersonType.json"
        );
        assert_eq!(qualified_schema_id(Some(""), "legacy", "T"), "legacy");
    }

    #[test]
    fn disambiguation_first_keeps_plain_later_gets_qualified() {
        let entries = vec![
            (
                Some("cdm".to_string()),
                "PersonType".to_string(),
                "cdm/PersonType.json".to_string(),
            ),
            (
                Some("cdm".to_string()),
                "PersonType".to_string(),
                "cdm/dup/PersonType.json".to_string(),
            ),
        ];
        let ids = disambiguate_schema_ids(&entries);
        assert_eq!(ids[0], "cdm::PersonType");
        assert_eq!(ids[1], "cdm::PersonType::2");
    }

    #[test]
    fn disambiguation_same_title_different_namespaces_do_not_collide() {
        let entries = vec![
            (
                Some("cdm".to_string()),
                "PersonType".to_string(),
                "a.json".to_string(),
            ),
            (
                Some("hr".to_string()),
                "PersonType".to_string(),
                "b.json".to_string(),
            ),
        ];
        let ids = disambiguate_schema_ids(&entries);
        assert_eq!(ids[0], "cdm::PersonType");
        assert_eq!(ids[1], "hr::PersonType");
    }

    #[test]
    fn topological_order_imports_before_importers() {
        // billing imports cdm.base ⇒ cdm.base first.
        let fqns = vec![fqn("billing"), fqn("cdm.base"), fqn("hr")];
        let imports = vec![("billing".to_string(), "cdm.base".to_string())];
        let order = topological_namespace_order(&fqns, &imports).unwrap();
        assert_eq!(order, vec!["cdm.base", "billing", "hr"]);
    }

    #[test]
    fn topological_order_is_lexicographically_stable() {
        // No imports: pure lexicographic order, stable across runs.
        let fqns = vec![fqn("zeta"), fqn("alpha"), fqn("mid")];
        let first = topological_namespace_order(&fqns, &[]).unwrap();
        let second = topological_namespace_order(&fqns, &[]).unwrap();
        assert_eq!(first, vec!["alpha", "mid", "zeta"]);
        assert_eq!(first, second);
    }

    #[test]
    fn topological_order_tie_break_is_lexicographic() {
        // Both b and c are ready after a; lexicographic picks b first.
        let fqns = vec![fqn("a"), fqn("c"), fqn("b")];
        let imports = vec![
            ("c".to_string(), "a".to_string()),
            ("b".to_string(), "a".to_string()),
        ];
        let order = topological_namespace_order(&fqns, &imports).unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn topological_order_deduplicates_repeated_imports() {
        let fqns = vec![fqn("a"), fqn("b")];
        let imports = vec![
            ("a".to_string(), "b".to_string()),
            ("a".to_string(), "b".to_string()),
        ];
        let order = topological_namespace_order(&fqns, &imports).unwrap();
        assert_eq!(order, vec!["b", "a"]);
    }

    #[test]
    fn topological_order_cycle_is_an_error_naming_members() {
        let fqns = vec![fqn("a"), fqn("b"), fqn("c")];
        let imports = vec![
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "c".to_string()),
            ("c".to_string(), "a".to_string()),
        ];
        let err = topological_namespace_order(&fqns, &imports).unwrap_err();
        assert!(err.contains("cycle"), "{err}");
        assert!(err.contains("a, b, c"), "{err}");
    }

    #[test]
    fn derive_namespace_depends_through_domain_plane() {
        let mut ns_domain = HashMap::new();
        ns_domain.insert("billing".to_string(), "billing".to_string());
        ns_domain.insert("cdm.base".to_string(), "common".to_string());
        ns_domain.insert("free".to_string(), "floating".to_string());
        let mut domain_depends = HashMap::new();
        domain_depends.insert("billing".to_string(), vec!["common".to_string()]);
        let pairs = derive_namespace_depends(&ns_domain, &domain_depends);
        assert_eq!(pairs, vec![("billing".to_string(), "cdm.base".to_string())]);
    }

    #[test]
    fn derive_namespace_depends_ignores_unassigned_namespaces() {
        let mut ns_domain = HashMap::new();
        ns_domain.insert("billing".to_string(), "billing".to_string());
        // cdm.base has NO domain assignment → no dependency derivable.
        let mut domain_depends = HashMap::new();
        domain_depends.insert("billing".to_string(), vec!["common".to_string()]);
        let pairs = derive_namespace_depends(&ns_domain, &domain_depends);
        assert!(pairs.is_empty());
    }
}
