// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<NameCreateRequest> for CreateNameCommand {
    fn from(req: NameCreateRequest) -> Self {
        Self {
            
            family_name: req.family_name,
            
            formatted_name: req.formatted_name,
            
            given_name: req.given_name,
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<NameUpdateRequest> for UpdateNameCommand {
    fn from(req: NameUpdateRequest) -> Self {
        Self {
            
            family_name: req.family_name,
            
            formatted_name: req.formatted_name,
            
            given_name: req.given_name,
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<NameResponse> for Name {
    fn from(resp: NameResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            family_name: resp.family_name,
            
            formatted_name: resp.formatted_name,
            
            given_name: resp.given_name,
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
