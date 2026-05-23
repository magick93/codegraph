use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use crate::config::DomainConfig;
use crate::error::DomainConfigError;

/// A bounded context in the HR domain model.
#[derive(Debug, Clone)]
pub struct DomainContext {
    /// Internal name, e.g. "payroll"
    pub name: String,
    /// Human-readable label, e.g. "Payroll"
    pub label: String,
    /// Folder name in HR Open specs, e.g. "payroll"
    pub schema_dir: String,
    /// Postgres schema namespace, e.g. "payroll"
    pub postgres_schema: String,
    /// Domain-level dependencies (other domain names)
    pub depends_on: Vec<String>,
    /// Schema types that are persistable entities (get tables).
    pub entities: HashSet<String>,
    /// JSON Schema files belonging to this domain (populated during assignment)
    pub schemas: Vec<PathBuf>,
}

/// The full domain registry — a DAG of bounded contexts.
#[derive(Debug)]
pub struct DomainRegistry {
    pub domains: HashMap<String, DomainContext>,
    /// Adjacency list: node index → set of outgoing neighbor indices
    out_edges: HashMap<usize, HashSet<usize>>,
    /// Adjacency list: node index → set of incoming neighbor indices
    in_edges: HashMap<usize, HashSet<usize>>,
    pub name_to_index: HashMap<String, usize>,
    pub index_to_name: HashMap<usize, String>,
}

impl DomainRegistry {
    /// Build a `DomainRegistry` from parsed TOML config.
    pub fn from_config(config: DomainConfig) -> Result<Self, DomainConfigError> {
        let mut domains = HashMap::new();
        let mut name_to_index: HashMap<String, usize> = HashMap::new();
        let mut index_to_name: HashMap<usize, String> = HashMap::new();
        let mut out_edges: HashMap<usize, HashSet<usize>> = HashMap::new();
        let mut in_edges: HashMap<usize, HashSet<usize>> = HashMap::new();

        let mut sorted_names: Vec<String> = config.domains.keys().cloned().collect();
        sorted_names.sort();

        for (idx, name) in sorted_names.iter().enumerate() {
            let entry = &config.domains[name];
            name_to_index.insert(name.clone(), idx);
            index_to_name.insert(idx, name.clone());
            out_edges.entry(idx).or_default();
            in_edges.entry(idx).or_default();

            domains.insert(
                name.clone(),
                DomainContext {
                    name: name.clone(),
                    label: entry.label.clone(),
                    schema_dir: entry.schema_dir.clone(),
                    postgres_schema: entry.postgres_schema.clone(),
                    depends_on: entry.depends_on.clone(),
                    entities: entry.entities.iter().cloned().collect(),
                    schemas: Vec::new(),
                },
            );
        }

        // Validate depends_on targets exist
        for (name, ctx) in &domains {
            for dep in &ctx.depends_on {
                if !domains.contains_key(dep) {
                    return Err(DomainConfigError::Invalid(format!(
                        "domain \"{}\" declares depends_on \"{}\" which does not exist",
                        name, dep
                    )));
                }
            }
        }

        // Add edges: dependent → dependency
        for (name, ctx) in &domains {
            let src_idx = name_to_index[name];
            for dep in &ctx.depends_on {
                let dst_idx = name_to_index[dep];
                out_edges.entry(src_idx).or_default().insert(dst_idx);
                in_edges.entry(dst_idx).or_default().insert(src_idx);
            }
        }

        Ok(DomainRegistry {
            domains,
            out_edges,
            in_edges,
            name_to_index,
            index_to_name,
        })
    }

    /// Check if an edge exists from src to dst.
    pub fn has_edge(&self, src: usize, dst: usize) -> bool {
        self.out_edges
            .get(&src)
            .map(|s| s.contains(&dst))
            .unwrap_or(false)
    }

