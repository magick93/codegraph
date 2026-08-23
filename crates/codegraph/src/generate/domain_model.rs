//! Canonical domain model types shared across all generators and templates.
//!
//! Every generator queries `build_entity_model` ONCE per entity to get a
//! fully-resolved `EntityModel`. Templates render from this canonical type,
//! eliminating field mismatches, missing imports, and inconsistent naming.

use std::collections::HashSet;

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{PropertyNode, SchemaNode};
use serde::Serialize;

use crate::error::Result;

// ── Canonical Types ──────────────────────────────────────────────────────────

/// Fully-resolved entity model queried from the graph DB.
/// This is the single source of truth for all generators and templates.
#[derive(Debug, Clone, Serialize)]
pub struct EntityModel {
    /// PascalCase: "PersonRecord"
    pub name: String,
    /// snake_case table name: "person_record"
    pub table_name: String,
    /// Entity module name (schema-prefixed): "crm_person_record"
    pub entity_module: String,
    /// Domain: "crm"
    pub domain: String,
    /// REST URL path segment: "person-record"
    pub api_path: String,
    /// AT Protocol NSID: "community.os.crm.person_record"
    pub nsid: String,
    /// Fields, deduplicated and fully typed
    pub fields: Vec<EntityField>,
    /// CRUD operations enabled for this entity
    pub operations: EntityOperations,
    /// Postgres schema name: "crm"
    pub pg_schema: String,
    /// AllOf parent schema titles (for inheritance tracking)
    pub all_of_parents: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntityField {
    /// camelCase name from JSON schema: "preferredName"
    pub name: String,
    /// snake_case column name: "preferred_name"
    pub column: String,
    /// snake_case Rust field name: "preferred_name"
    pub rust_field: String,
    /// Canonical Rust type: RustType::String, RustType::Option(...), etc.
    pub rust_type: RustType,
    /// SeaORM type string: "String", "Integer", "DateTime", etc.
    pub sea_orm_type: String,
    /// PostgreSQL column type: "TEXT", "UUID", "INTEGER", "JSONB", etc.
    pub pg_type: String,
    /// TypeScript type: "string", "number", "boolean", "any[]", etc.
    pub ts_type: String,
    /// Is this field required (non-nullable)?
    pub required: bool,
    /// Is this a primary key field?
    pub is_pk: bool,
    /// Is this a foreign key to another entity?
    pub is_fk: bool,
    /// FK target schema title, if is_fk
    pub fk_target: Option<String>,
    /// FK target table name, if is_fk
    pub fk_table: Option<String>,
    /// Classification kind from the graph DB
    pub classification: Option<String>,
    /// Smart example value for test fixtures
    pub example_value: String,
    /// Human-readable label (fallback to name)
    pub label: String,
    /// Is this field from an allOf parent (inherited)?
    pub inherited: bool,
    /// Is this field stored in a separate child table (array codelist / array ValueObject)?
    pub is_child_table: bool,
    /// Does the SeaORM Model use Option<T> for this field?
    pub is_model_optional: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum RustType {
    /// Simple scalar without generics: "String", "Uuid", "i32", "bool"
    Simple(String),
    /// Option<T>: Option(Box::new(RustType::Simple("String".into())))
    Optional {
        optional: Box<RustType>,
    },
    /// Vec<T>: Vec(Box::new(RustType::Simple("String".into())))
    Collection {
        collection: Box<RustType>,
    },
    /// Named custom type: Custom("PersonReferenceType".into())
    Custom(String),
}

impl RustType {
    /// Returns the inner type name, stripping Option and Vec wrappers.
    pub fn inner_type(&self) -> &str {
        match self {
            RustType::Simple(s) => s,
            RustType::Optional { optional } => optional.inner_type(),
            RustType::Collection { collection } => collection.inner_type(),
            RustType::Custom(s) => s,
        }
    }

    /// Returns the full Rust type string for codegen.
    pub fn to_rust_string(&self) -> String {
        match self {
            RustType::Simple(s) => s.clone(),
            RustType::Optional { optional } => format!("Option<{}>", optional.to_rust_string()),
            RustType::Collection { collection } => format!("Vec<{}>", collection.to_rust_string()),
            RustType::Custom(s) => s.clone(),
        }
    }

    /// Returns the SeaORM type-friendly representation.
    pub fn to_sea_orm_wrapper(&self) -> String {
        match self {
            RustType::Simple(_) => String::new(),
            RustType::Optional { .. } => "Option".to_string(),
            RustType::Collection { .. } => "Vec".to_string(),
            RustType::Custom(_) => String::new(),
        }
    }

    /// True if this type represents an Option<T>.
    pub fn is_optional(&self) -> bool {
        matches!(self, RustType::Optional { .. })
    }

    /// True if this type represents a Vec<T>.
    pub fn is_collection(&self) -> bool {
        matches!(self, RustType::Collection { .. })
    }
}

impl std::fmt::Display for RustType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_rust_string())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EntityOperations {
    pub create: bool,
    pub read: bool,
    pub update: bool,
    pub delete: bool,
    pub list: bool,
}

// ── Builder ──────────────────────────────────────────────────────────────────

/// Build a fully-resolved `EntityModel` from the graph database.
///
/// Queries the graph DB once per entity. Fields are deduplicated (inheritance
/// chains like NounType → AtProtocolInclusion → PersonBaseType → PersonRecordType
/// produce duplicate `did` fields — only the first occurrence is kept).
pub async fn build_entity_model(
    db: &dyn GraphQuerier,
    schema_title: &str,
    domain: &str,
    config: &DomainConfig,
    authority: &str,
) -> Result<EntityModel> {
    let schema = db
        .get_schema_in_domain(schema_title, domain)
        .await?
        .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

    let properties = db.get_properties_in_domain(schema_title, domain).await?;

    let all_of_parents = if schema.has_all_of {
        db.get_allof_targets(schema_title).await?
    } else {
        Vec::new()
    };

    let entity_cfg = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(&schema.rust_type_name));

