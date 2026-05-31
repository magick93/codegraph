use std::collections::HashMap;
use std::path::PathBuf;

use lsp_types::Uri;

#[derive(Debug, Clone)]
pub struct SchemaInfo {
    pub title: String,
    pub description: Option<String>,
    pub properties: Vec<String>,
    pub rel_path: String,
}

pub struct LspBackend {
    pub documents: HashMap<String, String>,
    pub entity_names: Vec<String>,
    pub schema_infos: HashMap<String, SchemaInfo>,
    pub schema_dirs: Vec<PathBuf>,
}

impl LspBackend {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
            entity_names: Vec::new(),
            schema_infos: HashMap::new(),
            schema_dirs: Vec::new(),
        }
    }

    pub fn with_entity_names(mut self, names: Vec<String>) -> Self {
        self.entity_names = names;
        self
    }

    pub fn with_schema_infos(mut self, infos: HashMap<String, SchemaInfo>) -> Self {
        self.schema_infos = infos;
        self
    }

    pub fn with_schema_dirs(mut self, dirs: Vec<PathBuf>) -> Self {
        self.schema_dirs = dirs;
        self
    }

    pub fn open_document(&mut self, uri: Uri, text: &str) {
        self.documents
            .insert(uri.as_str().to_string(), text.to_string());
    }

    pub fn update_document(&mut self, uri: Uri, text: &str) {
        self.documents
            .insert(uri.as_str().to_string(), text.to_string());
    }

    pub fn close_document(&mut self, uri: Uri) {
        self.documents.remove(uri.as_str());
    }

    pub fn get_document(&self, uri: &Uri) -> Option<&str> {
        self.documents.get(uri.as_str()).map(|s| s.as_str())
    }
}

impl Default for LspBackend {
    fn default() -> Self {
        Self::new()
    }
}
