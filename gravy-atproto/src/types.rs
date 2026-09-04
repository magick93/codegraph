use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRecordResponse {
    pub uri: String,
    pub cid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRecordsResponse<T> {
    pub records: Vec<RecordEntry<T>>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordEntry<T> {
    pub uri: String,
    pub cid: String,
    pub value: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobRef {
    #[serde(rename = "$type")]
    pub r#type: String,
    #[serde(rename = "ref")]
    pub ref_link: CidString,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub size: u64,
}

pub type CidString = String;
pub type AtUri = String;
pub type Did = String;
pub type Handle = String;
pub type Nsid = String;
pub type Rkey = String;
pub type Tid = String;
