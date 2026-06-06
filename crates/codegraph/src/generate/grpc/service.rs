use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PropertyNode;
use codegraph_type_contracts::RefClassificationKind;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use crate::generate::ProjectConfig;
use codegraph_config::DomainConfig;

use super::proto_context::ProtoContext;

#[derive(Debug, Serialize)]
pub struct GrpcServiceContext {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub package: String,
    pub operations: Vec<String>,
    pub has_create: bool,
    pub has_read: bool,
    pub has_update: bool,
    pub has_delete: bool,
    pub has_list: bool,
    pub has_fts: bool,
    pub has_embeddings: bool,
    pub has_workflow: bool,
    pub parent_ref: Option<String>,
    pub hierarchy_field: Option<String>,
    pub repo_trait: String,
    pub proto_service_mod: String,
    pub proto_service_trait: String,
    pub proto_types_prefix: String,
    pub create_fields: Vec<ConvFieldDef>,
    pub update_fields: Vec<ConvFieldDef>,
    pub response_fields: Vec<ConvFieldDef>,
}

#[derive(Debug, Serialize)]
pub struct ConvFieldDef {
    pub name: String,
    pub conversion: String,
}

pub struct GrpcServiceGenerator {
    output_dir: PathBuf,
}

impl GrpcServiceGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for GrpcServiceGenerator {
    fn name(&self) -> &str {
        "grpc_service"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let proto_ctx = ProtoContext::build(db, schema_title, domain, config).await?;

        if proto_ctx.is_empty() {
            return Ok(Vec::new());
        }

        let schema = db
            .get_schema(schema_title)
            .await?
            .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

        let entity_name = proto_ctx.entity_name.clone();
        let module_name = proto_ctx.module_name.clone();
        let properties = db.get_properties(schema_title).await?;

        let entity_cfg = config
            .domains
            .get(domain)
            .and_then(|d| d.get_entity_config(&entity_name));

        let parent_ref = entity_cfg.and_then(|ec| ec.parent_ref.clone());

        let op_set: std::collections::HashSet<String> =
            proto_ctx.operations.iter().cloned().collect();

        // Build field lists with conversion expressions
        let create_fields = build_field_conversions(&properties, &entity_name, db, true, false).await;
        let update_fields = build_field_conversions(&properties, &entity_name, db, false, false).await;
        let response_fields = build_field_conversions(&properties, &entity_name, db, true, true).await;

        let repo_trait = format!("{}Repository", entity_name);
        let proto_service_mod = format!("{}_service_server", module_name);
        let proto_service_trait = format!("{}Service", entity_name);

        let ctx = GrpcServiceContext {
            entity_name,
            module_name,
            package: proto_ctx.package,
            domain: domain.to_string(),
            operations: proto_ctx.operations,
            has_create: op_set.contains("create"),
            has_read: op_set.contains("read"),
            has_update: op_set.contains("update"),
            has_delete: op_set.contains("delete"),
            has_list: op_set.contains("list"),
            has_fts: proto_ctx.has_fts,
            has_embeddings: proto_ctx.has_embeddings,
            has_workflow: proto_ctx.has_workflow,
            parent_ref,
            hierarchy_field: proto_ctx.hierarchy_field,
            repo_trait,
            proto_service_mod,
            proto_service_trait,
            proto_types_prefix: "crate::api::grpc::proto::".to_string(),
            create_fields,
            update_fields,
            response_fields,
        };

        let grpc_dir = self.output_dir.join("src").join("api").join("grpc");

        let mut files = Vec::new();

        // Render server_impl.tera
        let server_content =
            render_template_with_project(tera, "grpc/tonic/server_impl.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: grpc_dir.join(format!("{}_grpc.rs", schema.pg_table_name)),
            content: server_content,
        });

        // Render conversions.tera
        let conv_content =
            render_template_with_project(tera, "grpc/tonic/conversions.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: grpc_dir.join(format!("{}_conversions.rs", schema.pg_table_name)),
            content: conv_content,
        });

        Ok(files)
    }
}

