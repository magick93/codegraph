use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::{GenerationEntry, ProjectConfig};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
pub struct ScaffoldContext {
    pub authority: String,
    pub lexicons: Vec<LexiconEntry>,
}

#[derive(Debug, Serialize)]
pub struct LexiconEntry {
    pub nsid: String,
    pub lex_type: String,
    pub description: String,
}

pub struct LexiconScaffoldEmitter {
    output_dir: PathBuf,
}

impl LexiconScaffoldEmitter {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for LexiconScaffoldEmitter {
    fn name(&self) -> &str {
        "lexicon_scaffold"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let authority = &project.atproto_authority;
        if authority.is_empty() {
            return Ok(Vec::new());
        }

        let lexicons = db
            .get_lexicons("")
            .await
            .unwrap_or_default();

        let context = ScaffoldContext {
            authority: authority.clone(),
            lexicons: lexicons
                .iter()
                .map(|l| LexiconEntry {
                    nsid: l.nsid.clone(),
                    lex_type: l.lex_type.clone(),
                    description: l.description.clone().unwrap_or_default(),
                })
                .collect(),
        };

        let content = render_template_with_project(tera, "atproto/scaffold.tera", &context, project)?;

        let path = self
            .output_dir
            .join("lexicons")
            .join(authority)
            .join("_meta.json");

        Ok(vec![GeneratedFile { path, content }])
    }
}
