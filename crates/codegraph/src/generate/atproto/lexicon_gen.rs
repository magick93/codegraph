use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use crate::generate::ProjectConfig;
use codegraph_config::DomainConfig;

use super::lexicon_context::LexiconContext;

pub struct LexiconEmitter {
    output_dir: PathBuf,
}

impl LexiconEmitter {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for LexiconEmitter {
    fn name(&self) -> &str {
        "lexicon"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        _config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let lexicon = match db.get_lexicon_by_schema(schema_title).await? {
            Some(l) => l,
            None => return Ok(Vec::new()),
        };

        let schema = match db.get_schema_in_domain(schema_title, domain).await? {
            Some(s) => s,
            None => return Ok(Vec::new()),
        };

        let properties = db
            .get_properties_in_domain(schema_title, domain)
            .await?;

        let context =
            LexiconContext::build(db, &lexicon, &schema, &properties, project).await?;

        let template = match lexicon.lex_type.as_str() {
            "record" => "atproto/lexicon_record.tera",
            "object" => "atproto/lexicon_object.tera",
            "string" => "atproto/lexicon_enum.tera",
            _ => return Ok(Vec::new()),
        };

        let content = render_template_with_project(tera, template, &context, project)?;

        let authority = project.atproto_authority.as_str();
        let nsid_parts: Vec<&str> = lexicon
            .nsid
            .strip_prefix(&format!("{}.", authority))
            .unwrap_or(&lexicon.nsid)
            .split('.')
            .collect();

        let file_stem = if nsid_parts.len() >= 2 {
            nsid_parts.join("/")
        } else {
            schema.title.to_lowercase()
        };

        let path = self
            .output_dir
            .join("lexicons")
            .join(authority)
            .join(format!("{}.json", file_stem));

        Ok(vec![GeneratedFile {
            path,
            content,
        }])
    }
}
