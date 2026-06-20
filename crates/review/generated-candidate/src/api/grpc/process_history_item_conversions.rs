// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<ProcessHistoryItemCreateRequest> for CreateProcessHistoryItemCommand {
    fn from(req: ProcessHistoryItemCreateRequest) -> Self {
        Self {
            
            action_date: req.action_date,
            
            descriptions: req.descriptions,
            
            id: req.id,
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<ProcessHistoryItemUpdateRequest> for UpdateProcessHistoryItemCommand {
    fn from(req: ProcessHistoryItemUpdateRequest) -> Self {
        Self {
            
            action_date: req.action_date,
            
            descriptions: req.descriptions,
            
            id: req.id,
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<ProcessHistoryItemResponse> for ProcessHistoryItem {
    fn from(resp: ProcessHistoryItemResponse) -> Self {
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