    let operations = entity_cfg
        .and_then(|ec| ec.operations.clone())
        .unwrap_or_else(|| config.defaults.operations.clone());

    let ops = EntityOperations {
        create: operations.iter().any(|s| s == "create"),
        read: operations.iter().any(|s| s == "read"),
        update: operations.iter().any(|s| s == "update"),
        delete: operations.iter().any(|s| s == "delete"),
        list: operations.iter().any(|s| s == "list"),
    };

    let nsid = if authority.is_empty() {
        format!("community.os.{}.{}", domain, schema.pg_table_name)
    } else {
        format!("{}.{}.{}", authority, domain, schema.pg_table_name)
    };

    let pg_schema = config
        .domains
        .get(domain)
        .map(|d| d.postgres_schema.clone())
        .unwrap_or_else(|| domain.to_string());

    let entity_module = format!("{}_{}", pg_schema, schema.pg_table_name);

    let fields = resolve_fields(&properties, &all_of_parents);

    Ok(EntityModel {
        name: schema.rust_type_name.clone(),
        table_name: schema.pg_table_name.clone(),
        entity_module: entity_module.clone(),
        domain: domain.to_string(),
        api_path: schema.api_path_segment.clone(),
        nsid,
        fields,
        operations: ops,
        pg_schema,
        all_of_parents,
    })
}

