use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{
    resolve_field, AuditEffect, AuditTimestampKind, AuditUserKind, PersistenceColumn,
    PersistenceColumnRole, PersistenceEntity, PersistenceEntityRelation, PersistencePolicies,
    PolicyKind, RetentionEffect, RowSecurityEffect, SoftDeleteEffect, SoftDeleteMarker,
    SoftDeletePolicy, SoftDeleteVisibility, TenantIsolationEffect, TenantIsolationPolicy,
    TenantPropagation, TenantStrategy,
};
use codegraph_type_contracts::RefClassificationKind;

use crate::error::Result;
use crate::generate::pg_cast_for_type;
use codegraph_config::DomainConfig;

use std::collections::HashSet;

/// Build an ORM-agnostic `PersistenceEntity` from the graph querier.
///
/// This is the single source of truth for extracting persistence knowledge
/// (columns, children, relations, policies) from the graph. Both SeaORM and
/// Cornucopia backends consume this IR and render it in their own way.
pub async fn build_persistence_entity(
    db: &dyn GraphQuerier,
    schema_title: &str,
    domain: &str,
    config: &DomainConfig,
    parent_candidates: &[codegraph_core::types::ParentCandidate],
) -> Result<PersistenceEntity> {
    let schema = db
        .get_schema_in_domain(schema_title, domain)
        .await?
        .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

    let table_name = &schema.pg_table_name;
    let schema_name = domain;
    let rust_type = &schema.rust_type_name;

    let all_props = db.get_properties(schema_title).await?;

    // Deduplicate properties by field name
    let mut props = {
        let mut seen = HashSet::new();
        all_props
            .into_iter()
            .filter(|p| seen.insert(p.rust_field_name.clone()))
            .collect::<Vec<_>>()
    };
    codegraph_core::types::inject_codelist_properties(&mut props, schema.is_codelist, domain);

    // Query policies
    let policies = db.get_policies_for_schema(schema_title).await?;
    let (audit_policy, soft_delete_policy, tenant_policy) = split_policy_kinds(&policies);

    let (composite_range, consumed_fields) = query_range_inputs(db, schema_title).await;

    let mut columns = initial_columns(composite_range.as_ref());

    // ── Parent FK injection ──────────────────────────────────────────────
    let entity_cfg = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(rust_type));
    if let Some(fk_field) = crate::generate::resolve_parent_fk_column(
        schema_title,
        parent_candidates,
        entity_cfg,
        &config.defaults.type_suffix,
    ) {
        push_parent_fk_column(&mut columns, &props, fk_field);
    }

    // ── Hierarchy field ──────────────────────────────────────────────────
    push_hierarchy_column(
        &mut columns,
        entity_cfg.and_then(|ec| ec.hierarchy_field.clone()),
    );

    // Collect entity titles for FK-on-VO detection
    let entity_titles: HashSet<String> = config
        .domains
        .values()
        .flat_map(|d| d.entities.iter().cloned())
        .collect();

    // ── Property-to-column classification ────────────────────────────────
    push_property_columns(
        db,
        &props,
        schema_title,
        &consumed_fields,
        &entity_titles,
        &mut columns,
    )
    .await?;

    // ── Deduplicate columns by field_name ─────────────────────────────────
    dedup_columns(&mut columns);

    // ── Tenant column ────────────────────────────────────────────────────
    add_tenant_column(&mut columns, tenant_policy, table_name, config);

    // ── Audit timestamp columns ──────────────────────────────────────────
    add_audit_timestamp_columns(&mut columns, audit_policy);

    // ── Soft-delete marker column ────────────────────────────────────────
    let soft_delete_marker_field = add_soft_delete_marker(&mut columns, soft_delete_policy);

    // ── Additional audit columns ─────────────────────────────────────────
    let is_auditable = audit_tracking_enabled(
        audit_policy,
        config.domains.get(domain).and_then(|d| d.auditable),
    );

    if is_auditable {
        push_audit_user_columns(&mut columns, soft_delete_marker_field.as_deref());
    }

    // ── Final dedup ──────────────────────────────────────────────────────
    dedup_columns(&mut columns);

    // ── Build policy effects ─────────────────────────────────────────────
    let policies = build_policy_effects(&policies, soft_delete_policy, tenant_policy, audit_policy);

    // ── Build relations (self-referential hierarchy only for now) ────────
    let relations = build_hierarchy_relations(
        entity_cfg.and_then(|ec| ec.hierarchy_field.clone()),
        schema_name,
        table_name,
    );

    // ── Collect child tables (deferred — child entities are built lazily) ─
    let child_tables = Vec::new();

    Ok(PersistenceEntity {
        title: schema_title.into(),
        table_name: table_name.clone(),
        schema_name: schema_name.into(),
        rust_type_name: rust_type.clone(),
        columns,
        child_tables,
        relations,
        policies,
    })
}