    /// Get the topological order of domains (dependencies first).
    ///
    /// Uses Kahn's algorithm. Returns error if cycle detected.
    pub fn topological_order(&self) -> Result<Vec<String>, DomainConfigError> {
        let mut in_degree: HashMap<usize, usize> = HashMap::new();
        for (&node, incoming) in &self.in_edges {
            in_degree.insert(node, incoming.len());
        }

        let mut initial: Vec<usize> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&node, _)| node)
            .collect();
        initial.sort();
        let mut queue: VecDeque<usize> = initial.into_iter().collect();

        let mut sorted = Vec::new();
        while let Some(node) = queue.pop_front() {
            sorted.push(node);
            if let Some(neighbors) = self.out_edges.get(&node) {
                let mut ready = Vec::new();
                for &neighbor in neighbors {
                    if let Some(deg) = in_degree.get_mut(&neighbor) {
                        *deg -= 1;
                        if *deg == 0 {
                            ready.push(neighbor);
                        }
                    }
                }
                ready.sort();
                for r in ready {
                    queue.push_back(r);
                }
            }
        }

        if sorted.len() != self.name_to_index.len() {
            // Find a node involved in the cycle
            let cycle_node = in_degree
                .iter()
                .find(|(_, &deg)| deg > 0)
                .map(|(&node, _)| node)
                .unwrap_or(0);
            let name = self
                .index_to_name
                .get(&cycle_node)
                .cloned()
                .unwrap_or_else(|| format!("unknown({})", cycle_node));
            return Err(DomainConfigError::CyclicDependency(name));
        }

        // Reverse: toposort has A→B with A before B, but A depends on B,
        // so B (the dependency) should come first.
        sorted.reverse();
        Ok(sorted
            .into_iter()
            .map(|idx| self.index_to_name[&idx].clone())
            .collect())
    }

    /// Check if a schema type is classified as an entity in the given domain.
    pub fn is_entity(&self, domain: &str, type_name: &str) -> bool {
        self.domains
            .get(domain)
            .map(|ctx| ctx.entities.contains(type_name))
            .unwrap_or(false)
    }

    /// Check if a type name is an entity in *any* domain.
    pub fn is_entity_any_domain(&self, type_name: &str) -> bool {
        self.domains
            .values()
            .any(|ctx| ctx.entities.contains(type_name))
    }
}

#[cfg(test)]
mod tests {
    use crate::config::parse_domain_config_str;

    use super::*;

    fn make_registry(toml: &str) -> DomainRegistry {
        let config = parse_domain_config_str(toml).unwrap();
        DomainRegistry::from_config(config).unwrap()
    }

    #[test]
    fn test_construction() {
        let toml = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"

[domains.payroll]
label = "Payroll"
schema_dir = "payroll"
postgres_schema = "payroll"
depends_on = ["common"]

[domains.compensation]
label = "Compensation"
schema_dir = "compensation"
postgres_schema = "compensation"
depends_on = ["common"]
"#;
        let registry = make_registry(toml);
        assert_eq!(registry.domains.len(), 3);
        assert!(registry.has_edge(
            registry.name_to_index["payroll"],
            registry.name_to_index["common"]
        ));
    }

    #[test]
    fn test_topological_order() {
        let toml = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"

[domains.payroll]
label = "Payroll"
schema_dir = "payroll"
postgres_schema = "payroll"
depends_on = ["common"]
"#;
        let registry = make_registry(toml);
        let order = registry.topological_order().unwrap();
        let common_pos = order.iter().position(|n| n == "common").unwrap();
        let payroll_pos = order.iter().position(|n| n == "payroll").unwrap();
        assert!(common_pos < payroll_pos);
    }

    #[test]
    fn test_invalid_depends_on() {
        let toml = r#"
[domains.payroll]
label = "Payroll"
schema_dir = "payroll"
postgres_schema = "payroll"
depends_on = ["nonexistent"]
"#;
        let config = parse_domain_config_str(toml).unwrap();
        let result = DomainRegistry::from_config(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_entity_classification() {
        let toml = r#"
[domains.payroll]
label = "Payroll"
schema_dir = "payroll"
postgres_schema = "payroll"
entities = ["PayRunType", "PayrollInstructionType"]
"#;
        let registry = make_registry(toml);
        assert!(registry.is_entity("payroll", "PayRunType"));
        assert!(!registry.is_entity("payroll", "PayPeriodType"));
        assert!(registry.is_entity_any_domain("PayRunType"));
        assert!(!registry.is_entity_any_domain("PayPeriodType"));
    }
}
