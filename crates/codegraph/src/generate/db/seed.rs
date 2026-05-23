use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

#[derive(Debug, Serialize)]
pub struct SeedContext {
    pub demo_org_name: String,
    pub demo_org_id: String,
    pub persons: Vec<SeedPerson>,
    pub api_keys: Vec<SeedApiKey>,
}

#[derive(Debug, Serialize)]
pub struct SeedPerson {
    pub id: String,
    pub given_name: String,
    pub family_name: String,
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct SeedApiKey {
    pub name: String,
    pub role: String,
    pub scopes_json: String,
}

#[derive(Debug, Deserialize)]
struct SeedConfigFile {
    defaults: SeedDefaults,
    demo_org: DemoOrgConfig,
}

#[derive(Debug, Deserialize)]
struct SeedDefaults {
    enabled: Option<bool>,
    demo_org_name: String,
}

#[derive(Debug, Deserialize)]
struct DemoOrgConfig {
    namespace: String,
    persons: DemoOrgPersonsConfig,
    api_keys: DemoOrgApiKeysConfig,
}

#[derive(Debug, Deserialize)]
struct DemoOrgPersonsConfig {
    names: Vec<SeedPersonNameConfig>,
}

#[derive(Debug, Deserialize)]
struct SeedPersonNameConfig {
    given: String,
    family: String,
    email: String,
}

#[derive(Debug, Deserialize)]
struct DemoOrgApiKeysConfig {
    keys: Vec<SeedApiKeyEntryConfig>,
}

#[derive(Debug, Deserialize)]
struct SeedApiKeyEntryConfig {
    name: String,
    role: String,
    scopes: Vec<SeedScopeConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SeedScopeConfig {
    entity_type: String,
    entity_id: String,
    action: String,
}

pub struct SeedDataGenerator {
    output_dir: PathBuf,
    seed_config_path: Option<PathBuf>,
}

impl SeedDataGenerator {
    pub fn new(output_dir: &Path, seed_config_path: Option<PathBuf>) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            seed_config_path,
        }
    }

    fn load_context(&self) -> SeedContext {
        if let Some(ref config_path) = self.seed_config_path {
            if config_path.exists() {
                match fs::read_to_string(config_path) {
                    Ok(content) => match toml::from_str::<SeedConfigFile>(&content) {
                        Ok(config) => return Self::convert_config_to_context(config),
                        Err(e) => {
                            tracing::warn!(
                                "Failed to parse seed config at {}: {}. \
                                 Falling back to hardcoded defaults.",
                                config_path.display(),
                                e
                            );
                        }
                    },
                    Err(e) => {
                        tracing::warn!(
                            "Failed to read seed config at {}: {}. \
                             Falling back to hardcoded defaults.",
                            config_path.display(),
                            e
                        );
                    }
                }
            }
        }

        Self::hardcoded_context()
    }

    fn convert_config_to_context(config: SeedConfigFile) -> SeedContext {
        let ns = match Uuid::parse_str(&config.demo_org.namespace) {
            Ok(n) => n,
            Err(_) => {
                tracing::warn!(
                    "Invalid UUID namespace '{}' in seed config; \
                     using hardcoded fallback",
                    config.demo_org.namespace
                );
                return Self::hardcoded_context();
            }
        };

        let demo_org_name = config.defaults.demo_org_name;
        let demo_org_id = Uuid::new_v5(&ns, format!("org:{demo_org_name}").as_bytes()).to_string();

        let persons: Vec<SeedPerson> = config
            .demo_org
            .persons
            .names
            .into_iter()
            .map(|name| {
                let id = Uuid::new_v5(&ns, format!("person:{}", &name.email).as_bytes()).to_string();
                SeedPerson {
                    id,
                    given_name: name.given,
                    family_name: name.family,
                    email: name.email,
                }
            })
            .collect();

        let api_keys: Vec<SeedApiKey> = config
            .demo_org
            .api_keys
            .keys
            .into_iter()
            .map(|key| {
                let scopes_json =
                    serde_json::to_string(&key.scopes).unwrap_or_else(|_| "[]".to_string());
                SeedApiKey {
                    name: key.name,
                    role: key.role,
                    scopes_json,
                }
            })
            .collect();

        SeedContext {
            demo_org_name,
            demo_org_id,
            persons,
            api_keys,
        }
    }

    fn hardcoded_context() -> SeedContext {
        SeedContext {
            demo_org_name: "Kiwi Consulting Ltd".to_string(),
            demo_org_id: "16a771f4-5e1d-5e4e-9504-2367702702db".to_string(),
            persons: vec![
                SeedPerson {
                    id: "4ff272cd-434c-5bc0-b9a6-383044367950".to_string(),
                    given_name: "Aroha".to_string(),
                    family_name: "Ngata".to_string(),
                    email: "aroha@kiwiconsulting.co.nz".to_string(),
                },
                SeedPerson {
                    id: "2017d5a7-498c-5431-9048-e9e869121b9d".to_string(),
                    given_name: "Tane".to_string(),
                    family_name: "Williams".to_string(),
                    email: "tane@kiwiconsulting.co.nz".to_string(),
                },
                SeedPerson {
                    id: "0027a7b8-2922-5bb9-aac5-d48feb6d4b1d".to_string(),
                    given_name: "Mere".to_string(),
                    family_name: "Johnson".to_string(),
                    email: "mere@kiwiconsulting.co.nz".to_string(),
                },
            ],
            api_keys: vec![
                SeedApiKey {
                    name: "Owner Key".to_string(),
                    role: "owner".to_string(),
                    scopes_json: r#"[{"entity_type":"*","entity_id":"*","action":"*"}]"#
                        .to_string(),
                },
                SeedApiKey {
                    name: "Manager Key".to_string(),
                    role: "manager".to_string(),
                    scopes_json: r#"[{"entity_type":"candidate","entity_id":"*","action":"*"},{"entity_type":"timecard","entity_id":"*","action":"*"},{"entity_type":"pay_run","entity_id":"*","action":"read"}]"#.to_string(),
                },
                SeedApiKey {
                    name: "Employee Key".to_string(),
                    role: "employee".to_string(),
                    scopes_json: r#"[{"entity_type":"*","entity_id":"*","action":"read"}]"#
                        .to_string(),
                },
            ],
        }
    }
}

#[async_trait]
impl GlobalGenerator for SeedDataGenerator {
    fn name(&self) -> &str {
        "seed_data"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
    ) -> Result<Vec<GeneratedFile>> {
        let ctx = self.load_context();

        let mut tera_ctx = tera::Context::new();
        tera_ctx.insert("seed", &ctx);

        let content = tera
            .render("db/seed.tera", &tera_ctx)
            .map_err(|e| crate::error::Error::Template(e.to_string()))?;

        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("migrations")
                .join("0900_seed_data.sql"),
            content,
        }])
    }
}
