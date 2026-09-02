// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<EventBaseCreateRequest> for CreateEventBaseCommand {
    fn from(req: EventBaseCreateRequest) -> Self {
        Self {
            
            capacity: req.capacity,
            
            title: req.title,
            
            birth_date: req.birth_date.map(|ts| ts.into()),
            
            family_name: req.family_name,
            
            given_name: req.given_name,
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<EventBaseUpdateRequest> for UpdateEventBaseCommand {
    fn from(req: EventBaseUpdateRequest) -> Self {
        Self {
            
            capacity: req.capacity,
            
            title: req.title,
            
            birth_date: req.birth_date.map(|ts| ts.into()),
            
            family_name: req.family_name,
            
            given_name: req.given_name,
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<EventBaseResponse> for EventBase {
    fn from(resp: EventBaseResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
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
