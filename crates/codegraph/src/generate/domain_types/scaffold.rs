use crate::generate::ProjectConfig;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_type_contracts::RefClassificationKind;
use serde::Serialize;

use crate::error::Result;
use crate::generate::api::api_model::resolve_entity_operations;
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
struct DomainModContext {
    domain: String,
    entities: Vec<DomainModEntity>,
}

#[derive(Debug, Serialize)]
struct DomainModEntity {
    module_name: String,
}

#[derive(Debug, Serialize)]
struct EntityModContext {
    entity_name: String,
    has_create: bool,
    has_update: bool,
}

/// Generates scaffold files (`lib.rs`, per-domain `mod.rs`, per-entity `mod.rs`)
/// for the domain-types crate.
pub struct DomainTypesScaffoldGenerator {
    /// Base directory for domain-types/src output.
    ///
    /// In production this is `{workspace_root}/crates/domain-types/src`.
    /// In tests this should be a temp directory to avoid corrupting the real
    /// workspace source files.
    src_dir: PathBuf,
    /// When true, write domain module declarations to `domain_modules.rs`
    /// instead of `lib.rs`. This is used when domain types are colocated
    /// with the application crate, where `ScaffoldGenerator` already
    /// generates `lib.rs` — the two generators would otherwise race.
    colocated: bool,
}

impl DomainTypesScaffoldGenerator {
    /// Creates a generator that writes output under `base_dir` (crate root), appending `src/` internally.
    /// Pass a `tempfile::tempdir()` path to avoid corrupting the real source when using a mock graph.
    pub fn new_with_base(base_dir: PathBuf) -> Self {
        Self {
            src_dir: base_dir.join("src"),
            colocated: false,
        }
    }

    /// Same as `new_with_base`, but with domain types colocated in the app crate.
    /// Writes `domain_modules.rs` instead of `lib.rs` so the app scaffold can
    /// `include!` it without a race.
    pub fn new_colocated(base_dir: PathBuf) -> Self {
        Self {
            src_dir: base_dir.join("src"),
            colocated: true,
        }
    }
}

#[async_trait]
impl GlobalGenerator for DomainTypesScaffoldGenerator {
    fn name(&self) -> &str {
        "domain_types_scaffold"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        // Group generation_order entries by domain, deduplicating by (domain, module_name).
        let mut domain_entity_map: std::collections::HashMap<String, Vec<(String, String)>> =
            std::collections::HashMap::new();
        let mut seen = std::collections::HashSet::new();
        let mut domain_order = Vec::new();
        let mut seen_domains = std::collections::HashSet::new();

        for entry in generation_order {
            let stripped = config.defaults.strip_suffix(&entry.schema_title);
            let module_name = codegraph_naming::to_snake_case(&stripped);

            if seen_domains.insert(entry.domain.clone()) {
                domain_order.push(entry.domain.clone());
            }

            // Dedup by (domain, module_name)
            if !seen.insert((entry.domain.clone(), module_name.clone())) {
                continue;
            }

            domain_entity_map
                .entry(entry.domain.clone())
                .or_default()
                .push((stripped, module_name));
        }

        let src_dir = &self.src_dir;
        let mut files = Vec::new();

        // Clean stale entity directories that no longer appear in the
        // generation order.  Previous pipeline runs may have written
        // directories for entities that are now classified differently
        // (e.g. demoted to value objects).  If left on disk they cause
        // the domain mod.rs to be out of sync with the actual entity set.
        {
            let mut valid_modules_by_domain: HashMap<String, std::collections::HashSet<String>> =
                HashMap::new();
            for entry in generation_order {
                let stripped = config.defaults.strip_suffix(&entry.schema_title);
                let module_name = codegraph_naming::to_snake_case(&stripped);
                valid_modules_by_domain
                    .entry(entry.domain.clone())
                    .or_default()
                    .insert(module_name);
            }

            for domain_name in &domain_order {
                let domain_dir = src_dir.join(domain_name);
                if !domain_dir.is_dir() {
                    continue;
                }
                let valid_for_domain = valid_modules_by_domain.get(domain_name);
                if let Ok(entries) = std::fs::read_dir(&domain_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if !path.is_dir() {
                            continue;
                        }
                        let name = entry.file_name().to_string_lossy().to_string();
                        let is_valid = valid_for_domain.map(|s| s.contains(&name)).unwrap_or(false);
                        if !is_valid {
                            tracing::debug!(
                                domain = %domain_name,
                                module = %name,
                                path = %path.display(),
                                "removing stale entity directory"
                            );
                            if let Err(e) = std::fs::remove_dir_all(&path) {
                                tracing::warn!(
                                    path = %path.display(),
                                    "failed to remove stale entity directory: {e}"
                                );
                            }
                        }
                    }
                }
            }
        }

        // 1. Generate per-domain mod.rs
        for domain_name in &domain_order {
            let entities = match domain_entity_map.get(domain_name) {
                Some(e) => e,
                None => continue,
            };

            let domain_mod_ctx = DomainModContext {
                domain: domain_name.clone(),
                entities: entities
                    .iter()
                    .map(|(_name, module)| DomainModEntity {
                        module_name: module.clone(),
                    })
                    .collect(),
            };

            let content = render_template_with_project(
                tera,
                "domain_types/domain_mod.tera",
                &domain_mod_ctx,
                project,
            )?;
            files.push(GeneratedFile {
                path: src_dir.join(domain_name).join("mod.rs"),
                content,
            });

            // 2. Generate per-entity mod.rs
            for (entity_name, module_name) in entities {
                let operations =
                    resolve_entity_operations(db, config, domain_name, entity_name).await;

                let entity_mod_ctx = EntityModContext {
                    entity_name: entity_name.clone(),
                    has_create: operations.contains(&"create".to_string()),
                    has_update: operations.contains(&"update".to_string()),
                };

                let content = render_template_with_project(
                    tera,
                    "domain_types/entity_mod.tera",
                    &entity_mod_ctx,
                    project,
                )?;
                files.push(GeneratedFile {
                    path: src_dir.join(domain_name).join(module_name).join("mod.rs"),
                    content,
                });
            }
        }

