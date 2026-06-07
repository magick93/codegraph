// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<ApplicationCreateRequest> for CreateApplicationCommand {
    fn from(req: ApplicationCreateRequest) -> Self {
        Self {
            
            application_id: req.application_id,
            
            applied_date: req.applied_date.map(|ts| ts.into()),
            
            candidate: req.candidate.map(|s| uuid::Uuid::parse_str(&s).unwrap_or_default()),
            
            status: req.status,
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<ApplicationUpdateRequest> for UpdateApplicationCommand {
    fn from(req: ApplicationUpdateRequest) -> Self {
        Self {
            
            application_id: req.application_id,
            
            applied_date: req.applied_date.map(|ts| ts.into()),
            
            candidate: req.candidate.map(|s| uuid::Uuid::parse_str(&s).unwrap_or_default()),
            
            status: req.status,
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<ApplicationResponse> for Application {
    fn from(resp: ApplicationResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            application_id: resp.application_id,
            
            applied_date: resp.applied_date.map(|dt| dt.into()),
            
            candidate: resp.candidate.map(|id| id.to_string()),
            
            status: resp.status.to_string(),
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
