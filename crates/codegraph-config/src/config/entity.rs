use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Per-entity workflow configuration.
///
/// When present, the entity participates in a stateful workflow.
/// The generated code provides action endpoints that delegate to a
/// hand-crafted `WorkflowService` trait from the platform runtime.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowConfig {
    /// The field on this entity that holds its workflow status.
    pub status_field: String,
    /// Optional approval status field (dual-status pattern from HR Open).
    pub approval_status_field: Option<String>,
    /// Named workflow states (for documentation/validation).
    #[serde(default)]
    pub states: Vec<String>,
    /// Initial state when entity is created.
    pub initial_state: String,
    /// Terminal states (workflow is complete).
    #[serde(default)]
    pub terminal_states: Vec<String>,
    /// Whether to generate workflow action API endpoints (transition, approve, reject).
    #[serde(default)]
    pub generate_action_endpoints: bool,
    /// State transition map: from_state → \[valid target states\].
    /// When empty, any non-terminal state can transition to any other state.
    #[serde(default)]
    pub transitions: HashMap<String, Vec<String>>,
    /// HR Open codelist name that defines valid status values
    /// (e.g. "RecruitingDocumentStatusCodeList").
    /// When set, a CHECK constraint validates the status column value.
    pub status_codelist: Option<String>,
    /// HR Open codelist name for the approval status field.
    pub approval_status_codelist: Option<String>,
    /// Compound guard conditions for dual-status entities.
    /// Maps "status_value" → required approval_status value
    /// (e.g. "active" requires "Approved").
    #[serde(default)]
    pub dual_status_guards: HashMap<String, String>,
    /// Data guard conditions evaluated by the rule engine.
    #[serde(default)]
    pub data_guards: Vec<DataGuard>,
    /// SLA timer definitions keyed by name.
    #[serde(default)]
    pub timers: HashMap<String, TimerDef>,
    /// Approval chain definitions keyed by name.
    #[serde(default)]
    pub approval_chains: HashMap<String, ApprovalChainDef>,
}

/// A data guard condition evaluated by the rule engine.
#[derive(Debug, Clone, Deserialize)]
pub struct DataGuard {
    pub transition_to: String,
    pub rule: String,
    pub message: String,
}

/// SLA timer definition.
#[derive(Debug, Clone, Deserialize)]
pub struct TimerDef {
    pub trigger_on_enter: String,
    #[serde(rename = "type")]
    pub timer_type: String,
    pub duration_hours: i64,
    pub target_state: Option<String>,
}

/// Approval chain definition for a specific transition.
#[derive(Debug, Clone, Deserialize)]
pub struct ApprovalChainDef {
    pub from: String,
    pub to: String,
    pub steps: Vec<ApprovalStepDef>,
}

/// A single step in an approval chain.
#[derive(Debug, Clone, Deserialize)]
pub struct ApprovalStepDef {
    pub role: String,
    #[serde(default = "default_true")]
    pub required: bool,
    pub timeout_hours: Option<i32>,
    pub auto_delegate: Option<serde_json::Value>,
}

fn default_true() -> bool {
    true
}

