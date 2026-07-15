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
struct DidWebContext {
    host: String,
    signing_key: String,
    pds_url: String,
}

#[derive(Debug, Serialize)]
struct HandleContext {
    primary_did: String,
    handles: Vec<HandleEntry>,
}

#[derive(Debug, Serialize)]
struct HandleEntry {
    domain: String,
    did: String,
}

#[derive(Debug, Serialize)]
struct AuthContext {
    signing_key: String,
}

pub struct AtprotoIdentityEmitter {
    output_dir: PathBuf,
}

impl AtprotoIdentityEmitter {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for AtprotoIdentityEmitter {
    fn name(&self) -> &str {
        "atproto_identity"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        if project.atproto_authority.is_empty() {
            return Ok(Vec::new());
        }

        let authority = &project.atproto_authority;
        let parts: Vec<&str> = authority.rsplitn(2, '.').collect();
        let host = if parts.len() == 2 {
            format!("{}.{}", parts[1], parts[0])
        } else {
            authority.clone()
        };

        let did = format!("did:web:{}", host);
        let signing_key = "z6MkhaXgBxD..." .to_string();
        let pds_url = format!("https://{}", host);

        let mut files = Vec::new();

        let did_ctx = DidWebContext {
            host: host.clone(),
            signing_key: signing_key.clone(),
            pds_url: pds_url.clone(),
        };

        let did_content = render_template_with_project(
            tera,
            "atproto/did_web.tera",
            &did_ctx,
            project,
        )?;

        files.push(GeneratedFile {
            path: self
                .output_dir
                .join(".well-known")
                .join("did.json"),
            content: did_content,
        });

        let handle_ctx = HandleContext {
            primary_did: did.clone(),
            handles: vec![HandleEntry {
                domain: host.clone(),
                did: did.clone(),
            }],
        };

        let handle_content = render_template_with_project(
            tera,
            "atproto/handle.tera",
            &handle_ctx,
            project,
        )?;

        files.push(GeneratedFile {
            path: self
                .output_dir
                .join(".well-known")
                .join("atproto-did"),
            content: handle_content,
        });

        let auth_ctx = AuthContext {
            signing_key,
        };

        let auth_content = render_template_with_project(
            tera,
            "atproto/auth.tera",
            &auth_ctx,
            project,
        )?;

        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("api")
                .join("auth")
                .join("atproto.rs"),
            content: auth_content,
        });

        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use codegraph_core::mock::MockEngine;
    use tera::Tera;

    use super::*;
    use crate::generate::traits::GlobalGenerator;

    fn make_domain_config() -> codegraph_config::DomainConfig {
        let domains = std::collections::HashMap::new();
        codegraph_config::DomainConfig {
            defaults: Default::default(),
            domains,
        }
    }

    fn make_project() -> ProjectConfig {
        ProjectConfig {
            atproto_authority: "nz.gravy".to_string(),
            ..Default::default()
        }
    }

    fn make_tera() -> Tera {
        let mut tera = Tera::default();
        tera.add_raw_template(
            "atproto/did_web.tera",
            r##"{"id":"did:web:{{host}}","verificationMethod":[{"id":"#atproto","type":"Multikey","controller":"did:web:{{host}}"}]}"##,
        )
        .unwrap();
        tera.add_raw_template(
            "atproto/handle.tera",
            r#"# Primary DID: {{primary_did}}"#,
        )
        .unwrap();
        tera.add_raw_template(
            "atproto/auth.tera",
            r#"pub async fn validate_atproto_token(){}"#,
        )
        .unwrap();
        tera
    }

    #[tokio::test]
    async fn test_did_doc_contains_did_web_prefix() {
        let engine = MockEngine::builder().build();
        let tera = make_tera();
        let project = make_project();
        let emitter = AtprotoIdentityEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(&engine, &make_domain_config(), &[], &tera, &project)
            .await
            .expect("generation should succeed");

        let did_file = result
            .iter()
            .find(|f| f.path.to_string_lossy().ends_with("did.json"))
            .expect("should produce did.json");

        assert!(
            did_file.content.contains("did:web:"),
            "DID doc should contain did:web: prefix"
        );
        assert!(
            did_file.content.contains("verificationMethod"),
            "DID doc should contain verificationMethod"
        );
    }

    #[tokio::test]
    async fn test_handle_config_references_correct_did() {
        let engine = MockEngine::builder().build();
        let tera = make_tera();
        let project = make_project();
        let emitter = AtprotoIdentityEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(&engine, &make_domain_config(), &[], &tera, &project)
            .await
            .expect("generation should succeed");

        let handle_file = result
            .iter()
            .find(|f| f.path.to_string_lossy().ends_with("atproto-did"))
            .expect("should produce atproto-did");

        assert!(
            handle_file.content.contains("did:web:nz.gravy"),
            "handle config should reference correct DID"
        );
    }

    #[tokio::test]
    async fn test_auth_middleware_has_validate_token() {
        let engine = MockEngine::builder().build();
        let tera = make_tera();
        let project = make_project();
        let emitter = AtprotoIdentityEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(&engine, &make_domain_config(), &[], &tera, &project)
            .await
            .expect("generation should succeed");

        let auth_file = result
            .iter()
            .find(|f| f.path.to_string_lossy().contains("atproto.rs"))
            .expect("should produce atproto.rs auth file");

        assert!(
            auth_file.content.contains("validate_atproto_token"),
            "auth middleware should have validate_atproto_token function"
        );
    }

    #[tokio::test]
    async fn test_identity_skips_empty_authority() {
        let engine = MockEngine::builder().build();
        let tera = make_tera();
        let project = ProjectConfig {
            atproto_authority: "".to_string(),
            ..Default::default()
        };
        let emitter = AtprotoIdentityEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(&engine, &make_domain_config(), &[], &tera, &project)
            .await
            .expect("generation should succeed");

        assert!(result.is_empty(), "should return empty when authority is blank");
    }
}
