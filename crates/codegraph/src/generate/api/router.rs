use crate::generate::ProjectConfig;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::ParentCandidate;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{DomainGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

#[derive(Debug, Clone, Serialize)]
pub struct ParentInfo {
    pub entity_name: String,
    pub module_name: String,
    pub path_segment: String,
    pub fk_column: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChildInfo {
    pub entity_name: String,
    pub module_name: String,
    pub path_segment: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CrossRefInfo {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub path_segment: String,
    pub fk_column: String,
    pub link_rel: String,
}

#[derive(Debug, Serialize)]
pub struct RouterContext {
    pub domain: String,
    pub entities: Vec<RouterEntity>,
}

#[derive(Debug, Serialize)]
pub struct RouterEntity {
    pub entity_name: String,
    pub module_name: String,
    pub path_segment: String,
    pub has_create: bool,
    pub has_update: bool,
    pub has_delete: bool,
    pub has_workflow: bool,
    pub has_approval_status: bool,
    pub has_embeddings: bool,
    pub role: String,
    /// Named path parameter for this entity's ID (e.g. `"worker_id"`).
    pub param_name: String,
    pub parent: Option<ParentInfo>,
    pub children: Vec<ChildInfo>,
    pub cross_refs: Vec<CrossRefInfo>,
    pub media_fields: Vec<String>,
    /// When set, this entity supports the /tree endpoint (self-referencing hierarchy).
    pub hierarchy_field: Option<String>,
}

pub struct RouterGenerator {
    output_dir: PathBuf,
    parent_candidates: Vec<ParentCandidate>,
}

/// Derive a named path parameter from a URL path segment.
///
/// Singularises the segment and appends `_id`:
/// - `"workers"` → `"worker_id"`
/// - `"military-service"` → `"military_service_id"`
/// - `"employment-permits"` → `"employment_permit_id"`
/// - `"education"` → `"education_id"` (already singular)
pub fn param_name_from_path_segment(segment: &str) -> String {
    let s = segment.replace('-', "_");
    let singular = if s.ends_with("ies") && s.len() > 3 {
        format!("{}y", &s[..s.len() - 3])
    } else if s.ends_with("sses") {
        // addresses → address (strip "es")
        s[..s.len() - 2].to_string()
    } else if s.ends_with("ses") && s.len() > 3 {
        // statuses → status, processes → process (strip "es")
        s[..s.len() - 2].to_string()
    } else if s.ends_with('s') && !s.ends_with("ss") {
        s[..s.len() - 1].to_string()
    } else {
        s
    };
    format!("{singular}_id")
}

/// Strip a configurable suffix from schema titles (e.g. "PersonType" → "Person").
pub fn strip_suffix<'a>(title: &'a str, suffix: &str) -> &'a str {
    title.strip_suffix(suffix).unwrap_or(title)
}

/// Strip the "Type" suffix from schema titles (e.g. "PersonType" → "Person").
///
/// Convenience wrapper — prefer `strip_suffix(title, &config.defaults.type_suffix)` in generators.
pub fn strip_type_suffix(title: &str) -> &str {
    strip_suffix(title, "Type")
}

/// Calculate the nesting depth of an entity given its raw schema title (with "Type" suffix).
pub(crate) fn calculate_nesting_depth(
    entity_name: &str,
    parent_candidates: &[ParentCandidate],
) -> usize {
    let parent_map: HashMap<&str, &str> = parent_candidates
        .iter()
        .map(|c| (c.child_title.as_str(), c.parent_title.as_str()))
        .collect();
    let mut depth = 1;
    let mut current = entity_name;
    let mut visited = std::collections::HashSet::new();
    visited.insert(current);
    while let Some(&parent) = parent_map.get(current) {
        if !visited.insert(parent) {
            break; // cycle detected — stop counting
        }
        depth += 1;
        current = parent;
    }
    depth
}

impl RouterGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            parent_candidates: Vec::new(),
        }
    }

    pub fn with_parent_candidates(mut self, candidates: Vec<ParentCandidate>) -> Self {
        self.parent_candidates = candidates;
        self
    }
}

