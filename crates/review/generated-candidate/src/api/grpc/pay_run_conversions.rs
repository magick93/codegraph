// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<PayRunCreateRequest> for CreatePayRunCommand {
    fn from(req: PayRunCreateRequest) -> Self {
        Self {
            
            pay_run_id: req.pay_run_id,
            
            run_date: req.run_date.map(|ts| ts.into()),
            
            total_amount: req.total_amount.into(),
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<PayRunUpdateRequest> for UpdatePayRunCommand {
    fn from(req: PayRunUpdateRequest) -> Self {
        Self {
            
            pay_run_id: req.pay_run_id,
            
            run_date: req.run_date.map(|ts| ts.into()),
            
            total_amount: req.total_amount.into(),
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<PayRunResponse> for PayRun {
    fn from(resp: PayRunResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            pay_run_id: resp.pay_run_id,
            
            run_date: resp.run_date.map(|dt| dt.into()),
            
            total_amount: resp.total_amount.into(),
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
