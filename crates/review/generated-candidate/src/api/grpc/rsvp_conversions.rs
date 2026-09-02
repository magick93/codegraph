// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<RsvpCreateRequest> for CreateRsvpCommand {
    fn from(req: RsvpCreateRequest) -> Self {
        Self {
            
            event: req.event.into(),
            
            status: req.status,
            
            timestamp: req.timestamp,
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<RsvpUpdateRequest> for UpdateRsvpCommand {
    fn from(req: RsvpUpdateRequest) -> Self {
        Self {
            
            event: req.event.into(),
            
            status: req.status,
            
            timestamp: req.timestamp,
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<RsvpResponse> for Rsvp {
    fn from(resp: RsvpResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            event: resp.event.into(),
            
            status: resp.status.to_string(),
            
            timestamp: resp.timestamp.into(),
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
