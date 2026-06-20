// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<DateCreateRequest> for CreateDateCommand {
    fn from(req: DateCreateRequest) -> Self {
        Self {
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<DateUpdateRequest> for UpdateDateCommand {
    fn from(req: DateUpdateRequest) -> Self {
        Self {
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<DateResponse> for Date {
    fn from(resp: DateResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