        // 3. Generate lib.rs with hand-written modules preserved and domain modules appended.
        // Suppress lints inherent to code-generated domain types:
        // - module_inception: HR Open entity names sometimes match their domain (e.g. screening::screening)
        // - unused_imports: Update DTOs import all Create* types for completeness

        // Collect structured wrapper types used by generated entities so that
        // lib.rs provides a local re-export (e.g. `pub use codegraph_type_contracts::IdentifierType;`).
        // This allows generated DTOs to write `use crate::IdentifierType;` when
        // `types_import_prefix = "crate"` in domains.toml, avoiding direct
        // dependency on the codegraph crate name.
        let mut structured_types: HashSet<String> = HashSet::new();
        for entry in generation_order {
            if let Ok(props) = db.get_properties(&entry.schema_title).await {
                for prop in &props {
                    if prop.effective_kind() == Some(RefClassificationKind::StructuredWrapper) {
                        let mut ty = prop.rust_field_type.as_str();
                        if let Some(s) = ty.strip_prefix("Vec<").and_then(|s| s.strip_suffix('>')) {
                            ty = s;
                        }
                        if let Some(s) =
                            ty.strip_prefix("Option<").and_then(|s| s.strip_suffix('>'))
                        {
                            ty = s;
                        }
                        if !ty.is_empty() && ty != "serde_json::Value" {
                            structured_types.insert(ty.to_string());
                        }
                    }
                }
            }
        }

        let mut sorted_domains = domain_order.clone();
        sorted_domains.sort();
        let domain_mods: String = sorted_domains
            .iter()
            .map(|d| format!("pub mod {};\n", d))
            .collect();

        let mut structured_re_exports = String::new();
        let mut sorted_types: Vec<&String> = structured_types.iter().collect();
        sorted_types.sort();
        let prefix = &project.types_import_prefix;
        for ty in &sorted_types {
            structured_re_exports.push_str(&format!("pub use {}::{};\n", prefix, ty));
        }
        if !structured_re_exports.is_empty() {
            structured_re_exports = format!(
                "\n// --- STRUCTURED WRAPPER RE-EXPORTS ---\n{}",
                structured_re_exports
            );
        }

        let lib_file_name = if self.colocated {
            "domain_modules.rs"
        } else {
            "lib.rs"
        };
        let lib_header = if self.colocated {
            "// Generated domain modules — do not edit.\n"
        } else {
            "// Generated crate — do not edit.\n"
        };
        let domain_mods_content = format!(
            "{lib_header}\
             #![allow(clippy::module_inception, unused_imports, ambiguous_glob_reexports)]\n\
             \n\
             pub mod codelist;\n\
             pub mod context;\n\
             pub mod query;\n\
             \n\
             pub use context::{{SourceContext, SourceOrigin}};\n\
             pub use query::{{ListParams, PagedResult, QueryError, SortOrder}};\n\
             pub use serde_json;\
             {structured_re_exports}\n\
             \n\
             // --- GENERATED DOMAIN MODULES ---\n\
             {domain_mods}"
        );
        files.push(GeneratedFile {
            path: src_dir.join(lib_file_name),
            content: domain_mods_content,
        });

        // 4. Generate context.rs with SourceContext types.
        let context_rs = r#"//! Source context for tracking the origin of mutations.
//! Generated by codegraph. DO NOT EDIT.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceContext {
    pub origin: SourceOrigin,
    pub correlation_id: Option<uuid::Uuid>,
    pub api_key_id: Option<uuid::Uuid>,
    pub organization_id: uuid::Uuid,
    pub user_id: Option<uuid::Uuid>,
}

impl SourceContext {
    pub fn api() -> Self {
        Self {
            origin: SourceOrigin::Api,
            correlation_id: None,
            api_key_id: None,
            organization_id: uuid::Uuid::nil(),
            user_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceOrigin {
    Api,
    Internal,
    Worker,
    Migration,
}

impl Default for SourceOrigin {
    fn default() -> Self {
        Self::Api
    }
}
"#;
        files.push(GeneratedFile {
            path: src_dir.join("context.rs"),
            content: context_rs.to_string(),
        });

        // 5. Generate query.rs with shared query types.
        let query_rs = r#"//! Shared query types for list/read operations.
//! Generated by codegraph. DO NOT EDIT.
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ListParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    #[serde(default)]
    pub sort: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PagedResult<T> {
    pub data: Vec<T>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone)]
pub enum QueryError {
    InvalidSort(String),
    Internal(String),
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSort(m) => write!(f, "Invalid sort: {}", m),
            Self::Internal(m) => write!(f, "Internal: {}", m),
        }
    }
}

impl std::error::Error for QueryError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    Asc,
    Desc,
}

impl Default for SortOrder {
    fn default() -> Self {
        Self::Asc
    }
}
"#;
        files.push(GeneratedFile {
            path: src_dir.join("query.rs"),
            content: query_rs.to_string(),
        });

        // 6. Generate Cargo.toml at the crate root (parent of src/)
        let cargo_toml = render_template_with_project(
            tera,
            "domain_types/cargo_toml.tera",
            &serde_json::json!({}),
            project,
        )?;
        files.push(GeneratedFile {
            path: src_dir.parent().unwrap().join("Cargo.toml"),
            content: cargo_toml,
        });

        Ok(files)
    }
}
