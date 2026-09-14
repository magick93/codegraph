use std::collections::HashSet;
use std::path::{Path, PathBuf};

use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{
    ActorNode, ActorPolicyModel, ActorPolicyNode, CapabilityNode, DelegationRecord, GrantEdge,
    NeverBothGroup,
};
use codegraph_ifml_dsl::IfmlModel;
use rex_driver::compile_actors_str;
use rex_ir::ActorModel;

use crate::error::{Error, Result};

/// Resolve the IFML model's `import` statements against rexlang actor-policy
/// files and artifacts, ingest the merged policy into the graph, and
/// validate view `roles`/`requires` against it.
///
/// Each import path is resolved relative to the `.ifml` file's directory and
/// deduplicated, so the same file imported twice is ingested once. `.json`
/// imports are standalone rexlang actor artifacts; `.actor` imports are
/// compiled against their own `import "x.mox"` domain files. Import
/// failures (missing files, compile errors, invalid artifacts) are reported
/// as warnings and never fail generation. Returns the number of imports
/// that yielded an ingested policy.
pub async fn ingest_actor_imports(
    ingestor: &dyn GraphIngestor,
    querier: &dyn GraphQuerier,
    model: &IfmlModel,
    ifml_path: &Path,
) -> Result<usize> {
    if model.imports.is_empty() {
        return Ok(0);
    }
    let base_dir = ifml_path.parent().unwrap_or_else(|| Path::new("."));
    let mut policies: Vec<ActorPolicyModel> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for raw in &model.imports {
        let resolved = base_dir.join(raw);
        let dedup_key = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());
        if !seen.insert(dedup_key) {
            continue;
        }
        if let Some(policy) = import_policy(raw, &resolved) {
            policies.push(policy);
        }
    }

    let imported = policies.len();
    if imported == 0 {
        return Ok(0);
    }

    let merged = merge_policies(policies);
    ingestor
        .ingest_actor_policy(&merged)
        .await
        .map_err(Error::Graph)?;

    validate_views(querier, model).await;
    Ok(imported)
}

/// Resolve raw `import` paths against `base_dir` to a merged actor policy
/// without ingesting anything — the read-only path used by the LSP for
/// capability/role diagnostics. Deduplicates like `ingest_actor_imports`;
/// unresolvable imports are skipped (with a warning on stderr). Returns
/// `None` when no import yields a policy.
pub fn resolve_actor_policy(import_paths: &[String], base_dir: &Path) -> Option<ActorPolicyModel> {
    let mut policies: Vec<ActorPolicyModel> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for raw in import_paths {
        let resolved = base_dir.join(raw);
        let dedup_key = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());
        if !seen.insert(dedup_key) {
            continue;
        }
        if let Some(policy) = import_policy(raw, &resolved) {
            policies.push(policy);
        }
    }
    (!policies.is_empty()).then(|| merge_policies(policies))
}

fn import_policy(raw: &str, resolved: &Path) -> Option<ActorPolicyModel> {
    let source = match std::fs::read_to_string(resolved) {
        Ok(source) => source,
        Err(e) => {
            eprintln!("Warning: policy import '{raw}' could not be read: {e}");
            return None;
        }
    };
    let extension = resolved
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    match extension {
        "json" => import_artifact(raw, &source),
        "actor" => import_actor_source(
            raw,
            resolved.parent().unwrap_or_else(|| Path::new(".")),
            &source,
        ),
        other => {
            eprintln!(
                "Warning: policy import '{raw}' has unsupported extension '{other}' (expected .actor or .json)"
            );
            None
        }
    }
}

fn import_artifact(raw: &str, source: &str) -> Option<ActorPolicyModel> {
    match ActorModel::from_json(source) {
        Ok(model) => Some(actor_policy_from_model(&model)),
        Err(e) => {
            eprintln!("Warning: policy import '{raw}' is not a valid rexlang actor artifact: {e}");
            None
        }
    }
}

fn import_actor_source(raw: &str, dir: &Path, source: &str) -> Option<ActorPolicyModel> {
    let mut domains: Vec<(String, String)> = Vec::new();
    collect_domains(dir, source, &mut domains);
    let compilation = compile_actors_str(raw, source, &domains);
    for (path, diagnostic) in &compilation.diagnostics {
        eprintln!(
            "Warning: policy import '{raw}' diagnostic in {path}: {}",
            diagnostic.message
        );
    }
    match compilation.model {
        Some(model) => Some(actor_policy_from_model(&model)),
        None => {
            eprintln!("Warning: policy import '{raw}' failed to compile — no policy ingested");
            None
        }
    }
}

/// Collect the `(path, source)` domain pairs an `.actor` file imports,
/// following `import "x.mox"` lines transitively. Paths are keyed exactly as
/// written (that is what `compile_actors_str` matches on) and resolved
/// relative to the importing file's directory; missing files warn and are
/// skipped.
fn collect_domains(dir: &Path, source: &str, domains: &mut Vec<(String, String)>) {
    for import in scan_imports(source) {
        if domains.iter().any(|(path, _)| *path == import) {
            continue;
        }
        let resolved = dir.join(&import);
        match std::fs::read_to_string(&resolved) {
            Ok(domain_source) => {
                domains.push((import.clone(), domain_source.clone()));
                if let Some(domain_dir) = resolved.parent() {
                    collect_domains(domain_dir, &domain_source, domains);
                }
            }
            Err(e) => {
                eprintln!(
                    "Warning: domain file '{import}' imported by an actor policy was not found ({e}) — skipped"
                );
            }
        }
    }
}

