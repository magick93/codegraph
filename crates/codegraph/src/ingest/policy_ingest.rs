use codegraph_config::config::DomainConfig;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{
    AuditPolicy, Cardinality, DeletionPropagation, ForeignKeySpec, Ownership, PolicyKind, PolicyNode,
    PropagationRule, PropagationTrigger, RelationshipNode, RowOperation, RowSecurityPolicy,
    SoftDeleteMarker, SoftDeletePolicy, SoftDeleteVisibility, TenantIsolationPolicy,
    TenantPropagation, TenantStrategy,
};

use crate::error::{Error, Result};

pub async fn ingest_policies_and_relationships(
    ingestor: &dyn GraphIngestor,
    config: &DomainConfig,
) -> Result<()> {
    for (domain_name, domain_entry) in &config.domains {
        for (entity_name, entity_config) in &domain_entry.entity_config {
            let schema_title = entity_name.clone();

            let policies = &entity_config.policies;
            if let Some(soft_delete) = &policies.soft_delete {
                let marker = if soft_delete.marker == "is_deleted" {
                    SoftDeleteMarker::Boolean(soft_delete.marker.clone())
                } else if soft_delete.marker == "status" {
                    SoftDeleteMarker::Status(soft_delete.marker.clone())
                } else {
                    SoftDeleteMarker::Timestamp(soft_delete.marker.clone())
                };
                let visibility = match soft_delete.visibility.as_str() {
                    "include_by_default" => SoftDeleteVisibility::IncludeByDefault,
                    "explicit_only" => SoftDeleteVisibility::ExplicitOnly,
                    _ => SoftDeleteVisibility::ExcludeByDefault,
                };
                let cascade = match soft_delete.cascade.as_str() {
                    "cascade" => DeletionPropagation::Cascade,
                    "soft_cascade" => DeletionPropagation::SoftCascade,
                    "ignore" => DeletionPropagation::Ignore,
                    _ => DeletionPropagation::Restrict,
                };
                let policy = PolicyNode {
                    name: format!("{}_{}_soft_delete", domain_name, entity_name),
                    kind: PolicyKind::SoftDelete(SoftDeletePolicy {
                        marker,
                        visibility,
                        cascade,
                    }),
                    target_schema: schema_title.clone(),
                    domain: Some(domain_name.clone()),
                };
                ingestor.ingest_policy(&policy).await.map_err(Error::Graph)?;
            }

            if let Some(tenant_isolation) = &policies.tenant_isolation {
                let strategy = match tenant_isolation.strategy.as_str() {
                    "relationship" => TenantStrategy::Relationship {
                        relationship: tenant_isolation
                            .relationship
                            .clone()
                            .unwrap_or_default(),
                    },
                    "schema" => TenantStrategy::Schema,
                    "database" => TenantStrategy::Database,
                    _ => TenantStrategy::Column {
                        property: tenant_isolation
                            .property
                            .clone()
                            .unwrap_or_else(|| "platform_organization_id".to_string()),
                    },
                };
                let propagation = match tenant_isolation.propagation.as_str() {
                    "inherited" => TenantPropagation::Inherited,
                    "derived" => TenantPropagation::Derived,
                    _ => TenantPropagation::Explicit,
                };
                let policy = PolicyNode {
                    name: format!("{}_{}_tenant_isolation", domain_name, entity_name),
                    kind: PolicyKind::TenantIsolation(TenantIsolationPolicy {
                        strategy,
                        propagation,
                    }),
                    target_schema: schema_title.clone(),
                    domain: Some(domain_name.clone()),
                };
                ingestor.ingest_policy(&policy).await.map_err(Error::Graph)?;
            }

            if let Some(audit) = &policies.audit {
                let policy = PolicyNode {
                    name: format!("{}_{}_audit", domain_name, entity_name),
                    kind: PolicyKind::Audit(AuditPolicy {
                        track_created: audit.track_created,
                        track_updated: audit.track_updated,
                        track_deleted: audit.track_deleted,
                    }),
                    target_schema: schema_title.clone(),
                    domain: Some(domain_name.clone()),
                };
                ingestor.ingest_policy(&policy).await.map_err(Error::Graph)?;
            }

            for rls in &policies.row_security {
                let operation = match rls.operation.as_str() {
                    "insert" => RowOperation::Insert,
                    "update" => RowOperation::Update,
                    "delete" => RowOperation::Delete,
                    "all" => RowOperation::All,
                    _ => RowOperation::Select,
                };
                let policy = PolicyNode {
                    name: format!(
                        "{}_{}_rls_{}",
                        domain_name, entity_name, rls.operation
                    ),
                    kind: PolicyKind::RowSecurity(RowSecurityPolicy {
                        operation,
                        using_expr: rls.using_expr.clone(),
                        check_expr: rls.check_expr.clone(),
                    }),
                    target_schema: schema_title.clone(),
                    domain: Some(domain_name.clone()),
                };
                ingestor.ingest_policy(&policy).await.map_err(Error::Graph)?;
            }

            for (rel_name, rel_config) in &entity_config.relationships {
                let cardinality = match rel_config.cardinality.as_str() {
                    "one_to_one" => Cardinality::OneToOne,
                    "one_to_many" => Cardinality::OneToMany,
                    "many_to_many" => Cardinality::ManyToMany,
                    _ => Cardinality::ManyToOne,
                };
                let ownership = match rel_config.ownership.as_str() {
                    "owns" => Ownership::Owns,
                    "belongs_to" => Ownership::BelongsTo,
                    _ => Ownership::References,
                };
                let fk = if rel_config.target_schema.is_empty() {
                    None
                } else {
                    Some(ForeignKeySpec {
                        source_column: rel_config
                            .source_column
                            .clone()
                            .unwrap_or_else(|| {
                                format!("{}_id", codegraph_naming::to_snake_case(rel_name))
                            }),
                        target_schema: domain_name.clone(),
                        target_table: codegraph_naming::to_snake_case(
                            rel_config.target_schema.trim_end_matches("Type"),
                        ),
                        target_column: "id".to_string(),
                        on_delete: rel_config.on_delete.clone(),
                        on_update: rel_config.on_update.clone(),
                    })
                };
                let propagation = match rel_config.on_delete.as_str() {
                    "cascade" => vec![PropagationRule {
                        trigger: PropagationTrigger::OnDelete,
                        behavior: DeletionPropagation::Cascade,
                    }],
                    "soft_cascade" => vec![PropagationRule {
                        trigger: PropagationTrigger::OnDelete,
                        behavior: DeletionPropagation::SoftCascade,
                    }],
                    "restrict" => vec![PropagationRule {
                        trigger: PropagationTrigger::OnDelete,
                        behavior: DeletionPropagation::Restrict,
                    }],
                    "ignore" => vec![PropagationRule {
                        trigger: PropagationTrigger::OnDelete,
                        behavior: DeletionPropagation::Ignore,
                    }],
                    _ => vec![],
                };
                let rel = RelationshipNode {
                    name: format!("{}_{}_to_{}", domain_name, entity_name, rel_name),
                    source_schema: schema_title.clone(),
                    target_schema: rel_config.target_schema.clone(),
                    cardinality,
                    ownership,
                    foreign_key: fk,
                    propagation,
                    domain: Some(domain_name.clone()),
                };
                ingestor.ingest_relationship(&rel).await.map_err(Error::Graph)?;
            }
        }
    }
    Ok(())
}
