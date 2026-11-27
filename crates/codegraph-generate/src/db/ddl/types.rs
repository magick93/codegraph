use serde::Serialize;

/// One per-operation minimum role for the role_enforced_* policies (#169).
#[derive(Debug, Clone, Serialize)]
pub struct RoleMinimum {
    pub operation: String,
    pub min_role: String,
}

/// Context for DDL table generation.
///
/// Identifier fields (`ColumnDef.name`, `ForeignKeyDef.column`,
/// `ColumnComment.column`, `CheckConstraint.column`, FTS weighted
/// columns) carry BARE names (#451); quoting for reserved words happens
/// only on the render clone (`quote_ddl_identifiers`).
#[derive(Debug, Clone, Serialize)]
pub struct DdlContext {
    pub schema_name: String,
    pub table_name: String,
    pub display_name: String,
    pub domain: String,
    pub columns: Vec<ColumnDef>,
    pub primary_key: String,
    pub foreign_keys: Vec<ForeignKeyDef>,
    pub check_constraints: Vec<CheckConstraint>,
    pub indexes: Vec<IndexDef>,
    pub has_updated_at: bool,
    pub is_tenant_scoped: bool,
    pub tenant_table: String,
    pub extensions: Vec<String>,
    pub child_tables: Vec<ChildTableDef>,
    pub comments: Vec<ColumnComment>,
    /// Whether this entity has a workflow config (generates process history view).
    pub has_workflow: bool,
    /// Kebab-case resource name for RLS scope checks (e.g. "candidate").
    pub resource_name: String,
    /// Full-text search configuration (tsvector column, GIN index, trigger).
    pub fts: Option<FtsContext>,
    /// Embedding columns for semantic search (pgvector).
    pub embeddings: Vec<EmbeddingContext>,
    /// Whether this entity tracks soft deletes and audit columns.
    pub is_auditable: bool,
    /// Whether the entity has `permissions.scope` configured (#169): gates
    /// the RESTRICTIVE role_enforced_* RLS policies that replace the request
    /// path's permission middleware.
    pub role_enforced: bool,
    /// Per-operation minimum roles for the role_enforced_* policies (#169),
    /// in canonical op order (create, read, update, delete, list). Defaults
    /// are synthesized from the built-in matrix when `permissions.min_roles`
    /// is not configured.
    pub role_minima: Vec<RoleMinimum>,
    /// The configured role hierarchy (#169) — drives `role_rank()` and the
    /// custom-role `ALTER TYPE` values in the RBAC migration.
    pub roles_hierarchy: Vec<String>,
    /// Optional per-row user scoping (#169): when configured, the RLS file
    /// adds the column + auto-set trigger and a RESTRICTIVE
    /// `user_scope_enforced_select` policy.
    pub user_scope_column: Option<String>,
    pub is_codelist: bool,
    /// Whether this entity supports demo data flagging.
    pub has_demo_flag: bool,
    /// Append-only snapshot semantics (issue #284, CDM TradeState pattern):
    /// explicit `entity_config.append_only` OR inferred from the effective
    /// operations excluding update+delete (the same inference the scaffold
    /// grants use). Gates the updated_at column, audit band, BEFORE UPDATE
    /// trigger, and SELECT/INSERT-only grants; child tables inherit.
    pub append_only: bool,
    /// Parent-side coordinates for a CHILD table's tenant backfill (issue
    /// #700). `None` on a parent table (or any non-child context): the
    /// parent's own INSERT trigger always stamps the session org, so it has
    /// no legacy rows to repair and keeps the zero tolerance in its policies.
    pub tenant_backfill: Option<TenantBackfill>,
    /// For a MEDIA child table (issue #701): its parent-FK column, which the
    /// child's RLS migration dedups on (newest row per parent kept, `id DESC`
    /// — the display rule) and then makes UNIQUE, so the upload handler's
    /// `INSERT … ON CONFLICT ({parent_fk}) DO UPDATE` enforces one reference
    /// row per (parent, field). `None` everywhere else: non-media `[0..1]`
    /// children keep their DDL byte-identical (their replace race is a
    /// recorded follow-up).
    #[serde(default)]
    pub media_unique_parent_fk: Option<String>,
}

