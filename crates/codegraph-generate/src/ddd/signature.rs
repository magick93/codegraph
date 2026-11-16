//! Shared DDD signature-slot resolution (DD3, pherolang 0.3).
//!
//! A design operation's return and parameter types are `phero_ir::ddd::DddTypeRef`,
//! which serializes **untagged**: the domain form is exactly
//! `phero_ir::TypeRef` (`{"type":"class","value":{...}}`), and a DTO boundary is
//! `{"dto":"<BoundaryName>"}`. codegraph's graph mirror stores those payloads as
//! pre-serialized JSON (codegraph-core may not depend on `phero-ir`), so every
//! consumer that needs a *domain* type has to classify the slot first.
//!
//! This module is that one classifier — `service_ops`, `design`'s finder
//! lowering, and the doctor's entity-binding predicate all route through it so
//! the interpretation cannot drift.

/// A resolved DDD signature slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DddSignatureType {
    /// A resolved domain type (class, enum, datatype, vocabulary, primitive).
    Domain(phero_ir::TypeRef),
    /// A modeled DTO boundary (DD3) — a transport shape, not a domain class.
    Dto(String),
    /// The payload was not a recognizable signature slot (a hand-built or
    /// stale graph node). The string carries the reason for diagnostics.
    Unresolvable(String),
}

impl DddSignatureType {
    /// The domain type, or `None` for a DTO boundary / an unresolvable slot.
    pub fn as_domain(&self) -> Option<&phero_ir::TypeRef> {
        match self {
            Self::Domain(type_ref) => Some(type_ref),
            Self::Dto(_) | Self::Unresolvable(_) => None,
        }
    }

    /// The DTO boundary name, or `None` for a domain type / an unresolvable slot.
    pub fn dto_name(&self) -> Option<&str> {
        match self {
            Self::Dto(name) => Some(name),
            Self::Domain(_) | Self::Unresolvable(_) => None,
        }
    }

    /// The class name when this slot is a domain class reference
    /// (package-qualified exactly as the artifact spells it), else `None`.
    pub fn class_name(&self) -> Option<&str> {
        match self.as_domain() {
            Some(phero_ir::TypeRef::Class { name, .. }) => Some(name),
            _ => None,
        }
    }

    /// A short, user-facing description for a diagnostic.
    pub fn describe(&self) -> String {
        match self {
            Self::Domain(type_ref) => type_ref
                .qualified_name()
                .unwrap_or_else(|| format!("{type_ref:?}")),
            Self::Dto(name) => format!("DTO boundary `{name}`"),
            Self::Unresolvable(reason) => format!("unresolvable signature type ({reason})"),
        }
    }
}

/// Classify one serialized DDD signature slot.
pub fn signature_type(value: &serde_json::Value) -> DddSignatureType {
    // A DTO boundary is `{"dto":"Name"}` — the only object shape that is not
    // the adjacently-tagged domain `TypeRef`.
    if let Some(name) = value.get("dto").and_then(serde_json::Value::as_str) {
        return DddSignatureType::Dto(name.to_string());
    }
    match serde_json::from_value::<phero_ir::TypeRef>(value.clone()) {
        Ok(type_ref) => DddSignatureType::Domain(type_ref),
        Err(error) => DddSignatureType::Unresolvable(error.to_string()),
    }
}

/// Classify an optional slot; `None` (an absent type) yields `None`.
pub fn signature_type_opt(value: Option<&serde_json::Value>) -> Option<DddSignatureType> {
    value.map(signature_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use phero_ir::{PrimitiveType, TypeRef};

    #[test]
    fn domain_class_slot_classifies_as_a_domain_type() {
        let json = serde_json::to_value(TypeRef::Class {
            package: "nz.example.library".to_string(),
            name: "Book".to_string(),
        })
        .unwrap();
        let slot = signature_type(&json);
        assert_eq!(slot.class_name(), Some("Book"));
        assert!(slot.dto_name().is_none());
    }

    #[test]
    fn dto_boundary_classifies_as_a_dto_and_never_a_domain_type() {
        let json = serde_json::json!({ "dto": "BookSummary" });
        let slot = signature_type(&json);
        assert_eq!(slot.dto_name(), Some("BookSummary"));
        assert!(slot.as_domain().is_none());
        assert!(slot.class_name().is_none());
        assert_eq!(slot.describe(), "DTO boundary `BookSummary`");
    }

    #[test]
    fn primitive_slot_classifies_as_a_domain_type() {
        let json = serde_json::to_value(TypeRef::Primitive(PrimitiveType::String)).unwrap();
        let slot = signature_type(&json);
        assert!(matches!(
            slot.as_domain(),
            Some(TypeRef::Primitive(PrimitiveType::String))
        ));
        assert!(slot.class_name().is_none());
    }

    #[test]
    fn garbage_slot_is_unresolvable_not_a_panic() {
        let slot = signature_type(&serde_json::json!({ "nonsense": true }));
        assert!(matches!(slot, DddSignatureType::Unresolvable(_)));
    }
}
