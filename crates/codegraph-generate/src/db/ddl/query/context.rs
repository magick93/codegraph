use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

use crate::db::ddl::DdlGenerator;
use crate::db::ddl::accumulators::DdlAccumulators;
use crate::db::ddl::types::{ChildTableDef, DdlContext, RoleMinimum};
use crate::error::Result;

use super::artifacts::{
    add_hierarchy_artifacts, add_parent_fk, add_property_ref_columns,
    add_timestamp_and_tenant_columns, add_tree_columns, dedup_ddl_artifacts, generated_table_set,
    retain_generated_fks,
};
use super::children::{composition_node_to_child_table, flatten_child_tables};
#[cfg(debug_assertions)]
use super::enrich::warn_on_ddl_dto_gap;
use super::enrich::{
    apply_workflow_defaults, build_embedding_contexts, build_fts_context,
    detect_extensions_from_columns,
};

impl DdlGenerator {
    /// Build this entity's DDL context.
    ///
    /// `pub(crate)` because it IS the shared trigger enumeration: the
    /// semantic event model (`crate::events::enumerate_event_trigger_tables`)
    /// calls it per generation-order entry so its per-domain trigger
    /// publications reproduce the per-entity event-trigger set byte for
    /// byte, by construction.
    pub(crate) async fn query_ddl_context(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
    ) -> Result<DdlContext> {
        let schema = db
            .get_schema_in_domain(schema_title, domain)
            .await?
            .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

        let schema_name = domain.to_string();
        let table_name = schema.pg_table_name.clone();
        let display_name = schema.rust_type_name.clone();

        // Get the pre-built composition tree — it already contains resolved columns,
        // FK targets, check constraint values, composite ranges, and child nodes
        // for ValueObject properties.
        let tree = db.get_composition_tree(schema_title).await?;
        let root = &tree.root;

        let mut artifacts = DdlAccumulators::new(root);

        // Inject FK column for parent-child relationships detected from the schema graph.
        // Honor the schema's `required` when the FK corresponds to a real property;
        // synthetic ArrayItems FKs (no child-side property) stay nullable.
        let entity_cfg = config
            .domains
            .get(domain)
            .and_then(|d| d.get_entity_config(&schema.rust_type_name));
        add_parent_fk(
            db,
            config,
            domain,
            schema_title,
            entity_cfg,
            &self.parent_candidates,
            &mut artifacts,
        )
        .await;

        // Inject self-referential FK column for hierarchy entities
        add_hierarchy_artifacts(entity_cfg, &mut artifacts, &schema_name, &table_name);

        // Convert tree columns to DDL artifacts — no graph queries needed
        add_tree_columns(root, &table_name, &mut artifacts);

        // Filter FK constraints: only keep FKs whose target table belongs to
        // the generation order's table universe — the set of tables that
        // will actually have migrations.
        // This prevents phantom FK constraints to:
        // - VO types classified as entities by the auto-classifier but not in any domain's entities list
        // - Excluded types that exist in a different domain
        // - Cross-domain refs pointing to non-existent schemas (e.g. "jdx")
        // Build (schema, table) pairs — a FK is valid only if its target
        // (references_schema, references_table) matches a generated table.
        let generated_tables = generated_table_set(db, config).await?;
        retain_generated_fks(&mut artifacts.foreign_keys, &generated_tables);

        // Query graph properties for entity-reference columns that the composition
        // tree may have missed (e.g. cross-domain references where the target schema
        // can't be resolved). The entity model generator includes these columns,
        // so the DDL must match to avoid "column does not exist" at runtime.
        //
        // ValueObject properties are handled by the composition tree as child
        // tables (not FK columns on the parent), so we only emit FK columns for
        // EntityReference properties whose target schema actually has a table
        // (i.e. is an entity configured in some domain). This prevents phantom
        // FK constraints to VO types that were force-classified but never
        // generated as entities, or to entities excluded in their domain.
        add_property_ref_columns(
            db,
            schema_title,
            &schema_name,
            &generated_tables,
            &mut artifacts,
        )
        .await;

        // Convert child CompositionNodes → ChildTableDefs
        let mut child_tables: Vec<ChildTableDef> = root
            .children
            .iter()
            .map(|child| {
                composition_node_to_child_table(
                    child,
                    &table_name,
                    &schema_name,
                    &display_name,
                    &generated_tables,
                )
            })
            .collect();

        // Append-only snapshot semantics (issue #284): explicit config flag
        // OR the same operations inference the scaffold grants use
        // (effective operations exclude update AND delete). The explicit
        // flag with update/delete ops is a parse-time config error
        // (validate_append_only_config), so the two sources agree.
        let entity_cfg_early = config
            .domains
            .get(domain)
            .and_then(|d| d.get_entity_config(schema_title));
        let append_only = entity_cfg_early.is_some_and(|ec| ec.is_append_only()) || {
            let ops =
                crate::api::api_model::resolve_entity_operations(db, config, domain, schema_title)
                    .await;
            !ops.iter().any(|op| op == "update" || op == "delete")
        };

        // Add standard timestamp columns and determine tenancy
        let (has_updated_at, is_tenant_scoped) = add_timestamp_and_tenant_columns(
            &mut artifacts.columns,
            &mut child_tables,
            config,
            &table_name,
            append_only,
        );

        // Deduplicate columns by name — CompositeWrapper expansion from
        // allOf-inherited properties can produce duplicate expanded columns.
        // Deduplicate foreign keys by constraint name — cross-domain
        // schema merging via allOf produces duplicate FK definitions.
        // Deduplicate comments by column name — same root cause.
        dedup_ddl_artifacts(&mut artifacts);

        // Query required extensions
        let mut extensions: Vec<String> = db
            .get_required_extensions(schema_title)
            .await
            .unwrap_or_default()
            .iter()
            .map(|ext| ext.name.clone())
            .collect();

        let domain = schema_name.clone();

        // Check if this entity has a workflow config
        let has_workflow =
            apply_workflow_defaults(config, &domain, schema_title, &mut artifacts.columns);

        let resource_name = table_name.replace('_', "-");

        // Flatten the recursive child table tree into a depth-first ordered list.
        // The template iterates this flat list; each entry carries its own parent info.
        let flat_child_tables = flatten_child_tables(child_tables);

        // Build search infrastructure (FTS + embeddings) from config + graph metadata
        let search_config = config
            .domains
            .get(&domain)
            .and_then(|d| d.get_entity_config(schema_title))
            .map(|ec| &ec.search);

        let fts = build_fts_context(search_config, &artifacts.columns, &table_name);

        let embeddings = build_embedding_contexts(search_config, &table_name);

        // Add pgvector extension if embeddings are configured
        if !embeddings.is_empty() && !extensions.contains(&"vector".to_string()) {
            extensions.push("vector".to_string());
        }

        // Detect extensions required by column types (safety net for transitive refs
        // that the ingestion pass may miss, e.g. PersonType → AddressType → GeoType).
        for ext in detect_extensions_from_columns(&artifacts.columns, &flat_child_tables) {
            if !extensions.contains(&ext) {
                extensions.push(ext);
            }
        }

        let is_auditable = config
            .domains
            .get(&domain)
            .and_then(|d| d.auditable)
            .unwrap_or(true)
            && !append_only;

        // Role enforcement (#169): same gate the router used for its
        // permission layers — entities with `permissions.scope` configured —
        // plus any entity carrying explicit `min_roles`. The per-op minima
        // default to the built-in matrix when not configured.
        let entity_cfg = entity_cfg_early;
        let min_roles_cfg = entity_cfg.and_then(|ec| ec.permissions.min_roles.clone());
        let role_enforced = entity_cfg
            .map(|ec| {
                ec.permissions
                    .scope
                    .as_ref()
                    .map(|s| !s.is_empty())
                    .unwrap_or(false)
                    || min_roles_cfg
                        .as_ref()
                        .map(|m| !m.is_empty())
                        .unwrap_or(false)
            })
            .unwrap_or(false);

        let roles_hierarchy = config
            .rbac
            .as_ref()
            .map(|r| r.hierarchy_or_default())
            .unwrap_or_else(codegraph_config::config::default_roles_hierarchy);

        // Canonical op order; explicit config wins over the synthesized
        // defaults.
        let mut role_minima: Vec<RoleMinimum> = Vec::new();
        for (op, default_min) in codegraph_config::config::default_min_roles() {
            let min_role = min_roles_cfg
                .as_ref()
                .and_then(|m| m.get(op))
                .cloned()
                .unwrap_or_else(|| default_min.to_string());
            role_minima.push(RoleMinimum {
                operation: op.to_string(),
                min_role,
            });
        }

        let user_scope_column = entity_cfg.and_then(|ec| ec.permissions.user_scope_column.clone());

        // Detect whether this entity has a _codelist.sql migration (codelist seed data).
        // The codelist generator only creates these for codelist entities in the
        // 'common' domain. Entities outside 'common' are always created by the entity
        // DDL generator with id UUID PRIMARY KEY, even if classified as codelists.
        let _has_codelist_seed = schema.is_codelist
            && domain == "common"
            && !db
                .get_enum_values(schema_title)
                .await
                .unwrap_or_default()
                .is_empty();

        let DdlAccumulators {
            columns,
            foreign_keys,
            check_constraints,
            comments,
            indexes,
        } = artifacts;

        // Debug-time invariant (#446 AC): the DDL context must resolve at
        // least the property-derived columns the dto context resolves. A
        // divergence is the audit-only-skeleton signature — generation is
        // silent while the emitted repository/command code reads columns
        // the table and model lack (E0560/E0609/E0599). Loud advisory, not
        // fatal: the dto plane is the reference resolution.
        #[cfg(debug_assertions)]
        warn_on_ddl_dto_gap(db, schema_title, &domain, config, &columns).await;

        Ok(DdlContext {
            schema_name,
            table_name,
            display_name,
            domain,
            columns,
            primary_key: "id".to_string(),
            foreign_keys,
            check_constraints,
            indexes,
            has_updated_at,
            is_tenant_scoped,
            tenant_table: "common.tenant".to_string(),
            extensions,
            child_tables: flat_child_tables,
            comments,
            has_workflow,
            resource_name,
            fts,
            embeddings,
            is_auditable,
            role_enforced,
            role_minima,
            roles_hierarchy,
            user_scope_column,
            has_demo_flag: is_auditable,
            is_codelist: schema.is_codelist,
            append_only,
            // Parent tables have no legacy zero-tenant rows to repair: their
            // own INSERT trigger has always stamped the session org (#700).
            tenant_backfill: None,
        })
    }
}