/// Build conversion field definitions for create, update, or response contexts.
async fn build_field_conversions(
    properties: &[PropertyNode],
    entity_name: &str,
    db: &dyn GraphQuerier,
    include_id: bool,
    is_response: bool,
) -> Vec<ConvFieldDef> {
    let mut fields = Vec::new();

    if include_id && !is_response {
        // Create requests include id conditionally
    }

    if is_response {
        // Response conversions are simpler: domain type → proto type
        // Skip synthetic fields (created_at/updated_at handled separately)
        let filtered: Vec<&PropertyNode> = properties
            .iter()
            .filter(|p| p.name != "created_at" && p.name != "updated_at")
            .collect();

        for prop in filtered {
            let conv = response_conversion_expr(prop, db, entity_name);
            fields.push(ConvFieldDef {
                name: prop.rust_field_name.clone(),
                conversion: conv,
            });
        }
    } else {
        // Create and update conversions: proto type → domain type
        let filtered: Vec<&PropertyNode> = properties
            .iter()
            .filter(|p| p.name != "created_at" && p.name != "updated_at")
            .collect();

        for prop in filtered {
            let conv = command_conversion_expr(prop, db, entity_name);
            fields.push(ConvFieldDef {
                name: prop.rust_field_name.clone(),
                conversion: conv,
            });
        }
    }

    fields
}

/// Generate conversion expression for proto → domain command (create/update).
fn command_conversion_expr(
    prop: &PropertyNode,
    _db: &dyn GraphQuerier,
    _entity_name: &str,
) -> String {
    let kind = prop.effective_kind();
    let is_optional = !prop.is_required || prop.is_nullable;

    match kind {
        Some(RefClassificationKind::EntityReference) => {
            if is_optional {
                ".map(|s| uuid::Uuid::parse_str(&s).unwrap_or_default())".to_string()
            } else {
                ".parse::<uuid::Uuid>().unwrap_or_default()".to_string()
            }
        }
        Some(RefClassificationKind::CodelistReference)
        | Some(RefClassificationKind::CodelistCheck)
        | Some(RefClassificationKind::InlineEnum) => {
            // Codelists are strings in proto — no conversion needed
            String::new()
        }
        Some(RefClassificationKind::ValueObject) => {
            // Value objects: use from_proto() — placeholder for now
            ".into()".to_string()
        }
        Some(RefClassificationKind::MediaWrapper)
        | Some(RefClassificationKind::CompositeWrapper) => {
            ".into()".to_string()
        }
        _ => {
            // PrimitiveWrapper — check rust type
            match prop.rust_field_type.as_str() {
                "uuid::Uuid" | "Uuid" => {
                    if is_optional {
                        ".map(|s| uuid::Uuid::parse_str(&s).unwrap_or_default())".to_string()
                    } else {
                        ".parse::<uuid::Uuid>().unwrap_or_default()".to_string()
                    }
                }
                "rust_decimal::Decimal" | "Decimal" => {
                    if is_optional {
                        ".as_ref().map(|s| rust_decimal::Decimal::from_str(s).unwrap_or_default())"
                            .to_string()
                    } else {
                        ".parse::<rust_decimal::Decimal>().unwrap_or_default()".to_string()
                    }
                }
                "chrono::NaiveDate" | "NaiveDate" => {
                    if is_optional {
                        ".map(|ts| ts.into())".to_string()
                    } else {
                        String::new()
                    }
                }
                "chrono::DateTime<chrono::Utc>" => {
                    String::new()
                }
                "serde_json::Value" => String::new(),
                _ => String::new(),
            }
        }
    }
}

/// Generate conversion expression for domain → proto response.
fn response_conversion_expr(
    prop: &PropertyNode,
    _db: &dyn GraphQuerier,
    _entity_name: &str,
) -> String {
    let kind = prop.effective_kind();
    let is_optional = !prop.is_required || prop.is_nullable;

    match kind {
        Some(RefClassificationKind::EntityReference) => {
            if is_optional {
                ".map(|id| id.to_string())".to_string()
            } else {
                ".to_string()".to_string()
            }
        }
        Some(RefClassificationKind::CodelistReference)
        | Some(RefClassificationKind::CodelistCheck)
        | Some(RefClassificationKind::InlineEnum) => {
            ".to_string()".to_string()
        }
        Some(RefClassificationKind::ValueObject)
        | Some(RefClassificationKind::MediaWrapper)
        | Some(RefClassificationKind::CompositeWrapper) => {
            ".into()".to_string()
        }
        _ => {
            match prop.rust_field_type.as_str() {
                "uuid::Uuid" | "Uuid" => ".to_string()".to_string(),
                "rust_decimal::Decimal" | "Decimal" => ".to_string()".to_string(),
                "chrono::NaiveDate" | "NaiveDate" | "chrono::DateTime<chrono::Utc>" => {
                    if is_optional {
                        ".map(|dt| dt.into())".to_string()
                    } else {
                        ".into()".to_string()
                    }
                }
                _ => String::new(),
            }
        }
    }
}
