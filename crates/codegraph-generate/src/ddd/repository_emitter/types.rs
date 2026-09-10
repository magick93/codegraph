use crate::filter_fields::{FilterFieldInfo, NestedFilterFieldInfo};

/// Resolved tree_include entry with concrete table/column names.
#[derive(Debug)]
pub struct TreeIncludeResolved {
    /// Response field alias (e.g. "deployed_worker").
    pub alias: String,
    /// Schema-qualified via table name (e.g. "common.deployment").
    pub via_table: String,
    /// FK column on via_entity referencing the hierarchy entity (e.g. "position_id").
    pub via_fk_column: String,
    /// Schema-qualified parent table name (e.g. "common.worker").
    pub parent_table: String,
    /// FK column on via_entity referencing its parent (e.g. "worker_type_id").
    pub parent_ref_column: String,
    /// Worker detail JOIN chain: (schema.table, fk_column_on_child, referenced_column_on_parent).
    /// Built from the composition tree of the parent entity.
    pub worker_detail_joins: Vec<(String, String, String)>,
}

/// Tree representation of an entity's value object structure.
#[allow(dead_code)]
#[derive(Debug)]
pub struct EntityTree {
    pub entity_name: String,
    pub module_name: String,
    pub schema_name: String,
    pub table_name: String,
    /// Domain-prefixed entity module name: `{schema_name}_{table_name}`.
    pub entity_module: String,
    pub direct_columns: Vec<TreeColumn>,
    pub child_tables: Vec<ChildTableInfo>,
    /// Junction (many-to-many) tables for array-of-entity-ref properties that
    /// lack a back-reference column on the target schema.
    pub junction_tables: Vec<JunctionTableInfo>,
    pub has_create: bool,
    pub has_read: bool,
    pub has_update: bool,
    pub has_delete: bool,
    pub has_workflow: bool,
    pub has_fts: bool,
    pub has_embeddings: bool,
    pub fts_language: String,
    pub is_auditable: bool,
    /// Soft-delete visibility mode: "exclude_by_default", "include_by_default", or "explicit_only".
    pub soft_delete_visibility: String,
    /// Soft-delete marker column name (e.g. "deleted_at").
    pub soft_delete_column: Option<String>,
    /// How deletions propagate to children: "restrict", "cascade", "soft_cascade", or "ignore".
    pub soft_delete_cascade: String,
    /// Whether the audit policy tracks the updating user (for `updated_by` column).
    pub track_updated_user: bool,
    /// Whether the audit policy tracks the deleting user (for `deleted_by` column).
    pub track_deleted_user: bool,
    pub filter_fields: Vec<FilterFieldInfo>,
    pub nested_filter_fields: Vec<NestedFilterFieldInfo>,
    /// FK column for parent-scoped lookups (child entities only).
    pub parent_ref: Option<String>,
    /// Self-referential FK column for tree/hierarchy queries (e.g. "parent_id").
    pub hierarchy_field: Option<String>,
    /// Resolved tree_include entries for JOIN-ing related data into tree responses.
    pub tree_include: Vec<TreeIncludeResolved>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct TreeColumn {
    /// Entity model field name (e.g. `gender_code` for codelist refs).
    pub field_name: String,
    /// PostgreSQL column name (e.g. `gender_code`). May differ from `field_name`
    /// when the snake_case name is a Rust keyword (field gets `r#` prefix).
    pub pg_column_name: String,
    /// DTO field name — the API-facing name (e.g. `gender`).
    /// When None, same as `field_name`.
    pub dto_field_name: Option<String>,
    pub rust_type: String,
    pub is_nullable: bool,
    /// Whether this column is an entity reference FK (excluded from Response DTO construction)
    pub is_entity_ref: bool,
    /// When the DTO uses a different type than the entity column (e.g. codelist enum
    /// `CurrencyCodeList` vs entity `String`), this holds the DTO type name.
    /// The emitter uses `.to_string()` for DTO→entity and `.parse()` for entity→DTO.
    pub dto_rust_type: Option<String>,
    /// Whether this column is a workflow-managed field (status, approval_status).
    /// These are excluded from create/update but included in responses.
    pub is_workflow_managed: bool,
    /// Whether this is an array column (Vec<T>). When true AND dto_rust_type is set,
    /// conversion needs .into_iter().map(|v| v.to_string()).collect() instead of .to_string().
    pub is_array: bool,
    /// When this column is a PostgreSQL range type, holds the lowercased PG cast
    /// (e.g. `"tstzrange"`) so INSERT/UPDATE SQL can include `$N::tstzrange`.
    pub pg_cast: Option<String>,
    /// True when this column was synthesised by composite-range collapsing
    /// (start/end → TSTZRANGE). These columns exist in the entity model and DDL
    /// but are NOT present on DTOs, so create/update/response must skip them.
    pub is_composite_range: bool,
    /// True when this column is a StructuredWrapper field stored as JSONB.
    /// The entity model holds `serde_json::Value` but the DTO may use
    /// `Vec<serde_json::Value>` when the property is an array. Emits
    /// serialization/deserialization conversions between the two.
    pub is_structured_wrapper: bool,
    /// True when this column is a media URL/MIME-type field managed by the
    /// dedicated upload/download handlers. Included in responses but excluded
    /// from create/update commands since media is set via separate endpoints.
    pub is_media: bool,
}

impl TreeColumn {
    /// Returns the DTO field name (falls back to `field_name`).
    pub fn dto_name(&self) -> &str {
        self.dto_field_name.as_deref().unwrap_or(&self.field_name)
    }
}

/// Tracks a child (value object) table that the repository must persist and read.
#[allow(dead_code)]
#[derive(Debug)]
pub struct ChildTableInfo {
    /// Rust field name on parent DTO (e.g. "person_name")
    pub field_name: String,
    /// DTO struct name prefix (e.g. "CandidatePersonName")
    pub struct_name: String,
    /// SQL table name (e.g. "candidate_person_name")
    pub sql_table_name: String,
    /// SQL schema name (e.g. "recruiting")
    pub sql_schema_name: String,
    /// Parent FK column (e.g. "candidate_id")
    pub parent_fk_column: String,
    /// Whether this child is an array (Vec) or single (Option)
    pub is_array: bool,
    /// Columns in the child table (excluding id and parent FK)
    pub columns: Vec<ChildColumn>,
    /// Nested child tables (ValueObject properties within this child table)
    pub child_tables: Vec<ChildTableInfo>,
}

/// Tracks a junction (many-to-many) table persisted and read via raw SQL.
#[allow(dead_code)]
#[derive(Debug)]
pub struct JunctionTableInfo {
    /// Rust field name on parent DTO — raw junction property name, no `_id`
    /// suffix (e.g. "settlor_ids" → Vec<uuid::Uuid>).
    pub field_name: String,
    /// SQL table name — `<parent_table>_<raw_field>` (e.g. "trust_settlor_ids"),
    /// matching the DDL junction-table formula exactly.
    pub sql_table_name: String,
    /// SQL schema name (e.g. "core")
    pub sql_schema_name: String,
    /// Parent FK column (e.g. "case_id")
    pub parent_fk_column: String,
    /// Child FK column (e.g. "party_id")
    pub child_fk_column: String,
    /// Whether the schema marks the array property as required
    pub is_required: bool,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct ChildColumn {
    pub field_name: String,
    pub pg_column_name: String,
    pub rust_type: String,
    pub is_nullable: bool,
    /// When the DTO uses a different type than the entity column (e.g. codelist enum
    /// `GenderCodeList` vs entity `String`), this holds the DTO type name.
    pub dto_rust_type: Option<String>,
    /// When this column is a PostgreSQL range type, holds the lowercased PG cast
    /// (e.g. `"tstzrange"`) so INSERT SQL can include `$N::tstzrange`.
    pub pg_cast: Option<String>,
}
