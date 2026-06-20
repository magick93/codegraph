use crate::generate::ProjectConfig;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{ColumnInfo, CompositionNode};
use codegraph_type_contracts::RefClassificationKind;
use serde::Serialize;

use crate::error::Result;
use crate::generate::db::dialect::{db_template_for, dialect_for_target, DatabaseTarget, SqlDialect};
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::{DomainConfig, SearchConfig};

/// PostgreSQL reserved words that must be double-quoted when used as column names.
static PG_RESERVED_WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "all",
        "analyse",
        "analyze",
        "and",
        "any",
        "array",
        "as",
        "asc",
        "asymmetric",
        "authorization",
        "between",
        "binary",
        "both",
        "case",
        "cast",
        "check",
        "collate",
        "collation",
        "column",
        "concurrently",
        "constraint",
        "create",
        "cross",
        "current_catalog",
        "current_date",
        "current_role",
        "current_schema",
        "current_time",
        "current_timestamp",
        "current_user",
        "default",
        "deferrable",
        "desc",
        "distinct",
        "do",
        "else",
        "end",
        "except",
        "false",
        "fetch",
        "for",
        "foreign",
        "freeze",
        "from",
        "full",
        "grant",
        "group",
        "having",
        "ilike",
        "in",
        "initially",
        "inner",
        "intersect",
        "into",
        "is",
        "isnull",
        "join",
        "lateral",
        "leading",
        "left",
        "like",
        "limit",
        "localtime",
        "localtimestamp",
        "natural",
        "not",
        "notnull",
        "null",
        "offset",
        "on",
        "only",
        "or",
        "order",
        "outer",
        "overlaps",
        "placing",
        "primary",
        "references",
        "returning",
        "right",
        "select",
        "session_user",
        "similar",
        "some",
        "symmetric",
        "table",
        "tablesample",
        "then",
        "to",
        "trailing",
        "true",
        "union",
        "unique",
        "user",
        "using",
        "variadic",
        "verbose",
        "when",
        "where",
        "window",
        "with",
        "abort",
        "absolute",
        "access",
        "action",
        "add",
        "admin",
        "after",
        "aggregate",
        "also",
        "alter",
        "always",
        "assertion",
        "assignment",
        "at",
        "attach",
        "attribute",
        "backward",
        "before",
        "begin",
        "by",
        "cache",
        "call",
        "called",
        "cascade",
        "cascaded",
        "catalog",
        "chain",
        "characteristics",
        "checkpoint",
        "class",
        "close",
        "cluster",
        "comment",
        "comments",
        "commit",
        "committed",
        "configuration",
        "conflict",
        "connection",
        "constraints",
        "content",
        "continue",
        "conversion",
        "copy",
        "cost",
        "csv",
        "cube",
        "current",
        "cursor",
        "cycle",
        "data",
        "database",
        "day",
        "deallocate",
        "declare",
        "defaults",
        "deferred",
        "definer",
        "delete",
        "delimiter",
        "delimiters",
        "depends",
        "detach",
        "dictionary",
        "disable",
        "discard",
        "document",
        "domain",
        "double",
        "drop",
        "each",
        "enable",
        "encoding",
        "encrypted",
        "enum",
        "escape",
        "event",
        "exclude",
        "excluding",
        "exclusive",
        "execute",
        "exists",
        "explain",
        "expression",
        "extension",
        "external",
        "family",
        "filter",
        "first",
        "float",
        "following",
        "force",
        "forward",
        "function",
        "functions",
        "generated",
        "global",
        "granted",
        "grouping",
        "groups",
        "handler",
        "header",
        "hold",
        "hour",
        "identity",
        "if",
        "immediate",
        "immutable",
        "implicit",
        "import",
        "include",
        "including",
        "increment",
        "index",
        "indexes",
        "inherit",
        "inherits",
        "inline",
        "input",
        "insensitive",
        "insert",
        "instead",
        "invoker",
        "isolation",
        "key",
        "label",
        "language",
        "large",
        "last",
        "leakproof",
        "level",
        "listen",
        "load",
        "local",
        "location",
        "lock",
        "locked",
        "logged",
        "mapping",
        "match",
        "materialized",
        "maxvalue",
        "method",
        "minute",
        "minvalue",
        "mode",
        "month",
        "move",
        "name",
        "names",
        "new",
        "next",
        "nfc",
        "nfd",
        "nfkc",
        "nfkd",
        "no",
        "none",
        "normalize",
        "normalized",
        "nothing",
        "notify",
        "nowait",
        "nulls",
        "object",
        "of",
        "off",
        "oids",
        "old",
        "operator",
        "option",
        "options",
        "ordinality",
        "others",
        "over",
        "overriding",
        "owned",
        "owner",
        "parallel",
        "parser",
        "partial",
        "partition",
        "passing",
        "password",
        "plans",
        "policy",
        "position",
        "preceding",
        "prepare",
        "prepared",
        "preserve",
        "prior",
        "privileges",
        "procedural",
        "procedure",
        "procedures",
        "program",
        "publication",
        "quote",
        "range",
        "read",
        "reassign",
        "recheck",
        "recursive",
        "ref",
        "referencing",
        "refresh",
        "reindex",
        "relative",
        "release",
        "rename",
        "repeatable",
        "replace",
        "replica",
        "reset",
        "restart",
        "restrict",
        "return",
        "returns",
        "revoke",
        "role",
        "rollback",
        "rollup",
        "routine",
        "routines",
        "row",
        "rows",
        "rule",
        "savepoint",
        "schema",
        "schemas",
        "scroll",
        "search",
        "second",
        "security",
        "sequence",
        "sequences",
        "serializable",
        "server",
        "session",
        "set",
        "sets",
        "share",
        "show",
        "simple",
        "skip",
        "snapshot",
        "sql",
        "stable",
        "standalone",
        "start",
        "statement",
        "statistics",
        "stdin",
        "stdout",
        "storage",
        "stored",
        "strict",
        "strip",
        "subscription",
        "support",
        "sysid",
        "system",
        "tables",
        "temp",
        "template",
        "temporary",
        "text",
        "ties",
        "transaction",
        "transform",
        "trigger",
        "truncate",
        "trusted",
        "type",
        "types",
        "uescape",
        "unbounded",
        "uncommitted",
        "unencrypted",
        "unknown",
        "unlisten",
        "unlogged",
        "until",
        "update",
        "vacuum",
        "valid",
        "validate",
        "validator",
        "value",
        "values",
        "varying",
        "version",
        "view",
        "views",
        "volatile",
        "whitespace",
        "without",
        "work",
        "wrapper",
        "write",
        "xml",
        "year",
        "yes",
        "zone",
    ]
    .into_iter()
    .collect()
});

