// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<DistributionBaseCreateRequest> for CreateDistributionBaseCommand {
    fn from(req: DistributionBaseCreateRequest) -> Self {
        Self {
            
            description: req.description,
            
            end_date: req.end_date.map(|ts| ts.into()),
            
            start_date: req.start_date,
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<DistributionBaseUpdateRequest> for UpdateDistributionBaseCommand {
    fn from(req: DistributionBaseUpdateRequest) -> Self {
        Self {
            
            description: req.description,
            
            end_date: req.end_date.map(|ts| ts.into()),
            
            start_date: req.start_date,
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<DistributionBaseResponse> for DistributionBase {
    fn from(resp: DistributionBaseResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            description: resp.description,
            
            end_date: resp.end_date.map(|dt| dt.into()),
            
            start_date: resp.start_date.into(),
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