/// Resolve canonical fields from raw PropertyNodes.
/// Deduplicates by name (first occurrence wins — inheritance produces duplicates).
fn resolve_fields(properties: &[PropertyNode], all_of_parents: &[String]) -> Vec<EntityField> {
    let mut seen = HashSet::new();
    let mut fields = Vec::new();

    for prop in properties {
        if !seen.insert(prop.name.clone()) {
            continue; // skip duplicates from inheritance chains
        }

        if prop.name == "id"
            || prop.name == "created_at"
            || prop.name == "updated_at"
            || prop.name == "deleted_at"
            || prop.name == "platform_organization_id"
            || prop.name == "is_demo_data"
        {
            continue; // skip system fields for DTO/API context
        }

        let raw_type = &prop.rust_field_type;
        let rust_type = parse_rust_type(raw_type, prop.is_required);

        // Does the SeaORM model use Option<T> for this field?
        let is_model_optional = !prop.is_required || raw_type.starts_with("Option<") || raw_type.starts_with("Nullable<");

        let is_fk = prop.ref_target.is_some()
            || matches!(
                prop.classification_kind,
                Some(codegraph_type_contracts::RefClassificationKind::EntityReference)
                    | Some(codegraph_type_contracts::RefClassificationKind::CodelistReference)
            );

        let kind = prop.classification_kind.as_ref();
        // Child table = field whose data isn't a direct column on the parent Model.
        // This covers array codelists (child table), array value objects (child table),
        // scalar value objects (child table OR expanded columns), entity references (FK),
        // composite wrappers (expanded columns), and structured wrappers (JSONB).
        let is_child_table = prop.is_array && matches!(
            kind,
            Some(codegraph_type_contracts::RefClassificationKind::CodelistReference)
                | Some(codegraph_type_contracts::RefClassificationKind::CodelistCheck)
                | Some(codegraph_type_contracts::RefClassificationKind::ValueObject))
            || matches!(
                kind,
                Some(codegraph_type_contracts::RefClassificationKind::ValueObject)
                    | Some(codegraph_type_contracts::RefClassificationKind::EntityReference)
                    | Some(codegraph_type_contracts::RefClassificationKind::CompositeWrapper)
                    | Some(codegraph_type_contracts::RefClassificationKind::StructuredWrapper)
                    | Some(codegraph_type_contracts::RefClassificationKind::MediaWrapper)
                    | Some(codegraph_type_contracts::RefClassificationKind::ArrayWrapper));

        let inherited = all_of_parents.iter().any(|_| false);

        let classification = prop
            .classification
            .clone()
            .or_else(|| kind.map(|k| format!("{:?}", k)));

        fields.push(EntityField {
            name: prop.name.clone(),
            column: prop.pg_column_name.clone(),
            rust_field: prop.rust_field_name.clone(),
            rust_type: rust_type.clone(),
            sea_orm_type: prop.sea_orm_type.clone(),
            pg_type: prop.pg_column_type.clone(),
            ts_type: ts_type_for_field(&rust_type),
            required: prop.is_required,
            is_pk: prop.pg_column_name == "id",
            is_fk,
            fk_target: prop.ref_target.clone(),
            fk_table: None,
            classification,
            example_value: example_for_field(&prop.name, &prop.rust_field_type, prop.ref_target.as_deref()),
            label: prop
                .description
                .clone()
                .unwrap_or_else(|| humanize_field_name(&prop.name)),
            inherited,
            is_child_table,
            is_model_optional,
        });
    }

    // Sort: required fields first, then optional. PK last.
    fields.sort_by(|a, b| {
        a.required
            .cmp(&b.required)
            .reverse()
            .then_with(|| a.name.cmp(&b.name))
    });

    fields
}

/// Parse a Rust field type string into canonical RustType.
pub(crate) fn parse_rust_type(rust_field_type: &str, is_required: bool) -> RustType {
    let stripped = rust_field_type.trim();

    // Option<T> → Optional { ... }
    if let Some(inner) = stripped
        .strip_prefix("Option<")
        .and_then(|s| s.strip_suffix('>'))
    {
        return RustType::Optional {
            optional: Box::new(parse_rust_type(inner, true)),
        };
    }

    // Vec<T> → Collection { ... }
    if let Some(inner) = stripped
        .strip_prefix("Vec<")
        .and_then(|s| s.strip_suffix('>'))
    {
        return RustType::Collection {
            collection: Box::new(parse_rust_type(inner, true)),
        };
    }

    // Known simple types
    match stripped {
        "String" | "Uuid" | "i32" | "i64" | "bool" | "f64" | "Decimal"
        | "NaiveDate" | "NaiveDateTime" | "DateTime<Utc>" | "DateTimeUtc" => {
            RustType::Simple(stripped.to_string())
        }
        "serde_json::Value" | "Json" => RustType::Simple("serde_json::Value".to_string()),
        _ => {
            // Treat unknown types as custom (named type references)
            if stripped.contains('<') || stripped.contains("::") {
                RustType::Custom(stripped.to_string())
            } else {
                RustType::Simple(stripped.to_string())
            }
        }
    }
}

/// Map canonical RustType to TypeScript type.
pub(crate) fn ts_type_for_field(rust_type: &RustType) -> String {
    match rust_type {
        RustType::Simple(s) => match s.as_str() {
            "String" | "Uuid" => "string",
            "i32" | "i64" | "f64" | "Decimal" => "number",
            "bool" => "boolean",
            "NaiveDate" | "NaiveDateTime" | "DateTime<Utc>" | "DateTimeUtc" => "string",
            "serde_json::Value" => "any",
            _ => "string",
        }
        .to_string(),
        RustType::Optional { optional } => {
            format!("{} | null", ts_type_for_field(optional))
        }
        RustType::Collection { .. } => "any[]".to_string(),
        RustType::Custom(_) => "any".to_string(),
    }
}