/// Wrap a column name in double quotes if it's a PostgreSQL reserved word.
fn quote_if_reserved(name: &str) -> String {
    if PG_RESERVED_WORDS.contains(name.to_lowercase().as_str()) {
        format!("\"{}\"", name)
    } else {
        name.to_string()
    }
}

/// Wrap a string value in PostgreSQL dollar-quoting.
/// Uses `$$` by default; if the value contains `$$`, falls back to `$q$...$q$`.
fn dollar_quote(val: &str) -> String {
    if val.contains("$$") {
        format!("$q${}$q$", val)
    } else {
        format!("$${}$$", val)
    }
}

/// Context for DDL table generation.
#[derive(Debug, Serialize)]
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
    /// Whether this entity supports demo data flagging.
    pub has_demo_flag: bool,
}

/// Full-text search context for DDL generation.
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
pub struct FtsColumnWeight {
    pub column: String,
    /// Postgres tsvector weight: A (highest), B, C, or D (lowest).
    pub weight: String,
}

/// Embedding column context for semantic search DDL generation.
#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub struct ColumnDef {
    pub name: String,
    pub pg_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub is_primary_key: bool,
    pub is_array: bool,
}

#[derive(Debug, Serialize)]
pub struct ForeignKeyDef {
    pub column: String,
    /// Column name without PG quotes — safe for use in constraint/index names.
    pub column_name: String,
    pub references_schema: String,
    pub references_table: String,
    pub references_column: String,
    pub on_delete: String,
}

#[derive(Debug, Serialize)]
pub struct CheckConstraint {
    pub name: String,
    pub column: String,
    pub expression: String,
}

#[derive(Debug, Serialize)]
pub struct IndexDef {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
}

#[derive(Debug, Serialize)]
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
}

#[derive(Debug, Serialize)]
pub struct ColumnComment {
    pub column: String,
    pub comment: String,
}

/// DDL artifacts produced from a single column classification.
type DdlArtifacts = (
    Vec<ColumnDef>,
    Vec<ForeignKeyDef>,
    Vec<CheckConstraint>,
    Vec<ColumnComment>,
);

