// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<PositionScheduleTypeCodeListCreateRequest> for CreatePositionScheduleTypeCodeListCommand {
    fn from(req: PositionScheduleTypeCodeListCreateRequest) -> Self {
        Self {
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<PositionScheduleTypeCodeListUpdateRequest> for UpdatePositionScheduleTypeCodeListCommand {
    fn from(req: PositionScheduleTypeCodeListUpdateRequest) -> Self {
        Self {
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<PositionScheduleTypeCodeListResponse> for PositionScheduleTypeCodeList {
    fn from(resp: PositionScheduleTypeCodeListResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
