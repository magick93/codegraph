use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// An actor declared in a rexlang `.actor` block: a human or agent principal
/// that capabilities are granted to. `extends` names the parent actor and
/// forms a single-parent inheritance chain resolved by
/// [`resolve_effective_permits`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActorNode {
    pub name: String,
    pub kind: Option<String>,
    pub extends: Option<String>,
    pub block: Option<String>,
}

/// A capability bound to a domain class. `class` is the domain class name the
/// capability guards (e.g. an entity title); it is persisted as a plain string
/// so the policy can be ingested before/without the schema graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityNode {
    pub name: String,
    pub class: String,
    pub block: Option<String>,
}

/// A grant edge: `actor` permits or forbids `capability`, optionally gated by
/// a `when` expression, with obligations that must be discharged when the
/// grant fires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrantEdge {
    pub actor: String,
    pub capability: String,
    pub effect: String,
    pub when: Option<String>,
    pub obligations: Vec<String>,
}

/// A pair of (or group of) capabilities that must never be held together.
/// Validation-time metadata only: no generator consumes it yet, so it is
/// persisted as JSON props on the model-level `ActorPolicy` carrier node
/// rather than as per-capability edges.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NeverBothGroup {
    pub capabilities: Vec<String>,
}

/// Model-level policy metadata carrier (a singleton `ActorPolicy` node):
/// the `.actor` blocks the model was assembled from plus all never_both
/// groups.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActorPolicyNode {
    pub blocks: Vec<String>,
    pub never_both: Vec<NeverBothGroup>,
}

/// The full actor policy model handed to `GraphIngestor::ingest_actor_policy`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActorPolicyModel {
    pub actors: Vec<ActorNode>,
    pub capabilities: Vec<CapabilityNode>,
    pub grants: Vec<GrantEdge>,
    pub policy: ActorPolicyNode,
}

/// One resolved grant decision for an actor after inheritance resolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Permit {
    pub capability: String,
    pub effect: String,
    pub when: Option<String>,
    pub obligations: Vec<String>,
}

/// Resolve the effective grants for `actor` across its `extends` chain.
///
/// Semantics:
/// - The chain walks from the actor through single-parent `extends` links to
///   the root; unknown parent names and cycles terminate the walk.
/// - Grants declared by any actor in the chain are unioned (no override:
///   child and parent grants both survive, exact duplicates collapse).
/// - Forbid wins: if any grant in the chain forbids a capability, all permit
///   grants for that capability are dropped and only the forbid entries are
///   returned.
/// - The result is sorted by (capability, effect) for determinism.
pub fn resolve_effective_permits(
    actors: &[ActorNode],
    grants: &[GrantEdge],
    actor: &str,
) -> Vec<Permit> {
    let mut chain: Vec<String> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut current = Some(actor.to_string());
    while let Some(name) = current {
        if !visited.insert(name.clone()) {
            break;
        }
        current = actors
            .iter()
            .find(|a| a.name == name)
            .and_then(|a| a.extends.clone());
        chain.push(name);
    }

    let mut collected: Vec<GrantEdge> = grants
        .iter()
        .filter(|g| chain.contains(&g.actor))
        .cloned()
        .collect();

    let forbidden: HashSet<String> = collected
        .iter()
        .filter(|g| g.effect == "forbid")
        .map(|g| g.capability.clone())
        .collect();
    collected.retain(|g| g.effect != "permit" || !forbidden.contains(&g.capability));

    let mut seen: HashSet<(String, String, Option<String>, Vec<String>)> = HashSet::new();
    let mut permits: Vec<Permit> = Vec::new();
    for g in collected {
        if seen.insert((
            g.capability.clone(),
            g.effect.clone(),
            g.when.clone(),
            g.obligations.clone(),
        )) {
            permits.push(Permit {
                capability: g.capability,
                effect: g.effect,
                when: g.when,
                obligations: g.obligations,
            });
        }
    }
    permits.sort_by(|a, b| {
        a.capability
            .cmp(&b.capability)
            .then_with(|| a.effect.cmp(&b.effect))
    });
    permits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(name: &str, extends: Option<&str>) -> ActorNode {
        ActorNode {
            name: name.to_string(),
            kind: Some("human".to_string()),
            extends: extends.map(|e| e.to_string()),
            block: Some("core".to_string()),
        }
    }

    fn grant(actor: &str, capability: &str, effect: &str) -> GrantEdge {
        GrantEdge {
            actor: actor.to_string(),
            capability: capability.to_string(),
            effect: effect.to_string(),
            when: None,
            obligations: vec![],
        }
    }

    #[test]
    fn inherited_permit_is_unioned() {
        let actors = vec![actor("Admin", None), actor("Manager", Some("Admin"))];
        let grants = vec![grant("Admin", "approve_expense", "permit")];
        let permits = resolve_effective_permits(&actors, &grants, "Manager");
        assert_eq!(permits.len(), 1);
        assert_eq!(permits[0].capability, "approve_expense");
        assert_eq!(permits[0].effect, "permit");
    }

    #[test]
    fn forbid_wins_over_inherited_permit() {
        let actors = vec![actor("Admin", None), actor("Manager", Some("Admin"))];
        let grants = vec![
            grant("Admin", "approve_expense", "permit"),
            grant("Manager", "approve_expense", "forbid"),
            grant("Admin", "view_report", "permit"),
        ];
        let permits = resolve_effective_permits(&actors, &grants, "Manager");
        let approve: Vec<_> = permits
            .iter()
            .filter(|p| p.capability == "approve_expense")
            .collect();
        assert_eq!(approve.len(), 1);
        assert_eq!(approve[0].effect, "forbid");
        assert!(permits.iter().any(|p| p.capability == "view_report"));
    }

    #[test]
    fn cycle_terminates_and_grants_resolve() {
        let actors = vec![actor("A", Some("B")), actor("B", Some("A"))];
        let grants = vec![grant("B", "view_report", "permit")];
        let permits = resolve_effective_permits(&actors, &grants, "A");
        assert_eq!(permits.len(), 1);
        assert_eq!(permits[0].capability, "view_report");
    }

    #[test]
    fn unknown_parent_chain_stops() {
        let actors = vec![actor("Manager", Some("Ghost"))];
        let grants = vec![grant("Manager", "approve_expense", "permit")];
        let permits = resolve_effective_permits(&actors, &grants, "Manager");
        assert_eq!(permits.len(), 1);
    }

    #[test]
    fn duplicate_grants_collapse_and_when_round_trips() {
        let actors = vec![actor("Admin", None)];
        let mut g = grant("Admin", "view_report", "permit");
        g.when = Some("admin.verified == true".to_string());
        g.obligations = vec!["log_access".to_string()];
        let grants = vec![g.clone(), g];
        let permits = resolve_effective_permits(&actors, &grants, "Admin");
        assert_eq!(permits.len(), 1);
        assert_eq!(permits[0].when.as_deref(), Some("admin.verified == true"));
        assert_eq!(permits[0].obligations, vec!["log_access".to_string()]);
    }

    #[test]
    fn unknown_actor_has_no_grants() {
        let actors = vec![actor("Admin", None)];
        let grants = vec![grant("Admin", "approve_expense", "permit")];
        let permits = resolve_effective_permits(&actors, &grants, "Nobody");
        assert!(permits.is_empty());
    }
}
