// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<GenderCodeListCreateRequest> for CreateGenderCodeListCommand {
    fn from(req: GenderCodeListCreateRequest) -> Self {
        Self {
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<GenderCodeListUpdateRequest> for UpdateGenderCodeListCommand {
    fn from(req: GenderCodeListUpdateRequest) -> Self {
        Self {
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<GenderCodeListResponse> for GenderCodeList {
    fn from(resp: GenderCodeListResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