/// Convert a `ColumnInfo` from the composition tree into DDL columns, FKs, and check constraints.
///
/// Returns `None` for ValueObject-classified columns — those are represented as child
/// `CompositionNode`s in the tree, not as columns.
fn column_info_to_ddl(col: &ColumnInfo, table_name: &str) -> Option<DdlArtifacts> {
    let raw_name = &col.name;
    let prop_name = quote_if_reserved(raw_name);
    let description = col.description.as_deref().unwrap_or("");

    let mut columns = Vec::new();
    let mut foreign_keys = Vec::new();
    let mut check_constraints = Vec::new();
    let mut comments = Vec::new();

    match col.classification.as_ref() {
        Some(RefClassificationKind::PrimitiveWrapper)
        | Some(RefClassificationKind::StructuredWrapper)
        | Some(RefClassificationKind::RangeWrapper) => {
            if !description.is_empty() {
                comments.push(ColumnComment {
                    column: prop_name.clone(),
                    comment: description.to_string(),
                });
            }
            let pg_type = if col.postgres_type.is_empty() {
                "TEXT".to_string()
            } else {
                col.postgres_type.clone()
            };
            columns.push(ColumnDef {
                name: prop_name,
                pg_type,
                nullable: col.is_optional,
                default: None,
                is_primary_key: false,
                is_array: false,
            });
        }
        Some(RefClassificationKind::ArrayWrapper) => {
            if !description.is_empty() {
                comments.push(ColumnComment {
                    column: prop_name.clone(),
                    comment: description.to_string(),
                });
            }
            let raw_base = col
                .postgres_type
                .strip_suffix("[]")
                .unwrap_or(&col.postgres_type);
            let pg_type = if raw_base.is_empty() {
                "TEXT".to_string()
            } else {
                raw_base.to_string()
            };
            columns.push(ColumnDef {
                name: prop_name,
                pg_type,
                nullable: col.is_optional,
                default: None,
                is_primary_key: false,
                is_array: true,
            });
        }
        Some(RefClassificationKind::CodelistReference) => {
            // Array codelists are represented as child CompositionNodes, not columns.
            if col.is_array {
                return None;
            }
            let col_name = prop_name.clone();
            if !description.is_empty() {
                comments.push(ColumnComment {
                    column: col_name.clone(),
                    comment: description.to_string(),
                });
            }
            columns.push(ColumnDef {
                name: col_name.clone(),
                pg_type: "TEXT".to_string(),
                nullable: col.is_optional,
                default: None,
                is_primary_key: false,
                is_array: false,
            });
            if let Some(ref fk) = col.fk_target {
                foreign_keys.push(ForeignKeyDef {
                    column_name: raw_name.to_string(),
                    column: col_name,
                    references_schema: fk.schema.clone(),
                    references_table: fk.table.clone(),
                    references_column: fk.column.clone(),
                    on_delete: fk.on_delete.clone(),
                });
            }
        }
        Some(RefClassificationKind::EntityReference) => {
            let col_name = format!("{}_id", raw_name);
            if !description.is_empty() {
                comments.push(ColumnComment {
                    column: col_name.clone(),
                    comment: description.to_string(),
                });
            }
            columns.push(ColumnDef {
                name: col_name.clone(),
                pg_type: "UUID".to_string(),
                nullable: col.is_optional,
                default: None,
                is_primary_key: false,
                is_array: false,
            });
            if let Some(ref fk) = col.fk_target {
                foreign_keys.push(ForeignKeyDef {
                    column_name: col_name.clone(),
                    column: col_name,
                    references_schema: fk.schema.clone(),
                    references_table: fk.table.clone(),
                    references_column: fk.column.clone(),
                    on_delete: fk.on_delete.clone(),
                });
            }
        }
        Some(RefClassificationKind::CodelistCheck) | Some(RefClassificationKind::InlineEnum) => {
            // Array CodelistCheck properties are child CompositionNodes, not columns.
            if col.is_array && col.classification == Some(RefClassificationKind::CodelistCheck) {
                return None;
            }
            if !description.is_empty() {
                comments.push(ColumnComment {
                    column: prop_name.clone(),
                    comment: description.to_string(),
                });
            }
            columns.push(ColumnDef {
                name: prop_name.clone(),
                pg_type: "TEXT".to_string(),
                nullable: col.is_optional,
                default: None,
                is_primary_key: false,
                is_array: col.is_array,
            });
            if !col.is_array && !col.check_values.is_empty() {
                let expr = col
                    .check_values
                    .iter()
                    .map(|v| dollar_quote(v))
                    .collect::<Vec<_>>()
                    .join(", ");
                check_constraints.push(CheckConstraint {
                    name: codegraph_naming::truncate_pg_identifier(&format!(
                        "chk_{}_{}",
                        table_name, raw_name
                    )),
                    column: prop_name.clone(),
                    expression: format!("{} IN ({})", prop_name, expr),
                });
            }
        }
        Some(RefClassificationKind::CompositeWrapper)
        | Some(RefClassificationKind::MediaWrapper) => {
            let is_media = col.classification == Some(RefClassificationKind::MediaWrapper);
            let mut col_names_for_check = Vec::new();
            for comp_col in &col.composite_columns {
                let composite_col_name =
                    quote_if_reserved(&format!("{}{}", raw_name, comp_col.suffix));
                if !description.is_empty() {
                    comments.push(ColumnComment {
                        column: composite_col_name.clone(),
                        comment: description.to_string(),
                    });
                }
                if is_media {
                    col_names_for_check.push(composite_col_name.clone());
                }
                columns.push(ColumnDef {
                    name: composite_col_name,
                    pg_type: comp_col.pg_type.clone(),
                    nullable: col.is_optional,
                    default: None,
                    is_primary_key: false,
                    is_array: false,
                });
            }
            if is_media && col_names_for_check.len() == 2 {
                let nulls = col_names_for_check
                    .iter()
                    .map(|c| format!("{c} IS NULL"))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                let not_nulls = col_names_for_check
                    .iter()
                    .map(|c| format!("{c} IS NOT NULL"))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                check_constraints.push(CheckConstraint {
                    name: format!("chk_{}_complete", raw_name),
                    column: raw_name.clone(),
                    expression: format!("({nulls}) OR ({not_nulls})"),
                });
            }
        }
        Some(RefClassificationKind::ValueObject) => {
            // ValueObjects are represented as child CompositionNodes, not columns.
            return None;
        }
        None => {
            if !description.is_empty() {
                comments.push(ColumnComment {
                    column: prop_name.clone(),
                    comment: description.to_string(),
                });
            }
            let pg_type = if col.postgres_type.is_empty() {
                "TEXT".to_string()
            } else {
                col.postgres_type.clone()
            };
            columns.push(ColumnDef {
                name: prop_name,
                pg_type,
                nullable: col.is_optional,
                default: None,
                is_primary_key: false,
                is_array: false,
            });
        }
    }

    Some((columns, foreign_keys, check_constraints, comments))
}

