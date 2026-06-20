// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<PersonBaseCreateRequest> for CreatePersonBaseCommand {
    fn from(req: PersonBaseCreateRequest) -> Self {
        Self {
            
            birth_date: req.birth_date.map(|ts| ts.into()),
            
            family_name: req.family_name,
            
            given_name: req.given_name,
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<PersonBaseUpdateRequest> for UpdatePersonBaseCommand {
    fn from(req: PersonBaseUpdateRequest) -> Self {
        Self {
            
            birth_date: req.birth_date.map(|ts| ts.into()),
            
            family_name: req.family_name,
            
            given_name: req.given_name,
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<PersonBaseResponse> for PersonBase {
    fn from(resp: PersonBaseResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            birth_date: resp.birth_date.map(|dt| dt.into()),
            
            family_name: resp.family_name,
            
            given_name: resp.given_name,
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
