// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<ProcessHistoryCreateRequest> for CreateProcessHistoryCommand {
    fn from(req: ProcessHistoryCreateRequest) -> Self {
        Self {
            
            action_date: req.action_date,
            
            descriptions: req.descriptions,
            
            id: req.id,
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<ProcessHistoryUpdateRequest> for UpdateProcessHistoryCommand {
    fn from(req: ProcessHistoryUpdateRequest) -> Self {
        Self {
            
            action_date: req.action_date,
            
            descriptions: req.descriptions,
            
            id: req.id,
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<ProcessHistoryResponse> for ProcessHistory {
    fn from(resp: ProcessHistoryResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            action_date: resp.action_date.map(|dt| dt.into()),
            
            descriptions: resp.descriptions,
            
            id: resp.id,
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