/// Convert a child `CompositionNode` into a `ChildTableDef`, recursively processing
/// nested children.
fn composition_node_to_child_table(
    node: &CompositionNode,
    parent_table_name: &str,
    parent_schema_name: &str,
    parent_display_name: &str,
) -> ChildTableDef {
    let child_table_name =
        codegraph_naming::truncate_pg_identifier(&format!("{}_{}", parent_table_name, node.field_name));
    let child_display_name = format!("{} {}", parent_display_name, node.field_name);

    let mut columns = Vec::new();
    let mut foreign_keys = Vec::new();
    let mut check_constraints = Vec::new();
    let mut comments = Vec::new();

    // Emit composite range column if present
    if let Some(ref range) = node.composite_range {
        columns.push(ColumnDef {
            name: range.pg_column_name.clone(),
            pg_type: range.pg_type.clone(),
            nullable: true,
            default: None,
            is_primary_key: false,
            is_array: false,
        });
    }

    // Convert ColumnInfo → DDL artifacts
    for col in &node.columns {
        if col.name == "id" {
            continue;
        }
        if let Some((cols, fks, checks, cmts)) = column_info_to_ddl(col, &child_table_name) {
            columns.extend(cols);
            foreign_keys.extend(fks);
            check_constraints.extend(checks);
            comments.extend(cmts);
        }
    }

    // Deduplicate check constraints by name
    {
        let mut seen = HashSet::new();
        check_constraints.retain(|chk| seen.insert(chk.name.clone()));
    }

    // Deduplicate columns
    {
        let mut seen = HashSet::new();
        columns.retain(|col| seen.insert(col.name.clone()));
    }

    // Remove any column that collides with the parent FK column
    let parent_fk_col = codegraph_naming::truncate_pg_identifier(&format!("{}_id", parent_table_name));
    columns.retain(|col| col.name != parent_fk_col);

    // Recursively convert nested children
    let nested_children: Vec<ChildTableDef> = node
        .children
        .iter()
        .map(|child| {
            composition_node_to_child_table(
                child,
                &child_table_name,
                parent_schema_name,
                &child_display_name,
            )
        })
        .collect();

    ChildTableDef {
        schema_name: parent_schema_name.to_string(),
        table_name: child_table_name,
        parent_fk_column: parent_fk_col,
        parent_schema: parent_schema_name.to_string(),
        parent_table: parent_table_name.to_string(),
        columns,
        display_name: child_display_name,
        comments,
        foreign_keys,
        check_constraints,
        child_tables: nested_children,
    }
}

pub struct DdlGenerator {
    output_dir: PathBuf,
    parent_candidates: Vec<codegraph_core::types::ParentCandidate>,
    dialect: Box<dyn SqlDialect>,
}

impl DdlGenerator {
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

    async fn query_ddl_context(
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

        let mut columns = vec![ColumnDef {
            name: "id".to_string(),
            pg_type: "UUID".to_string(),
            nullable: false,
            default: Some("gen_random_uuid()".to_string()),
            is_primary_key: true,
            is_array: false,
        }];

        let mut foreign_keys = Vec::new();
        let mut check_constraints = Vec::new();
        let mut comments = Vec::new();

        // Emit composite range column if present (already resolved on the tree node)
        if let Some(ref range) = root.composite_range {
            columns.push(ColumnDef {
                name: range.pg_column_name.clone(),
                pg_type: range.pg_type.clone(),
                nullable: true,
                default: None,
                is_primary_key: false,
                is_array: false,
            });
        }

        // Inject FK column for parent-child relationships detected from the schema graph.
        let entity_cfg = config
            .domains
            .get(domain)
            .and_then(|d| d.get_entity_config(&schema.rust_type_name));
        if let Some(fk_col) = crate::generate::resolve_parent_fk_column(
            schema_title,
            &self.parent_candidates,
            entity_cfg,
            &config.defaults.type_suffix,
        ) {
            columns.push(ColumnDef {
                name: fk_col.clone(),
                pg_type: "UUID".to_string(),
                nullable: true,
                default: None,
                is_primary_key: false,
                is_array: false,
            });
            // Resolve the parent's schema and table for the FK constraint.
            // Manual config takes priority over graph detection.
            let mut fk_resolved = false;
            if let Some(ec) = entity_cfg {
                if ec.role.as_deref() == Some("child") {
                    if let Some(ref parent_title) = ec.parent {
                        if let Ok(Some(parent_schema)) = db.get_schema_in_domain(parent_title, domain).await {
                            let parent_domain = if config
                                .domains
                                .get(domain)
                                .map(|d| d.entities.contains(parent_title))
                                .unwrap_or(false)
                            {
                                domain
                            } else {
                                parent_schema.domain.as_deref().unwrap_or(domain)
                            };
                            foreign_keys.push(ForeignKeyDef {
                                column_name: fk_col.clone(),
                                column: fk_col.clone(),
                                references_schema: parent_domain.to_string(),
                                references_table: parent_schema.pg_table_name.clone(),
                                references_column: "id".to_string(),
                                on_delete: "CASCADE".to_string(),
                            });
                            fk_resolved = true;
                        }
                    }
                }
            }
            if !fk_resolved {
                let stripped = crate::generate::api::router::strip_suffix(schema_title, &config.defaults.type_suffix);
                if let Some(pc) = self.parent_candidates.iter().find(|pc| {
                    crate::generate::api::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix) == stripped
                }) {
                    if let Ok(Some(parent_schema)) = db.get_schema_in_domain(&pc.parent_title, domain).await {
                        let parent_domain = if config
                            .domains
                            .get(domain)
                            .map(|d| d.entities.contains(&pc.parent_title))
                            .unwrap_or(false)
                        {
                            domain
                        } else {
                            parent_schema.domain.as_deref().unwrap_or(domain)
                        };
                        foreign_keys.push(ForeignKeyDef {
                            column_name: fk_col.clone(),
                            column: fk_col,
                            references_schema: parent_domain.to_string(),
                            references_table: parent_schema.pg_table_name.clone(),
                            references_column: "id".to_string(),
                            on_delete: "CASCADE".to_string(),
                        });
                    }
                }
            }
        }