/// Generate a smart example value for test fixtures.
pub(crate) fn example_for_field(name: &str, rust_type: &str, codelist_target: Option<&str>) -> String {
    match name {
        "email" => "\"test@example.com\"".into(),
        "first_name" | "firstName" | "preferredName" | "preferred_name" => "\"Test\"".into(),
        "last_name" | "lastName" | "legalName" | "legal_name" => "\"Person\"".into(),
        "name" | "title" | "displayName" | "display_name" => "\"Test Entry\"".into(),
        "personName" | "person_name" => "\"Test Person\"".into(),
        "personDid" | "person_did" => "\"did:web:test.community.os\"".into(),
        "personRelationship" | "person_relationship" => "\"family\"".into(),
        "phone" | "phoneNumber" | "mobile" => "\"+64 4 123 4567\"".into(),
        "website" => "\"https://example.com\"".into(),
        "description" | "notes" | "crmNotes" | "crm_notes" => "\"Generated by E2E test\"".into(),
        "address" | "location" => "\"123 Test Street\"".into(),
        "city" => "\"Wellington\"".into(),
        "region" => "\"wellington\"".into(),
        "postalCode" | "postal_code" => "\"6011\"".into(),
        "country" => "\"NZ\"".into(),
        "date_of_birth" | "dateOfBirth" | "startDate" | "endDate" | "date" | "start_date"
        | "end_date" | "openedDate" | "opened_date" | "closingDate" | "closing_date"
        | "grantedAt" | "granted_at" | "expiresAt" | "expires_at" | "revokedAt"
        | "revoked_at" | "validFrom" | "valid_from" | "validUntil" | "valid_until" => {
            if rust_type.contains("DateTime") {
                "\"2025-01-15T00:00:00Z\"".into()
            } else {
                "\"2025-01-15\"".into()
            }
        }
        "time" | "startTime" | "endTime" | "start_time" | "end_time" => "\"14:00\"".into(),
        "channel" | "notificationChannel" | "notification_channel" => "\"Email\"".into(),
        "checkInMethod" | "check_in_method" => "\"QR\"".into(),
        "period" | "reportPeriod" | "report_period" => "\"[2025-01-01,2025-12-31)\"".into(),
        "capacity" | "maxAttendees" | "max_attendees" => "50".into(),
        "did" => "\"did:web:test.community.os\"".into(),
        "atUri" | "at_uri" => "\"at://did:web:test.community.os/test\"".into(),
        "pronouns" => "\"they_them\"".into(),
        "locale" => "\"en_NZ\"".into(),
        _ if name.contains("accessibility") && name.contains("pref") => "[\"screen_reader\"]".into(),
        _ if name.contains("interest") => "[\"advocacy\"]".into(),
        _ if name.contains("skill") => "[\"communication\"]".into(),
        _ if name.contains("volunteer") && name.contains("interest") => "[\"event_support\"]".into(),
        _ if name.contains("support") && name.contains("need") => "[\"mobility\"]".into(),
        _ if name.contains("consent") => "[\"newsletter\"]".into(),
        _ if name.contains("contact") && name.contains("method") => "[]".into(),
        _ if name.contains("data") && name.contains("class") => "\"internal\"".into(),
        _ if name.contains("document") || (name.contains("alternate") && name.contains("id")) => {
            "\"test-doc-001\"".into()
        }
        _ if name == "targetType" || name == "target_type" => "\"Person\"".into(),
        _ if name == "trustLevel"
            || name == "trust_level"
            || rust_type.contains("TrustLevel") =>
        {
            "\"Medium\"".into()
        }
        _ if rust_type.contains("RelationshipTypeCodeList")
            || codelist_target.is_some_and(|t| t.contains("RelationshipType")) =>
        {
            "\"Invited\"".into()
        }
        _ if name == "theme" || rust_type.contains("ConsultationTheme") => "\"Accessibility\"".into(),
        _ if name == "subjectDid"
            || name == "subject_did"
            || name == "delegateDid"
            || name == "delegate_did"
            || name == "granteeDid"
            || name == "grantee_did"
            || name == "workerDid"
            || name == "worker_did"
            || name == "coordinatorDid"
            || name == "coordinator_did"
            || name == "organizationDid"
            || name == "organization_did"
            || name == "ownerDid"
            || name == "owner_did" =>
        {
            "\"did:plc:test.community.os\"".into()
        }
        _ if rust_type.contains("ConsentConsentGrantPermission")
            || codelist_target.is_some_and(|t| t.contains("ConsentConsentGrantPermission")) =>
        {
            "\"read\"".into()
        }
        _ if rust_type.contains("ConsentConsentGrantStatus")
            || rust_type.contains("DelegationDelegationStatus")
            || rust_type.contains("SupportArrangementStatus")
            || codelist_target.is_some_and(|t| t.contains("ConsentConsentGrantStatus"))
            || codelist_target.is_some_and(|t| t.contains("DelegationDelegationStatus"))
            || codelist_target.is_some_and(|t| t.contains("SupportArrangementStatus")) =>
        {
            "\"active\"".into()
        }
        _ if rust_type.contains("RsvpStatus")
            || rust_type.contains("EventAttendanceStatus")
            || codelist_target.is_some_and(|t| t.contains("RsvpStatus"))
            || codelist_target.is_some_and(|t| t.contains("EventAttendanceStatus")) =>
        {
            "\"Confirmed\"".into()
        }
        _ if codelist_target.is_some_and(|t| t.contains("NotificationChannel")) => {
            "\"Email\"".into()
        }
        _ if codelist_target.is_some_and(|t| t.contains("AdvocacyStatus")) => "\"Opened\"".into(),
        _ => {
            if rust_type.contains("Vec<") {
                "[]".into()
            } else if rust_type.contains("Range") {
                "\"[2025-01-01,2025-12-31)\"".into()
            } else if rust_type.contains("NaiveDate") {
                "\"2025-01-15\"".into()
            } else if rust_type.contains("DateTime") {
                "\"2025-01-15T00:00:00Z\"".into()
            } else if rust_type.contains("String") {
                "\"test\"".into()
            } else if rust_type.contains("Uuid") {
                "\"00000000-0000-0000-0000-000000000000\"".into()
            } else if rust_type.contains("i32")
                || rust_type.contains("i64")
                || rust_type.contains("f32")
                || rust_type.contains("f64")
                || rust_type.contains("Decimal")
            {
                "42".into()
            } else if rust_type.contains("bool") {
                "true".into()
            } else {
                "\"test\"".into()
            }
        }
    }
}