fn split_policy_kinds(
    policies: &[codegraph_core::types::PolicyNode],
) -> (
    Option<&codegraph_core::types::AuditPolicy>,
    Option<&SoftDeletePolicy>,
    Option<&TenantIsolationPolicy>,
) {
    let audit_policy: Option<&codegraph_core::types::AuditPolicy> =
        policies.iter().find_map(|p| match &p.kind {
            PolicyKind::Audit(a) => Some(a),
            _ => None,
        });
    let soft_delete_policy: Option<&SoftDeletePolicy> =
        policies.iter().find_map(|p| match &p.kind {
            PolicyKind::SoftDelete(sd) => Some(sd),
            _ => None,
        });
    let tenant_policy: Option<&TenantIsolationPolicy> =
        policies.iter().find_map(|p| match &p.kind {
            PolicyKind::TenantIsolation(ti) => Some(ti),
            _ => None,
        });
    (audit_policy, soft_delete_policy, tenant_policy)
}

async fn query_range_inputs(
    db: &dyn GraphQuerier,
    schema_title: &str,
) -> (
    Option<codegraph_core::types::CompositeRange>,
    HashSet<String>,
) {
    let composite_range = db.get_composite_range(schema_title).await.ok().flatten();
    let consumed_fields: HashSet<String> = db
        .get_consumed_fields(schema_title)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(prop, _role)| prop.name)
        .collect();
    (composite_range, consumed_fields)
}

fn initial_columns(
    composite_range: Option<&codegraph_core::types::CompositeRange>,
) -> Vec<PersistenceColumn> {
    let mut columns = Vec::new();

    // ── Primary key ──────────────────────────────────────────────────────
    columns.push(PersistenceColumn {
        field_name: "id".into(),
        column_name: "id".into(),
        rust_type: "Uuid".into(),
        pg_type: "UUID".into(),
        is_primary_key: true,
        is_nullable: false,
        is_jsonb: false,
        is_range: false,
        pg_cast: None,
        role: PersistenceColumnRole::PrimaryKey,
    });

    // ── Composite range column ────────────────────────────────────────────
    if let Some(range) = composite_range {
        columns.push(PersistenceColumn {
            field_name: range.pg_column_name.clone(),
            column_name: range.pg_column_name.clone(),
            rust_type: "Option<String>".into(),
            pg_type: range.pg_type.clone(),
            is_primary_key: false,
            is_nullable: true,
            is_jsonb: false,
            is_range: true,
            pg_cast: pg_cast_for_type(&range.pg_type),
            role: PersistenceColumnRole::Data,
        });
    }

    columns
}

fn push_parent_fk_column(
    columns: &mut Vec<PersistenceColumn>,
    props: &[codegraph_core::types::PropertyNode],
    fk_field: String,
) {
    let prop_is_required = props
        .iter()
        .find(|p| resolve_field(p).column_name == fk_field || p.pg_column_name == fk_field)
        .map(|p| p.is_required)
        .unwrap_or(false);
    let is_nullable = !prop_is_required;
    let rust_type = if is_nullable {
        "Option<Uuid>".into()
    } else {
        "Uuid".into()
    };
    columns.push(PersistenceColumn {
        field_name: fk_field.clone(),
        column_name: fk_field,
        rust_type,
        pg_type: "UUID".into(),
        is_primary_key: false,
        is_nullable,
        is_jsonb: false,
        is_range: false,
        pg_cast: None,
        role: PersistenceColumnRole::ForeignKey {
            ref_entity: String::new(),
        },
    });
}

