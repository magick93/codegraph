// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<AmountCreateRequest> for CreateAmountCommand {
    fn from(req: AmountCreateRequest) -> Self {
        Self {
            
            currency: req.currency,
            
            value: req.value,
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<AmountUpdateRequest> for UpdateAmountCommand {
    fn from(req: AmountUpdateRequest) -> Self {
        Self {
            
            currency: req.currency,
            
            value: req.value,
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<AmountResponse> for Amount {
    fn from(resp: AmountResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            currency: resp.currency.to_string(),
            
            value: resp.value,
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