/// Convert camelCase/snake_case field name to human-readable label.
fn humanize_field_name(name: &str) -> String {
    let mut result = String::new();
    let mut prev_was_upper = false;
    let mut prev_was_separator = true;

    for c in name.chars() {
        if c == '_' {
            result.push(' ');
            prev_was_separator = true;
            prev_was_upper = false;
        } else if c.is_uppercase() {
            if !prev_was_upper && !prev_was_separator {
                result.push(' ');
            }
            result.push(c);
            prev_was_upper = true;
            prev_was_separator = false;
        } else {
            if prev_was_separator {
                // Capitalize first char after separator
                let mut upper = c.to_uppercase().collect::<Vec<_>>();
                result.extend(upper.drain(..));
            } else {
                result.push(c);
            }
            prev_was_upper = false;
            prev_was_separator = false;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rust_type_simple() {
        let t = parse_rust_type("String", true);
        assert_eq!(t.to_rust_string(), "String");
        assert!(!t.is_optional());
    }

    #[test]
    fn test_parse_rust_type_optional() {
        let t = parse_rust_type("Option<String>", false);
        assert_eq!(t.to_rust_string(), "Option<String>");
        assert!(t.is_optional());
    }

    #[test]
    fn test_parse_rust_type_vec() {
        let t = parse_rust_type("Vec<String>", true);
        assert_eq!(t.to_rust_string(), "Vec<String>");
    }

    #[test]
    fn test_humanize_camel_case() {
        assert_eq!(humanize_field_name("preferredName"), "Preferred Name");
        assert_eq!(humanize_field_name("crmNotes"), "Crm Notes");
        assert_eq!(humanize_field_name("accessibilityPreferences"), "Accessibility Preferences");
    }

    #[test]
    fn test_humanize_snake_case() {
        assert_eq!(humanize_field_name("preferred_name"), "Preferred Name");
        assert_eq!(humanize_field_name("first_name"), "First Name");
    }

    #[test]
    fn test_ts_type_mapping() {
        let t = parse_rust_type("String", true);
        assert_eq!(ts_type_for_field(&t), "string");

        let t = parse_rust_type("Option<String>", false);
        assert_eq!(ts_type_for_field(&t), "string | null");

        let t = parse_rust_type("i32", true);
        assert_eq!(ts_type_for_field(&t), "number");

        let t = parse_rust_type("bool", true);
        assert_eq!(ts_type_for_field(&t), "boolean");
    }
}
