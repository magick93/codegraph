use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PropertyNode;
use codegraph_type_contracts::RefClassificationKind;
use serde::Serialize;

use crate::error::Result;
use crate::generate::db::dialect::{db_template_for, dialect_for_target, DatabaseTarget, SqlDialect};
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

use crate::generate::pg_cast_for_type;

#[derive(Debug, Serialize)]
pub struct EntityContext {
    pub module_name: String,
    pub struct_name: String,
    pub table_name: String,
    pub schema_name: String,
    pub columns: Vec<EntityColumn>,
    pub relations: Vec<EntityRelation>,
    /// Import paths for structured JSONB wrapper types used by columns.
    pub structured_imports: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct EntityColumn {
    pub field_name: String,
    pub rust_type: String,
    pub sea_orm_type: String,
    pub column_name: String,
    pub is_primary_key: bool,
    pub is_nullable: bool,
    /// When this column is a PostgreSQL range type, holds the lowercased PG cast
    /// (e.g. `"tstzrange"`) for `column_type = "Custom(…)"` annotation.
    pub pg_cast: Option<String>,
    /// Extra `#[sea_orm(...)]` attribute content for structured JSONB columns
    /// (e.g. `column_type = "JsonBinary"`).
    #[serde(default)]
    pub sea_orm_attr: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EntityRelation {
    pub name: String,
    pub relation_type: String,
    pub related_entity: String,
    pub from_column: String,
    pub to_column: String,
    /// Whether this is a self-referential relation (same entity as from_column)
    #[serde(default)]
    pub is_self_ref: bool,
}

pub struct SeaOrmEntityGenerator {
    output_dir: PathBuf,
    parent_candidates: Vec<codegraph_core::types::ParentCandidate>,
    dialect: Box<dyn SqlDialect>,
}

impl SeaOrmEntityGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            parent_candidates: Vec::new(),
            dialect: dialect_for_target(DatabaseTarget::Postgres),
        }
    }

    pub fn with_dialect(mut self, dialect: Box<dyn SqlDialect>) -> Self {
        self.dialect = dialect;
        self
    }

    pub fn with_parent_candidates(
        mut self,
        candidates: Vec<codegraph_core::types::ParentCandidate>,
    ) -> Self {
        self.parent_candidates = candidates;
        self
    }
}

#[async_trait]
impl EntityGenerator for SeaOrmEntityGenerator {
    fn name(&self) -> &str {
        "sea_orm_entity"
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
        let schema = db
            .get_schema(schema_title)
            .await?
            .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

        let table_name = &schema.pg_table_name;
        let schema_name = domain;
        let rust_type = &schema.rust_type_name;

        if table_name.is_empty() {
            return Ok(Vec::new());
        }

        let all_props = db.get_properties(schema_title).await?;

        // Deduplicate properties by field name — allOf composition can produce
        // duplicate HasProperty edges (parent + child both contribute the same field).
        let props = {
            let mut seen = std::collections::HashSet::new();
            all_props
                .into_iter()
                .filter(|p| seen.insert(p.rust_field_name.clone()))
                .collect::<Vec<_>>()
        };

        // Composite range: collapse start/end fields into a single range column
        let composite_range = db.get_composite_range(schema_title).await.ok().flatten();
        let consumed_fields: std::collections::HashSet<String> = db
            .get_consumed_fields(schema_title)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(prop, _role)| prop.name)
            .collect();

        let mut columns = vec![EntityColumn {
            field_name: "id".to_string(),
            rust_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            column_name: "id".to_string(),
            is_primary_key: true,
            is_nullable: false,
            pg_cast: None,
            sea_orm_attr: None,
        }];

        // Emit the range column if present — uses Custom column type for correct PG casting
        if let Some(ref range) = composite_range {
            let pg_cast = pg_cast_for_type(&range.pg_type);
            columns.push(EntityColumn {
                field_name: range.pg_column_name.clone(),
                rust_type: "Option<String>".to_string(),
                sea_orm_type: "Text".to_string(),
                column_name: range.pg_column_name.clone(),
                is_primary_key: false,
                is_nullable: true,
                pg_cast,
                sea_orm_attr: None,
            });
        }

