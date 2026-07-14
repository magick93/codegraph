use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::RefClassificationKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LexiconType {
    String { format: Option<LexiconStringFormat> },
    Integer,
    Boolean,
    Bytes { max_size: Option<u64> },
    CidLink,
    Blob { accept: Vec<String>, max_size: Option<u64> },
    Array { items: Box<LexiconType> },
    Object { properties: Vec<(String, LexiconType)> },
    Ref { nsid: String },
    StrongRef { nsid: String },
    Union { refs: Vec<String>, closed: bool },
    Token,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LexiconStringFormat {
    DateTime,
    AtUri,
    Did,
    Handle,
    Nsid,
    LanguageTag,
    Cid,
    Uri,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyStrategy {
    Tid,
    LiteralSelf,
    Any,
    Nsid,
    Composite(String),
}

pub fn lexicon_type_from_ref_classification(kind: &RefClassificationKind) -> LexiconType {
    match kind {
        RefClassificationKind::PrimitiveWrapper => LexiconType::String { format: None },
        RefClassificationKind::ArrayWrapper => LexiconType::Array {
            items: Box::new(LexiconType::Unknown),
        },
        RefClassificationKind::RangeWrapper => LexiconType::Unknown,
        RefClassificationKind::CodelistReference => LexiconType::String { format: None },
        RefClassificationKind::CodelistCheck => LexiconType::String { format: None },
        RefClassificationKind::InlineEnum => LexiconType::String { format: None },
        RefClassificationKind::EntityReference => LexiconType::Ref {
            nsid: String::new(),
        },
        RefClassificationKind::ValueObject => LexiconType::Object {
            properties: vec![],
        },
        RefClassificationKind::CompositeWrapper => LexiconType::Object {
            properties: vec![],
        },
        RefClassificationKind::StructuredWrapper => LexiconType::Object {
            properties: vec![],
        },
        RefClassificationKind::MediaWrapper => LexiconType::Blob {
            accept: vec![],
            max_size: None,
        },
    }
}

impl fmt::Display for LexiconType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LexiconType::String { format } => match format {
                Some(fmt) => write!(f, "string({})", fmt),
                None => write!(f, "string"),
            },
            LexiconType::Integer => write!(f, "integer"),
            LexiconType::Boolean => write!(f, "boolean"),
            LexiconType::Bytes { max_size } => match max_size {
                Some(n) => write!(f, "bytes({n})"),
                None => write!(f, "bytes"),
            },
            LexiconType::CidLink => write!(f, "cid-link"),
            LexiconType::Blob { accept, max_size } => {
                write!(f, "blob")?;
                if !accept.is_empty() {
                    write!(f, "({})", accept.join(","))?;
                }
                if let Some(n) = max_size {
                    write!(f, "[{n}]")?;
                }
                Ok(())
            }
            LexiconType::Array { items } => write!(f, "array({items})"),
            LexiconType::Object { properties } => {
                write!(f, "object")?;
                if !properties.is_empty() {
                    write!(f, "(")?;
                    for (i, (name, ty)) in properties.iter().enumerate() {
                        if i > 0 {
                            write!(f, ",")?;
                        }
                        write!(f, "{name}:{ty}")?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            LexiconType::Ref { nsid } => write!(f, "ref({nsid})"),
            LexiconType::StrongRef { nsid } => write!(f, "strongref({nsid})"),
            LexiconType::Union { refs, closed } => {
                let kind = if *closed { "closed-union" } else { "union" };
                write!(f, "{kind}({})", refs.join(","))
            }
            LexiconType::Token => write!(f, "token"),
            LexiconType::Unknown => write!(f, "unknown"),
        }
    }
}

impl fmt::Display for LexiconStringFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LexiconStringFormat::DateTime => write!(f, "datetime"),
            LexiconStringFormat::AtUri => write!(f, "at-uri"),
            LexiconStringFormat::Did => write!(f, "did"),
            LexiconStringFormat::Handle => write!(f, "handle"),
            LexiconStringFormat::Nsid => write!(f, "nsid"),
            LexiconStringFormat::LanguageTag => write!(f, "language"),
            LexiconStringFormat::Cid => write!(f, "cid"),
            LexiconStringFormat::Uri => write!(f, "uri"),
        }
    }
}