/// A child table's backfill source (issue #700). Every writer of a child row
/// — the SeaORM `child_insert_sql`, the cornucopia `insert_{entity}_{child}`
/// and `link_{junction}` queries, and the media handler until #690 — omitted
/// `platform_organization_id`, so the DDL's zero default was persisted and
/// the zero-tolerant RLS predicate exposed those rows to every organization.
/// Children now carry the parent's `set_org_id_from_session` trigger, and
/// each child's RLS migration backfills the rows written before it existed
/// from the parent column named here, then drops the tolerance.
#[derive(Debug, Clone, Serialize)]
pub struct TenantBackfill {
    /// Schema of the parent table.
    pub parent_schema: String,
    /// Table name of the parent (the backfill's `FROM`).
    pub parent_table: String,
    /// The child's parent FK column (the backfill's join).
    pub parent_fk_column: String,
}

/// Full-text search context for DDL generation.
#[derive(Debug, Clone, Serialize)]
pub struct FtsContext {
    /// Name of the generated tsvector column (e.g. "search_tsv").
    pub tsvector_column: String,
    /// Postgres text search configuration name (e.g. "english").
    pub language: String,
    /// Columns with their FTS weights.
    pub weighted_columns: Vec<FtsColumnWeight>,
    /// GIN index name.
    pub index_name: String,
}

/// A column participating in full-text search with its weight.
#[derive(Debug, Clone, Serialize)]
pub struct FtsColumnWeight {
    pub column: String,
    /// Postgres tsvector weight: A (highest), B, C, or D (lowest).
    pub weight: String,
}

/// Embedding column context for semantic search DDL generation.
#[derive(Debug, Clone, Serialize)]
pub struct EmbeddingContext {
    /// Source text column name.
    pub source_column: String,
    /// Generated vector column name (e.g. "executive_summary_embedding").
    pub vector_column: String,
    /// Vector dimensions (e.g. 1536).
    pub dimensions: u32,
    /// HNSW index name.
    pub index_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnDef {
    pub name: String,
    pub pg_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub is_primary_key: bool,
    pub is_array: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForeignKeyDef {
    pub column: String,
    /// Column name without PG quotes — safe for use in constraint/index names.
    pub column_name: String,
    pub references_schema: String,
    pub references_table: String,
    pub references_column: String,
    pub on_delete: String,
    /// True when the FK targets a codelist table. Codelists are generated in
    /// a separate band and are not entity tables, so the entity-table FK
    /// filter must not drop them.
    pub is_codelist: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckConstraint {
    pub name: String,
    pub column: String,
    pub expression: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexDef {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChildTableDef {
    pub schema_name: String,
    pub table_name: String,
    pub parent_fk_column: String,
    /// Schema of the parent table (for FK REFERENCES clause in nested children)
    pub parent_schema: String,
    /// Table name of the parent (for FK REFERENCES clause in nested children)
    pub parent_table: String,
    pub columns: Vec<ColumnDef>,
    pub display_name: String,
    pub comments: Vec<ColumnComment>,
    pub foreign_keys: Vec<ForeignKeyDef>,
    pub check_constraints: Vec<CheckConstraint>,
    pub child_tables: Vec<ChildTableDef>,
    /// `true` for a MEDIA reference child table (issue #701): one reference
    /// row per (parent, field), enforced by a UNIQUE index on
    /// `parent_fk_column` that the upload handler's upsert targets. Emitted
    /// in the child's RLS migration (inside its NO FORCE window) for a
    /// tenant-scoped parent, and in the table migration for a global one
    /// (no RLS file exists there).
    pub media_reference: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnComment {
    pub column: String,
    pub comment: String,
}
