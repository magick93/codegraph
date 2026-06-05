use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::traits::{DomainGenerator, GeneratedFile};
use crate::generate::ProjectConfig;

static LINKS_MODULE: &str = include_str!("../../../templates/api/links.tera");

pub struct LinksGenerator {
    output_dir: PathBuf,
}

impl LinksGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl DomainGenerator for LinksGenerator {
    fn name(&self) -> &str {
        "links"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _domain: &str,
        _entity_titles: &[String],
        _config: &DomainConfig,
        _tera: &tera::Tera,
        _project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        Ok(vec![GeneratedFile {
            path: self.output_dir.join("src").join("api").join("links.rs"),
            content: LINKS_MODULE.to_string(),
        }])
    }
}
