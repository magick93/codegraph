use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PropertyNode;
use codegraph_type_contracts::RefClassificationKind;
use serde::Serialize;

use crate::api::api_model::resolve_entity_operations;
use crate::error::Result;
use codegraph_config::DomainConfig;

use super::proto_type::proto_type_from_field;

#[derive(Debug, Serialize)]
pub struct ProtoContext {
    pub package: String,
    pub entity_name: String,
    pub module_name: String,
    pub proto_file_name: String,
    pub imports: Vec<String>,
    pub messages: Vec<ProtoMsgDef>,
    pub enums: Vec<ProtoEnumDef>,
    pub service_methods: Vec<ProtoServiceMethod>,
    pub operations: Vec<String>,
    pub has_fts: bool,
    pub has_embeddings: bool,
    pub has_workflow: bool,
    pub hierarchy_field: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProtoMsgDef {
    pub name: String,
    pub description: Option<String>,
    pub fields: Vec<ProtoFieldDef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProtoFieldDef {
    pub field_number: u32,
    pub name: String,
    pub proto_type: String,
    pub is_optional: bool,
    pub is_repeated: bool,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProtoEnumDef {
    pub name: String,
    pub values: Vec<ProtoEnumValue>,
}

#[derive(Debug, Serialize)]
pub struct ProtoEnumValue {
    pub name: String,
    pub number: i32,
}

#[derive(Debug, Serialize)]
pub struct ProtoServiceMethod {
    pub name: String,
    pub input_type: String,
    pub output_type: String,
    pub client_streaming: bool,
    pub server_streaming: bool,
    pub description: Option<String>,
}

impl ProtoContext {
    pub async fn build(
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
    ) -> Result<Self> {
        let schema = db
            .get_schema_in_domain(schema_title, domain)
            .await?
            .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

        let entity_name = schema.rust_type_name.clone();
        let module_name = schema.pg_table_name.clone();
        let package = domain.to_string();

        if module_name.is_empty() {
            return Ok(Self::empty(package, entity_name, module_name));
        }

        let properties = db.get_properties(schema_title).await?;

        let entity_cfg = config
            .domains
            .get(domain)
            .and_then(|d| d.get_entity_config(&entity_name));

        let operations = resolve_entity_operations(db, config, domain, &entity_name).await;

        let workflow = entity_cfg.and_then(|ec| ec.workflow.as_ref());
        let has_workflow = workflow
            .map(|wf| wf.generate_action_endpoints)
            .unwrap_or(false);

        let search = entity_cfg.map(|ec| &ec.search);
        let has_fts = search
            .and_then(|s| s.fts_columns.as_ref())
            .map(|cols| !cols.is_empty())
            .unwrap_or(false);
        let has_embeddings = search
            .map(|s| !s.embedding_columns.is_empty())
            .unwrap_or(false);

        let hierarchy_field = entity_cfg.and_then(|ec| ec.hierarchy_field.clone());
        // Entity message fields + collected imports/enums live in their own
        // helpers (issue #751); evaluation order is unchanged.
        let entity_fields = build_entity_fields(db, &properties, &entity_name);
        let mut imports = entity_fields.imports;
        let enums = entity_fields.enums;

        let mut messages = Vec::new();
        push_crud_messages(
            &mut messages,
            &entity_fields.fields,
            &entity_name,
            has_fts,
            has_workflow,
        );
        push_special_messages(
            &mut messages,
            &entity_name,
            has_fts,
            has_embeddings,
            has_workflow,
            &hierarchy_field,
        );

        let service_methods = build_service_methods(
            &entity_name,
            &operations,
            has_fts,
            has_embeddings,
            has_workflow,
            &hierarchy_field,
        );
        // Field-level well-known-type usage collects its import via
        // `proto_type_from_field`; method input/output slots are built
        // independently, so their WKT imports must be collected here
        // (issue #793).
        push_well_known_imports(&mut imports, &service_methods);

        let proto_file_name = format!("{}.proto", module_name);

        Ok(Self {
            package,
            entity_name,
            module_name,
            proto_file_name,
            imports,
            messages,
            enums,
            service_methods,
            operations,
            has_fts,
            has_embeddings,
            has_workflow,
            hierarchy_field,
        })
    }

    fn empty(package: String, entity_name: String, module_name: String) -> Self {
        Self {
            package,
            entity_name,
            module_name,
            proto_file_name: String::new(),
            imports: Vec::new(),
            messages: Vec::new(),
            enums: Vec::new(),
            service_methods: Vec::new(),
            operations: Vec::new(),
            has_fts: false,
            has_embeddings: false,
            has_workflow: false,
            hierarchy_field: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.proto_file_name.is_empty()
    }
}

/// The entity message's field set plus the collected imports/enums
/// (extracted from `ProtoContext::build`, issue #751).
struct EntityMessageFields {
    fields: Vec<ProtoFieldDef>,
    imports: Vec<String>,
    enums: Vec<ProtoEnumDef>,
}

/// Build the entity message fields from properties. Field numbers start
/// at 2: field 1 is the synthetic `id`; 998/999 are the synthetic
/// timestamps. Extracted verbatim from `ProtoContext::build` (issue #751).
fn build_entity_fields(
    db: &dyn GraphQuerier,
    properties: &[PropertyNode],
    entity_name: &str,
) -> EntityMessageFields {
    // Build entity message fields from properties
    let mut all_field_defs = Vec::new();
    let mut imports = Vec::new();
    let mut enums: Vec<ProtoEnumDef> = Vec::new();
    // Field numbers start at 2: field 1 is the synthetic `id` above.

    // id = 1
    all_field_defs.push(ProtoFieldDef {
        field_number: 1,
        name: "id".to_string(),
        proto_type: "string".to_string(),
        is_optional: false,
        is_repeated: false,
        description: Some("Unique identifier (UUID)".to_string()),
    });

    for (prop_offset, prop) in properties.iter().enumerate() {
        let field_number = prop_offset as u32 + 2;
        let field_type = proto_type_from_field(prop, db, entity_name);

        if field_type.is_import
            && let Some(ref path) = field_type.import_path
            && !imports.contains(path)
        {
            imports.push(path.clone());
        }

        // Check if this property should be rendered as a proto enum
        let proto_type = maybe_collect_enum(prop, entity_name, &mut enums)
            .unwrap_or_else(|| field_type.proto_type.clone());

        let is_repeated = field_type.proto_type.starts_with("repeated ");
        let base_proto_type = if is_repeated {
            field_type
                .proto_type
                .strip_prefix("repeated ")
                .unwrap()
                .to_string()
        } else {
            proto_type.clone()
        };

        all_field_defs.push(ProtoFieldDef {
            field_number,
            name: prop.name.clone(),
            proto_type: base_proto_type,
            is_optional: !prop.is_required || prop.is_nullable,
            is_repeated,
            description: prop.description.clone(),
        });
    }

    // created_at = 998, updated_at = 999
    let synthetic_fields = vec![
        ProtoFieldDef {
            field_number: 998,
            name: "created_at".to_string(),
            proto_type: "google.protobuf.Timestamp".to_string(),
            is_optional: true,
            is_repeated: false,
            description: Some("Creation timestamp".to_string()),
        },
        ProtoFieldDef {
            field_number: 999,
            name: "updated_at".to_string(),
            proto_type: "google.protobuf.Timestamp".to_string(),
            is_optional: true,
            is_repeated: false,
            description: Some("Last update timestamp".to_string()),
        },
    ];

    // Add timestamp import if not already present
    for sf in synthetic_fields {
        all_field_defs.push(sf);
    }
    if !imports.contains(&"google/protobuf/timestamp.proto".to_string()) {
        imports.push("google/protobuf/timestamp.proto".to_string());
    }

    EntityMessageFields {
        fields: all_field_defs,
        imports,
        enums,
    }
}

/// Entity + Create/Update/Get/Delete/List request and List response
/// messages. Extracted verbatim from `ProtoContext::build` (issue #751).
fn push_crud_messages(
    messages: &mut Vec<ProtoMsgDef>,
    all_field_defs: &[ProtoFieldDef],
    entity_name: &str,
    has_fts: bool,
    has_workflow: bool,
) {
    // Entity message
    messages.push(ProtoMsgDef {
        name: entity_name.to_string(),
        description: Some(format!("{} entity", entity_name)),
        fields: all_field_defs.to_vec(),
    });

    // CreateRequest — id excluded, synthetic fields excluded
    let create_fields: Vec<ProtoFieldDef> = all_field_defs
        .iter()
        .filter(|f| f.name != "id" && f.name != "created_at" && f.name != "updated_at")
        .cloned()
        .collect();
    messages.push(ProtoMsgDef {
        name: format!("Create{}Request", entity_name),
        description: Some(format!("Create request for {}", entity_name)),
        fields: create_fields,
    });

    // UpdateRequest — all fields optional, id required, immutable fields excluded
    let update_fields: Vec<ProtoFieldDef> = all_field_defs
        .iter()
        .filter(|f| f.name != "created_at" && f.name != "updated_at")
        .map(|f| {
            if f.name == "id" {
                f.clone()
            } else {
                ProtoFieldDef {
                    is_optional: true,
                    ..f.clone()
                }
            }
        })
        .collect();
    messages.push(ProtoMsgDef {
        name: format!("Update{}Request", entity_name),
        description: Some(format!("Update request for {}", entity_name)),
        fields: update_fields,
    });

    // GetRequest
    messages.push(ProtoMsgDef {
        name: format!("Get{}Request", entity_name),
        description: Some(format!("Get request for {}", entity_name)),
        fields: vec![ProtoFieldDef {
            field_number: 1,
            name: "id".to_string(),
            proto_type: "string".to_string(),
            is_optional: false,
            is_repeated: false,
            description: Some("Entity UUID".to_string()),
        }],
    });

    // DeleteRequest
    messages.push(ProtoMsgDef {
        name: format!("Delete{}Request", entity_name),
        description: Some(format!("Delete request for {}", entity_name)),
        fields: vec![ProtoFieldDef {
            field_number: 1,
            name: "id".to_string(),
            proto_type: "string".to_string(),
            is_optional: false,
            is_repeated: false,
            description: Some("Entity UUID".to_string()),
        }],
    });

    // ListRequest
    let mut list_fields = vec![
        ProtoFieldDef {
            field_number: 1,
            name: "page_size".to_string(),
            proto_type: "int32".to_string(),
            is_optional: true,
            is_repeated: false,
            description: Some("Number of items per page".to_string()),
        },
        ProtoFieldDef {
            field_number: 2,
            name: "page_token".to_string(),
            proto_type: "string".to_string(),
            is_optional: true,
            is_repeated: false,
            description: Some("Pagination token".to_string()),
        },
    ];
    if has_fts {
        list_fields.push(ProtoFieldDef {
            field_number: 3,
            name: "query".to_string(),
            proto_type: "string".to_string(),
            is_optional: true,
            is_repeated: false,
            description: Some("Full-text search query".to_string()),
        });
    }
    if has_workflow {
        list_fields.push(ProtoFieldDef {
            field_number: 4,
            name: "status".to_string(),
            proto_type: "string".to_string(),
            is_optional: true,
            is_repeated: false,
            description: Some("Filter by workflow status".to_string()),
        });
    }
    list_fields.push(ProtoFieldDef {
        field_number: 5,
        name: "filters".to_string(),
        // `FilterClause` lives in the `shared` package (`proto/shared.proto`),
        // so a bare reference cannot resolve from a domain package
        // (issue #793).
        proto_type: "shared.FilterClause".to_string(),
        is_optional: true,
        is_repeated: true,
        description: Some("Additional field filters".to_string()),
    });
    messages.push(ProtoMsgDef {
        name: format!("List{}Request", entity_name),
        description: Some(format!("List request for {}", entity_name)),
        fields: list_fields,
    });

    // ListResponse
    messages.push(ProtoMsgDef {
        name: format!("List{}Response", entity_name),
        description: Some(format!("List response for {}", entity_name)),
        fields: vec![
            ProtoFieldDef {
                field_number: 1,
                name: "data".to_string(),
                proto_type: entity_name.to_string(),
                is_optional: false,
                is_repeated: true,
                description: None,
            },
            ProtoFieldDef {
                field_number: 2,
                name: "total".to_string(),
                proto_type: "int32".to_string(),
                is_optional: false,
                is_repeated: false,
                description: Some("Total number of items".to_string()),
            },
            ProtoFieldDef {
                field_number: 3,
                name: "next_page_token".to_string(),
                proto_type: "string".to_string(),
                is_optional: true,
                is_repeated: false,
                description: Some("Pagination token for next page".to_string()),
            },
        ],
    });
}

/// Search / semantic-search / transition / tree request messages, gated on
/// the entity's FTS / embedding / workflow / hierarchy capabilities.
/// Extracted verbatim from `ProtoContext::build` (issue #751).
fn push_special_messages(
    messages: &mut Vec<ProtoMsgDef>,
    entity_name: &str,
    has_fts: bool,
    has_embeddings: bool,
    has_workflow: bool,
    hierarchy_field: &Option<String>,
) {
    // SearchRequest (FTS)
    if has_fts {
        messages.push(ProtoMsgDef {
            name: "SearchRequest".to_string(),
            description: Some("Full-text search request".to_string()),
            fields: vec![
                ProtoFieldDef {
                    field_number: 1,
                    name: "query".to_string(),
                    proto_type: "string".to_string(),
                    is_optional: false,
                    is_repeated: false,
                    description: Some("Search query".to_string()),
                },
                ProtoFieldDef {
                    field_number: 2,
                    name: "page_size".to_string(),
                    proto_type: "int32".to_string(),
                    is_optional: true,
                    is_repeated: false,
                    description: Some("Number of items per page".to_string()),
                },
                ProtoFieldDef {
                    field_number: 3,
                    name: "page_token".to_string(),
                    proto_type: "string".to_string(),
                    is_optional: true,
                    is_repeated: false,
                    description: Some("Pagination token".to_string()),
                },
            ],
        });
        messages.push(ProtoMsgDef {
            name: "SearchResponse".to_string(),
            description: Some("Full-text search response".to_string()),
            fields: vec![
                ProtoFieldDef {
                    field_number: 1,
                    name: "data".to_string(),
                    proto_type: entity_name.to_string(),
                    is_optional: false,
                    is_repeated: true,
                    description: None,
                },
                ProtoFieldDef {
                    field_number: 2,
                    name: "total".to_string(),
                    proto_type: "int32".to_string(),
                    is_optional: false,
                    is_repeated: false,
                    description: Some("Total number of results".to_string()),
                },
                ProtoFieldDef {
                    field_number: 3,
                    name: "next_page_token".to_string(),
                    proto_type: "string".to_string(),
                    is_optional: true,
                    is_repeated: false,
                    description: Some("Pagination token for next page".to_string()),
                },
            ],
        });
    }

    // SemanticSearchRequest (embeddings)
    if has_embeddings {
        messages.push(ProtoMsgDef {
            name: "SemanticSearchRequest".to_string(),
            description: Some("Semantic search request".to_string()),
            fields: vec![
                ProtoFieldDef {
                    field_number: 1,
                    name: "query".to_string(),
                    proto_type: "string".to_string(),
                    is_optional: false,
                    is_repeated: false,
                    description: Some("Search query".to_string()),
                },
                ProtoFieldDef {
                    field_number: 2,
                    name: "limit".to_string(),
                    proto_type: "int32".to_string(),
                    is_optional: true,
                    is_repeated: false,
                    description: Some("Maximum number of results".to_string()),
                },
            ],
        });
    }

    // TransitionRequest (workflow)
    if has_workflow {
        messages.push(ProtoMsgDef {
            name: "TransitionRequest".to_string(),
            description: Some("Workflow transition request".to_string()),
            fields: vec![
                ProtoFieldDef {
                    field_number: 1,
                    name: "id".to_string(),
                    proto_type: "string".to_string(),
                    is_optional: false,
                    is_repeated: false,
                    description: Some("Entity UUID".to_string()),
                },
                ProtoFieldDef {
                    field_number: 2,
                    name: "action".to_string(),
                    proto_type: "string".to_string(),
                    is_optional: false,
                    is_repeated: false,
                    description: Some("Transition action name".to_string()),
                },
                ProtoFieldDef {
                    field_number: 3,
                    name: "comment".to_string(),
                    proto_type: "string".to_string(),
                    is_optional: true,
                    is_repeated: false,
                    description: Some("Optional comment".to_string()),
                },
                ProtoFieldDef {
                    field_number: 4,
                    name: "assignee_id".to_string(),
                    proto_type: "string".to_string(),
                    is_optional: true,
                    is_repeated: false,
                    description: Some("Optional assignee UUID".to_string()),
                },
            ],
        });
    }

    // TreeRequest (hierarchy)
    if hierarchy_field.is_some() {
        messages.push(ProtoMsgDef {
            name: "TreeRequest".to_string(),
            description: Some("Tree/hierarchy request".to_string()),
            fields: vec![
                ProtoFieldDef {
                    field_number: 1,
                    name: "id".to_string(),
                    proto_type: "string".to_string(),
                    is_optional: false,
                    is_repeated: false,
                    description: Some("Root entity UUID".to_string()),
                },
                ProtoFieldDef {
                    field_number: 2,
                    name: "max_depth".to_string(),
                    proto_type: "int32".to_string(),
                    is_optional: true,
                    is_repeated: false,
                    description: Some("Maximum tree depth".to_string()),
                },
            ],
        });
    }
}

/// Service methods derived from the entity's resolved operations and
/// capabilities. Extracted verbatim from `ProtoContext::build` (issue #751).
fn build_service_methods(
    entity_name: &str,
    operations: &[String],
    has_fts: bool,
    has_embeddings: bool,
    has_workflow: bool,
    hierarchy_field: &Option<String>,
) -> Vec<ProtoServiceMethod> {
    let mut service_methods = Vec::new();
    let op_set: std::collections::HashSet<String> = operations.iter().cloned().collect();

    if op_set.contains("create") {
        service_methods.push(ProtoServiceMethod {
            name: "Create".to_string(),
            input_type: format!("Create{}Request", entity_name),
            output_type: entity_name.to_string(),
            client_streaming: false,
            server_streaming: false,
            description: Some("Create a new entity".to_string()),
        });
    }

    if op_set.contains("read") {
        service_methods.push(ProtoServiceMethod {
            name: "Get".to_string(),
            input_type: format!("Get{}Request", entity_name),
            output_type: entity_name.to_string(),
            client_streaming: false,
            server_streaming: false,
            description: Some("Get an entity by ID".to_string()),
        });
    }

    if op_set.contains("update") {
        service_methods.push(ProtoServiceMethod {
            name: "Update".to_string(),
            input_type: format!("Update{}Request", entity_name),
            output_type: entity_name.to_string(),
            client_streaming: false,
            server_streaming: false,
            description: Some("Update an existing entity".to_string()),
        });
    }

    if op_set.contains("delete") {
        service_methods.push(ProtoServiceMethod {
            name: "Delete".to_string(),
            input_type: format!("Delete{}Request", entity_name),
            output_type: "google.protobuf.Empty".to_string(),
            client_streaming: false,
            server_streaming: false,
            description: Some("Delete an entity by ID".to_string()),
        });
    }

    if op_set.contains("list") {
        service_methods.push(ProtoServiceMethod {
            name: "List".to_string(),
            input_type: format!("List{}Request", entity_name),
            output_type: format!("List{}Response", entity_name),
            client_streaming: false,
            server_streaming: false,
            description: Some("List entities with pagination".to_string()),
        });
    }

    if has_fts && op_set.contains("list") {
        service_methods.push(ProtoServiceMethod {
            name: "Search".to_string(),
            input_type: "SearchRequest".to_string(),
            output_type: "SearchResponse".to_string(),
            client_streaming: false,
            server_streaming: false,
            description: Some("Full-text search".to_string()),
        });
    }

    if has_embeddings {
        service_methods.push(ProtoServiceMethod {
            name: "SemanticSearch".to_string(),
            input_type: "SemanticSearchRequest".to_string(),
            // `SearchResult` lives in the `shared` package (issue #793).
            output_type: "shared.SearchResult".to_string(),
            client_streaming: false,
            server_streaming: true,
            description: Some("Semantic/vector search".to_string()),
        });
    }

    if has_workflow {
        service_methods.push(ProtoServiceMethod {
            name: "Transition".to_string(),
            input_type: "TransitionRequest".to_string(),
            output_type: entity_name.to_string(),
            client_streaming: false,
            server_streaming: false,
            description: Some("Execute a workflow transition".to_string()),
        });
    }

    if hierarchy_field.is_some() {
        service_methods.push(ProtoServiceMethod {
            name: "GetTree".to_string(),
            input_type: "TreeRequest".to_string(),
            output_type: entity_name.to_string(),
            client_streaming: false,
            server_streaming: true,
            description: Some("Get entity tree/hierarchy".to_string()),
        });
    }

    service_methods
}

/// Push the `google/protobuf/*.proto` imports required by SERVICE method
/// input/output slots.
///
/// Message FIELDS report their own `is_import`/`import_path` through
/// `proto_type_from_field`, but service signatures are assembled
/// independently — before issue #793 nothing pushed the import for the
/// `Delete` method's `google.protobuf.Empty` return, so the emitted proto
/// failed to compile under protoc (`"google.protobuf.Empty" is not
/// defined`). Mirrors the synthetic-timestamp import push in
/// `build_entity_fields`.
fn push_well_known_imports(imports: &mut Vec<String>, service_methods: &[ProtoServiceMethod]) {
    for method in service_methods {
        for slot in [&method.input_type, &method.output_type] {
            if let Some(path) = well_known_import_path(slot)
                && !imports.contains(&path)
            {
                imports.push(path);
            }
        }
    }
}

/// Map a well-known-type reference (`google.protobuf.Empty`) to the proto
/// file that declares it (`google/protobuf/empty.proto`). The distro
/// protobuf ships these under its own include dir; `timestamp.proto` and
/// `struct.proto` follow the same naming rule.
fn well_known_import_path(proto_type: &str) -> Option<String> {
    let file = proto_type.strip_prefix("google.protobuf.")?;
    Some(format!(
        "google/protobuf/{}.proto",
        codegraph_naming::to_snake_case(file)
    ))
}

/// Check if a property should be rendered as a proto enum.
/// Returns Some(enum_name) if an enum was collected, or None for scalar types.
fn maybe_collect_enum(
    prop: &PropertyNode,
    entity_name: &str,
    enums: &mut Vec<ProtoEnumDef>,
) -> Option<String> {
    let kind = prop.effective_kind()?;
    match kind {
        RefClassificationKind::InlineEnum => {
            let enum_name = format!(
                "{}{}",
                entity_name,
                codegraph_naming::to_pascal_case(&prop.name)
            );
            // Create a basic enum definition; real values come from the codelist data.
            // The context builder will refine this when codelist data is available.
            let values = vec![ProtoEnumValue {
                name: format!("{}_UNSPECIFIED", enum_name.to_uppercase()),
                number: 0,
            }];
            enums.push(ProtoEnumDef {
                name: enum_name.clone(),
                values,
            });
            Some(enum_name)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_import_path_maps_every_wkt_file() {
        assert_eq!(
            well_known_import_path("google.protobuf.Empty").as_deref(),
            Some("google/protobuf/empty.proto")
        );
        assert_eq!(
            well_known_import_path("google.protobuf.Timestamp").as_deref(),
            Some("google/protobuf/timestamp.proto")
        );
        assert_eq!(
            well_known_import_path("google.protobuf.Struct").as_deref(),
            Some("google/protobuf/struct.proto")
        );
        assert_eq!(
            well_known_import_path("google.protobuf.FieldMask").as_deref(),
            Some("google/protobuf/field_mask.proto")
        );
        assert_eq!(well_known_import_path("Candidate"), None);
    }

    #[test]
    fn service_method_wkt_slots_collect_their_imports() {
        let methods = build_service_methods(
            "Candidate",
            &["delete".to_string()],
            false,
            false,
            false,
            &None,
        );
        let mut imports: Vec<String> = Vec::new();
        push_well_known_imports(&mut imports, &methods);
        assert_eq!(imports, vec!["google/protobuf/empty.proto".to_string()]);
    }

    /// `SearchResult` is declared in the `shared` package, so the
    /// semantic-search slot must be package-qualified (issue #793).
    #[test]
    fn semantic_search_returns_the_shared_search_result() {
        let methods = build_service_methods("Candidate", &[], false, true, false, &None);
        let method = methods
            .iter()
            .find(|m| m.name == "SemanticSearch")
            .expect("semantic search method");
        assert_eq!(method.output_type, "shared.SearchResult");
    }
}