#[async_trait]
impl DomainGenerator for RouterGenerator {
    fn name(&self) -> &str {
        "router"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        domain: &str,
        entity_titles: &[String],
        config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let mut entities = Vec::new();
        // Maps pg_table_name → entity index (for dedup)
        let mut module_to_idx: HashMap<String, usize> = HashMap::new();
        // Maps schema title (e.g. "LER-RSType") → entity index.
        // Used in the second pass to look up child/parent entities by title
        // rather than by strip_type_suffix(title), which breaks for hyphenated
        // titles like "LER-RSType" where rust_type_name == "LERRS" != "LER-RS".
        let mut title_to_entity_idx: HashMap<String, usize> = HashMap::new();

        for title in entity_titles {
            if let Ok(Some(schema)) = db.get_schema(title).await {
                if !schema.pg_table_name.is_empty() {
                    // Dedup by module name to prevent duplicate route functions
                    if let Some(&existing_idx) = module_to_idx.get(&schema.pg_table_name) {
                        title_to_entity_idx.insert(title.clone(), existing_idx);
                        continue;
                    }
                    let entity_name = &schema.rust_type_name;
                    let entity_cfg = config
                        .domains
                        .get(domain)
                        .and_then(|d| d.get_entity_config(title));
                    let operations = entity_cfg
                        .and_then(|ec| ec.operations.clone())
                        .unwrap_or_else(|| config.defaults.operations.clone());

                    let workflow = entity_cfg.and_then(|ec| ec.workflow.as_ref());
                    let has_workflow = workflow
                        .map(|wf| wf.generate_action_endpoints)
                        .unwrap_or(false);
                    let has_approval_status = workflow
                        .and_then(|wf| wf.approval_status_field.as_ref())
                        .is_some();

                    let has_embeddings = entity_cfg
                        .map(|ec| !ec.search.embedding_columns.is_empty())
                        .unwrap_or(false);

                    let media_fields: Vec<String> = db
                        .get_properties(title)
                        .await
                        .unwrap_or_default()
                        .iter()
                        .filter(|p| {
                            p.effective_kind()
                                == Some(codegraph_type_contracts::RefClassificationKind::MediaWrapper)
                        })
                        .map(|p| p.pg_column_name.clone())
                        .collect();

                    let entity_idx = entities.len();
                    module_to_idx.insert(schema.pg_table_name.clone(), entity_idx);
                    title_to_entity_idx.insert(title.clone(), entity_idx);

                    entities.push(RouterEntity {
                        entity_name: entity_name.clone(),
                        module_name: schema.pg_table_name.clone(),
                        path_segment: schema.api_path_segment.clone(),
                        has_create: operations.contains(&"create".to_string()),
                        has_update: operations.contains(&"update".to_string()),
                        has_delete: operations.contains(&"delete".to_string()),
                        has_workflow,
                        has_approval_status,
                        has_embeddings,
                        role: entity_cfg
                            .and_then(|ec| ec.role.clone())
                            .unwrap_or_else(|| "root".into()),
                        param_name: param_name_from_path_segment(&schema.api_path_segment),
                        parent: None,
                        children: vec![],
                        cross_refs: vec![],
                        media_fields,
                        hierarchy_field: entity_cfg.and_then(|ec| ec.hierarchy_field.clone()),
                    });
                }
            }
        }

        // Second pass: populate parent/child relationships.
        // Manual config takes priority over graph detection.
        {
            // Source 1: manual config from domains.toml (role/parent/parent_ref)
            for title in entity_titles {
                if let Some(ec) = config
                    .domains
                    .get(domain)
                    .and_then(|d| d.get_entity_config(title))
                {
                    if ec.role.as_deref() == Some("child") {
                        if let Some(parent_title) = &ec.parent {
                            // Use title_to_entity_idx so hyphenated schema titles
                            // like "LER-RSType" (rust_type_name "LERRS") resolve
                            // correctly instead of failing via strip_type_suffix.
                            if let (Some(&ci), Some(&pi)) = (
                                title_to_entity_idx.get(title.as_str()),
                                title_to_entity_idx.get(parent_title.as_str()),
                            ) {
                                if entities[ci].parent.is_none() {
                                    let parent_name = strip_suffix(parent_title, &config.defaults.type_suffix);
                                    let fk_column = ec.parent_ref.clone().unwrap_or_else(|| {
                                        format!("{}_id", codegraph_naming::to_snake_case(parent_name))
                                    });
                                    let parent_module = entities[pi].module_name.clone();
                                    let parent_path = entities[pi].path_segment.clone();
                                    let parent_entity = entities[pi].entity_name.clone();

                                    entities[ci].role = "child".to_string();
                                    entities[ci].parent = Some(ParentInfo {
                                        entity_name: parent_entity,
                                        module_name: parent_module,
                                        path_segment: parent_path,
                                        fk_column,
                                    });

                                    let child_entity = entities[ci].entity_name.clone();
                                    let child_module = entities[ci].module_name.clone();
                                    let child_path = entities[ci].path_segment.clone();
                                    entities[pi].children.push(ChildInfo {
                                        entity_name: child_entity,
                                        module_name: child_module,
                                        path_segment: child_path,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // Source 2: graph-detected parent_candidates (only for entities
            // not already assigned by manual config above)
            for pc in &self.parent_candidates {
                let child_idx = title_to_entity_idx.get(pc.child_title.as_str()).copied();
                let parent_idx = title_to_entity_idx.get(pc.parent_title.as_str()).copied();

                if let (Some(ci), Some(pi)) = (child_idx, parent_idx) {
                    if entities[ci].parent.is_none() {
                        let fk_column = crate::generate::fk_column_for_candidate(pc, &config.defaults.type_suffix);
                        let parent_module = entities[pi].module_name.clone();
                        let parent_path = entities[pi].path_segment.clone();
                        let parent_entity = entities[pi].entity_name.clone();

                        entities[ci].role = "child".to_string();
                        entities[ci].parent = Some(ParentInfo {
                            entity_name: parent_entity,
                            module_name: parent_module,
                            path_segment: parent_path,
                            fk_column,
                        });

                        let child_entity = entities[ci].entity_name.clone();
                        let child_module = entities[ci].module_name.clone();
                        let child_path = entities[ci].path_segment.clone();
                        entities[pi].children.push(ChildInfo {
                            entity_name: child_entity,
                            module_name: child_module,
                            path_segment: child_path,
                        });
                    }
                }
            }
        }

        // Warn on entities with nesting depth > 3 (long URLs).
        for entity in &entities {
            if entity.role == "child" {
                let title = format!("{}Type", entity.entity_name);
                let depth = calculate_nesting_depth(&title, &self.parent_candidates);
                if depth > 3 {
                    tracing::warn!(
                        "Entity '{}' nesting depth is {} (max 3). URL will be long: consider restructuring.",
                        entity.entity_name, depth
                    );
                }
            }
        }

        // Third pass: detect cross-aggregate entity references.
        {
            let index: HashMap<String, usize> = entities
                .iter()
                .enumerate()
                .map(|(i, e)| (e.entity_name.clone(), i))
                .collect();

            let mut cross_refs_by_idx: HashMap<usize, Vec<CrossRefInfo>> = HashMap::new();

            for (i, entity) in entities.iter().enumerate() {
                let schema_title = format!("{}Type", entity.entity_name);
                let referenced = match db.get_referenced_schemas(&schema_title).await {
                    Ok(refs) => refs,
                    Err(_) => continue,
                };

                let parent_name = entity
                    .parent
                    .as_ref()
                    .map(|p| p.entity_name.as_str())
                    .unwrap_or("");
                let child_names: std::collections::HashSet<&str> = entity
                    .children
                    .iter()
                    .map(|c| c.entity_name.as_str())
                    .collect();

                for ref_title in &referenced {
                    let ref_entity_name = strip_suffix(ref_title, &config.defaults.type_suffix);

                    // Skip self
                    if ref_entity_name == entity.entity_name {
                        continue;
                    }
                    // Skip parent
                    if ref_entity_name == parent_name {
                        continue;
                    }
                    // Skip children
                    if child_names.contains(ref_entity_name) {
                        continue;
                    }
                    // Only link to entities present in this domain's entity list
                    let Some(&ref_idx) = index.get(ref_entity_name) else {
                        continue;
                    };

                    let ref_entity = &entities[ref_idx];
                    let fk_col = codegraph_naming::to_snake_case(ref_entity_name) + "_id";
                    let link_rel = codegraph_naming::to_snake_case(ref_entity_name);
                    cross_refs_by_idx.entry(i).or_default().push(CrossRefInfo {
                        entity_name: ref_entity.entity_name.clone(),
                        module_name: ref_entity.module_name.clone(),
                        domain: domain.to_string(),
                        path_segment: ref_entity.path_segment.clone(),
                        fk_column: fk_col,
                        link_rel,
                    });
                }
            }

            for (i, refs) in cross_refs_by_idx {
                entities[i].cross_refs = refs;
            }
        }

        let ctx = RouterContext {
            domain: domain.to_string(),
            entities,
        };

        let content = render_template_with_project(tera, "api/router.tera", &ctx, project)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("api")
                .join(domain)
                .join("router.rs"),
            content,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_core::types::{DetectionSource, ParentCandidate};

    #[test]
    fn test_nesting_depth_calculation() {
        let candidates = vec![
            ParentCandidate {
                child_title: "BType".into(),
                parent_title: "AType".into(),
                field_name: "a_id".into(),
                source: DetectionSource::ScalarRef,
            },
            ParentCandidate {
                child_title: "CType".into(),
                parent_title: "BType".into(),
                field_name: "b_id".into(),
                source: DetectionSource::ScalarRef,
            },
            ParentCandidate {
                child_title: "DType".into(),
                parent_title: "CType".into(),
                field_name: "c_id".into(),
                source: DetectionSource::ScalarRef,
            },
        ];
        assert_eq!(calculate_nesting_depth("AType", &candidates), 1); // root
        assert_eq!(calculate_nesting_depth("BType", &candidates), 2); // child
        assert_eq!(calculate_nesting_depth("CType", &candidates), 3); // grandchild
        assert_eq!(calculate_nesting_depth("DType", &candidates), 4); // too deep
    }

    #[test]
    fn test_strip_suffix() {
        assert_eq!(strip_suffix("PersonType", "Type"), "Person");
        assert_eq!(strip_suffix("CompensationType", "Type"), "Compensation");
        assert_eq!(strip_suffix("Worker", "Type"), "Worker");
        assert_eq!(strip_suffix("Type", "Type"), "");
        assert_eq!(strip_suffix("", "Type"), "");
        assert_eq!(strip_suffix("PersonView", "View"), "Person");
        assert_eq!(strip_suffix("CustomEntitySuf", "Suf"), "CustomEntity");
    }

    #[allow(deprecated)]
    #[test]
    fn test_strip_type_suffix_backward_compat() {
        assert_eq!(strip_type_suffix("PersonType"), "Person");
        assert_eq!(strip_type_suffix("CompensationType"), "Compensation");
        assert_eq!(strip_type_suffix("Worker"), "Worker");
        assert_eq!(strip_type_suffix("Type"), "");
        assert_eq!(strip_type_suffix(""), "");
    }

    #[test]
    fn test_nesting_depth_root_entity() {
        let candidates = vec![];
        assert_eq!(calculate_nesting_depth("PersonType", &candidates), 1);
    }

    #[test]
    fn test_nesting_depth_no_cycle_on_unrelated() {
        let candidates = vec![ParentCandidate {
            child_title: "BType".into(),
            parent_title: "AType".into(),
            field_name: "a_id".into(),
            source: DetectionSource::ScalarRef,
        }];
        // Unrelated entity not in candidates
        assert_eq!(calculate_nesting_depth("XType", &candidates), 1);
    }

    #[test]
    fn test_parent_info_fields() {
        let info = ParentInfo {
            entity_name: "Compensation".into(),
            module_name: "compensation".into(),
            path_segment: "compensation".into(),
            fk_column: "compensation_type_id".into(),
        };
        assert_eq!(info.fk_column, "compensation_type_id");
    }

    #[test]
    fn test_child_info_fields() {
        let info = ChildInfo {
            entity_name: "Reward".into(),
            module_name: "reward".into(),
            path_segment: "reward".into(),
        };
        assert_eq!(info.path_segment, "reward");
    }

    #[test]
    fn test_cross_ref_info_fields() {
        let info = CrossRefInfo {
            entity_name: "Person".into(),
            domain: "common".into(),
            path_segment: "person".into(),
            fk_column: "person_id".into(),
            link_rel: "person".into(),
            module_name: "person_type".into(),
        };
        assert_eq!(info.link_rel, "person");
        assert_eq!(info.domain, "common");
    }

    #[test]
    fn test_nesting_depth_terminates_on_cycle() {
        let candidates = vec![
            ParentCandidate {
                child_title: "AType".into(),
                parent_title: "BType".into(),
                field_name: "b_id".into(),
                source: DetectionSource::ScalarRef,
            },
            ParentCandidate {
                child_title: "BType".into(),
                parent_title: "AType".into(),
                field_name: "a_id".into(),
                source: DetectionSource::ScalarRef,
            },
        ];
        // Must terminate (not infinite loop) and return a finite depth
        let depth = calculate_nesting_depth("AType", &candidates);
        assert!(
            depth <= 3,
            "Cycle should not produce depth > 3, got {depth}"
        );
    }

    #[test]
    fn param_name_simple_plural() {
        assert_eq!(param_name_from_path_segment("workers"), "worker_id");
    }

    #[test]
    fn param_name_kebab_case() {
        assert_eq!(
            param_name_from_path_segment("employment-permits"),
            "employment_permit_id"
        );
    }

    #[test]
    fn param_name_ies_suffix() {
        assert_eq!(param_name_from_path_segment("companies"), "company_id");
        assert_eq!(param_name_from_path_segment("policies"), "policy_id");
    }

    #[test]
    fn param_name_statuses() {
        assert_eq!(param_name_from_path_segment("statuses"), "status_id");
    }

    #[test]
    fn param_name_addresses() {
        assert_eq!(param_name_from_path_segment("addresses"), "address_id");
    }

    #[test]
    fn param_name_processes() {
        assert_eq!(param_name_from_path_segment("processes"), "process_id");
    }

    #[test]
    fn param_name_singular_passthrough() {
        assert_eq!(param_name_from_path_segment("person"), "person_id");
    }

    #[test]
    fn param_name_ss_suffix_kept() {
        assert_eq!(param_name_from_path_segment("access"), "access_id");
    }
}
