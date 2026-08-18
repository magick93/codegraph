// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<PublicEventCreateRequest> for CreatePublicEventCommand {
    fn from(req: PublicEventCreateRequest) -> Self {
        Self {
            
            is_published: req.is_published,
            
            capacity: req.capacity,
            
            title: req.title,
            
            birth_date: req.birth_date.map(|ts| ts.into()),
            
            family_name: req.family_name,
            
            given_name: req.given_name,
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<PublicEventUpdateRequest> for UpdatePublicEventCommand {
    fn from(req: PublicEventUpdateRequest) -> Self {
        Self {
            
            is_published: req.is_published,
            
            capacity: req.capacity,
            
            title: req.title,
            
            birth_date: req.birth_date.map(|ts| ts.into()),
            
            family_name: req.family_name,
            
            given_name: req.given_name,
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<PublicEventResponse> for PublicEvent {
    fn from(resp: PublicEventResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            is_published: resp.is_published,
            
            capacity: resp.capacity,
            
            title: resp.title,
            
            birth_date: resp.birth_date.map(|dt| dt.into()),
            
            family_name: resp.family_name,
            
            given_name: resp.given_name,
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