        // Inject FK column for parent-child relationships detected from the schema graph.
        // This ensures child entities have a `{parent}_id` UUID FK column even when the
        // relationship was detected via ArrayItems (parent's array property) rather than
        // a property on the child schema.
        let entity_cfg = config
            .domains
            .get(domain)
            .and_then(|d| d.get_entity_config(rust_type));
        if let Some(fk_field) = crate::generate::resolve_parent_fk_column(
            schema_title,
            &self.parent_candidates,
            entity_cfg,
            &config.defaults.type_suffix,
        ) {
            columns.push(EntityColumn {
                field_name: fk_field.clone(),
                rust_type: "Option<Uuid>".to_string(),
                sea_orm_type: "Uuid".to_string(),
                column_name: fk_field,
                is_primary_key: false,
                is_nullable: true,
                pg_cast: None,
                sea_orm_attr: None,
            });
        }

        // Inject hierarchy field if configured (self-referential FK for tree/hierarchy support)
        if let Some(hf) = entity_cfg.and_then(|ec| ec.hierarchy_field.clone()) {
            columns.push(EntityColumn {
                field_name: hf.clone(),
                rust_type: "Option<Uuid>".to_string(),
                sea_orm_type: "Uuid".to_string(),
                column_name: hf,
                is_primary_key: false,
                is_nullable: true,
                pg_cast: None,
                sea_orm_attr: None,
            });
        }

