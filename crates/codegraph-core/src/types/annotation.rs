use serde::{Deserialize, Serialize};

/// A structured, first-class annotation (issue #279, the Morphir
/// annotation shape): a dotted/FQName-style `name` plus typed arguments.
/// Stored beside the legacy ad-hoc `custom_annotations` string map on
/// [`crate::types::SchemaNode`]; both coexist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub name: String,
    #[serde(default)]
    pub arguments: Vec<AnnotationArg>,
}

/// One annotation argument: a `name`/`value` pair or a bare literal.
/// Untagged order matters: the named shape is tried first, since a bare
/// JSON literal would otherwise match the catch-all `Literal` variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnnotationArg {
    Named {
        name: String,
        value: serde_json::Value,
    },
    Literal(serde_json::Value),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotation_serde_round_trip() {
        let a = Annotation {
            name: "acme.doc.deprecated".into(),
            arguments: vec![
                AnnotationArg::Named {
                    name: "since".into(),
                    value: serde_json::json!("2026-01-01"),
                },
                AnnotationArg::Literal(serde_json::json!(2)),
            ],
        };
        let json = serde_json::to_string(&a).unwrap();
        let back: Annotation = serde_json::from_str(&json).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn annotation_defaults_arguments_for_bare_form() {
        let a: Annotation = serde_json::from_str(r#"{"name":"acme.tags.review"}"#).unwrap();
        assert!(a.arguments.is_empty());
    }
}
