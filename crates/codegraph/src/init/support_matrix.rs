//! The v1 support-matrix doctor check (issue #849 §3).
//!
//! `codegraph doctor` warns when the resolved plan selects a topology ×
//! feature combination the v1 support matrix does not claim. The run path
//! already gates each of these off (or only template-pins them), so a run
//! succeeds and the surface is simply absent or unverified; doctor makes
//! that visible BEFORE a consumer discovers it in a deployed app. Findings
//! are advisory (WARN, +1 soft warning each), never hard failures.
//!
//! Every rule keys on a fact doctor actually holds: the resolved
//! [`BuildPlan`] and the parsed `domains.toml`. Graph-dependent cells of the
//! matrix (the `/machine` read model, function-event consumer placement)
//! need the model graph and stay the run path's own warnings.

use codegraph_config::DomainConfig;

use crate::profile::{BuildPlan, DeploymentTopology};

/// One unsupported combination the plan selects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportFinding {
    /// The feature, as the support matrix names it.
    pub feature: &'static str,
    /// Why it is not supported under the selected topology.
    pub reason: String,
    /// How to resolve the finding.
    pub hint: &'static str,
}

/// Every unsupported topology × feature combination `plan` (with `config`,
/// when it parsed) selects, in a fixed order.
pub fn unsupported_combinations(
    plan: &BuildPlan,
    config: Option<&DomainConfig>,
) -> Vec<SupportFinding> {
    let mut findings = Vec::new();
    if plan.deployment_topology() != DeploymentTopology::Workers {
        return findings;
    }

    if let Some(config) = config {
        let media_domains: Vec<&str> = config
            .domains
            .iter()
            .filter(|(_, entry)| entry.media.is_some())
            .map(|(name, _)| name.as_str())
            .collect();
        if !media_domains.is_empty() {
            findings.push(SupportFinding {
                feature: "media",
                reason: format!(
                    "domain(s) {} declare [media], but the media route plane is \
                     monolith-only (#703) — no upload/display/remove routes are emitted",
                    media_domains.join(", ")
                ),
                hint: "use deployment_topology = \"monolith\" for media, or drop the [media] table",
            });
        }
    }

    let grpc: Vec<&str> = plan
        .entity_generators
        .iter()
        .chain(&plan.domain_generators)
        .chain(&plan.global_generators)
        .map(String::as_str)
        .filter(|name| name.starts_with("grpc"))
        .collect();
    if !grpc.is_empty() {
        findings.push(SupportFinding {
            feature: "gRPC",
            reason: format!(
                "the plan lists {}, but the gRPC server is a monolith build only — \
                 no worker crate compiles or serves it",
                grpc.join(", ")
            ),
            hint: "remove the grpc generators from the workers profile (or set grpc_backend = false)",
        });
    }

    if plan.has_domain_gen("rules") {
        findings.push(SupportFinding {
            feature: "design invariants / eligibility rules",
            reason: "the plan lists `rules`, but the rules plane (predicate fns and their \
                     handler call sites) is monolith-only (#338/#666) — modeled guards are \
                     not enforced under workers"
                .to_string(),
            hint: "enforce the rules elsewhere, or deploy the guarded domains as a monolith",
        });
    }

    if plan.events_producer {
        findings.push(SupportFinding {
            feature: "events producer",
            reason: "events_producer = true publishes from monolith action handlers; under \
                     workers the publishing leg is template-pinned only (no live gate, #842)"
                .to_string(),
            hint: "treat workers publishing as unverified until #842 lands",
        });
    }

    findings
}

/// Print each finding as a WARN line plus hint; returns the soft-warning
/// count.
pub fn report_unsupported_combinations(plan: &BuildPlan, config: Option<&DomainConfig>) -> usize {
    let findings = unsupported_combinations(plan, config);
    for finding in &findings {
        println!(
            "WARN support matrix — {} under workers: {}",
            finding.feature, finding.reason
        );
        println!("     hint: {}", finding.hint);
    }
    findings.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real resolved plan: `topology`, plus the extra generator names
    /// and feature lines the case needs.
    fn plan_with(topology: &str, extra_gens: &[&str], extra_features: &str) -> BuildPlan {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("profiles.toml");
        let gens: Vec<String> = ["ddl", "dto", "handler", "router"]
            .iter()
            .chain(extra_gens)
            .map(|g| format!("\"{g}\""))
            .collect();
        std::fs::write(
            &path,
            format!(
                r#"[profiles.default.meta]
name = "default"
version = "1.0.0"
app_name = "matrix-app"

[profiles.default.features]
persistence_provider = "cornucopia"
deployment_topology = "{topology}"
{extra_features}

[profiles.default.api]
generators = [{}]
output = "generated/"
"#,
                gens.join(", ")
            ),
        )
        .expect("write profile");
        let resolved =
            crate::profile::load_and_resolve_profile(&path, "default", None).expect("resolve");
        BuildPlan::from_profile(&resolved, &crate::profile::CapabilityRegistry::new())
            .expect("plan")
    }

    fn plan(topology: &str) -> BuildPlan {
        plan_with(topology, &[], "")
    }

    fn everything(topology: &str) -> BuildPlan {
        plan_with(
            topology,
            &["grpc_proto", "rules"],
            "grpc_backend = true\nrosetta_backend = true\nevents_producer = true",
        )
    }

    fn media_config() -> DomainConfig {
        toml::from_str(
            r#"
[domains.assets]
label = "Assets"
schema_dir = "assets"
postgres_schema = "assets"

[domains.assets.media]
provider = "r2"
upload_mode = "through-app"
bucket = "assets"
max_bytes = 1024
accept = ["image/png"]
"#,
        )
        .expect("config")
    }

    #[test]
    fn a_monolith_plan_is_always_quiet() {
        let p = everything("monolith");
        assert!(unsupported_combinations(&p, Some(&media_config())).is_empty());
    }

    #[test]
    fn a_plain_workers_plan_is_quiet() {
        let p = plan("workers");
        assert!(unsupported_combinations(&p, None).is_empty());
    }

    #[test]
    fn workers_flags_each_unsupported_feature_in_order() {
        let p = everything("workers");
        let features: Vec<&str> = unsupported_combinations(&p, Some(&media_config()))
            .iter()
            .map(|f| f.feature)
            .collect();
        assert_eq!(
            features,
            vec![
                "media",
                "gRPC",
                "design invariants / eligibility rules",
                "events producer"
            ]
        );
    }

    #[test]
    fn the_media_finding_names_the_domain() {
        let p = plan("workers");
        let findings = unsupported_combinations(&p, Some(&media_config()));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].reason.contains("assets"), "{:?}", findings[0]);
    }
}