/// Configuration for a single entity's API generation.
#[derive(Debug, Clone, Deserialize)]
pub struct EntityConfig {
    /// Source JSON schema path relative to the schema root.
    pub source_schema: Option<String>,
    /// Which CRUD operations are enabled. Defaults to global defaults.
    pub operations: Option<Vec<String>>,
    /// FK field name that defines URL nesting under a parent entity.
    pub parent_ref: Option<String>,
    /// URL path segment override (default: auto-pluralized kebab-case).
    pub path_segment: Option<String>,
    /// utoipa tag for grouping in OpenAPI docs.
    pub tag: Option<String>,
    /// Entity role: "root", "child", or "value_object".
    pub role: Option<String>,
    /// Append-only snapshot semantics (CDM TradeState pattern, issue #284):
    /// the table only ever receives INSERTs — DDL drops the updated_at
    /// column + audit band and the BEFORE UPDATE trigger, grants narrow to
    /// SELECT/INSERT (child tables inherit). When set, `update`/`delete`
    /// MUST NOT appear in the entity's effective operations (parse error).
    pub append_only: Option<bool>,
    /// Parent entity name (for child entities or roots with optional parent nesting).
    pub parent: Option<String>,
    /// DTO configuration overrides.
    #[serde(default)]
    pub dto: DtoConfig,
    /// Workflow configuration (opt-in).
    pub workflow: Option<WorkflowConfig>,
    /// Path to external workflow config file (relative to workspace root).
    pub workflow_file: Option<String>,
    /// Search configuration (full-text search + semantic/embedding search).
    #[serde(default)]
    pub search: SearchConfig,
    /// Columns exposed as JSON:API `?filter[field]=value` query params on the list endpoint.
    /// `None` (default) = auto-discover from graph classifications.
    /// Explicit `[]` = disable filtering for this entity.
    #[serde(default)]
    pub filter_fields: Option<Vec<String>>,
    /// Maximum number of items allowed in a bulk create request.
    /// `None` = inherit from domain defaults or global default (100).
    #[serde(default)]
    pub max_bulk_size: Option<usize>,
    /// Self-referential FK column name for hierarchy/tree queries.
    /// When set, generators produce recursive CTE endpoints and parent FK indexes.
    pub hierarchy_field: Option<String>,
    /// Tree include — resolve related entity data into tree responses.
    /// Each entry adds a LEFT JOIN LATERAL from the via entity to its parent,
    /// returning resolved data as a JSONB field in the tree response.
    /// Requires `hierarchy_field` to be set.
    #[serde(default)]
    pub tree_include: Option<Vec<TreeIncludeConfig>>,
    /// Whether to generate an org-chart SvelteKit page for this entity.
    /// When set on an entity (e.g. OrganizationType), the pipeline produces
    /// (app)/org-chart/+page.server.ts and +page.svelte.
    #[serde(default)]
    pub has_orgchart: bool,
    /// Allowed eager-load include paths for `?include=` query parameter.
    /// Each entry is a relationship path like `"person"`, `"deployment"`,
    /// or `"deployment.position"` (dot-delimited, max 3 levels).
    /// `None` (default) = auto-discover from graph (children + entity-refs).
    /// Explicit `[]` = disable includes for this entity.
    #[serde(default)]
    pub allow_include: Option<Vec<String>>,
    /// Generation mode for this entity's handlers/routes.
    /// "full" (default): generate everything.
    /// "handler_only": generate handler but not router.
    /// "ddd_only": generate DDD layer (repo, command, query) but not API layer.
    /// "none": skip all generation for this entity.
    #[serde(default)]
    pub generation_mode: Option<String>,
    /// Custom per-entity error definitions keyed by error code
    /// (e.g. "DUPLICATE_EMAIL"). These are ingested as additional
    /// ErrorDefinition nodes for the entity's domain.
    #[serde(default)]
    pub errors: std::collections::HashMap<String, ErrorDefConfig>,
    /// Operations that do not require a permission check (public endpoints).
    /// E.g. ["read", "list"] to allow unauthenticated access to read operations.
    #[serde(default)]
    pub public_operations: Option<Vec<String>>,
    /// Consumer-owned Svelte components mounted on this entity's generated
    /// detail page. Each entry is a kebab-case component name (e.g.
    /// "ird-registration-panel") mapped to
    /// `#lib/components/extensions/IrdRegistrationPanel.svelte`. The
    /// component files stay consumer-owned (non-synced), so generated pages
    /// can host custom panels without client-side overlay hacks (#162).
    #[serde(default)]
    pub ui_detail_extensions: Vec<String>,
    /// AT Protocol permission gating. When `scope` is set, the entity's API
    /// routes run through the generated permission middleware.
    #[serde(default)]
    pub permissions: PermissionConfig,
    /// Entity part of the API-key scope string (`{domain}.{entity}.read|write`)
    /// enforced by the generated route-level scope guard. Defaults to the
    /// entity's module name (snake_case table name) at codegen time; override
    /// when the scope vocabulary differs from the module name (the hand-written
    /// platform routes, for example, scope on `tenants` / `api_keys`).
    #[serde(default)]
    pub api_key_scope: Option<String>,
}

impl EntityConfig {
    /// Whether the entity is explicitly marked append-only (issue #284).
    pub fn is_append_only(&self) -> bool {
        self.append_only.unwrap_or(false)
    }
}

/// AT Protocol permission gating configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionConfig {
    /// Permission scope prefix, e.g. "support:support-plan". Ops are appended:
    /// ":create", ":read", ":update", ":delete", ":list".
    pub scope: Option<String>,
    /// When true, read-by-id / update / delete require record-level authorization
    /// (the AuthorizationService resolves the target record's owner DID).
    #[serde(default)]
    pub record_scoped: bool,
    /// Per-operation minimum role for the DB-level `role_enforced_*` RLS
    /// policies (#169). Keys are operations ("create", "read", "update",
    /// "delete", "list"), values are role names ranked by the `[rbac]`
    /// roles_hierarchy. When absent, defaults are synthesized from the
    /// built-in matrix: delete → "manager", create/update → "member",
    /// read/list → "employee".
    #[serde(default)]
    pub min_roles: Option<HashMap<String, String>>,
    /// Optional per-row user scoping (#169, ported from hr-specs' RBAC fork):
    /// names a UUID column that is auto-stamped from `app.user_id` on INSERT;
    /// a RESTRICTIVE `user_scope_enforced_select` policy then limits reads of
    /// those rows to the owning user unless the caller's role ranks above
    /// "member".
    #[serde(default)]
    pub user_scope_column: Option<String>,
}