        // Inject self-referential FK column for hierarchy entities
        let mut indexes: Vec<IndexDef> = Vec::new();
        if let Some(ec) = entity_cfg {
            if let Some(ref hierarchy_field) = ec.hierarchy_field {
                columns.push(ColumnDef {
                    name: hierarchy_field.clone(),
                    pg_type: "UUID".to_string(),
                    nullable: true,
                    default: None,
                    is_primary_key: false,
                    is_array: false,
                });
                foreign_keys.push(ForeignKeyDef {
                    column: hierarchy_field.clone(),
                    column_name: hierarchy_field.clone(),
                    references_schema: schema_name.clone(),
                    references_table: table_name.clone(),
                    references_column: "id".to_string(),
                    on_delete: "SET NULL".to_string(),
                });
                indexes.push(IndexDef {
                    name: format!("idx_{}_{}", table_name, hierarchy_field),
                    columns: vec![hierarchy_field.clone()],
                    unique: false,
                });
            }
        }

        // Convert tree columns to DDL artifacts — no graph queries needed
        for col in &root.columns {
            if col.name == "id" {
                continue;
            }
            if let Some((cols, fks, checks, cmts)) = column_info_to_ddl(col, &table_name) {
                columns.extend(cols);
                foreign_keys.extend(fks);
                check_constraints.extend(checks);
                comments.extend(cmts);
            }
        }

        // Deduplicate check constraints by name — duplicate ColumnInfo entries
        // from cross-domain schema merging produce duplicate constraints.
        {
            let mut seen = HashSet::new();
            check_constraints.retain(|chk| seen.insert(chk.name.clone()));
        }

        // Convert child CompositionNodes → ChildTableDefs
        let child_tables: Vec<ChildTableDef> = root
            .children
            .iter()
            .map(|child| {
                composition_node_to_child_table(child, &table_name, &schema_name, &display_name)
            })
            .collect();

        // Add standard timestamp columns
        columns.push(ColumnDef {
            name: "created_at".to_string(),
            pg_type: "TIMESTAMPTZ".to_string(),
            nullable: false,
            default: Some("now()".to_string()),
            is_primary_key: false,
            is_array: false,
        });
        columns.push(ColumnDef {
            name: "updated_at".to_string(),
            pg_type: "TIMESTAMPTZ".to_string(),
            nullable: false,
            default: Some("now()".to_string()),
            is_primary_key: false,
            is_array: false,
        });
        let has_updated_at = true;

        // Determine tenancy
        let is_tenant_scoped = !is_global_entity(&table_name, config);

        // Add platform_organization_id for tenant-scoped entities
        if is_tenant_scoped {
            columns.insert(
                1,
                ColumnDef {
                    name: "platform_organization_id".to_string(),
                    pg_type: "UUID".to_string(),
                    nullable: false,
                    default: Some("'00000000-0000-0000-0000-000000000000'::UUID".to_string()),
                    is_primary_key: false,
                    is_array: false,
                },
            );
        }

        // Deduplicate columns by name — CompositeWrapper expansion from
        // allOf-inherited properties can produce duplicate expanded columns.
        {
            let mut seen = std::collections::HashSet::new();
            columns.retain(|col| seen.insert(col.name.clone()));
        }

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
        let workflow_cfg = config
            .domains
            .get(&domain)
            .and_then(|d| d.get_entity_config(schema_title))
            .and_then(|ec| ec.workflow.as_ref());

        let has_workflow = workflow_cfg
            .map(|wf| wf.generate_action_endpoints)
            .unwrap_or(false);

        // Workflow status fields from native schema columns are NOT NULL but lack a
        // DEFAULT. Set the initial_state as the DB DEFAULT so INSERTs that don't
        // include the status column (the API excludes it from CreateRequest) succeed.
        if let Some(wf) = workflow_cfg {
            for col in columns.iter_mut() {
                if col.name == wf.status_field && !col.nullable && col.default.is_none() {
                    col.default = Some(format!("'{}'", wf.initial_state));
                }
            }
        }

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

        let fts = build_fts_context(search_config, &columns, &table_name);

        let embeddings = build_embedding_contexts(search_config, &table_name);

