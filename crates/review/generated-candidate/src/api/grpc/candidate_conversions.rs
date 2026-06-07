// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<CandidateCreateRequest> for CreateCandidateCommand {
    fn from(req: CandidateCreateRequest) -> Self {
        Self {
            
            birth_date: req.birth_date.map(|ts| ts.into()),
            
            family_name: req.family_name,
            
            given_name: req.given_name,
            
            application_process_history: req.application_process_history.into(),
            
            candidate_id: req.candidate_id,
            
            compensation_expectation: req.compensation_expectation.into(),
            
            distribution_guidelines: req.distribution_guidelines.into(),
            
            external_identifier: req.external_identifier,
            
            gender: req.gender,
            
            person_name: req.person_name.into(),
            
            position_schedule_type_codes: req.position_schedule_type_codes,
            
            position_titles: req.position_titles,
            
            qualifications: req.qualifications.into(),
            
            referred_by_application: req.referred_by_application.map(|s| uuid::Uuid::parse_str(&s).unwrap_or_default()),
            
            status: req.status,
            
            uri: req.uri,
            
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<CandidateUpdateRequest> for UpdateCandidateCommand {
    fn from(req: CandidateUpdateRequest) -> Self {
        Self {
            
            birth_date: req.birth_date.map(|ts| ts.into()),
            
            family_name: req.family_name,
            
            given_name: req.given_name,
            
            application_process_history: req.application_process_history.into(),
            
            candidate_id: req.candidate_id,
            
            compensation_expectation: req.compensation_expectation.into(),
            
            distribution_guidelines: req.distribution_guidelines.into(),
            
            external_identifier: req.external_identifier,
            
            gender: req.gender,
            
            person_name: req.person_name.into(),
            
            position_schedule_type_codes: req.position_schedule_type_codes,
            
            position_titles: req.position_titles,
            
            qualifications: req.qualifications.into(),
            
            referred_by_application: req.referred_by_application.map(|s| uuid::Uuid::parse_str(&s).unwrap_or_default()),
            
            status: req.status,
            
            uri: req.uri,
            
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<CandidateResponse> for Candidate {
    fn from(resp: CandidateResponse) -> Self {
        Self {
            id: resp.id.to_string(),
            
            birth_date: resp.birth_date.map(|dt| dt.into()),
            
            family_name: resp.family_name,
            
            given_name: resp.given_name,
            
            application_process_history: resp.application_process_history.into(),
            
            candidate_id: resp.candidate_id,
            
            compensation_expectation: resp.compensation_expectation.into(),
            
            distribution_guidelines: resp.distribution_guidelines.into(),
            
            external_identifier: resp.external_identifier,
            
            gender: resp.gender.to_string(),
            
            person_name: resp.person_name.into(),
            
            position_schedule_type_codes: resp.position_schedule_type_codes.to_string(),
            
            position_titles: resp.position_titles,
            
            qualifications: resp.qualifications.into(),
            
            referred_by_application: resp.referred_by_application.map(|id| id.to_string()),
            
            status: resp.status.to_string(),
            
            uri: resp.uri,
            
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
