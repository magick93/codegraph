// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<CodeCreateRequest> for CreateCodeCommand {
    fn from(req: CodeCreateRequest) -> Self {
        Self {
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<CodeUpdateRequest> for UpdateCodeCommand {
    fn from(req: CodeUpdateRequest) -> Self {
        Self {
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<CodeResponse> for Code {
    fn from(resp: CodeResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
