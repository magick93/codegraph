use std::path::PathBuf;

use codegraph_core::mock::MockEngine;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::CollectionNode;
use tera::Tera;

use super::super::client_gen::{AtprotoClientEmitter, AtprotoClientScaffoldEmitter};
use super::{make_domain_config, make_primitive_prop, make_project, make_schema};
use crate::project_config::AtprotoConfig;
use crate::traits::{EntityGenerator, GlobalGenerator};
use crate::ProjectConfig;

fn make_client_tera() -> Tera {
    let mut tera = Tera::default();

    tera.add_raw_template(
            "atproto/client.tera",
            r#"use std::sync::Arc;

pub const COLLECTION: &str = "{{ collection_nsid }}";

pub struct {{ entity_name }}Client {
    repo: Arc<dyn RepoWriter>,
    pds_endpoint: String,
}

impl {{ entity_name }}Client {
    pub fn new(repo: Arc<dyn RepoWriter>, pds_endpoint: &str) -> Self {
        Self { repo, pds_endpoint: pds_endpoint.to_string() }
    }

    pub async fn create(&self, record: &{{ record_type }}) -> Result<CreateRecordResponse, AtprotoError> {
        self.repo.create_record(
            &self.pds_endpoint,
            "{{ collection_nsid }}",
            record,
        ).await
    }

    pub async fn get(&self, rkey: &str) -> Result<Option<{{ record_type }}>, AtprotoError> {
        self.repo.get_record::<{{ record_type }}>(
            &self.pds_endpoint,
            "{{ collection_nsid }}",
            rkey,
        ).await
    }

    pub async fn delete(&self, rkey: &str) -> Result<(), AtprotoError> {
        self.repo.delete_record(
            &self.pds_endpoint,
            "{{ collection_nsid }}",
            rkey,
        ).await
    }

    pub async fn list(&self, repo_did: &str, limit: Option<u32>, cursor: Option<&str>) -> Result<ListRecordsResponse<{{ record_type }}>, AtprotoError> {
        self.repo.list_records::<{{ record_type }}>(
            &self.pds_endpoint,
            repo_did,
            "{{ collection_nsid }}",
            limit.unwrap_or(50),
            cursor,
        ).await
    }
}
"#,
        )
        .unwrap();

    tera.add_raw_template(
        "atproto/client_scaffold.tera",
        r#"{% for entity in entities %}pub mod {{ entity.module_name }}_client;
{% endfor %}
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub struct AtprotoClient {
    pub repo: Arc<dyn RepoWriter>,
    pub pds_endpoint: String,
    pub did: String,
}

impl AtprotoClient {
    pub fn new(repo: Arc<dyn RepoWriter>, pds_endpoint: &str, did: &str) -> Self {
        Self { repo, pds_endpoint: pds_endpoint.to_string(), did: did.to_string() }
    }
}

#[derive(Debug, Serialize)]
pub struct CreateRecordResponse {
    pub uri: String,
    pub cid: String,
}

#[derive(Debug, Deserialize)]
pub struct ListRecordsResponse<T> {
    pub records: Vec<T>,
    pub cursor: Option<String>,
}

pub trait RepoWriter: Send + Sync {
    async fn create_record<T: Serialize + Send + Sync>(
        &self, pds_endpoint: &str, collection: &str, record: &T,
    ) -> Result<CreateRecordResponse, AtprotoError>;

    async fn get_record<T: serde::de::DeserializeOwned + Send>(
        &self, pds_endpoint: &str, collection: &str, rkey: &str,
    ) -> Result<Option<T>, AtprotoError>;

    async fn delete_record(
        &self, pds_endpoint: &str, collection: &str, rkey: &str,
    ) -> Result<(), AtprotoError>;

    async fn list_records<T: serde::de::DeserializeOwned>(
        &self, pds_endpoint: &str, repo_did: &str, collection: &str,
        limit: u32, cursor: Option<&str>,
    ) -> Result<ListRecordsResponse<T>, AtprotoError>;
}

#[derive(Debug, thiserror::Error)]
pub enum AtprotoError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("AT Protocol error: {0}")]
    Atproto(String),
}
"#,
    )
    .unwrap();

    tera
}

