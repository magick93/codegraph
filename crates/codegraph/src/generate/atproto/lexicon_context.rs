use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{LexiconNode, PropertyNode, SchemaNode};
use codegraph_type_contracts::RefClassificationKind;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::generate::ProjectConfig;

#[derive(Debug, Serialize)]
pub struct LexiconContext {
    pub lexicon: LexiconMeta,
    pub namespace: NamespaceMeta,
    pub record: ObjectContext,
    pub defs: Vec<ObjectContext>,
}

#[derive(Debug, Serialize)]
pub struct LexiconMeta {
    pub nsid: String,
    pub lex_type: String,
    pub key_strategy: String,
    pub revision: i64,
    pub description: String,
    pub domain: String,
}

#[derive(Debug, Serialize)]
pub struct NamespaceMeta {
    pub authority: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct PropertyContext {
    pub name: String,
    pub r#type: LexiconType,
    pub is_required: bool,
    pub description: String,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum LexiconType {
    #[serde(rename = "string")]
    String,
    #[serde(rename = "integer")]
    Integer,
    #[serde(rename = "number")]
    Number,
    #[serde(rename = "boolean")]
    Boolean,
    #[serde(rename = "ref")]
    Ref { ref_name: String },
    #[serde(rename = "union")]
    Union { refs: Vec<String> },
    #[serde(rename = "array")]
    Array { items: Box<LexiconType> },
    #[serde(rename = "bytes")]
    Bytes,
    #[serde(rename = "datetime")]
    DateTime,
    #[serde(rename = "uri")]
    Uri,
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "object")]
    Object { def_name: String },
}

#[derive(Debug, Serialize)]
pub struct ObjectContext {
    pub name: String,
    pub description: String,
    pub properties: Vec<PropertyContext>,
    pub required_fields: Vec<String>,
}

impl LexiconContext {
    pub async fn build(
        db: &dyn GraphQuerier,
        lexicon: &LexiconNode,
        schema: &SchemaNode,
        properties: &[PropertyNode],
        project: &ProjectConfig,
    ) -> Result<Self> {
        let authority = &project.atproto_authority;

        let namespaces = db.get_namespaces().await.map_err(|e| {
            crate::error::Error::Config(format!("Failed to get namespaces: {}", e))
        })?;
        let namespace = namespaces
            .iter()
            .find(|n| n.authority == *authority)
            .map(|n| NamespaceMeta {
                authority: n.authority.clone(),
            })
            .unwrap_or_else(|| NamespaceMeta {
                authority: authority.clone(),
            });

        let mut property_contexts = Vec::new();
        let mut required_fields = Vec::new();

        for prop in properties {
            let kind = prop.effective_kind();
            let lexicon_type = lexicon_type_from_ref_classification(&kind, prop, db).await;

            if prop.is_required {
                required_fields.push(prop.name.clone());
            }

            property_contexts.push(PropertyContext {
                name: prop.name.clone(),
                r#type: lexicon_type,
                is_required: prop.is_required,
                description: prop.description.clone().unwrap_or_default(),
            });
        }

        let record = ObjectContext {
            name: "main".to_string(),
            description: schema.description.clone().unwrap_or_default(),
            properties: property_contexts,
            required_fields,
        };

        let revision = lexicon.revision.unwrap_or(1);

        Ok(Self {
            lexicon: LexiconMeta {
                nsid: lexicon.nsid.clone(),
                lex_type: lexicon.lex_type.clone(),
                key_strategy: lexicon.key_strategy.clone(),
                revision,
                description: lexicon.description.clone().unwrap_or_default(),
                domain: lexicon.domain.clone(),
            },
            namespace,
            record,
            defs: Vec::new(),
        })
    }
}

/// Map a RefClassificationKind to a LexiconType for the Tera template.
async fn lexicon_type_from_ref_classification(
    kind: &Option<RefClassificationKind>,
    prop: &PropertyNode,
    db: &dyn GraphQuerier,
) -> LexiconType {
    match kind {
        Some(RefClassificationKind::PrimitiveWrapper) => match prop.prop_type.as_str() {
            "integer" => LexiconType::Integer,
            "number" => LexiconType::Number,
            "boolean" => LexiconType::Boolean,
            _ => LexiconType::String,
        },
        Some(RefClassificationKind::EntityReference) => {
            if let Some(ref target) = prop.ref_target {
                if let Ok(Some(lex)) = db.get_lexicon_by_schema(target).await {
                    return LexiconType::Ref {
                        ref_name: lex.nsid,
                    };
                }
            }
            LexiconType::String
        }
        Some(RefClassificationKind::InlineEnum) | Some(RefClassificationKind::CodelistReference)
        | Some(RefClassificationKind::CodelistCheck) => LexiconType::String,
        Some(RefClassificationKind::ValueObject) => {
            if let Some(ref target) = prop.ref_target {
                LexiconType::Object {
                    def_name: target.clone(),
                }
            } else {
                LexiconType::Unknown
            }
        }
        Some(RefClassificationKind::ArrayWrapper) => {
            if let Some(ref target) = prop.ref_target {
                if let Ok(Some(lex)) = db.get_lexicon_by_schema(target).await {
                    LexiconType::Array {
                        items: Box::new(LexiconType::Ref {
                            ref_name: lex.nsid,
                        }),
                    }
                } else if target == "String" || target == "string" {
                    LexiconType::Array {
                        items: Box::new(LexiconType::String),
                    }
                } else {
                    LexiconType::Array {
                        items: Box::new(LexiconType::Unknown),
                    }
                }
            } else {
                LexiconType::Array {
                    items: Box::new(LexiconType::String),
                }
            }
        }
        Some(RefClassificationKind::CompositeWrapper)
        | Some(RefClassificationKind::StructuredWrapper) => {
            if let Some(ref target) = prop.ref_target {
                LexiconType::Object {
                    def_name: target.clone(),
                }
            } else {
                LexiconType::Unknown
            }
        }
        Some(RefClassificationKind::RangeWrapper) => LexiconType::String,
        Some(RefClassificationKind::MediaWrapper) => LexiconType::Uri,
        None => match prop.prop_type.as_str() {
            "integer" => LexiconType::Integer,
            "number" => LexiconType::Number,
            "boolean" => LexiconType::Boolean,
            "string" => {
                if prop.format.as_deref() == Some("date-time") {
                    LexiconType::DateTime
                } else if prop.format.as_deref() == Some("uri") {
                    LexiconType::Uri
                } else if prop.format.as_deref() == Some("byte") {
                    LexiconType::Bytes
                } else {
                    LexiconType::String
                }
            }
            _ => LexiconType::String,
        },
    }
}
