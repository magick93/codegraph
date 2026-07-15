use crate::error::AtprotoError;
use crate::types::{CreateRecordResponse, ListRecordsResponse};
use serde::{de::DeserializeOwned, Serialize};

pub trait RepoWriter: Send + Sync {
    async fn create_record<T: Serialize + Send + Sync>(
        &self,
        collection: &str,
        record: &T,
    ) -> Result<CreateRecordResponse, AtprotoError>;

    async fn get_record<T: DeserializeOwned + Send>(
        &self,
        collection: &str,
        rkey: &str,
    ) -> Result<Option<T>, AtprotoError>;

    async fn delete_record(
        &self,
        collection: &str,
        rkey: &str,
    ) -> Result<(), AtprotoError>;

    async fn list_records<T: DeserializeOwned>(
        &self,
        collection: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<ListRecordsResponse<T>, AtprotoError>;
}

pub struct HttpRepoWriter {
    pds_endpoint: String,
    did: String,
    client: reqwest::Client,
}

impl HttpRepoWriter {
    pub fn new(pds_endpoint: &str, did: &str) -> Self {
        Self {
            pds_endpoint: pds_endpoint.to_string(),
            did: did.to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub fn set_auth_token(&self, _token: &str) {
        // TODO: implement auth header injection
    }
}

impl RepoWriter for HttpRepoWriter {
    async fn create_record<T: Serialize + Send + Sync>(
        &self,
        collection: &str,
        record: &T,
    ) -> Result<CreateRecordResponse, AtprotoError> {
        let url = format!(
            "{}/xrpc/com.atproto.repo.createRecord",
            self.pds_endpoint
        );
        let body = serde_json::json!({
            "repo": self.did,
            "collection": collection,
            "record": record,
        });
        let resp = self.client.post(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            let err_body = resp.text().await.unwrap_or_default();
            return Err(AtprotoError::Repo(err_body));
        }
        Ok(resp.json().await?)
    }

    async fn get_record<T: DeserializeOwned + Send>(
        &self,
        collection: &str,
        rkey: &str,
    ) -> Result<Option<T>, AtprotoError> {
        let url = format!(
            "{}/xrpc/com.atproto.repo.getRecord",
            self.pds_endpoint
        );
        let resp = self
            .client
            .get(&url)
            .query(&[
                ("repo", &self.did),
                ("collection", &collection.to_string()),
                ("rkey", &rkey.to_string()),
            ])
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            return Err(AtprotoError::Repo(resp.text().await.unwrap_or_default()));
        }
        #[derive(serde::Deserialize)]
        struct GetRecordResponse<T> {
            value: T,
            #[allow(dead_code)]
            uri: String,
            #[allow(dead_code)]
            cid: Option<String>,
        }
        let body: GetRecordResponse<T> = resp.json().await?;
        Ok(Some(body.value))
    }

    async fn delete_record(&self, collection: &str, rkey: &str) -> Result<(), AtprotoError> {
        let url = format!(
            "{}/xrpc/com.atproto.repo.deleteRecord",
            self.pds_endpoint
        );
        let body = serde_json::json!({
            "repo": self.did,
            "collection": collection,
            "rkey": rkey,
        });
        let resp = self.client.post(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            return Err(AtprotoError::Repo(resp.text().await.unwrap_or_default()));
        }
        Ok(())
    }

    async fn list_records<T: DeserializeOwned>(
        &self,
        collection: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<ListRecordsResponse<T>, AtprotoError> {
        let url = format!(
            "{}/xrpc/com.atproto.repo.listRecords",
            self.pds_endpoint
        );
        let mut query = vec![
            ("repo", self.did.clone()),
            ("collection", collection.to_string()),
            ("limit", limit.to_string()),
        ];
        if let Some(c) = cursor {
            query.push(("cursor", c.to_string()));
        }
        let resp = self.client.get(&url).query(&query).send().await?;
        if !resp.status().is_success() {
            return Err(AtprotoError::Repo(resp.text().await.unwrap_or_default()));
        }
        Ok(resp.json().await?)
    }
}