#[tokio::test]
async fn test_client_emitter_produces_client_struct() {
    let engine = MockEngine::builder()
        .with_schema(make_schema("Grant", "grants"))
        .with_properties(
            "Grant",
            vec![
                make_primitive_prop("name", "string", true),
                make_primitive_prop("amount", "number", false),
            ],
        )
        .with_lexicon_mapping("Grant", "nz.gravy.grants.grant")
        .build();

    let collection = CollectionNode {
        nsid: "nz.gravy.grants.grant".to_string(),
        key_strategy: "tid".to_string(),
        domain: "grants".to_string(),
    };
    engine.ingest_collection(&collection).await.unwrap();

    let tera = make_client_tera();
    let project = make_project();
    let emitter = AtprotoClientEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(
            &engine,
            "Grant",
            "grants",
            &make_domain_config(),
            &tera,
            &project,
        )
        .await
        .expect("generation should succeed");

    assert_eq!(result.len(), 1, "should produce one file");

    let content = &result[0].content;

    assert!(
        content.contains("pub struct GrantClient"),
        "should contain pub struct GrantClient, got:\n{}",
        content
    );
    assert!(
        content.contains("pub const COLLECTION: &str"),
        "should contain pub const COLLECTION: &str, got:\n{}",
        content
    );
    assert!(
        content.contains("create_record"),
        "should contain create_record method, got:\n{}",
        content
    );
    assert!(
        content.contains("nz.gravy.grants.grant"),
        "should contain collection NSID, got:\n{}",
        content
    );
}

#[tokio::test]
async fn test_client_emitter_skips_when_no_collection() {
    let engine = MockEngine::builder()
        .with_schema(make_schema("Grant", "grants"))
        .with_properties("Grant", vec![make_primitive_prop("name", "string", true)])
        .build();

    let tera = make_client_tera();
    let project = make_project();
    let emitter = AtprotoClientEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(
            &engine,
            "Grant",
            "grants",
            &make_domain_config(),
            &tera,
            &project,
        )
        .await
        .expect("generation should succeed");

    assert!(
        result.is_empty(),
        "should return empty when no collection exists"
    );
}

#[tokio::test]
async fn test_client_emitter_skips_when_authority_empty() {
    let engine = MockEngine::builder()
        .with_schema(make_schema("Grant", "grants"))
        .build();

    let tera = make_client_tera();
    let project = ProjectConfig {
        atproto: AtprotoConfig {
            atproto_authority: "".to_string(),
            ..Default::default()
        },
        ..Default::default()
    };
    let emitter = AtprotoClientEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(
            &engine,
            "Grant",
            "grants",
            &make_domain_config(),
            &tera,
            &project,
        )
        .await
        .expect("generation should succeed");

    assert!(
        result.is_empty(),
        "should return empty when authority is blank"
    );
}

#[tokio::test]
async fn test_client_scaffold_emitter_produces_repo_writer_trait() {
    let engine = MockEngine::builder().build();

    let coll1 = CollectionNode {
        nsid: "nz.gravy.grants.grant".to_string(),
        key_strategy: "tid".to_string(),
        domain: "grants".to_string(),
    };
    engine.ingest_collection(&coll1).await.unwrap();

    let coll2 = CollectionNode {
        nsid: "nz.gravy.grants.grantee".to_string(),
        key_strategy: "tid".to_string(),
        domain: "grants".to_string(),
    };
    engine.ingest_collection(&coll2).await.unwrap();

    let tera = make_client_tera();
    let project = make_project();
    let emitter = AtprotoClientScaffoldEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(&engine, &make_domain_config(), &[], &tera, &project)
        .await
        .expect("scaffold generation should succeed");

    assert_eq!(result.len(), 1, "should produce one file");

    let content = &result[0].content;

    assert!(
        content.contains("pub struct AtprotoClient"),
        "should contain AtprotoClient struct, got:\n{}",
        content
    );
    assert!(
        content.contains("pub trait RepoWriter"),
        "should contain pub trait RepoWriter, got:\n{}",
        content
    );
    assert!(
        content.contains("async fn create_record"),
        "should contain create_record method, got:\n{}",
        content
    );
    assert!(
        content.contains("async fn get_record"),
        "should contain get_record method, got:\n{}",
        content
    );
    assert!(
        content.contains("async fn delete_record"),
        "should contain delete_record method, got:\n{}",
        content
    );
    assert!(
        content.contains("async fn list_records"),
        "should contain list_records method, got:\n{}",
        content
    );
    assert!(
        content.contains("pub mod grant_client"),
        "should contain grant_client module declaration, got:\n{}",
        content
    );
    assert!(
        content.contains("pub mod grantee_client"),
        "should contain grantee_client module declaration, got:\n{}",
        content
    );
}

#[tokio::test]
async fn test_client_scaffold_skips_when_authority_empty() {
    let engine = MockEngine::builder().build();

    let coll = CollectionNode {
        nsid: "nz.gravy.grants.grant".to_string(),
        key_strategy: "tid".to_string(),
        domain: "grants".to_string(),
    };
    engine.ingest_collection(&coll).await.unwrap();

    let tera = make_client_tera();
    let project = ProjectConfig {
        atproto: AtprotoConfig {
            atproto_authority: "".to_string(),
            ..Default::default()
        },
        ..Default::default()
    };
    let emitter = AtprotoClientScaffoldEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(&engine, &make_domain_config(), &[], &tera, &project)
        .await
        .expect("generation should succeed");

    assert!(
        result.is_empty(),
        "should return empty when authority is blank"
    );
}
