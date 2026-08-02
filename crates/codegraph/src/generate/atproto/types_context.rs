use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::codelist_enum_name_from_ref;
use codegraph_core::types::{LexiconNode, PropertyNode, SchemaNode};
use codegraph_type_contracts::RefClassificationKind;
use serde::Serialize;

use crate::error::Result;
use crate::generate::ProjectConfig;

#[derive(Debug, Serialize)]
pub struct TypesContext {
    pub struct_name: String,
    pub description: Option<String>,
    pub nsid: String,
    pub is_record: bool,
    pub fields: Vec<RustFieldContext>,
    pub enum_defs: Vec<EnumDefContext>,
    pub needs_blob_ref: bool,
    pub needs_generated_types_import: bool,
    pub needs_atproto_syntax_imports: bool,
}

#[derive(Debug, Serialize)]
pub struct RustFieldContext {
    pub field_name: String,
    pub rust_type: String,
    pub serde_attr: Option<String>,
    pub doc: Option<String>,
    pub is_option: bool,
    pub has_serde_with: bool,
    pub serde_with: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EnumDefContext {
    pub enum_name: String,
    pub description: Option<String>,
    pub nsid: String,
    pub variants: Vec<EnumVariantContext>,
}

#[derive(Debug, Serialize)]
pub struct EnumVariantContext {
    pub name: String,
    pub rename: Option<String>,
    pub display: String,
}

impl TypesContext {
    pub async fn build(
        db: &dyn GraphQuerier,
        lexicon: &LexiconNode,
        schema: &SchemaNode,
        properties: &[PropertyNode],
        _project: &ProjectConfig,
    ) -> Result<Self> {
        let is_record = lexicon.lex_type == "record";
        let struct_name = if is_record {
            format!("{}Record", schema.rust_type_name)
        } else {
            schema.rust_type_name.clone()
        };

        let mut fields = Vec::new();
        let mut enum_defs = Vec::new();
        let mut needs_blob_ref = false;
        let mut needs_atproto_syntax_imports = false;
        let mut seen_enum_names = std::collections::HashSet::new();
        let mut seen_field_names = std::collections::HashSet::new();
        // Use String (not rsky_syntax types) for AT Protocol identifier fields.
        // rsky_syntax types lack Serialize/Deserialize, which breaks generated structs.
        // The semantic type is preserved via docs and the `needs_atproto_syntax_imports` flag.
        let atproto_field_types: std::collections::HashMap<&str, &str> = [
            ("at_uri", "String"),    // rsky_syntax::aturi::AtUri
            ("did", "String"),       // cosmos_domain_types::identifiers::Did
            ("collection", "String"), // rsky_syntax::nsid::Nsid
            ("rkey", "String"),      // cosmos_domain_types::identifiers::RecordKey
        ]
        .iter()
        .cloned()
        .collect();

        for prop in properties {
            let field_name = prop.rust_field_name.clone();
            if !seen_field_names.insert(field_name.clone()) {
                continue;
            }

            let (rust_type, serde_with) = if let Some(atproto_type) =
                atproto_field_types.get(field_name.as_str())
            {
                (atproto_type.to_string(), None)
            } else {
                let kind = prop.effective_kind();
                field_rust_type(
                    &kind,
                    prop,
                    db,
                    &mut needs_blob_ref,
                    &mut seen_enum_names,
                    &mut enum_defs,
                )
                .await
            };

            let is_option = !prop.is_required;

            fields.push(RustFieldContext {
                field_name,
                rust_type,
                serde_attr: None,
                doc: prop.description.clone(),
                is_option,
                has_serde_with: serde_with.is_some(),
                serde_with,
            });
        }

        let needs_generated_types_import = fields
            .iter()
            .any(|f| f.rust_type == "serde_json::Value");

        Ok(Self {
            struct_name,
            description: schema.description.clone(),
            nsid: lexicon.nsid.clone(),
            is_record,
            fields,
            enum_defs,
            needs_blob_ref,
            needs_generated_types_import,
            needs_atproto_syntax_imports,
        })
    }
}

async fn field_rust_type(
    kind: &Option<RefClassificationKind>,
    prop: &PropertyNode,
    db: &dyn GraphQuerier,
    needs_blob_ref: &mut bool,
    seen_enum_names: &mut std::collections::HashSet<String>,
    enum_defs: &mut Vec<EnumDefContext>,
) -> (String, Option<String>) {
    let kind = match kind {
        Some(k) => k.clone(),
        None => {
            return (
                rust_type_from_prop_builtin(prop),
                serde_with_for_prop(prop),
            );
        }
    };

    match kind {
        RefClassificationKind::PrimitiveWrapper => {
            (rust_type_from_prop_builtin(prop), serde_with_for_prop(prop))
        }
        RefClassificationKind::EntityReference => {
            ("String".to_string(), None)
        }
        RefClassificationKind::InlineEnum | RefClassificationKind::CodelistReference
        | RefClassificationKind::CodelistCheck => {
            resolve_codelist_enum(prop, &kind, db, seen_enum_names, enum_defs).await
        }
        RefClassificationKind::ValueObject
        | RefClassificationKind::CompositeWrapper
        | RefClassificationKind::StructuredWrapper => {
            ("serde_json::Value".to_string(), None)
        }
        RefClassificationKind::ArrayWrapper => {
            if prop.is_array {
                let inner = rust_type_from_prop_builtin(prop);
                (format!("Vec<{}>", inner), None)
            } else {
                ("Vec<String>".to_string(), None)
            }
        }
        RefClassificationKind::RangeWrapper => {
            ("String".to_string(), None)
        }
        RefClassificationKind::MediaWrapper => {
            *needs_blob_ref = true;
            ("BlobRef".to_string(), None)
        }
    }
}

fn rust_type_from_prop_builtin(prop: &PropertyNode) -> String {
    if let Some(ref fmt) = prop.format {
        if fmt == "date-time" {
            return "chrono::DateTime<chrono::Utc>".to_string();
        }
        if fmt == "byte" || fmt == "bytes" {
            return "Vec<u8>".to_string();
        }
    }
    match prop.prop_type.as_str() {
        "integer" => "i64".to_string(),
        "number" => "i64".to_string(),
        "boolean" => "bool".to_string(),
        _ => "String".to_string(),
    }
}

fn serde_with_for_prop(prop: &PropertyNode) -> Option<String> {
    if prop.format.as_deref() == Some("byte") {
        Some("serde_bytes".to_string())
    } else {
        None
    }
}

async fn resolve_codelist_enum(
    prop: &PropertyNode,
    kind: &RefClassificationKind,
    db: &dyn GraphQuerier,
    seen: &mut std::collections::HashSet<String>,
    defs: &mut Vec<EnumDefContext>,
) -> (String, Option<String>) {
    if let Some(ref target) = prop.ref_target {
        match kind {
            RefClassificationKind::CodelistReference | RefClassificationKind::CodelistCheck => {
                let codelist_name = codelist_enum_name_from_ref(&prop.ref_target)
                    .unwrap_or_else(|| codegraph_naming::to_pascal_case(target));
                let rust_type = format!("cosmos_domain_types::codelist::{}", codelist_name);
                (rust_type, None)
            }
            RefClassificationKind::InlineEnum => {
                let enum_name = codelist_enum_name_from_ref(&prop.ref_target)
                    .unwrap_or_else(|| codegraph_naming::to_pascal_case(target));
                if !seen.contains(&enum_name) {
                    let codelist_query_name = codelist_enum_name_from_ref(&prop.ref_target)
                        .unwrap_or_else(|| target.clone());
                    if let Ok(values) = db.get_enum_values(&codelist_query_name).await {
                        if !values.is_empty() {
                            let variants: Vec<EnumVariantContext> = values
                                .iter()
                                .map(|v| EnumVariantContext {
                                    name: codegraph_naming::to_pascal_case(&v.value),
                                    rename: Some(v.value.clone()),
                                    display: v.display_name.clone().unwrap_or(v.value.clone()),
                                })
                                .collect();
                            seen.insert(enum_name.clone());
                            defs.push(EnumDefContext {
                                enum_name: enum_name.clone(),
                                description: None,
                                nsid: String::new(),
                                variants,
                            });
                        }
                    }
                }
                (enum_name, None)
            }
            _ => (rust_type_from_prop_builtin(prop), serde_with_for_prop(prop)),
        }
    } else {
        ("String".to_string(), None)
    }
}