fn push_hierarchy_column(columns: &mut Vec<PersistenceColumn>, hierarchy_field: Option<String>) {
    if let Some(hf) = hierarchy_field {
        columns.push(PersistenceColumn {
            field_name: hf.clone(),
            column_name: hf,
            rust_type: "Option<Uuid>".into(),
            pg_type: "UUID".into(),
            is_primary_key: false,
            is_nullable: true,
            is_jsonb: false,
            is_range: false,
            pg_cast: None,
            role: PersistenceColumnRole::HierarchyParent,
        });
    }
}

async fn push_property_columns(
    db: &dyn GraphQuerier,
    props: &[codegraph_core::types::PropertyNode],
    schema_title: &str,
    consumed_fields: &HashSet<String>,
    entity_titles: &HashSet<String>,
    columns: &mut Vec<PersistenceColumn>,
) -> Result<()> {
    for prop in props {
        if prop.rust_field_name == "id" {
            continue;
        }
        if consumed_fields.contains(&prop.name) {
            continue;
        }
        let field_def = resolve_field(prop);
        match prop.effective_kind() {
            Some(RefClassificationKind::PrimitiveWrapper)
            | Some(RefClassificationKind::StructuredWrapper)
            | Some(RefClassificationKind::ArrayWrapper)
            | Some(RefClassificationKind::RangeWrapper)
            | Some(RefClassificationKind::InlineEnum) => {
                columns.push(wrapper_column(prop, field_def));
            }
            Some(RefClassificationKind::CodelistReference)
            | Some(RefClassificationKind::CodelistCheck) => {
                if prop.is_array {
                    continue;
                }
                columns.push(codelist_column(prop, field_def));
            }
            Some(RefClassificationKind::EntityReference) => {
                columns.push(entity_ref_column(prop, field_def));
            }
            Some(RefClassificationKind::CompositeWrapper)
            | Some(RefClassificationKind::MediaWrapper) => {
                let comp_cols = composite_wrapper_columns(db, prop, schema_title, field_def).await;
                for col in comp_cols {
                    columns.push(col);
                }
            }
            Some(RefClassificationKind::ValueObject) if !prop.is_array => {
                if let Some(column) =
                    value_object_fk_column(db, prop, schema_title, entity_titles).await?
                {
                    columns.push(column);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn wrapper_column(
    prop: &codegraph_core::types::PropertyNode,
    field_def: codegraph_core::types::FieldDefinition,
) -> PersistenceColumn {
    let is_structured = prop.effective_kind() == Some(RefClassificationKind::StructuredWrapper);
    let is_nullable = !prop.is_required;
    let base_type = if is_structured {
        "serde_json::Value".into()
    } else {
        prop.rust_field_type.clone()
    };
    let rust_type = if is_nullable {
        format!("Option<{base_type}>")
    } else {
        base_type
    };
    let pg_cast = if prop.effective_kind() == Some(RefClassificationKind::RangeWrapper) {
        pg_cast_for_type(&prop.pg_column_type)
    } else {
        None
    };
    let is_range = prop.effective_kind() == Some(RefClassificationKind::RangeWrapper);

    PersistenceColumn {
        field_name: field_def.rust_field_name,
        column_name: field_def.column_name,
        rust_type,
        pg_type: prop.pg_column_type.clone(),
        is_primary_key: false,
        is_nullable,
        is_jsonb: is_structured,
        is_range,
        pg_cast,
        role: PersistenceColumnRole::Data,
    }
}

fn codelist_column(
    prop: &codegraph_core::types::PropertyNode,
    field_def: codegraph_core::types::FieldDefinition,
) -> PersistenceColumn {
    let is_nullable = !prop.is_required;
    let rust_type = if is_nullable {
        "Option<String>".into()
    } else {
        "String".into()
    };
    PersistenceColumn {
        field_name: field_def.rust_field_name,
        column_name: field_def.column_name,
        rust_type,
        pg_type: "TEXT".into(),
        is_primary_key: false,
        is_nullable,
        is_jsonb: false,
        is_range: false,
        pg_cast: None,
        role: PersistenceColumnRole::Data,
    }
}

fn entity_ref_column(
    prop: &codegraph_core::types::PropertyNode,
    field_def: codegraph_core::types::FieldDefinition,
) -> PersistenceColumn {
    let is_nullable = !prop.is_required;
    PersistenceColumn {
        field_name: field_def.rust_field_name,
        column_name: field_def.column_name,
        rust_type: if is_nullable {
            "Option<Uuid>".into()
        } else {
            "Uuid".into()
        },
        pg_type: "UUID".into(),
        is_primary_key: false,
        is_nullable,
        is_jsonb: false,
        is_range: false,
        pg_cast: None,
        role: PersistenceColumnRole::ForeignKey {
            ref_entity: prop.rust_field_type.clone(),
        },
    }
}

async fn composite_wrapper_columns(
    db: &dyn GraphQuerier,
    prop: &codegraph_core::types::PropertyNode,
    schema_title: &str,
    field_def: codegraph_core::types::FieldDefinition,
) -> Vec<PersistenceColumn> {
    let mut out = Vec::new();
    if let Ok(comp_cols) = db.get_composite_columns(&prop.name, schema_title).await {
        for col in &comp_cols {
            let field_name = format!("{}{}", field_def.rust_field_name, col.suffix);
            let column_name = format!("{}{}", field_def.column_name, col.suffix);
            let is_nullable = !prop.is_required;
            let rust_type = if is_nullable {
                format!("Option<{}>", col.rust_type)
            } else {
                col.rust_type.clone()
            };
            out.push(PersistenceColumn {
                field_name,
                column_name,
                rust_type,
                pg_type: col.pg_type.clone(),
                is_primary_key: false,
                is_nullable,
                is_jsonb: false,
                is_range: false,
                pg_cast: crate::generate::pg_cast_for_type(&col.pg_type),
                role: PersistenceColumnRole::Data,
            });
        }
    }
    out
}

async fn value_object_fk_column(
    db: &dyn GraphQuerier,
    prop: &codegraph_core::types::PropertyNode,
    schema_title: &str,
    entity_titles: &HashSet<String>,
) -> Result<Option<PersistenceColumn>> {
    let Some((fk_field, fk_col)) =
        codegraph_core::types::resolve_fk_column_name(db, prop, schema_title, entity_titles)
            .await?
    else {
        return Ok(None);
    };
    Ok(Some(PersistenceColumn {
        field_name: fk_field,
        column_name: fk_col,
        rust_type: "Option<Uuid>".into(),
        pg_type: "UUID".into(),
        is_primary_key: false,
        is_nullable: true,
        is_jsonb: false,
        is_range: false,
        pg_cast: None,
        role: PersistenceColumnRole::ForeignKey {
            ref_entity: String::new(),
        },
    }))
}

fn dedup_columns(columns: &mut Vec<PersistenceColumn>) {
    let mut seen_fields = HashSet::new();
    columns.retain(|c| seen_fields.insert(c.field_name.clone()));
}

fn add_tenant_column(
    columns: &mut Vec<PersistenceColumn>,
    tenant_policy: Option<&TenantIsolationPolicy>,
    table_name: &str,
    config: &DomainConfig,
) {
    let (is_tenant_scoped, tenant_column_name): (bool, String) = if let Some(ti) = tenant_policy {
        match &ti.strategy {
            TenantStrategy::Column { property } => (true, property.clone()),
            _ => (true, String::new()),
        }
    } else {
        (
            !is_global_entity(table_name, config),
            "platform_organization_id".into(),
        )
    };
    if is_tenant_scoped && !tenant_column_name.is_empty() {
        columns.insert(
            1,
            PersistenceColumn {
                field_name: tenant_column_name.clone(),
                column_name: tenant_column_name,
                rust_type: "Uuid".into(),
                pg_type: "UUID".into(),
                is_primary_key: false,
                is_nullable: false,
                is_jsonb: false,
                is_range: false,
                pg_cast: None,
                role: PersistenceColumnRole::TenantScope,
            },
        );
    }
}

fn add_audit_timestamp_columns(
    columns: &mut Vec<PersistenceColumn>,
    audit_policy: Option<&codegraph_core::types::AuditPolicy>,
) {
    if let Some(audit) = audit_policy {
        if audit.track_created {
            columns.push(make_audit_column(
                "created_at",
                "chrono::DateTime<chrono::Utc>",
                "TIMESTAMPTZ",
                AuditTimestampKind::Created,
            ));
        }
        if audit.track_updated {
            columns.push(make_audit_column(
                "updated_at",
                "chrono::DateTime<chrono::Utc>",
                "TIMESTAMPTZ",
                AuditTimestampKind::Updated,
            ));
        }
    } else {
        // Backward compat: always add timestamps
        columns.push(make_audit_column(
            "created_at",
            "chrono::DateTime<chrono::Utc>",
            "TIMESTAMPTZ",
            AuditTimestampKind::Created,
        ));
        columns.push(make_audit_column(
            "updated_at",
            "chrono::DateTime<chrono::Utc>",
            "TIMESTAMPTZ",
            AuditTimestampKind::Updated,
        ));
    }
}

fn add_soft_delete_marker(
    columns: &mut Vec<PersistenceColumn>,
    soft_delete_policy: Option<&SoftDeletePolicy>,
) -> Option<String> {
    let mut soft_delete_marker_field: Option<String> = None;
    if let Some(sd) = soft_delete_policy {
        let (marker_name, marker_type) = match &sd.marker {
            SoftDeleteMarker::Timestamp(name) => (name.clone(), "chrono::DateTime<chrono::Utc>"),
            SoftDeleteMarker::Boolean(name) => (name.clone(), "bool"),
            SoftDeleteMarker::Status(name) => (name.clone(), "String"),
        };
        soft_delete_marker_field = Some(marker_name.clone());
        columns.push(PersistenceColumn {
            field_name: marker_name.clone(),
            column_name: marker_name,
            rust_type: format!("Option<{}>", marker_type),
            pg_type: "TIMESTAMPTZ".into(),
            is_primary_key: false,
            is_nullable: true,
            is_jsonb: false,
            is_range: false,
            pg_cast: None,
            role: PersistenceColumnRole::SoftDeleteMarker,
        });
    }
    soft_delete_marker_field
}

fn audit_tracking_enabled(
    audit_policy: Option<&codegraph_core::types::AuditPolicy>,
    config_auditable: Option<bool>,
) -> bool {
    if let Some(audit) = audit_policy {
        audit.track_deleted
    } else {
        config_auditable.unwrap_or(true)
    }
}

fn push_audit_user_columns(
    columns: &mut Vec<PersistenceColumn>,
    soft_delete_marker_field: Option<&str>,
) {
    if soft_delete_marker_field != Some("deleted_at") {
        columns.push(make_audit_column(
            "deleted_at",
            "Option<chrono::DateTime<chrono::Utc>>",
            "TIMESTAMPTZ",
            AuditTimestampKind::Deleted,
        ));
    }
    columns.push(PersistenceColumn {
        field_name: "deleted_by".into(),
        column_name: "deleted_by".into(),
        rust_type: "Option<Uuid>".into(),
        pg_type: "UUID".into(),
        is_primary_key: false,
        is_nullable: true,
        is_jsonb: false,
        is_range: false,
        pg_cast: None,
        role: PersistenceColumnRole::AuditUser {
            kind: AuditUserKind::DeletedBy,
        },
    });
    columns.push(PersistenceColumn {
        field_name: "updated_by".into(),
        column_name: "updated_by".into(),
        rust_type: "Option<Uuid>".into(),
        pg_type: "UUID".into(),
        is_primary_key: false,
        is_nullable: true,
        is_jsonb: false,
        is_range: false,
        pg_cast: None,
        role: PersistenceColumnRole::AuditUser {
            kind: AuditUserKind::UpdatedBy,
        },
    });
    columns.push(PersistenceColumn {
        field_name: "is_demo_data".into(),
        column_name: "is_demo_data".into(),
        rust_type: "bool".into(),
        pg_type: "BOOLEAN".into(),
        is_primary_key: false,
        is_nullable: false,
        is_jsonb: false,
        is_range: false,
        pg_cast: None,
        role: PersistenceColumnRole::AuditFlag,
    });
}

fn build_policy_effects(
    policies: &[codegraph_core::types::PolicyNode],
    soft_delete_policy: Option<&SoftDeletePolicy>,
    tenant_policy: Option<&TenantIsolationPolicy>,
    audit_policy: Option<&codegraph_core::types::AuditPolicy>,
) -> PersistencePolicies {
    PersistencePolicies {
        soft_delete: soft_delete_policy.map(|sd| SoftDeleteEffect {
            policy_name: "soft_delete".into(),
            marker_column: match &sd.marker {
                SoftDeleteMarker::Timestamp(n)
                | SoftDeleteMarker::Boolean(n)
                | SoftDeleteMarker::Status(n) => n.clone(),
            },
            marker_type: match &sd.marker {
                SoftDeleteMarker::Timestamp(_) => "timestamp".into(),
                SoftDeleteMarker::Boolean(_) => "boolean".into(),
                SoftDeleteMarker::Status(_) => "status".into(),
            },
            visibility: match sd.visibility {
                SoftDeleteVisibility::ExcludeByDefault => "exclude_by_default".into(),
                SoftDeleteVisibility::IncludeByDefault => "include_by_default".into(),
                SoftDeleteVisibility::ExplicitOnly => "explicit_only".into(),
            },
            cascade: format!("{:?}", sd.cascade).to_lowercase(),
        }),
        tenant_isolation: tenant_policy.map(|ti| {
            let (strategy_name, column) = match &ti.strategy {
                TenantStrategy::Column { property } => ("column".into(), Some(property.clone())),
                TenantStrategy::Relationship { relationship } => {
                    ("relationship".into(), Some(relationship.clone()))
                }
                TenantStrategy::Schema => ("schema".into(), None),
                TenantStrategy::Database => ("database".into(), None),
            };
            TenantIsolationEffect {
                policy_name: "tenant_isolation".into(),
                strategy: strategy_name,
                column,
                propagation: match ti.propagation {
                    TenantPropagation::Explicit => "explicit".into(),
                    TenantPropagation::Inherited => "inherited".into(),
                    TenantPropagation::Derived => "derived".into(),
                },
            }
        }),
        row_security: policies
            .iter()
            .filter_map(|p| match &p.kind {
                PolicyKind::RowSecurity(rs) => Some(RowSecurityEffect {
                    operation: format!("{:?}", rs.operation).to_lowercase(),
                    using_expr: rs.using_expr.clone(),
                    check_expr: rs.check_expr.clone(),
                }),
                _ => None,
            })
            .collect(),
        audit: audit_policy.map(|a| AuditEffect {
            track_created: a.track_created,
            track_updated: a.track_updated,
            track_deleted: a.track_deleted,
        }),
        retention: policies.iter().find_map(|p| match &p.kind {
            PolicyKind::Retention(r) => Some(RetentionEffect {
                retention_period_days: r.retention_period_days,
                archive_strategy: r.archive_strategy.clone(),
            }),
            _ => None,
        }),
    }
}

fn build_hierarchy_relations(
    hierarchy_field: Option<String>,
    schema_name: &str,
    table_name: &str,
) -> Vec<PersistenceEntityRelation> {
    let mut relations = Vec::new();
    if let Some(hf) = hierarchy_field {
        let entity_module_name = format!("{}_{}", schema_name, table_name);
        relations.push(PersistenceEntityRelation {
            name: "Parent".into(),
            relation_type: "belongs_to".into(),
            related_entity: entity_module_name,
            from_column: codegraph_naming::to_pascal_case(&hf),
            to_column: "Id".into(),
            is_self_ref: true,
        });
    }
    relations
}

fn make_audit_column(
    name: &str,
    rust_type: &str,
    pg_type: &str,
    kind: AuditTimestampKind,
) -> PersistenceColumn {
    PersistenceColumn {
        field_name: name.into(),
        column_name: name.into(),
        rust_type: rust_type.into(),
        pg_type: pg_type.into(),
        is_primary_key: false,
        is_nullable: matches!(kind, AuditTimestampKind::Deleted),
        is_jsonb: false,
        is_range: false,
        pg_cast: None,
        role: PersistenceColumnRole::AuditTimestamp { kind },
    }
}

fn is_global_entity(_table_name: &str, _config: &DomainConfig) -> bool {
    false
}
