// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<EffectiveDateCreateRequest> for CreateEffectiveDateCommand {
    fn from(req: EffectiveDateCreateRequest) -> Self {
        Self {
            
            valid_from: req.valid_from.map(|ts| ts.into()),
            
            valid_to: req.valid_to.map(|ts| ts.into()),
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<EffectiveDateUpdateRequest> for UpdateEffectiveDateCommand {
    fn from(req: EffectiveDateUpdateRequest) -> Self {
        Self {
            
            valid_from: req.valid_from.map(|ts| ts.into()),
            
            valid_to: req.valid_to.map(|ts| ts.into()),
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<EffectiveDateResponse> for EffectiveDate {
    fn from(resp: EffectiveDateResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            valid_from: resp.valid_from.map(|dt| dt.into()),
            
            valid_to: resp.valid_to.map(|dt| dt.into()),
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