        for prop in &props {
            // Skip 'id' — hardcoded above as the UUID primary key
            if prop.rust_field_name == "id" {
                continue;
            }
            // Skip fields consumed by composite ranges
            if consumed_fields.contains(&prop.name) {
                continue;
            }
            match prop.effective_kind() {
                Some(RefClassificationKind::PrimitiveWrapper)
                | Some(RefClassificationKind::StructuredWrapper)
                | Some(RefClassificationKind::ArrayWrapper)
                | Some(RefClassificationKind::RangeWrapper)
                | Some(RefClassificationKind::InlineEnum) => {
                    let is_structured =
                        prop.effective_kind() == Some(RefClassificationKind::StructuredWrapper);
                    let is_nullable = !prop.is_required;
                    // Entity layer uses serde_json::Value for JSONB structured
                    // wrappers — the typed struct is used at the DTO layer only.
                    let base_type = if is_structured {
                        "serde_json::Value".to_string()
                    } else {
                        prop.rust_field_type.clone()
                    };
                    let rust_type = if is_nullable {
                        format!("Option<{base_type}>")
                    } else {
                        base_type
                    };
                    let pg_cast =
                        if prop.effective_kind() == Some(RefClassificationKind::RangeWrapper) {
                            pg_cast_for_type(&prop.pg_column_type)
                        } else {
                            None
                        };
                    let sea_orm_attr = if is_structured {
                        Some(r#"column_type = "JsonBinary""#.to_string())
                    } else {
                        None
                    };

                    columns.push(EntityColumn {
                        field_name: prop.rust_field_name.clone(),
                        rust_type,
                        sea_orm_type: if is_structured {
                            "JsonBinary".to_string()
                        } else {
                            prop.sea_orm_type.clone()
                        },
                        column_name: prop.pg_column_name.clone(),
                        is_primary_key: false,
                        is_nullable,
                        pg_cast,
                        sea_orm_attr,
                    });
                }
                Some(RefClassificationKind::CodelistReference) => {
                    // Array codelist properties are child tables, not columns.
                    if prop.is_array {
                        continue;
                    }
                    // Codelist FK columns — pg_column_name already has _code suffix when
                    // the JSON property ends in "Code" (e.g. workerTypeCode → worker_type_code).
                    // Do NOT append _code again to avoid "worker_type_code_code" double suffix.
                    let is_nullable = !prop.is_required;
                    let rust_type = if is_nullable {
                        "Option<String>".to_string()
                    } else {
                        "String".to_string()
                    };
                    let field_name = prop.rust_field_name.clone();
                    let column_name = prop.pg_column_name.clone();

                    columns.push(EntityColumn {
                        field_name,
                        rust_type,
                        sea_orm_type: "String".to_string(),
                        column_name,
                        is_primary_key: false,
                        is_nullable,
                        pg_cast: None,
                        sea_orm_attr: None,
                    });
                }
                Some(RefClassificationKind::CodelistCheck) => {
                    // Array codelist properties are child tables, not columns.
                    if prop.is_array {
                        continue;
                    }
                    // CodelistCheck uses CHECK constraint, no _code suffix
                    let is_nullable = !prop.is_required;
                    let rust_type = if is_nullable {
                        "Option<String>".to_string()
                    } else {
                        "String".to_string()
                    };

                    columns.push(EntityColumn {
                        field_name: prop.rust_field_name.clone(),
                        rust_type,
                        sea_orm_type: "String".to_string(),
                        column_name: prop.pg_column_name.clone(),
                        is_primary_key: false,
                        is_nullable,
                        pg_cast: None,
                        sea_orm_attr: None,
                    });
                }
                Some(RefClassificationKind::EntityReference) => {
                    let field_name = format!("{}_id", prop.rust_field_name);
                    let column_name = format!("{}_id", prop.pg_column_name);
                    columns.push(EntityColumn {
                        field_name,
                        rust_type: "Option<Uuid>".to_string(),
                        sea_orm_type: "Uuid".to_string(),
                        column_name,
                        is_primary_key: false,
                        is_nullable: true,
                        pg_cast: None,
                        sea_orm_attr: None,
                    });
                }
                Some(RefClassificationKind::CompositeWrapper)
                | Some(RefClassificationKind::MediaWrapper) => {
                    if let Ok(comp_cols) = db.get_composite_columns(&prop.name, schema_title).await
                    {
                        for col in &comp_cols {
                            let field_name = format!("{}{}", prop.rust_field_name, col.suffix);
                            let column_name = format!("{}{}", prop.pg_column_name, col.suffix);
                            let is_nullable = !prop.is_required;
                            // Entity models always use the raw column type (e.g. String),
                            // not the DTO enum type (e.g. CurrencyCodeList).
                            let rust_type = if is_nullable {
                                format!("Option<{}>", col.rust_type)
                            } else {
                                col.rust_type.clone()
                            };
                            columns.push(EntityColumn {
                                field_name,
                                rust_type,
                                sea_orm_type: col.sea_orm_type.clone(),
                                column_name,
                                is_primary_key: false,
                                is_nullable,
                                pg_cast: crate::generate::pg_cast_for_type(&col.pg_type),
                                sea_orm_attr: None,
                            });
                        }
                    }
                }
                Some(RefClassificationKind::ValueObject) => {
                    // Child tables are generated as separate entity files below
                }
                _ => {}
            }
        }

        // Deduplicate columns by field_name — composite wrappers, allOf composition, and
        // EntityReference _id suffixes can produce duplicate column names. Keep the first.
        {
            let mut seen_fields = std::collections::HashSet::new();
            columns.retain(|c| seen_fields.insert(c.field_name.clone()));
        }

        // Add platform_organization_id for tenant-scoped entities (must match DDL)
        let is_tenant_scoped = !is_global_entity(table_name, config);
        if is_tenant_scoped {
            columns.insert(
                1, // After id, before other columns
                EntityColumn {
                    field_name: "platform_organization_id".to_string(),
                    rust_type: "Uuid".to_string(),
                    sea_orm_type: "Uuid".to_string(),
                    column_name: "platform_organization_id".to_string(),
                    is_primary_key: false,
                    is_nullable: false,
                    pg_cast: None,
                    sea_orm_attr: None,
                },
            );
        }

        // Add timestamp columns
        columns.push(EntityColumn {
            field_name: "created_at".to_string(),
            rust_type: "chrono::DateTime<chrono::Utc>".to_string(),
            sea_orm_type: "TimestampWithTimeZone".to_string(),
            column_name: "created_at".to_string(),
            is_primary_key: false,
            is_nullable: false,
            pg_cast: None,
            sea_orm_attr: None,
        });
        columns.push(EntityColumn {
            field_name: "updated_at".to_string(),
            rust_type: "chrono::DateTime<chrono::Utc>".to_string(),
            sea_orm_type: "TimestampWithTimeZone".to_string(),
            column_name: "updated_at".to_string(),
            is_primary_key: false,
            is_nullable: false,
            pg_cast: None,
            sea_orm_attr: None,
        });

        // Add soft-delete / audit columns for auditable root entities
        let is_auditable = config
            .domains
            .get(domain)
            .and_then(|d| d.auditable)
            .unwrap_or(true);
        if is_auditable {
            columns.push(EntityColumn {
                field_name: "deleted_at".to_string(),
                rust_type: "Option<DateTimeWithTimeZone>".to_string(),
                sea_orm_type: "TimestampWithTimeZone".to_string(),
                column_name: "deleted_at".to_string(),
                is_primary_key: false,
                is_nullable: true,
                pg_cast: None,
                sea_orm_attr: None,
            });
            columns.push(EntityColumn {
                field_name: "deleted_by".to_string(),
                rust_type: "Option<Uuid>".to_string(),
                sea_orm_type: "Uuid".to_string(),
                column_name: "deleted_by".to_string(),
                is_primary_key: false,
                is_nullable: true,
                pg_cast: None,
                sea_orm_attr: None,
            });
            columns.push(EntityColumn {
                field_name: "updated_by".to_string(),
                rust_type: "Option<Uuid>".to_string(),
                sea_orm_type: "Uuid".to_string(),
                column_name: "updated_by".to_string(),
                is_primary_key: false,
                is_nullable: true,
                pg_cast: None,
                sea_orm_attr: None,
            });
            columns.push(EntityColumn {
                field_name: "is_demo_data".to_string(),
                rust_type: "bool".to_string(),
                sea_orm_type: "Boolean".to_string(),
                column_name: "is_demo_data".to_string(),
                is_primary_key: false,
                is_nullable: false,
                pg_cast: None,
                sea_orm_attr: None,
            });
        }

        // Deduplicate columns by field_name — CompositeWrapper expansion from
        // allOf-inherited properties can produce duplicate expanded columns.
        {
            let mut seen = std::collections::HashSet::new();
            columns.retain(|col| seen.insert(col.field_name.clone()));
        }

        // Domain-prefix the entity module name to avoid cross-domain
        // collisions (e.g. common::PositionType vs screening::PositionType).
        let entity_module_name = format!("{}_{}", schema_name, table_name);

        let import_prefix = &config.defaults.types_import_prefix;
        let structured_imports: Vec<String> = columns
            .iter()
            .filter(|c| c.sea_orm_attr.as_deref() == Some(r#"column_type = "JsonBinary""#))
            .filter_map(|c| {
                let mut inner = c.rust_type.as_str();
                // Strip Option<> and Vec<> wrappers to get the base type
                if let Some(s) = inner
                    .strip_prefix("Option<")
                    .and_then(|s| s.strip_suffix('>'))
                {
                    inner = s;
                }
                if let Some(s) = inner.strip_prefix("Vec<").and_then(|s| s.strip_suffix('>')) {
                    inner = s;
                }
                if inner != "serde_json::Value" && !inner.is_empty() {
                    Some(format!("use {import_prefix}::{inner};"))
                } else {
                    None
                }
            })
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Add self-referential relation for hierarchy if configured
        let mut relations = Vec::new();
        if let Some(hf) = entity_cfg.and_then(|ec| ec.hierarchy_field.clone()) {
            relations.push(EntityRelation {
                name: "Parent".to_string(),
                relation_type: "belongs_to = \"Entity\"".to_string(),
                related_entity: entity_module_name.clone(),
                from_column: codegraph_naming::to_pascal_case(&hf),
                to_column: "Id".to_string(),
                is_self_ref: true,
            });
        }

        let ctx = EntityContext {
            module_name: entity_module_name.clone(),
            struct_name: rust_type.to_string(),
            table_name: table_name.to_string(),
            schema_name: schema_name.to_string(),
            columns,
            relations,
            structured_imports,
        };

        let content = render_template_with_project(
            tera,
            &db_template_for(&*self.dialect, "entity"),
            &ctx,
            project,
        )?;
        let mut files = vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("entity")
                .join(format!("{}.rs", entity_module_name)),
            content,
        }];

        // Generate child entity files for ValueObject properties.
        // Skip non-array VOs that target a known entity — the DDL emits an FK
        // column for those, so no child table/entity is needed.
        let entity_titles: std::collections::HashSet<String> = config
            .domains
            .values()
            .flat_map(|d| d.entities.iter().cloned())
            .collect();
        for prop in &props {
            if prop.effective_kind() == Some(RefClassificationKind::ValueObject) {
                // Mirror DDL's try_emit_entity_fk_for_vo: skip when target is entity and non-array
                if !prop.is_array {
                    let is_entity_target = db
                        .get_property_ref_target(&prop.name, schema_title)
                        .await
                        .ok()
                        .flatten()
                        .map(|t| entity_titles.contains(&t.title))
                        .unwrap_or(false);
                    if is_entity_target {
                        continue;
                    }
                }
                let mut visited = std::collections::HashSet::new();
                visited.insert(schema_title.to_string());
                let child_files = Box::pin(build_child_entity(
                    db,
                    prop,
                    schema_title,
                    table_name,
                    schema_name,
                    rust_type,
                    &self.output_dir,
                    tera,
                    config,
                    &mut visited,
                    0,
                    project,
                ))
                .await?;
                files.extend(child_files);
            }

            // Codelist array properties → synthetic child entity with single "code" column.
            if prop.is_array
                && matches!(
                    prop.effective_kind(),
                    Some(RefClassificationKind::CodelistReference)
                        | Some(RefClassificationKind::CodelistCheck)
                )
            {
                let child_files = build_codelist_child_entity(
                    prop,
                    table_name,
                    schema_name,
                    rust_type,
                    &self.output_dir,
                    tera,
                    config,
                    project,
                )?;
                files.extend(child_files);
            }
        }

        Ok(files)
    }
}