        // Add pgvector extension if embeddings are configured
        if !embeddings.is_empty() && !extensions.contains(&"vector".to_string()) {
            extensions.push("vector".to_string());
        }

        // Detect extensions required by column types (safety net for transitive refs
        // that the ingestion pass may miss, e.g. PersonType → AddressType → GeoType).
        for ext in detect_extensions_from_columns(&columns, &flat_child_tables) {
            if !extensions.contains(&ext) {
                extensions.push(ext);
            }
        }

        let is_auditable = config
            .domains
            .get(&domain)
            .and_then(|d| d.auditable)
            .unwrap_or(true);

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
            has_demo_flag: is_auditable,
        })
    }
}

/// Mapping from Postgres extension name to the column type patterns that require it.
const EXTENSION_TYPE_PATTERNS: &[(&str, &[&str])] = &[
    ("postgis", &["GEOMETRY", "GEOGRAPHY"]),
    ("vector", &["VECTOR"]),
];

/// Scan column pg_type values and return any Postgres extensions they require.
/// Generator-side safety net for transitive references the ingestion pass may miss.
fn detect_extensions_from_columns(
    columns: &[ColumnDef],
    child_tables: &[ChildTableDef],
) -> Vec<String> {
    let mut found = Vec::new();
    let mut check = |pg_type: &str| {
        let upper = pg_type.to_uppercase();
        for &(ext, patterns) in EXTENSION_TYPE_PATTERNS {
            if patterns.iter().any(|p| upper.contains(p))
                && !found.iter().any(|s: &String| s == ext)
            {
                found.push(ext.to_string());
            }
        }
    };
    for col in columns {
        check(&col.pg_type);
    }
    for child in child_tables {
        for col in &child.columns {
            check(&col.pg_type);
        }
    }
    found
}

/// System-managed columns that should never be included in full-text search.
const FTS_EXCLUDED_COLUMNS: &[&str] =
    &["id", "platform_organization_id", "created_at", "updated_at"];

/// Build FTS context from search config and auto-discovered TEXT columns.
///
/// When `fts_columns` is `None` in config, auto-discovers all TEXT data columns.
/// When `fts_columns` is `Some([])` (explicit empty), FTS is disabled.
/// When `fts_columns` is `Some([...])`, uses the explicit list.
fn build_fts_context(
    search_config: Option<&SearchConfig>,
    columns: &[ColumnDef],
    table_name: &str,
) -> Option<FtsContext> {
    let defaults = SearchConfig::default();
    let cfg = search_config.unwrap_or(&defaults);

    let column_names: HashSet<&str> = columns.iter().map(|c| c.name.as_str()).collect();

    let fts_columns: Vec<String> = match &cfg.fts_columns {
        // Explicit empty = FTS disabled
        Some(cols) if cols.is_empty() => return None,
        // Explicit list — filter to columns that actually exist in this table
        Some(cols) => cols
            .iter()
            .filter(|c| column_names.contains(c.as_str()))
            .cloned()
            .collect(),
        // When fts_weights are specified but fts_columns is None, use the weight
        // keys as the FTS columns — but only those that exist in this table
        None if !cfg.fts_weights.is_empty() => cfg
            .fts_weights
            .keys()
            .filter(|c| column_names.contains(c.as_str()))
            .cloned()
            .collect(),
        // Auto-discover: all TEXT columns that aren't system-managed or FKs
        None => columns
            .iter()
            .filter(|c| {
                c.pg_type == "TEXT"
                    && !c.is_primary_key
                    && !c.is_array
                    && !FTS_EXCLUDED_COLUMNS.contains(&c.name.as_str())
                    && !c.name.ends_with("_id")
                    && !c.name.ends_with("_code")
            })
            .map(|c| c.name.clone())
            .collect(),
    };

    if fts_columns.is_empty() {
        return None;
    }

    let weighted_columns: Vec<FtsColumnWeight> = fts_columns
        .iter()
        .map(|col| {
            let weight = cfg
                .fts_weights
                .get(col)
                .cloned()
                .unwrap_or_else(|| "D".to_string());
            FtsColumnWeight {
                column: col.clone(),
                weight,
            }
        })
        .collect();

    Some(FtsContext {
        tsvector_column: "search_tsv".to_string(),
        language: cfg.fts_language.clone(),
        weighted_columns,
        index_name: format!("idx_{}_search_tsv", table_name),
    })
}

/// Build embedding contexts from explicit search config.
/// Embedding columns are never auto-discovered — must be explicitly configured.
fn build_embedding_contexts(
    search_config: Option<&SearchConfig>,
    table_name: &str,
) -> Vec<EmbeddingContext> {
    let cfg = match search_config {
        Some(c) if !c.embedding_columns.is_empty() => c,
        _ => return Vec::new(),
    };

    cfg.embedding_columns
        .iter()
        .map(|col| EmbeddingContext {
            source_column: col.clone(),
            vector_column: format!("{}_embedding", col),
            dimensions: cfg.embedding_dimensions,
            index_name: format!("idx_{}_{}_embedding", table_name, col),
        })
        .collect()
}

/// Flatten a recursive child table tree into a depth-first ordered Vec.
/// Each child table already carries its own parent_schema/parent_table for FK references.
fn flatten_child_tables(children: Vec<ChildTableDef>) -> Vec<ChildTableDef> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    flatten_child_tables_inner(children, &mut result, &mut seen);
    result
}