/// Role hierarchy for the DB-level role policies (#169). Roles earlier in the
/// list outrank later ones. Custom roles (e.g. "hr_admin") are added to the
/// `basejump.account_role` enum by the generated RBAC migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RbacConfig {
    #[serde(default)]
    pub roles_hierarchy: Option<Vec<String>>,
}

impl RbacConfig {
    /// The hierarchy in force: configured roles, else the basejump defaults.
    pub fn hierarchy_or_default(&self) -> Vec<String> {
        self.roles_hierarchy
            .clone()
            .unwrap_or_else(default_roles_hierarchy)
    }
}

/// Default role hierarchy — the basejump roles the fixed matrix was built on.
pub fn default_roles_hierarchy() -> Vec<String> {
    vec![
        "owner".to_string(),
        "manager".to_string(),
        "member".to_string(),
        "employee".to_string(),
    ]
}

/// Default per-operation minimum roles — the built-in matrix expressed as
/// hierarchy minima ("delete needs manager or better", etc.).
pub fn default_min_roles() -> Vec<(&'static str, &'static str)> {
    vec![
        ("create", "member"),
        ("read", "employee"),
        ("update", "member"),
        ("delete", "manager"),
        ("list", "employee"),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDefConfig {
    pub description: String,
    pub http_status: i32,
}

/// Configuration for resolving a related entity into a tree response.
/// The pipeline joins from the `via_entity` table (which references the hierarchy
/// entity via an FK) to the `via_entity`'s parent (via its `parent_ref`),
/// and returns the resolved data under the given `alias`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeIncludeConfig {
    /// The entity type to join through (e.g. "DeploymentType").
    /// Must be a generated entity with a `parent_ref` and an FK to the hierarchy entity.
    pub via_entity: String,
    /// Field name in the tree response JSON (e.g. "deployed_worker").
    pub alias: String,
}

fn default_fts_language() -> String {
    "english".to_string()
}

fn default_fts_rest_mode() -> String {
    "query_param".to_string()
}

fn default_embedding_dimensions() -> u32 {
    1536
}

/// Per-entity search configuration.
///
/// When `fts_columns` is `None`, full-text search columns are auto-discovered
/// from the graph (all `TEXT` data columns). Set to an explicit empty vec `[]`
/// to disable FTS for this entity.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchConfig {
    /// Columns to include in the tsvector. `None` = auto-discover from graph.
    /// Explicit empty `[]` = disable FTS.
    #[serde(default)]
    pub fts_columns: Option<Vec<String>>,
    /// Per-column FTS weight (A/B/C/D). Unspecified columns default to D.
    #[serde(default)]
    pub fts_weights: HashMap<String, String>,
    /// Postgres text search configuration name (default: "english").
    #[serde(default = "default_fts_language")]
    pub fts_language: String,
    /// Columns to generate embedding vectors for (opt-in only, never auto-discovered).
    #[serde(default)]
    pub embedding_columns: Vec<String>,
    /// Vector dimensions for pgvector (default: 1536 for OpenAI text-embedding-ada-002).
    #[serde(default = "default_embedding_dimensions")]
    pub embedding_dimensions: u32,
    /// REST surface for full-text search. "query_param" (default) exposes FTS
    /// via ?q= on the list route; "dedicated" generates a standalone
    /// GET /{entity}/search endpoint with a typed SearchParams struct;
    /// "both" enables both surfaces.
    #[serde(default = "default_fts_rest_mode")]
    pub fts_rest_mode: String,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            fts_columns: None,
            fts_weights: HashMap::new(),
            fts_language: default_fts_language(),
            fts_rest_mode: default_fts_rest_mode(),
            embedding_columns: Vec::new(),
            embedding_dimensions: default_embedding_dimensions(),
        }
    }
}

/// DTO field configuration for an entity.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DtoConfig {
    /// Fields that cannot be changed after creation (excluded from Update DTO).
    #[serde(default)]
    pub immutable_fields: Vec<String>,
    /// Fields excluded from the list/summary response.
    #[serde(default)]
    pub list_exclude: Vec<String>,
    /// Fields that should be included in the list response even if normally excluded.
    #[serde(default)]
    pub list_include: Vec<String>,
    /// Fields to expand in the detail response (show related entity inline).
    #[serde(default)]
    pub expand_in_response: Vec<String>,
    /// Custom field grouping for documentation/SDK clarity.
    #[serde(default)]
    pub groups: HashMap<String, Vec<String>>,
}
