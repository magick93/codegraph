use std::collections::HashMap;

use lsp_types::Uri;

/// LSP server state
pub struct LspBackend {
    /// Open documents: URI string → text content
    pub documents: HashMap<String, String>,
}

impl LspBackend {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
        }
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
