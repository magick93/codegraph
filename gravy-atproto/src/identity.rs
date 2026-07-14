use crate::error::AtprotoError;

pub fn parse_did(_did: &str) -> Result<ParsedDid, AtprotoError> {
    // TODO: implement with rsky-syntax (behind the "rsky" feature gate)
    Err(AtprotoError::Validation(
        "rsky feature not enabled — DID parsing not available".into(),
    ))
}

pub struct ParsedDid {
    pub method: String,
    pub identifier: String,
}

pub fn parse_handle(_handle: &str) -> Result<(), AtprotoError> {
    // TODO: implement with rsky-syntax (behind the "rsky" feature gate)
    Err(AtprotoError::Validation(
        "rsky feature not enabled — handle parsing not available".into(),
    ))
}