fn flatten_child_tables_inner(
    children: Vec<ChildTableDef>,
    result: &mut Vec<ChildTableDef>,
    seen: &mut std::collections::HashSet<String>,
) {
    for mut child in children {
        let nested = std::mem::take(&mut child.child_tables);
        if seen.insert(child.table_name.clone()) {
            result.push(child);
        } else {
            eprintln!(
                "DDL: skipping duplicate child table '{}' (same VO type referenced by multiple properties)",
                child.table_name
            );
        }
        flatten_child_tables_inner(nested, result, seen);
    }
}

fn is_global_entity(_table_name: &str, _config: &DomainConfig) -> bool {
    // TODO: check tenancy config for global tables
    false
}

/// Quote PostgreSQL reserved-word column names throughout a `DdlContext`.
///
/// Applies `codegraph_naming::quote_pg_column` to all column names, FK column references,
/// comment column references, and check constraint expressions so that reserved
/// words like `order`, `start`, `end` are rendered as `"order"`, `"start"`, `"end"`.
fn quote_ddl_identifiers(ctx: &mut DdlContext) {
    fn quote_columns(columns: &mut [ColumnDef]) {
        for col in columns {
            col.name = codegraph_naming::quote_pg_column(&col.name);
        }
    }

    fn quote_fks(fks: &mut [ForeignKeyDef]) {
        for fk in fks {
            fk.column = codegraph_naming::quote_pg_column(&fk.column);
        }
    }

    fn quote_comments(comments: &mut [ColumnComment]) {
        for c in comments {
            c.column = codegraph_naming::quote_pg_column(&c.column);
        }
    }

    fn quote_checks(checks: &mut [CheckConstraint]) {
        for chk in checks {
            let quoted = codegraph_naming::quote_pg_column(&chk.column);
            if quoted != chk.column {
                chk.expression = chk.expression.replace(&chk.column, &quoted);
            }
            chk.column = quoted;
        }
    }

    fn quote_child_tables(children: &mut [ChildTableDef]) {
        for child in children {
            quote_columns(&mut child.columns);
            quote_fks(&mut child.foreign_keys);
            quote_comments(&mut child.comments);
            quote_checks(&mut child.check_constraints);
            quote_child_tables(&mut child.child_tables);
        }
    }

    quote_columns(&mut ctx.columns);
    quote_fks(&mut ctx.foreign_keys);
    quote_comments(&mut ctx.comments);
    quote_checks(&mut ctx.check_constraints);
    quote_child_tables(&mut ctx.child_tables);
}

/// Post-process column types and defaults through the dialect.
/// Converts PG types to dialect-appropriate types (e.g. UUID → TEXT for SQLite)
/// and wraps default expressions (e.g. strips ::type casts, removes gen_random_uuid()).
fn apply_dialect_type_mapping(dialect: &dyn SqlDialect, ctx: &mut DdlContext) {
    for col in &mut ctx.columns {
        let original_type = col.pg_type.clone();
        if let Some(mapped) = dialect.map_pg_type(&original_type) {
            col.pg_type = mapped;
        }
        if let Some(default) = col.default.take() {
            let wrapped = dialect.wrap_default(&default, &original_type);
            if !wrapped.is_empty() {
                col.default = Some(wrapped);
            }
        }
    }
    for child in &mut ctx.child_tables {
        for col in &mut child.columns {
            let original_type = col.pg_type.clone();
            if let Some(mapped) = dialect.map_pg_type(&original_type) {
                col.pg_type = mapped;
            }
            if let Some(default) = col.default.take() {
                let wrapped = dialect.wrap_default(&default, &original_type);
                if !wrapped.is_empty() {
                    col.default = Some(wrapped);
                }
            }
        }
    }
}