/// Recursively build SeaORM entity files for a ValueObject child table.
///
/// Applies the same classification-aware column resolution as the parent entity:
/// PrimitiveWrapper/RangeWrapper become typed columns, CodelistReference becomes String,
/// EntityReference becomes UUID FK, CompositeWrapper expands into multiple columns,
/// and nested ValueObjects recurse.
#[allow(clippy::too_many_arguments)]
/// Maximum nesting depth for recursive child entity building.
const MAX_CHILD_ENTITY_DEPTH: usize = 10;

#[allow(clippy::too_many_arguments)]
async fn build_child_entity(
    db: &dyn GraphQuerier,
    prop: &PropertyNode,
    schema_title: &str,
    parent_table_name: &str,
    schema_name: &str,
    parent_rust_type: &str,
    output_dir: &Path,
    tera: &tera::Tera,
    config: &DomainConfig,
    visited: &mut std::collections::HashSet<String>,
    depth: usize,
    project: &ProjectConfig,
) -> Result<Vec<GeneratedFile>> {
    if depth >= MAX_CHILD_ENTITY_DEPTH {
        return Ok(Vec::new());
    }

    // Resolve the target schema (handles array vs non-array)
    let target = if prop.is_array {
        db.get_array_item_schema(&prop.name, schema_title)
            .await
            .ok()
            .flatten()
    } else {
        db.get_property_ref_target(&prop.name, schema_title)
            .await
            .ok()
            .flatten()
    };

    let ts = match target {
        Some(ts) => ts,
        None => return Ok(Vec::new()),
    };

    // Cycle guard: skip schemas we've already visited in this recursion path
    if !visited.insert(ts.title.clone()) {
        return Ok(Vec::new());
    }

    let child_props_raw = match db.get_properties(&ts.title).await {
        Ok(props) => props,
        Err(_) => return Ok(Vec::new()),
    };

    // Deduplicate properties, skip "id" (we add our own)
    let child_props = {
        let mut seen = std::collections::HashSet::new();
        child_props_raw
            .into_iter()
            .filter(|p| p.rust_field_name != "id" && p.pg_column_name != "id")
            .filter(|p| seen.insert(p.rust_field_name.clone()))
            .collect::<Vec<_>>()
    };

    let child_table_name = codegraph_naming::truncate_pg_identifier(&format!(
        "{}_{}",
        parent_table_name, prop.pg_column_name
    ));
    let child_struct_name = format!(
        "{}{}",
        parent_rust_type,
        config.defaults.strip_suffix(&ts.rust_type_name)
    );

    let mut columns = vec![
        EntityColumn {
            field_name: "id".to_string(),
            rust_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            column_name: "id".to_string(),
            is_primary_key: true,
            is_nullable: false,
            pg_cast: None,
            sea_orm_attr: None,
        },
        EntityColumn {
            field_name: codegraph_naming::truncate_pg_identifier(&format!("{}_id", parent_table_name)),
            rust_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            column_name: codegraph_naming::truncate_pg_identifier(&format!("{}_id", parent_table_name)),
            is_primary_key: false,
            is_nullable: false,
            pg_cast: None,
            sea_orm_attr: None,
        },
    ];

    // Composite range: collapse start/end fields into a single range column
    let composite_range = db.get_composite_range(&ts.title).await.ok().flatten();
    let consumed_fields: std::collections::HashSet<String> = db
        .get_consumed_fields(&ts.title)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(prop, _role)| prop.name)
        .collect();

    // Emit the range column if present — uses Custom column type for correct PG casting
    if let Some(ref range) = composite_range {
        let pg_cast = pg_cast_for_type(&range.pg_type);
        columns.push(EntityColumn {
            field_name: range.pg_column_name.clone(),
            rust_type: "Option<String>".to_string(),
            sea_orm_type: "Text".to_string(),
            column_name: range.pg_column_name.clone(),
            is_primary_key: false,
            is_nullable: true,
            pg_cast,
            sea_orm_attr: None,
        });
    }

    let mut nested_files = Vec::new();

    for child_prop in &child_props {
        // Skip fields consumed by composite ranges
        if consumed_fields.contains(&child_prop.name) {
            continue;
        }
        match child_prop.effective_kind() {
            Some(RefClassificationKind::PrimitiveWrapper)
            | Some(RefClassificationKind::StructuredWrapper)
            | Some(RefClassificationKind::ArrayWrapper)
            | Some(RefClassificationKind::RangeWrapper)
            | Some(RefClassificationKind::InlineEnum) => {
                let is_structured =
                    child_prop.effective_kind() == Some(RefClassificationKind::StructuredWrapper);
                let is_nullable = !child_prop.is_required;
                let base_type = if is_structured {
                    "serde_json::Value".to_string()
                } else {
                    child_prop.rust_field_type.clone()
                };
                let rust_type = if is_nullable {
                    format!("Option<{base_type}>")
                } else {
                    base_type
                };
                let pg_cast =
                    if child_prop.effective_kind() == Some(RefClassificationKind::RangeWrapper) {
                        pg_cast_for_type(&child_prop.pg_column_type)
                    } else {
                        None
                    };
                let sea_orm_attr = if is_structured {
                    Some(r#"column_type = "JsonBinary""#.to_string())
                } else {
                    None
                };

                columns.push(EntityColumn {
                    field_name: child_prop.rust_field_name.clone(),
                    rust_type,
                    sea_orm_type: if is_structured {
                        "JsonBinary".to_string()
                    } else {
                        child_prop.sea_orm_type.clone()
                    },
                    column_name: child_prop.pg_column_name.clone(),
                    is_primary_key: false,
                    is_nullable,
                    pg_cast,
                    sea_orm_attr,
                });
            }
            Some(RefClassificationKind::CodelistReference) => {
                let is_nullable = !child_prop.is_required;
                let rust_type = if is_nullable {
                    "Option<String>".to_string()
                } else {
                    "String".to_string()
                };

                columns.push(EntityColumn {
                    field_name: child_prop.rust_field_name.clone(),
                    rust_type,
                    sea_orm_type: "String".to_string(),
                    column_name: child_prop.pg_column_name.clone(),
                    is_primary_key: false,
                    is_nullable,
                    pg_cast: None,
                    sea_orm_attr: None,
                });
            }
            Some(RefClassificationKind::CodelistCheck) => {
                let is_nullable = !child_prop.is_required;
                let rust_type = if is_nullable {
                    "Option<String>".to_string()
                } else {
                    "String".to_string()
                };

                columns.push(EntityColumn {
                    field_name: child_prop.rust_field_name.clone(),
                    rust_type,
                    sea_orm_type: "String".to_string(),
                    column_name: child_prop.pg_column_name.clone(),
                    is_primary_key: false,
                    is_nullable,
                    pg_cast: None,
                    sea_orm_attr: None,
                });
            }
            Some(RefClassificationKind::EntityReference) => {
                let field_name = format!("{}_id", child_prop.rust_field_name);
                let column_name = format!("{}_id", child_prop.pg_column_name);
                columns.push(EntityColumn {
                    field_name,
                    rust_type: "Option<Uuid>".to_string(),
                    sea_orm_type: "Uuid".to_string(),
                    column_name,
                    is_primary_key: false,
                    is_nullable: true,
                    pg_cast: None,
                    sea_orm_attr: None,
                });
            }
            Some(RefClassificationKind::CompositeWrapper)
            | Some(RefClassificationKind::MediaWrapper) => {
                if let Ok(comp_cols) = db.get_composite_columns(&child_prop.name, &ts.title).await {
                    for col in &comp_cols {
                        let field_name = format!("{}{}", child_prop.rust_field_name, col.suffix);
                        let column_name = format!("{}{}", child_prop.pg_column_name, col.suffix);
                        let is_nullable = !child_prop.is_required;
                        let rust_type = if is_nullable {
                            format!("Option<{}>", col.rust_type)
                        } else {
                            col.rust_type.clone()
                        };
                        columns.push(EntityColumn {
                            field_name,
                            rust_type,
                            sea_orm_type: col.sea_orm_type.clone(),
                            column_name,
                            is_primary_key: false,
                            is_nullable,
                            pg_cast: None,
                            sea_orm_attr: None,
                        });
                    }
                }
            }
            Some(RefClassificationKind::ValueObject) => {
                // Recurse: nested ValueObject becomes a nested child entity
                let nested = Box::pin(build_child_entity(
                    db,
                    child_prop,
                    &ts.title,
                    &child_table_name,
                    schema_name,
                    &child_struct_name,
                    output_dir,
                    tera,
                    config,
                    visited,
                    depth + 1,
                    project,
                ))
                .await?;
                nested_files.extend(nested);
            }
            None => {}
        }
    }

    // Deduplicate columns by field_name — composite wrappers, allOf composition, and
    // EntityReference _id suffixes can produce duplicate column names. Keep the first.
    {
        let mut seen_fields = std::collections::HashSet::new();
        columns.retain(|c| seen_fields.insert(c.field_name.clone()));
    }

    // Add platform_organization_id for tenant-scoped entities
    let is_tenant_scoped = !is_global_entity(&child_table_name, config);
    if is_tenant_scoped {
        columns.insert(
            1, // After id, before other columns
            EntityColumn {
                field_name: "platform_organization_id".to_string(),
                rust_type: "Uuid".to_string(),
                sea_orm_type: "Uuid".to_string(),
                column_name: "platform_organization_id".to_string(),
                is_primary_key: false,
                is_nullable: false,
                pg_cast: None,
                sea_orm_attr: None,
            },
        );
    }

    // Add timestamp columns
    columns.push(EntityColumn {
        field_name: "created_at".to_string(),
        rust_type: "chrono::DateTime<chrono::Utc>".to_string(),
        sea_orm_type: "TimestampWithTimeZone".to_string(),
        column_name: "created_at".to_string(),
        is_primary_key: false,
        is_nullable: false,
        pg_cast: None,
        sea_orm_attr: None,
    });
    columns.push(EntityColumn {
        field_name: "updated_at".to_string(),
        rust_type: "chrono::DateTime<chrono::Utc>".to_string(),
        sea_orm_type: "TimestampWithTimeZone".to_string(),
        column_name: "updated_at".to_string(),
        is_primary_key: false,
        is_nullable: false,
        pg_cast: None,
        sea_orm_attr: None,
    });

    // Deduplicate columns by field_name — same rationale as parent entity.
    {
        let mut seen = std::collections::HashSet::new();
        columns.retain(|col| seen.insert(col.field_name.clone()));
    }

    let entity_module_name = format!("{}_{}", schema_name, child_table_name);

    let import_prefix = &config.defaults.types_import_prefix;
    let structured_imports: Vec<String> = columns
        .iter()
        .filter(|c| c.sea_orm_attr.as_deref() == Some(r#"column_type = "JsonBinary""#))
        .filter_map(|c| {
            let mut inner = c.rust_type.as_str();
            // Strip Option<> and Vec<> wrappers to get the base type
            if let Some(s) = inner
                .strip_prefix("Option<")
                .and_then(|s| s.strip_suffix('>'))
            {
                inner = s;
            }
            if let Some(s) = inner.strip_prefix("Vec<").and_then(|s| s.strip_suffix('>')) {
                inner = s;
            }
            if inner != "serde_json::Value" && !inner.is_empty() {
                Some(format!("use {import_prefix}::{inner};"))
            } else {
                None
            }
        })
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let ctx = EntityContext {
        module_name: entity_module_name.clone(),
        struct_name: child_struct_name,
        table_name: child_table_name,
        schema_name: schema_name.to_string(),
        columns,
        relations: Vec::new(),
        structured_imports,
    };

    let content = render_template_with_project(tera, "db/entity.tera", &ctx, project)?;
    let mut files = vec![GeneratedFile {
        path: output_dir
            .join("src")
            .join("entity")
            .join(format!("{}.rs", entity_module_name)),
        content,
    }];
    files.extend(nested_files);

    Ok(files)
}

/// Build a synthetic SeaORM entity for a codelist array property.
///
/// Codelist schemas are plain enums with no object properties, so we create
/// a child entity with a single `code TEXT NOT NULL` column instead of
/// recursing into the schema.
fn build_codelist_child_entity(
    prop: &PropertyNode,
    parent_table_name: &str,
    schema_name: &str,
    parent_rust_type: &str,
    output_dir: &Path,
    tera: &tera::Tera,
    config: &DomainConfig,
    project: &ProjectConfig,
) -> Result<Vec<GeneratedFile>> {
    let child_table_name = codegraph_naming::truncate_pg_identifier(&format!(
        "{}_{}",
        parent_table_name, prop.pg_column_name
    ));
    let child_struct_name = format!(
        "{}{}",
        parent_rust_type,
        codegraph_naming::to_pascal_case(&prop.rust_field_name)
    );

    let mut columns = vec![
        EntityColumn {
            field_name: "id".to_string(),
            rust_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            column_name: "id".to_string(),
            is_primary_key: true,
            is_nullable: false,
            pg_cast: None,
            sea_orm_attr: None,
        },
        EntityColumn {
            field_name: codegraph_naming::truncate_pg_identifier(&format!("{}_id", parent_table_name)),
            rust_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            column_name: codegraph_naming::truncate_pg_identifier(&format!("{}_id", parent_table_name)),
            is_primary_key: false,
            is_nullable: false,
            pg_cast: None,
            sea_orm_attr: None,
        },
        EntityColumn {
            field_name: "code".to_string(),
            rust_type: "String".to_string(),
            sea_orm_type: "String".to_string(),
            column_name: "code".to_string(),
            is_primary_key: false,
            is_nullable: false,
            pg_cast: None,
            sea_orm_attr: None,
        },
    ];

    // Add platform_organization_id for tenant-scoped entities
    let is_tenant_scoped = !is_global_entity(&child_table_name, config);
    if is_tenant_scoped {
        columns.insert(
            1,
            EntityColumn {
                field_name: "platform_organization_id".to_string(),
                rust_type: "Uuid".to_string(),
                sea_orm_type: "Uuid".to_string(),
                column_name: "platform_organization_id".to_string(),
                is_primary_key: false,
                is_nullable: false,
                pg_cast: None,
                sea_orm_attr: None,
            },
        );
    }

    // Timestamp columns
    columns.push(EntityColumn {
        field_name: "created_at".to_string(),
        rust_type: "chrono::DateTime<chrono::Utc>".to_string(),
        sea_orm_type: "TimestampWithTimeZone".to_string(),
        column_name: "created_at".to_string(),
        is_primary_key: false,
        is_nullable: false,
        pg_cast: None,
        sea_orm_attr: None,
    });
    columns.push(EntityColumn {
        field_name: "updated_at".to_string(),
        rust_type: "chrono::DateTime<chrono::Utc>".to_string(),
        sea_orm_type: "TimestampWithTimeZone".to_string(),
        column_name: "updated_at".to_string(),
        is_primary_key: false,
        is_nullable: false,
        pg_cast: None,
        sea_orm_attr: None,
    });

    let entity_module_name = format!("{}_{}", schema_name, child_table_name);

    let ctx = EntityContext {
        module_name: entity_module_name.clone(),
        struct_name: child_struct_name,
        table_name: child_table_name,
        schema_name: schema_name.to_string(),
        columns,
        relations: Vec::new(),
        structured_imports: Vec::new(),
    };

    let content = render_template_with_project(tera, "db/entity.tera", &ctx, project)?;
    Ok(vec![GeneratedFile {
        path: output_dir
            .join("src")
            .join("entity")
            .join(format!("{}.rs", entity_module_name)),
        content,
    }])
}

fn is_global_entity(_table_name: &str, _config: &DomainConfig) -> bool {
    // TODO: check tenancy config for global tables
    false
}
