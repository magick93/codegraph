// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<IdentifierCreateRequest> for CreateIdentifierCommand {
    fn from(req: IdentifierCreateRequest) -> Self {
        Self {
            
            scheme_agency_id: req.scheme_agency_id,
            
            scheme_id: req.scheme_id,
            
            scheme_version_id: req.scheme_version_id,
            
            value: req.value,
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<IdentifierUpdateRequest> for UpdateIdentifierCommand {
    fn from(req: IdentifierUpdateRequest) -> Self {
        Self {
            
            scheme_agency_id: req.scheme_agency_id,
            
            scheme_id: req.scheme_id,
            
            scheme_version_id: req.scheme_version_id,
            
            value: req.value,
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<IdentifierResponse> for Identifier {
    fn from(resp: IdentifierResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            scheme_agency_id: resp.scheme_agency_id,
            
            scheme_id: resp.scheme_id,
            
            scheme_version_id: resp.scheme_version_id,
            
            value: resp.value,
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
