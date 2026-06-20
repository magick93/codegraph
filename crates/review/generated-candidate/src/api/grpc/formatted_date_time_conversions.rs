// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<FormattedDateTimeCreateRequest> for CreateFormattedDateTimeCommand {
    fn from(req: FormattedDateTimeCreateRequest) -> Self {
        Self {
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<FormattedDateTimeUpdateRequest> for UpdateFormattedDateTimeCommand {
    fn from(req: FormattedDateTimeUpdateRequest) -> Self {
        Self {
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<FormattedDateTimeResponse> for FormattedDateTime {
    fn from(resp: FormattedDateTimeResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