impl fmt::Display for KeyStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyStrategy::Tid => write!(f, "tid"),
            KeyStrategy::LiteralSelf => write!(f, "#self"),
            KeyStrategy::Any => write!(f, "*"),
            KeyStrategy::Nsid => write!(f, "nsid"),
            KeyStrategy::Composite(pat) => write!(f, "composite({pat})"),
        }
    }
}

impl FromStr for KeyStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tid" => Ok(KeyStrategy::Tid),
            "#self" => Ok(KeyStrategy::LiteralSelf),
            "*" => Ok(KeyStrategy::Any),
            "nsid" => Ok(KeyStrategy::Nsid),
            other => {
                if let Some(inner) = other
                    .strip_prefix("composite(")
                    .and_then(|s| s.strip_suffix(')'))
                {
                    Ok(KeyStrategy::Composite(inner.to_owned()))
                } else {
                    Err(format!("unknown key strategy: {other}"))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexicon_type_string_variant() {
        let t = LexiconType::String { format: None };
        assert_eq!(t, LexiconType::String { format: None });
        let t_fmt = LexiconType::String {
            format: Some(LexiconStringFormat::DateTime),
        };
        assert_ne!(t, t_fmt);
    }

    #[test]
    fn test_lexicon_type_integer_variant() {
        assert_eq!(LexiconType::Integer, LexiconType::Integer);
    }

    #[test]
    fn test_lexicon_type_boolean_variant() {
        assert_eq!(LexiconType::Boolean, LexiconType::Boolean);
    }

    #[test]
    fn test_lexicon_type_bytes_variant() {
        assert_eq!(
            LexiconType::Bytes { max_size: None },
            LexiconType::Bytes { max_size: None }
        );
        assert_eq!(
            LexiconType::Bytes {
                max_size: Some(1024)
            },
            LexiconType::Bytes {
                max_size: Some(1024)
            }
        );
    }

    #[test]
    fn test_lexicon_type_cid_link_variant() {
        assert_eq!(LexiconType::CidLink, LexiconType::CidLink);
    }

    #[test]
    fn test_lexicon_type_blob_variant() {
        assert_eq!(
            LexiconType::Blob {
                accept: vec![],
                max_size: None
            },
            LexiconType::Blob {
                accept: vec![],
                max_size: None
            }
        );
        assert_eq!(
            LexiconType::Blob {
                accept: vec!["image/png".into(), "image/jpeg".into()],
                max_size: Some(1_000_000)
            },
            LexiconType::Blob {
                accept: vec!["image/png".into(), "image/jpeg".into()],
                max_size: Some(1_000_000)
            }
        );
    }

    #[test]
    fn test_lexicon_type_array_variant() {
        assert_eq!(
            LexiconType::Array {
                items: Box::new(LexiconType::String { format: None })
            },
            LexiconType::Array {
                items: Box::new(LexiconType::String { format: None })
            }
        );
    }

    #[test]
    fn test_lexicon_type_object_variant() {
        assert_eq!(
            LexiconType::Object {
                properties: vec![]
            },
            LexiconType::Object {
                properties: vec![]
            }
        );
        assert_eq!(
            LexiconType::Object {
                properties: vec![(
                    "name".into(),
                    LexiconType::String { format: None }
                )]
            },
            LexiconType::Object {
                properties: vec![(
                    "name".into(),
                    LexiconType::String { format: None }
                )]
            }
        );
    }

    #[test]
    fn test_lexicon_type_ref_variant() {
        assert_eq!(
            LexiconType::Ref {
                nsid: "app.bsky.feed.post".into()
            },
            LexiconType::Ref {
                nsid: "app.bsky.feed.post".into()
            }
        );
    }

    #[test]
    fn test_lexicon_type_strong_ref_variant() {
        assert_eq!(
            LexiconType::StrongRef {
                nsid: "app.bsky.feed.post".into()
            },
            LexiconType::StrongRef {
                nsid: "app.bsky.feed.post".into()
            }
        );
    }

    #[test]
    fn test_lexicon_type_union_variant() {
        assert_eq!(
            LexiconType::Union {
                refs: vec![],
                closed: true
            },
            LexiconType::Union {
                refs: vec![],
                closed: true
            }
        );
        assert_eq!(
            LexiconType::Union {
                refs: vec!["a.b.c".into(), "x.y.z".into()],
                closed: false
            },
            LexiconType::Union {
                refs: vec!["a.b.c".into(), "x.y.z".into()],
                closed: false
            }
        );
    }

    #[test]
    fn test_lexicon_type_token_variant() {
        assert_eq!(LexiconType::Token, LexiconType::Token);
    }

    #[test]
    fn test_lexicon_type_unknown_variant() {
        assert_eq!(LexiconType::Unknown, LexiconType::Unknown);
    }

    #[test]
    fn test_key_strategy_tid() {
        assert_eq!(KeyStrategy::Tid, KeyStrategy::Tid);
    }

    #[test]
    fn test_key_strategy_literal_self() {
        assert_eq!(KeyStrategy::LiteralSelf, KeyStrategy::LiteralSelf);
    }

    #[test]
    fn test_key_strategy_any() {
        assert_eq!(KeyStrategy::Any, KeyStrategy::Any);
    }

    #[test]
    fn test_key_strategy_nsid() {
        assert_eq!(KeyStrategy::Nsid, KeyStrategy::Nsid);
    }

    #[test]
    fn test_key_strategy_composite() {
        assert_eq!(
            KeyStrategy::Composite("entity_field".into()),
            KeyStrategy::Composite("entity_field".into())
        );
    }

    #[test]
    fn test_key_strategy_display() {
        assert_eq!(KeyStrategy::Tid.to_string(), "tid");
        assert_eq!(KeyStrategy::LiteralSelf.to_string(), "#self");
        assert_eq!(KeyStrategy::Any.to_string(), "*");
        assert_eq!(KeyStrategy::Nsid.to_string(), "nsid");
        assert_eq!(
            KeyStrategy::Composite("entity_field".into()).to_string(),
            "composite(entity_field)"
        );
    }

    #[test]
    fn test_key_strategy_from_str() {
        assert_eq!("tid".parse(), Ok(KeyStrategy::Tid));
        assert_eq!("#self".parse(), Ok(KeyStrategy::LiteralSelf));
        assert_eq!("*".parse(), Ok(KeyStrategy::Any));
        assert_eq!("nsid".parse(), Ok(KeyStrategy::Nsid));
        assert_eq!(
            "composite(entity_field)".parse(),
            Ok(KeyStrategy::Composite("entity_field".into()))
        );
    }

    #[test]
    fn test_key_strategy_from_str_invalid() {
        assert!("not_a_key".parse::<KeyStrategy>().is_err());
    }

    #[test]
    fn test_key_strategy_round_trip() {
        let cases = vec![
            KeyStrategy::Tid,
            KeyStrategy::LiteralSelf,
            KeyStrategy::Any,
            KeyStrategy::Nsid,
            KeyStrategy::Composite("entity_field".into()),
        ];
        for ks in cases {
            let s = ks.to_string();
            let parsed: KeyStrategy = s.parse().expect("round-trip failed");
            assert_eq!(ks, parsed, "round-trip mismatch for {s}");
        }
    }

    #[test]
    fn test_canonical_mapping_primitive_wrapper() {
        assert_eq!(
            lexicon_type_from_ref_classification(&RefClassificationKind::PrimitiveWrapper),
            LexiconType::String { format: None }
        );
    }

    #[test]
    fn test_canonical_mapping_array_wrapper() {
        assert_eq!(
            lexicon_type_from_ref_classification(&RefClassificationKind::ArrayWrapper),
            LexiconType::Array {
                items: Box::new(LexiconType::Unknown)
            }
        );
    }

    #[test]
    fn test_canonical_mapping_range_wrapper() {
        assert_eq!(
            lexicon_type_from_ref_classification(&RefClassificationKind::RangeWrapper),
            LexiconType::Unknown
        );
    }

    #[test]
    fn test_canonical_mapping_codelist_reference() {
        assert_eq!(
            lexicon_type_from_ref_classification(&RefClassificationKind::CodelistReference),
            LexiconType::String { format: None }
        );
    }

    #[test]
    fn test_canonical_mapping_codelist_check() {
        assert_eq!(
            lexicon_type_from_ref_classification(&RefClassificationKind::CodelistCheck),
            LexiconType::String { format: None }
        );
    }

    #[test]
    fn test_canonical_mapping_inline_enum() {
        assert_eq!(
            lexicon_type_from_ref_classification(&RefClassificationKind::InlineEnum),
            LexiconType::String { format: None }
        );
    }

    #[test]
    fn test_canonical_mapping_entity_reference() {
        assert_eq!(
            lexicon_type_from_ref_classification(&RefClassificationKind::EntityReference),
            LexiconType::Ref {
                nsid: String::new()
            }
        );
    }

    #[test]
    fn test_canonical_mapping_value_object() {
        assert_eq!(
            lexicon_type_from_ref_classification(&RefClassificationKind::ValueObject),
            LexiconType::Object {
                properties: vec![]
            }
        );
    }

    #[test]
    fn test_canonical_mapping_composite_wrapper() {
        assert_eq!(
            lexicon_type_from_ref_classification(&RefClassificationKind::CompositeWrapper),
            LexiconType::Object {
                properties: vec![]
            }
        );
    }

    #[test]
    fn test_canonical_mapping_structured_wrapper() {
        assert_eq!(
            lexicon_type_from_ref_classification(&RefClassificationKind::StructuredWrapper),
            LexiconType::Object {
                properties: vec![]
            }
        );
    }

    #[test]
    fn test_canonical_mapping_media_wrapper() {
        assert_eq!(
            lexicon_type_from_ref_classification(&RefClassificationKind::MediaWrapper),
            LexiconType::Blob {
                accept: vec![],
                max_size: None
            }
        );
    }

    #[test]
    fn test_lexicon_string_format_display() {
        assert_eq!(LexiconStringFormat::DateTime.to_string(), "datetime");
        assert_eq!(LexiconStringFormat::AtUri.to_string(), "at-uri");
        assert_eq!(LexiconStringFormat::Did.to_string(), "did");
        assert_eq!(LexiconStringFormat::Handle.to_string(), "handle");
        assert_eq!(LexiconStringFormat::Nsid.to_string(), "nsid");
        assert_eq!(LexiconStringFormat::LanguageTag.to_string(), "language");
        assert_eq!(LexiconStringFormat::Cid.to_string(), "cid");
        assert_eq!(LexiconStringFormat::Uri.to_string(), "uri");
    }

    #[test]
    fn test_lexicon_type_display_primitives() {
        assert_eq!(LexiconType::Integer.to_string(), "integer");
        assert_eq!(LexiconType::Boolean.to_string(), "boolean");
        assert_eq!(LexiconType::CidLink.to_string(), "cid-link");
        assert_eq!(LexiconType::Token.to_string(), "token");
        assert_eq!(LexiconType::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_lexicon_type_display_string_formats() {
        assert_eq!(
            LexiconType::String { format: None }.to_string(),
            "string"
        );
        assert_eq!(
            LexiconType::String {
                format: Some(LexiconStringFormat::DateTime)
            }
            .to_string(),
            "string(datetime)"
        );
    }

    #[test]
    fn test_lexicon_type_display_bytes() {
        assert_eq!(LexiconType::Bytes { max_size: None }.to_string(), "bytes");
        assert_eq!(
            LexiconType::Bytes {
                max_size: Some(1024)
            }
            .to_string(),
            "bytes(1024)"
        );
    }

    #[test]
    fn test_lexicon_type_display_ref_strongref() {
        assert_eq!(
            LexiconType::Ref {
                nsid: "app.bsky.feed.post".into()
            }
            .to_string(),
            "ref(app.bsky.feed.post)"
        );
        assert_eq!(
            LexiconType::StrongRef {
                nsid: "app.bsky.feed.post".into()
            }
            .to_string(),
            "strongref(app.bsky.feed.post)"
        );
    }

    #[test]
    fn test_lexicon_type_display_union() {
        assert_eq!(
            LexiconType::Union {
                refs: vec!["a.b".into()],
                closed: false
            }
            .to_string(),
            "union(a.b)"
        );
        assert_eq!(
            LexiconType::Union {
                refs: vec!["a.b".into()],
                closed: true
            }
            .to_string(),
            "closed-union(a.b)"
        );
    }

    #[test]
    fn test_lexicon_type_display_blob() {
        assert_eq!(
            LexiconType::Blob {
                accept: vec!["image/png".into()],
                max_size: None
            }
            .to_string(),
            "blob(image/png)"
        );
        assert_eq!(
            LexiconType::Blob {
                accept: vec!["image/png".into(), "image/jpeg".into()],
                max_size: Some(1_000_000)
            }
            .to_string(),
            "blob(image/png,image/jpeg)[1000000]"
        );
    }

    #[test]
    fn test_lexicon_type_display_array() {
        assert_eq!(
            LexiconType::Array {
                items: Box::new(LexiconType::Integer)
            }
            .to_string(),
            "array(integer)"
        );
    }

    #[test]
    fn test_lexicon_type_display_object() {
        assert_eq!(
            LexiconType::Object {
                properties: vec![]
            }
            .to_string(),
            "object"
        );
        assert_eq!(
            LexiconType::Object {
                properties: vec![(
                    "name".into(),
                    LexiconType::String { format: None }
                )]
            }
            .to_string(),
            "object(name:string)"
        );
    }

    #[test]
    fn test_lexicon_type_serde_round_trip() {
        let cases: Vec<LexiconType> = vec![
            LexiconType::String { format: None },
            LexiconType::String {
                format: Some(LexiconStringFormat::DateTime),
            },
            LexiconType::Integer,
            LexiconType::Boolean,
            LexiconType::Bytes { max_size: None },
            LexiconType::Bytes {
                max_size: Some(1024),
            },
            LexiconType::CidLink,
            LexiconType::Blob {
                accept: vec!["image/png".into()],
                max_size: Some(1_000_000),
            },
            LexiconType::Array {
                items: Box::new(LexiconType::Integer),
            },
            LexiconType::Object {
                properties: vec![("name".into(), LexiconType::String { format: None })],
            },
            LexiconType::Ref {
                nsid: "app.bsky.feed.post".into(),
            },
            LexiconType::StrongRef {
                nsid: "app.bsky.feed.post".into(),
            },
            LexiconType::Union {
                refs: vec!["a.b".into()],
                closed: true,
            },
            LexiconType::Token,
            LexiconType::Unknown,
        ];
        for t in cases {
            let json = serde_json::to_string(&t).expect("serialize failed");
            let back: LexiconType = serde_json::from_str(&json).expect("deserialize failed");
            assert_eq!(t, back, "serde round-trip failed for {t}");
        }
    }

    #[test]
    fn test_key_strategy_serde_round_trip() {
        let cases = vec![
            KeyStrategy::Tid,
            KeyStrategy::LiteralSelf,
            KeyStrategy::Any,
            KeyStrategy::Nsid,
            KeyStrategy::Composite("entity_field".into()),
        ];
        for ks in cases {
            let json = serde_json::to_string(&ks).expect("serialize failed");
            let back: KeyStrategy = serde_json::from_str(&json).expect("deserialize failed");
            assert_eq!(ks, back, "serde round-trip failed for {ks}");
        }
    }
}