/// Scan source text for rexlang import lines (`import "path.mox"`). A simple
/// line-oriented scan is sufficient for codegraph's policy resolution.
fn scan_imports(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("import ")?.trim();
            let rest = rest.trim_end_matches(';').trim();
            let path = rest.strip_prefix('"')?.strip_suffix('"')?;
            (!path.is_empty()).then(|| path.to_string())
        })
        .collect()
}

fn actor_policy_from_model(model: &ActorModel) -> ActorPolicyModel {
    let mut actors = Vec::new();
    let mut capabilities = Vec::new();
    let mut grants = Vec::new();
    let mut never_both = Vec::new();
    let mut purposes = Vec::new();
    let mut delegations = Vec::new();
    let mut blocks = Vec::new();
    for block in &model.blocks {
        blocks.push(block.name.clone());
        for actor in &block.actors {
            actors.push(ActorNode {
                name: actor.name.clone(),
                kind: actor.kind.map(|kind| match kind {
                    rex_ir::ActorKind::Human => "human".to_string(),
                    rex_ir::ActorKind::Agent => "agent".to_string(),
                }),
                extends: actor.extends.clone(),
                block: Some(block.name.clone()),
            });
        }
        for capability in &block.capabilities {
            capabilities.push(CapabilityNode {
                name: capability.name.clone(),
                class: class_name(&capability.class),
                block: Some(block.name.clone()),
            });
        }
        for grant in &block.grants {
            for entry in &grant.entries {
                grants.push(GrantEdge {
                    actor: grant.actor.clone(),
                    capability: entry.capability.clone(),
                    effect: match entry.effect {
                        rex_ir::GrantEffect::Permit => "permit".to_string(),
                        rex_ir::GrantEffect::Forbid => "forbid".to_string(),
                    },
                    when: entry.when.clone(),
                    obligations: entry.obligations.clone(),
                });
            }
        }
        for group in &block.never_both {
            never_both.push(NeverBothGroup {
                capabilities: group.capabilities.clone(),
            });
        }
        purposes.extend(block.purposes.iter().cloned());
        for delegation in &block.delegations {
            delegations.push(DelegationRecord {
                name: delegation.name.clone(),
                from_actor: delegation.from.clone(),
                to_actor: delegation.to.clone(),
                purpose: delegation.purpose.clone(),
                entries: delegation
                    .entries
                    .iter()
                    .map(|entry| GrantEdge {
                        actor: delegation.from.clone(),
                        capability: entry.capability.clone(),
                        effect: match entry.effect {
                            rex_ir::GrantEffect::Permit => "permit".to_string(),
                            rex_ir::GrantEffect::Forbid => "forbid".to_string(),
                        },
                        when: entry.when.clone(),
                        obligations: entry.obligations.clone(),
                    })
                    .collect(),
            });
        }
    }
    ActorPolicyModel {
        actors,
        capabilities,
        grants,
        policy: ActorPolicyNode {
            blocks,
            never_both,
            purposes,
            delegations,
        },
    }
}

fn class_name(type_ref: &rex_ir::TypeRef) -> String {
    match type_ref {
        rex_ir::TypeRef::Primitive(p) => p.to_string(),
        other => other.qualified_name().unwrap_or_default(),
    }
}

fn merge_policies(policies: Vec<ActorPolicyModel>) -> ActorPolicyModel {
    let mut merged = ActorPolicyModel {
        actors: Vec::new(),
        capabilities: Vec::new(),
        grants: Vec::new(),
        policy: ActorPolicyNode {
            blocks: Vec::new(),
            never_both: Vec::new(),
            purposes: Vec::new(),
            delegations: Vec::new(),
        },
    };
    for policy in policies {
        merged.actors.extend(policy.actors);
        merged.capabilities.extend(policy.capabilities);
        merged.grants.extend(policy.grants);
        merged.policy.blocks.extend(policy.policy.blocks);
        merged.policy.never_both.extend(policy.policy.never_both);
        merged.policy.purposes.extend(policy.policy.purposes);
        merged.policy.delegations.extend(policy.policy.delegations);
    }
    merged
}

/// Warn on view `roles` that match no actor and view `requires` that match
/// no capability. Validation only runs when a policy was ingested; without
/// one, roles/requires stay inert.
async fn validate_views(querier: &dyn GraphQuerier, model: &IfmlModel) {
    let actor_names: HashSet<String> = match querier.get_actors().await {
        Ok(actors) => actors.into_iter().map(|a| a.name).collect(),
        Err(e) => {
            eprintln!("Warning: role validation skipped (actors unqueryable): {e}");
            return;
        }
    };
    let capability_names: HashSet<String> = match querier.get_capabilities().await {
        Ok(capabilities) => capabilities.into_iter().map(|c| c.name).collect(),
        Err(e) => {
            eprintln!("Warning: capability validation skipped (capabilities unqueryable): {e}");
            return;
        }
    };
    for view in &model.views {
        for role in &view.roles {
            if !actor_names.contains(role) {
                eprintln!(
                    "Warning: view '{}' declares role '{role}' but no actor policy defines it",
                    view.name
                );
            }
        }
        for required in &view.requires {
            if !capability_names.contains(required) {
                eprintln!(
                    "Warning: view '{}' requires capability '{required}' but no actor policy defines it",
                    view.name
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_imports_finds_bare_and_semicolon_lines() {
        let source = "import \"support.mox\"\n\nactors S {\n    actor A\n}";
        assert_eq!(scan_imports(source), vec!["support.mox".to_string()]);
        assert_eq!(
            scan_imports("import \"a.mox\";\nimport \"b.mox\""),
            vec!["a.mox".to_string(), "b.mox".to_string()]
        );
        assert!(scan_imports("actors S { actor A }").is_empty());
    }
}