#[async_trait]
impl EntityGenerator for DdlGenerator {
    fn name(&self) -> &str {
        "ddl"
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
        let mut ctx = self
            .query_ddl_context(db, schema_title, domain, config)
            .await?;

        // Skip non-entity schemas (codelists handled separately)
        if ctx.table_name.is_empty() {
            return Ok(Vec::new());
        }

        // Apply dialect-specific type mapping and default wrapping
        apply_dialect_type_mapping(&*self.dialect, &mut ctx);

        // Quote PostgreSQL reserved words in column names (PG-specific, harmless pass-through for other dialects)
        quote_ddl_identifiers(&mut ctx);

        let mut files = Vec::new();

        // Main table DDL — dialect-aware template path
        let table_sql = render_template_with_project(
            tera,
            &db_template_for(&*self.dialect, "table"),
            &ctx,
            project,
        )?;
        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("migrations")
                .join(format!("{}_{}.sql", ctx.schema_name, ctx.table_name)),
            content: table_sql,
        });

        // RLS policy — only on dialects that support it
        if ctx.is_tenant_scoped && self.dialect.has_rls() {
            let rls_sql = render_template_with_project(
                tera,
                &db_template_for(&*self.dialect, "rls"),
                &ctx,
                project,
            )?;
            files.push(GeneratedFile {
                path: self
                    .output_dir
                    .join("migrations")
                    .join(format!("{}_{}_rls.sql", ctx.schema_name, ctx.table_name)),
                content: rls_sql,
            });
        }

        // Updated_at trigger — dialect-aware style
        if ctx.has_updated_at && self.dialect.has_plpgsql() {
            let trigger_sql = render_template_with_project(
                tera,
                &db_template_for(&*self.dialect, "trigger"),
                &ctx,
                project,
            )?;
            files.push(GeneratedFile {
                path: self.output_dir.join("migrations").join(format!(
                    "{}_{}_trigger.sql",
                    ctx.schema_name, ctx.table_name
                )),
                content: trigger_sql,
            });
        }

        // Domain event trigger — dialect-aware style
        let event_trigger_sql = render_template_with_project(
            tera,
            &db_template_for(&*self.dialect, "domain_event_trigger"),
            &ctx,
            project,
        )?;
        files.push(GeneratedFile {
            path: self.output_dir.join("migrations").join(format!(
                "{}_{}_event_trigger.sql",
                ctx.schema_name, ctx.table_name
            )),
            content: event_trigger_sql,
        });

        // ProcessHistoryType-compatible view for workflow entities
        if ctx.has_workflow {
            let view_sql = render_template_with_project(
                tera,
                "db/process_history_view.tera",
                &ctx,
                project,
            )?;
            files.push(GeneratedFile {
                path: self.output_dir.join("migrations").join(format!(
                    "{}_{}_process_history_view.sql",
                    ctx.schema_name, ctx.table_name
                )),
                content: view_sql,
            });
        }

        // Full-text search — only on dialects that support it
        if ctx.fts.is_some() && self.dialect.has_fulltext_search() {
            let fts_sql = render_template_with_project(
                tera,
                &db_template_for(&*self.dialect, "fts"),
                &ctx,
                project,
            )?;
            files.push(GeneratedFile {
                path: self
                    .output_dir
                    .join("migrations")
                    .join(format!("{}_{}_fts.sql", ctx.schema_name, ctx.table_name)),
                content: fts_sql,
            });
        }

        // Semantic search (embeddings) — only on dialects that support it
        if !ctx.embeddings.is_empty() && self.dialect.has_embeddings() {
            let embedding_sql = render_template_with_project(
                tera,
                "db/embedding.tera",
                &ctx,
                project,
            )?;
            files.push(GeneratedFile {
                path: self.output_dir.join("migrations").join(format!(
                    "{}_{}_embedding.sql",
                    ctx.schema_name, ctx.table_name
                )),
                content: embedding_sql,
            });
        }

        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(pg_type: &str) -> ColumnDef {
        ColumnDef {
            name: "col".to_string(),
            pg_type: pg_type.to_string(),
            nullable: true,
            default: None,
            is_primary_key: false,
            is_array: false,
        }
    }

    fn child(columns: Vec<ColumnDef>) -> ChildTableDef {
        ChildTableDef {
            schema_name: "s".to_string(),
            table_name: "t".to_string(),
            parent_fk_column: "fk".to_string(),
            parent_schema: "s".to_string(),
            parent_table: "p".to_string(),
            columns,
            display_name: "T".to_string(),
            comments: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            child_tables: vec![],
        }
    }

    #[test]
    fn empty_columns_returns_empty() {
        assert!(detect_extensions_from_columns(&[], &[]).is_empty());
    }

    #[test]
    fn geometry_column_returns_postgis() {
        let cols = vec![col("GEOMETRY(Point, 4326)")];
        assert_eq!(detect_extensions_from_columns(&cols, &[]), vec!["postgis"]);
    }

    #[test]
    fn geometry_lowercase_returns_postgis() {
        let cols = vec![col("geometry")];
        assert_eq!(detect_extensions_from_columns(&cols, &[]), vec!["postgis"]);
    }

    #[test]
    fn geography_column_returns_postgis() {
        let cols = vec![col("GEOGRAPHY(Point, 4326)")];
        assert_eq!(detect_extensions_from_columns(&cols, &[]), vec!["postgis"]);
    }

    #[test]
    fn vector_column_returns_vector() {
        let cols = vec![col("VECTOR(1536)")];
        assert_eq!(detect_extensions_from_columns(&cols, &[]), vec!["vector"]);
    }

    #[test]
    fn postgis_in_child_table_detected() {
        let children = vec![child(vec![col("GEOMETRY(Point, 4326)")])];
        assert_eq!(
            detect_extensions_from_columns(&[], &children),
            vec!["postgis"]
        );
    }

    #[test]
    fn multiple_geometry_columns_produce_one_entry() {
        let cols = vec![col("GEOMETRY(Point, 4326)"), col("GEOGRAPHY(Point, 4326)")];
        assert_eq!(detect_extensions_from_columns(&cols, &[]), vec!["postgis"]);
    }

    #[test]
    fn unrelated_types_return_empty() {
        let cols = vec![col("TEXT"), col("UUID"), col("TIMESTAMPTZ"), col("JSONB")];
        assert!(detect_extensions_from_columns(&cols, &[]).is_empty());
    }

    #[test]
    fn mixed_extensions_detected() {
        let cols = vec![col("GEOMETRY(Point, 4326)"), col("VECTOR(1536)")];
        let exts = detect_extensions_from_columns(&cols, &[]);
        assert_eq!(exts, vec!["postgis", "vector"]);
    }
}
