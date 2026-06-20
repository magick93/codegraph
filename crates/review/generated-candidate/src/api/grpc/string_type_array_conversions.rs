// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<StringTypeArrayCreateRequest> for CreateStringTypeArrayCommand {
    fn from(req: StringTypeArrayCreateRequest) -> Self {
        Self {
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<StringTypeArrayUpdateRequest> for UpdateStringTypeArrayCommand {
    fn from(req: StringTypeArrayUpdateRequest) -> Self {
        Self {
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<StringTypeArrayResponse> for StringTypeArray {
    fn from(resp: StringTypeArrayResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
