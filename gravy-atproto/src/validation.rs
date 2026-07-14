use crate::error::AtprotoError;
use crate::identity::{parse_did, parse_handle};

pub fn validate_string_format(value: &str, format: &str) -> Result<(), AtprotoError> {
    match format {
        "did" => parse_did(value).map(|_| ()),
        "handle" => parse_handle(value),
        "at-uri" => validate_at_uri(value),
        "datetime" => validate_datetime(value),
        _ => Ok(()),
    }
}

pub fn validate_at_uri(value: &str) -> Result<(), AtprotoError> {
    if !value.starts_with("at://") {
        return Err(AtprotoError::Validation(format!(
            "Invalid at-uri: {}",
            value
        )));
    }
    Ok(())
}

pub fn validate_datetime(value: &str) -> Result<(), AtprotoError> {
    // ISO 8601 datetime validation
    chrono::DateTime::parse_from_rfc3339(value).map_err(|e| {
        AtprotoError::Validation(format!("Invalid datetime '{}': {}", value, e))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_datetime_valid() {
        assert!(validate_datetime("2024-01-01T00:00:00Z").is_ok());
        assert!(validate_datetime("2024-01-01T00:00:00+01:00").is_ok());
    }

    #[test]
    fn test_validate_datetime_invalid() {
        assert!(validate_datetime("not-a-date").is_err());
    }

    #[test]
    fn test_validate_at_uri_valid() {
        assert!(validate_at_uri("at://did:plc:abc/collection/rkey").is_ok());
    }

    #[test]
    fn test_validate_at_uri_invalid() {
        assert!(validate_at_uri("https://example.com").is_err());
    }

    #[test]
    fn test_validate_unknown_format_passes() {
        assert!(validate_string_format("anything", "unknown-format").is_ok());
    }
}
