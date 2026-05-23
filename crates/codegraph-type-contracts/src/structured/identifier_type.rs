use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// HR Open Standards IdentifierType — a character string used to uniquely
/// identify one instance of an object within an identification scheme that
/// is managed by an agency.
///
/// Use this type when the list or values are controlled by an external entity,
/// the list or values are public and could be referenced or validated.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IdentifierType {
    /// The identifier value.
    pub value: String,

    /// The identification of the identifier scheme.
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "schemeId")]
    pub scheme_id: Option<String>,

    /// The identification of the version of the identifier scheme.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "schemeVersionId"
    )]
    pub scheme_version_id: Option<String>,

    /// The identification of the agency that manages the identifier scheme.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "schemeAgencyId"
    )]
    pub scheme_agency_id: Option<String>,

    /// The description of the identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// The URI that identifies where the identification scheme data is located.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "schemeLink"
    )]
    pub scheme_link: Option<String>,

    /// The URI that identifies where the identification scheme is located.
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "agencyUri")]
    pub agency_uri: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_round_trip_minimal() {
        let id = IdentifierType {
            value: "ABC-123".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_string(&id).unwrap();
        let parsed: IdentifierType = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn serde_round_trip_full() {
        let id = IdentifierType {
            value: "ABC-123".to_string(),
            scheme_id: Some("ISO-6523".to_string()),
            scheme_version_id: Some("1.0".to_string()),
            scheme_agency_id: Some("UN/CEFACT".to_string()),
            description: Some("Global Location Number".to_string()),
            scheme_link: Some("https://example.com/scheme".to_string()),
            agency_uri: Some("https://example.com/agency".to_string()),
        };
        let json = serde_json::to_string(&id).unwrap();
        let parsed: IdentifierType = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn deserialize_from_json_schema_field_names() {
        let json = r#"{
            "value": "XYZ",
            "schemeId": "ISO",
            "schemeVersionId": "2.0",
            "schemeAgencyId": "OASIS",
            "description": "Test ID",
            "schemeLink": "https://example.com",
            "agencyUri": "https://agency.example.com"
        }"#;
        let id: IdentifierType = serde_json::from_str(json).unwrap();
        assert_eq!(id.value, "XYZ");
        assert_eq!(id.scheme_id.as_deref(), Some("ISO"));
        assert_eq!(id.scheme_version_id.as_deref(), Some("2.0"));
        assert_eq!(id.scheme_agency_id.as_deref(), Some("OASIS"));
        assert_eq!(id.description.as_deref(), Some("Test ID"));
        assert_eq!(id.scheme_link.as_deref(), Some("https://example.com"));
        assert_eq!(id.agency_uri.as_deref(), Some("https://agency.example.com"));
    }

    #[test]
    fn skip_serializing_none_fields() {
        let id = IdentifierType {
            value: "simple".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_string(&id).unwrap();
        assert!(!json.contains("schemeId"));
        assert!(!json.contains("description"));
    }
}
