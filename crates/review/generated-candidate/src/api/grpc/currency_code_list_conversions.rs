// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<CurrencyCodeListCreateRequest> for CreateCurrencyCodeListCommand {
    fn from(req: CurrencyCodeListCreateRequest) -> Self {
        Self {
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<CurrencyCodeListUpdateRequest> for UpdateCurrencyCodeListCommand {
    fn from(req: CurrencyCodeListUpdateRequest) -> Self {
        Self {
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<CurrencyCodeListResponse> for CurrencyCodeList {
    fn from(resp: CurrencyCodeListResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
