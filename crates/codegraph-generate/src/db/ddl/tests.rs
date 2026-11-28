use super::*;
use crate::db::ddl::query::ddl_dto_gap_fields;
use crate::db::dialect::{PostgresDialect, SqliteDialect};

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
        media_reference: false,
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

fn info_col(name: &str, pg_type: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.to_string(),
        description: None,
        rust_type: "String".to_string(),
        postgres_type: pg_type.to_string(),
        is_optional: false,
        is_codelist_fk: false,
        composite_columns: vec![],
        is_array: false,
        classification: None,
        fk_target: None,
        check_values: vec![],
    }
}

fn node_with_columns(columns: Vec<ColumnInfo>) -> CompositionNode {
    CompositionNode {
        field_name: "provenance".to_string(),
        schema_title: "Provenance".to_string(),
        table_schema: "core".to_string(),
        table_name: "trust_provenance".to_string(),
        fk: None,
        is_collection: false,
        columns,
        jsonb_columns: vec![],
        children: vec![],
        composite_range: None,
        consumed_fields: vec![],
    }
}

#[test]
fn child_parent_fk_column_avoids_double_id_suffix() {
    assert_eq!(child_parent_fk_column("trust"), "trust_id");
    assert_eq!(
        child_parent_fk_column("evidence_extracted_field_id"),
        "evidence_extracted_field_id"
    );
}

#[test]
fn child_table_dedupes_schema_derived_timestamps() {
    let node = node_with_columns(vec![
        info_col("createdAt", "TIMESTAMPTZ"),
        info_col("note", "TEXT"),
    ]);
    let def = composition_node_to_child_table(
        &node,
        "trust",
        "core",
        "Trust",
        &query::GeneratedTables::default(),
    );
    let names: Vec<&str> = def.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(!names.contains(&"created_at"), "got {:?}", names);
    assert!(!names.contains(&"updated_at"), "got {:?}", names);
    assert!(names.contains(&"note"));
}

#[test]
fn child_table_timestamp_dedupe_is_case_insensitive() {
    let node = node_with_columns(vec![info_col("CreatedAt", "TIMESTAMPTZ")]);
    let def = composition_node_to_child_table(
        &node,
        "trust",
        "core",
        "Trust",
        &query::GeneratedTables::default(),
    );
    assert!(def.columns.iter().all(|c| c.name != "CreatedAt"));
}

#[test]
fn child_table_parent_fk_single_suffix_for_id_named_parent() {
    let node = node_with_columns(vec![info_col("value", "TEXT")]);
    let def = composition_node_to_child_table(
        &node,
        "evidence_extracted_field_id",
        "compliance",
        "Evidence Extracted Field Id",
        &query::GeneratedTables::default(),
    );
    assert_eq!(def.parent_fk_column, "evidence_extracted_field_id");
    assert!(
        def.columns
            .iter()
            .all(|c| c.name != "evidence_extracted_field_id_id")
    );
}

/// A tenant-scoped parent with one junction child — the #700 shape (a
/// contained VO renders identically; the backfill only needs the parent FK).
fn trust_parent_context() -> DdlContext {
    DdlContext {
        schema_name: "core".to_string(),
        table_name: "trust".to_string(),
        display_name: "Trust".to_string(),
        domain: "core".to_string(),
        columns: vec![],
        primary_key: "id".to_string(),
        foreign_keys: vec![],
        check_constraints: vec![],
        indexes: vec![],
        has_updated_at: true,
        is_tenant_scoped: true,
        tenant_table: "platform.organization".to_string(),
        extensions: vec![],
        child_tables: vec![],
        comments: vec![],
        has_workflow: false,
        resource_name: "trust".to_string(),
        fts: None,
        embeddings: vec![],
        is_auditable: true,
        role_enforced: true,
        role_minima: Vec::new(),
        roles_hierarchy: Vec::new(),
        user_scope_column: None,
        is_codelist: false,
        has_demo_flag: false,
        append_only: false,
        tenant_backfill: None,
        media_unique_parent_fk: None,
    }
}

fn trust_junction_child() -> ChildTableDef {
    ChildTableDef {
        schema_name: "core".to_string(),
        table_name: "trust_settlor_ids".to_string(),
        parent_fk_column: "trust_id".to_string(),
        parent_schema: "core".to_string(),
        parent_table: "trust".to_string(),
        columns: vec![
            ColumnDef {
                name: "platform_organization_id".to_string(),
                pg_type: "UUID".to_string(),
                nullable: false,
                default: Some("'00000000-0000-0000-0000-000000000000'::UUID".to_string()),
                is_primary_key: false,
                is_array: false,
            },
            ColumnDef {
                name: "party_id".to_string(),
                pg_type: "UUID".to_string(),
                nullable: false,
                default: None,
                is_primary_key: false,
                is_array: false,
            },
        ],
        display_name: "Trust Settlor Ids".to_string(),
        comments: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        child_tables: vec![],
        media_reference: false,
    }
}

fn render_ddl_template(template: &str, ctx: &DdlContext) -> String {
    let template_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = crate::template_engine::create_tera(&template_dir).unwrap();
    let project = crate::ProjectConfig::default();
    render_template_with_project(&tera, template, ctx, &project).unwrap()
}

#[test]
fn rls_template_renders_org_isolation_for_child_table() {
    let parent = trust_parent_context();
    let junction = trust_junction_child();
    let sql = render_ddl_template("db/rls.tera", &child_table_rls_context(&parent, &junction));

    assert!(sql.contains("ALTER TABLE core.trust_settlor_ids ENABLE ROW LEVEL SECURITY"));
    assert!(sql.contains("CREATE POLICY \"org_isolation_select\""));
    assert!(sql.contains("CREATE POLICY \"org_isolation_delete\""));
    // Children are not api-key-scoped resources.
    assert!(!sql.contains("api_key_scoped_"));
}

/// Issue #700: child rows were written with the DDL's zero tenant by every
/// writer, so every child policy tolerated `tenant = '000…'` — which exposed
/// those rows to EVERY organization. The tolerance is the hole; the backfill
/// is what lets it go. Both must be in the child's own migration, in this
/// order: enable → let the owner backfill → backfill → force → policies.
#[test]
fn child_rls_backfills_the_legacy_zero_tenant_then_drops_the_tolerance() {
    const ZERO: &str = "'00000000-0000-0000-0000-000000000000'::uuid";
    let parent = trust_parent_context();
    let junction = trust_junction_child();
    let sql = render_ddl_template("db/rls.tera", &child_table_rls_context(&parent, &junction));

    // The backfill: child rows repair from the parent through the child FK.
    assert!(
        sql.contains(
            "UPDATE core.trust_settlor_ids AS c\n   SET platform_organization_id = p.platform_organization_id\n  FROM core.trust AS p\n WHERE c.trust_id = p.id\n   AND c.platform_organization_id = '00000000-0000-0000-0000-000000000000'::uuid;"
        ),
        "the child migration must backfill legacy zero-tenant rows from the parent:\n{sql}"
    );
    // The owner can actually repair on a re-run: the previous policy's
    // WITH CHECK would reject the rewritten tenant when no session org is
    // set, so the backfill rides a NO FORCE window.
    let no_force = sql
        .find("NO FORCE ROW LEVEL SECURITY")
        .expect("NO FORCE window");
    let backfill = sql.find("UPDATE core.trust_settlor_ids").expect("backfill");
    let force = sql
        .find("ALTER TABLE core.trust_settlor_ids FORCE ROW LEVEL SECURITY")
        .expect("FORCE re-enable");
    assert!(
        no_force < backfill && backfill < force,
        "enable → NO FORCE → backfill → FORCE:\n{sql}"
    );

    // The tolerance is gone from every child policy — this is the fix. The
    // literal survives exactly ONCE: as the backfill's own predicate.
    assert_eq!(
        sql.matches(ZERO).count(),
        1,
        "only the backfill may name the zero tenant:\n{sql}"
    );
    assert!(
        !sql.contains("OR platform_organization_id ="),
        "no child policy may tolerate the zero tenant after #700:\n{sql}"
    );
    assert_eq!(
        sql.matches("USING ((platform_organization_id = public.get_current_org_id()))")
            .count(),
        3,
        "select, update and delete USING are exact-org only:\n{sql}"
    );
    assert_eq!(
        sql.matches("WITH CHECK ((platform_organization_id = public.get_current_org_id()))")
            .count(),
        2,
        "insert + update WITH CHECK are exact-org only:\n{sql}"
    );
    // The tenancy is set at write time instead: the child INSERT trigger.
    assert!(
        sql.contains("#700"),
        "the child tenancy block names its issue:\n{sql}"
    );
}

/// Characterization: #700 is scoped to CHILD tables. A parent's INSERT
/// trigger has always stamped the session org, so its policies keep the
/// legacy tolerance byte-for-byte (flipping parents would hide pre-trigger
/// rows in existing deployments for no security gain).
#[test]
fn parent_rls_policies_keep_the_legacy_zero_tolerance() {
    let parent = trust_parent_context();
    let sql = render_ddl_template("db/rls.tera", &parent);

    assert_eq!(
        sql.matches("OR platform_organization_id = '00000000-0000-0000-0000-000000000000'::uuid")
            .count(),
        5,
        "select, insert, update (using + check) and delete keep the tolerance:\n{sql}"
    );
    assert!(
        !sql.contains("NO FORCE ROW LEVEL SECURITY"),
        "a parent has no backfill and therefore no NO FORCE window:\n{sql}"
    );
}

/// Issue #700: no writer binds `platform_organization_id` on a child INSERT
/// (SeaORM `child_insert_sql`, cornucopia `insert_*`/`link_*`, the media
/// handler), so the ONLY writer-agnostic fix is the same BEFORE INSERT
/// trigger the parent carries. The function lives in the parent's schema
/// (it is emitted by the parent's own trigger file).
#[test]
fn child_tables_get_the_org_assignment_trigger() {
    let mut parent = trust_parent_context();
    parent.child_tables = vec![trust_junction_child()];
    let sql = render_ddl_template("db/trigger.tera", &parent);

    assert!(
        sql.contains(
            "DROP TRIGGER IF EXISTS trg_trust_settlor_ids_set_org_id ON core.trust_settlor_ids;"
        ),
        "the child gets the org trigger:\n{sql}"
    );
    assert!(
        sql.contains(
            "CREATE TRIGGER trg_trust_settlor_ids_set_org_id\n    BEFORE INSERT ON core.trust_settlor_ids\n    FOR EACH ROW\n    EXECUTE FUNCTION core.set_org_id_from_session();"
        ),
        "the child trigger reuses the parent's function:\n{sql}"
    );
    // The parent keeps its own trigger (unchanged bytes).
    assert!(sql.contains("CREATE TRIGGER trg_trust_set_org_id\n    BEFORE INSERT ON core.trust\n"));
}

/// An append-only parent has no `updated_at` column — its child loop used to
/// be gated on `has_updated_at`, which would have skipped the tenant trigger
/// too (the #700 bug in one shape).
#[test]
fn child_org_trigger_is_independent_of_the_updated_at_gate() {
    let mut parent = trust_parent_context();
    parent.has_updated_at = false;
    parent.append_only = true;
    parent.child_tables = vec![trust_junction_child()];
    let sql = render_ddl_template("db/trigger.tera", &parent);

    assert!(
        !sql.contains("set_updated_at()"),
        "no timestamp trigger:\n{sql}"
    );
    assert!(
        sql.contains("CREATE TRIGGER trg_trust_settlor_ids_set_org_id"),
        "the tenant trigger is not gated on updated_at:\n{sql}"
    );
}

/// #169: role_enforced_* policies rank the caller's role against the
/// configured per-op minima via role_at_least(); defaults come from the
/// built-in matrix, explicit min_roles win.
#[test]
fn rls_role_enforced_policies_use_configured_minima() {
    let template_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = crate::template_engine::create_tera(&template_dir).unwrap();
    let project = crate::ProjectConfig::default();

    let mut ctx = DdlContext {
        schema_name: "compensation".to_string(),
        table_name: "pay_run".to_string(),
        display_name: "Pay Run".to_string(),
        domain: "compensation".to_string(),
        columns: vec![],
        primary_key: "id".to_string(),
        foreign_keys: vec![],
        check_constraints: vec![],
        indexes: vec![],
        has_updated_at: true,
        is_tenant_scoped: true,
        tenant_table: "platform.organization".to_string(),
        extensions: vec![],
        child_tables: vec![],
        comments: vec![],
        has_workflow: false,
        resource_name: "pay-run".to_string(),
        fts: None,
        embeddings: vec![],
        is_auditable: true,
        role_enforced: true,
        role_minima: vec![
            RoleMinimum {
                operation: "create".into(),
                min_role: "payroll_admin".into(),
            },
            RoleMinimum {
                operation: "read".into(),
                min_role: "employee".into(),
            },
            RoleMinimum {
                operation: "update".into(),
                min_role: "payroll_admin".into(),
            },
            RoleMinimum {
                operation: "delete".into(),
                min_role: "admin".into(),
            },
            RoleMinimum {
                operation: "list".into(),
                min_role: "employee".into(),
            },
        ],
        roles_hierarchy: vec![
            "owner".into(),
            "admin".into(),
            "hr_admin".into(),
            "payroll_admin".into(),
            "manager".into(),
            "member".into(),
            "employee".into(),
        ],
        user_scope_column: None,
        is_codelist: false,
        has_demo_flag: false,
        append_only: false,
        tenant_backfill: None,
        media_unique_parent_fk: None,
    };
    let sql = render_template_with_project(&tera, "db/rls.tera", &ctx, &project).unwrap();

    assert!(sql.contains("public.role_at_least('payroll_admin')"));
    assert!(sql.contains("public.role_at_least('admin')"));
    assert!(sql.contains("CREATE POLICY \"role_enforced_select\""));
    assert!(sql.contains("CREATE POLICY \"role_enforced_delete\""));
    assert!(
        !sql.contains("enforce_role_action"),
        "the fixed matrix fn is retired"
    );
    assert!(!sql.contains("user_scope_enforced_select"));

    // User-scope column config: column + auto-set trigger + RESTRICTIVE policy.
    ctx.user_scope_column = Some("requested_by_user_id".to_string());
    let sql = render_template_with_project(&tera, "db/rls.tera", &ctx, &project).unwrap();
    assert!(sql.contains("ADD COLUMN IF NOT EXISTS requested_by_user_id UUID"));
    assert!(sql.contains("CREATE TRIGGER trg_pay_run_set_req_user_id"));
    assert!(sql.contains("CREATE POLICY \"user_scope_enforced_select\""));
    assert!(sql.contains(
        "role_rank(coalesce(current_setting('app.role', true), '')) < public.role_rank('member')"
    ));
}

/// #169: the RBAC migration generalizes the role enum over the configured
/// hierarchy and emits role_rank/role_at_least.
#[test]
fn rbac_roles_renders_hierarchy_rank_helpers() {
    let template_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = crate::template_engine::create_tera(&template_dir).unwrap();
    let project = crate::ProjectConfig::default();

    let ctx = serde_json::json!({
        "roles_hierarchy": ["owner", "admin", "hr_admin", "payroll_admin", "manager", "member", "employee"],
    });
    let sql = render_template_with_project(&tera, "db/rbac_roles.tera", &ctx, &project).unwrap();

    // Custom roles get enum-extension guards.
    assert!(sql.contains("AND enumlabel = 'hr_admin'"));
    assert!(sql.contains("AND enumlabel = 'payroll_admin'"));
    assert!(sql.contains("AND enumlabel = 'manager'"));
    // rank fn covers every role in order; 'member' normalizes to 'employee'.
    assert!(sql.contains("CREATE OR REPLACE FUNCTION public.role_rank(p_role text)"));
    assert!(sql.contains("IF r = 'hr_admin' THEN"));
    assert!(sql.contains("IF r = 'member' THEN"));
    assert!(sql.contains("CREATE OR REPLACE FUNCTION public.role_at_least(p_min_role text)"));
    assert!(sql.contains("'ROLE_FORBIDDEN'"));
    // The retired fixed-matrix function is gone.
    assert!(!sql.contains("enforce_role_action"));
    // Default `api_key_mode = "bypass"`: the role-literal pass-through.
    assert!(sql.contains("IF v_role = '' OR v_role = 'api_key' THEN"));
    assert!(!sql.contains("app.session_kind"));
}

/// Issue #635: under `api_key_mode = "agent"` the role gate's key-session
/// pass-through keys on `app.session_kind`, never on the role literal.
#[test]
fn rbac_role_gate_keys_on_session_kind_in_agent_mode() {
    let template_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = crate::template_engine::create_tera(&template_dir).unwrap();
    let mut project = crate::ProjectConfig::default();
    project.codegen.api_key_mode = crate::profile::ApiKeyMode::Agent;
    let ctx = serde_json::json!({ "roles_hierarchy": ["owner", "manager", "member"] });
    let sql = render_template_with_project(&tera, "db/rbac_roles.tera", &ctx, &project).unwrap();
    assert!(
        sql.contains(
            "IF v_role = '' OR coalesce(current_setting('app.session_kind', true), '') = 'agent' THEN"
        ),
        "{sql}"
    );
    assert!(!sql.contains("v_role = 'api_key'"), "{sql}");

    // The live DB leg (`codegraph-ops` `authz_contract`) installs exactly
    // this function from a committed fixture — held byte-equal here so the
    // live proof cannot drift from the template. Re-bless with
    // `ROLE_GATE_BLESS=1`.
    let start = sql
        .find("CREATE OR REPLACE FUNCTION public.role_at_least")
        .expect("role_at_least");
    let end = start + sql[start..].find("$function$;").expect("function end") + "$function$;".len();
    let rendered = format!("{}\n", &sql[start..end]);
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../codegraph-ops/tests/fixtures/role_at_least_agent.sql");
    if std::env::var_os("ROLE_GATE_BLESS").is_some() {
        std::fs::write(&fixture, &rendered).unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(&fixture).unwrap_or_default(),
        rendered,
        "the authz_contract agent-mode fixture drifted from db/rbac_roles.tera \
         (re-bless with ROLE_GATE_BLESS=1)"
    );
}

fn test_context() -> DdlContext {
    DdlContext {
        schema_name: "hr".to_string(),
        table_name: "candidate".to_string(),
        display_name: "Candidate".to_string(),
        domain: "hr".to_string(),
        columns: vec![],
        primary_key: "id".to_string(),
        foreign_keys: vec![],
        check_constraints: vec![],
        indexes: vec![],
        has_updated_at: false,
        is_tenant_scoped: false,
        tenant_table: String::new(),
        extensions: vec![],
        child_tables: vec![],
        comments: vec![],
        has_workflow: false,
        resource_name: "candidate".to_string(),
        fts: None,
        embeddings: vec![],
        is_auditable: false,
        role_enforced: false,
        role_minima: vec![],
        roles_hierarchy: vec![],
        user_scope_column: None,
        is_codelist: false,
        has_demo_flag: false,
        append_only: false,
        tenant_backfill: None,
        media_unique_parent_fk: None,
    }
}

#[test]
fn sqlite_dialect_refuses_unrepresentable_column_types() {
    let mut ctx = test_context();
    ctx.columns.push(col("DATERANGE"));
    ctx.child_tables.push(child(vec![col("TEXT[]")]));
    let err = match apply_dialect_type_mapping(&SqliteDialect::new(), &mut ctx) {
        Err(err) => err,
        Ok(()) => panic!("sqlite must refuse range and array types"),
    };
    let msg = err.to_string();
    assert!(msg.contains("sqlite"), "must name the dialect: {msg}");
    assert!(msg.contains("candidate"), "must name the table: {msg}");
    assert!(msg.contains("DATERANGE"), "must name the type: {msg}");
    assert!(msg.contains("TEXT[]"), "must name the child type: {msg}");
}

#[test]
fn sqlite_dialect_maps_date_numeric_and_bytea() {
    let mut ctx = test_context();
    ctx.columns = vec![col("DATE"), col("NUMERIC(10,2)"), col("BYTEA")];
    apply_dialect_type_mapping(&SqliteDialect::new(), &mut ctx)
        .expect("date/numeric/bytea must map for sqlite");
    assert_eq!(ctx.columns[0].pg_type, "TEXT");
    assert_eq!(ctx.columns[1].pg_type, "REAL");
    assert_eq!(ctx.columns[2].pg_type, "BLOB");
}

#[test]
fn postgres_dialect_accepts_all_pg_types() {
    let mut ctx = test_context();
    ctx.columns.push(col("DATERANGE"));
    ctx.columns.push(col("TEXT[]"));
    apply_dialect_type_mapping(&PostgresDialect::new(), &mut ctx)
        .expect("postgres accepts every PostgreSQL type");
    // Unmapped types pass through unchanged.
    assert_eq!(ctx.columns[0].pg_type, "DATERANGE");
    assert_eq!(ctx.columns[1].pg_type, "TEXT[]");
}

fn workflow_config_toml() -> codegraph_config::config::DomainConfig {
    codegraph_config::config::parse_domain_config_str(
        r#"
[domains.refunds]
label = "Refunds"
schema_dir = "refunds"
postgres_schema = "refunds"
entities = ["RefundRequestType"]

[domains.refunds.entity_config.RefundRequestType.workflow]
status_field = "status"
states = ["draft", "submitted", "approved", "rejected"]
initial_state = "draft"
terminal_states = ["approved", "rejected"]
generate_action_endpoints = true
"#,
    )
    .expect("workflow domains.toml must parse")
}

fn status_col(nullable: bool, default: Option<&str>) -> ColumnDef {
    ColumnDef {
        name: "status".to_string(),
        pg_type: "TEXT".to_string(),
        nullable,
        default: default.map(str::to_string),
        is_primary_key: false,
        is_array: false,
    }
}

/// Issue #311: the workflow status column gains a DEFAULT of the
/// configured initial_state, so API creates (which omit the column —
/// the create DTO excludes workflow-managed fields) materialize the
/// initial state. Applies to NULLABLE columns too: the gate fixture's
/// status column is a non-required codelist ref, and the pre-fix
/// `!col.nullable` guard silently skipped it.
#[test]
fn workflow_status_column_defaults_to_initial_state() {
    let config = workflow_config_toml();
    let mut columns = vec![status_col(true, None), col("TEXT")];
    let has_workflow =
        apply_workflow_defaults(&config, "refunds", "RefundRequestType", &mut columns);
    assert!(has_workflow);
    assert_eq!(
        columns[0].default.as_deref(),
        Some("'draft'"),
        "nullable status column must carry the initial_state default"
    );
    // Nullability is untouched — the default alone fixes creates.
    assert!(columns[0].nullable);
}

#[test]
fn workflow_status_default_survives_when_column_not_null() {
    let config = workflow_config_toml();
    let mut columns = vec![status_col(false, None)];
    apply_workflow_defaults(&config, "refunds", "RefundRequestType", &mut columns);
    assert_eq!(columns[0].default.as_deref(), Some("'draft'"));
}

#[test]
fn workflow_default_preserves_explicit_column_default() {
    let config = workflow_config_toml();
    let mut columns = vec![status_col(true, Some("'new'"))];
    apply_workflow_defaults(&config, "refunds", "RefundRequestType", &mut columns);
    assert_eq!(
        columns[0].default.as_deref(),
        Some("'new'"),
        "an explicit schema/config default must win over the workflow default"
    );
}

#[test]
fn workflow_default_escapes_quoted_initial_state() {
    let mut quoted = workflow_config_toml();
    if let Some(ec) = quoted
        .domains
        .get_mut("refunds")
        .and_then(|d| d.entity_config.get_mut("RefundRequestType"))
        && let Some(wf) = ec.workflow.as_mut()
    {
        wf.initial_state = "o'clock".to_string();
    }
    let mut columns = vec![status_col(true, None)];
    apply_workflow_defaults(&quoted, "refunds", "RefundRequestType", &mut columns);
    assert_eq!(columns[0].default.as_deref(), Some("'o''clock'"));
}

#[test]
fn non_status_columns_and_workflowless_entities_are_untouched() {
    let config = workflow_config_toml();
    let mut columns = vec![col("TEXT"), status_col(true, None)];
    apply_workflow_defaults(&config, "refunds", "OtherEntityType", &mut columns);
    assert!(
        columns.iter().all(|c| c.default.is_none()),
        "no workflow config on the entity → no defaults injected"
    );
}

// ── ddl_dto_gap_fields (#446 debug invariant) ────────────────────────────

fn dto_field(name: &str, is_array: bool, is_entity_ref: bool) -> crate::ddd::dto::DtoField {
    crate::ddd::dto::DtoField {
        name: name.to_string(),
        rust_type: "String".to_string(),
        is_required: true,
        is_array,
        description: String::new(),
        render_strategy: String::new(),
        is_entity_ref,
        is_hierarchy_field: false,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pattern: None,
        format: None,
    }
}

fn ddl_cols(names: &[&str]) -> Vec<ColumnDef> {
    names
        .iter()
        .map(|n| ColumnDef {
            name: (*n).to_string(),
            pg_type: "TEXT".to_string(),
            nullable: true,
            default: None,
            is_primary_key: false,
            is_array: false,
        })
        .collect()
}

#[test]
fn gap_fields_flags_missing_columns() {
    let ddl = ddl_cols(&["id", "note", "parent_id"]);
    let dto = vec![
        dto_field("id", false, false),
        dto_field("note", false, false),
        dto_field("parent_id", false, true),
        dto_field("missing_id", false, true),
    ];
    assert_eq!(ddl_dto_gap_fields(&ddl, &dto), vec!["missing_id"]);
}

#[test]
fn gap_fields_allows_codelist_code_suffix_and_junction_arrays() {
    let ddl = ddl_cols(&["status_code"]);
    let dto = vec![
        // Codelist field: rust-side `_code` strip → status_code column.
        dto_field("status", false, false),
        // Junction array entity ref: child table, never a column.
        dto_field("tags_id", true, true),
    ];
    assert!(ddl_dto_gap_fields(&ddl, &dto).is_empty());
}

#[test]
fn gap_fields_skips_hierarchy_fields() {
    let ddl = ddl_cols(&[]);
    let mut hierarchy = dto_field("parent_organization_id", false, false);
    hierarchy.is_hierarchy_field = true;
    assert!(ddl_dto_gap_fields(&ddl, &[hierarchy]).is_empty());
}

// ── issue #451: ColumnDef.name is the bare identifier; quoting is render-time ──

#[test]
fn reserved_word_column_def_name_is_bare() {
    let (cols, _, _, _) =
        column_info_to_ddl(&info_col("name", "TEXT"), "store_order").expect("plain column");
    assert_eq!(cols[0].name, "name");
}

#[test]
fn reserved_word_fk_and_check_identifiers_are_bare() {
    let mut col = info_col("order", "TEXT");
    col.classification = Some(codegraph_type_contracts::RefClassificationKind::InlineEnum);
    col.check_values = vec!["asc".to_string(), "desc".to_string()];
    let (cols, _, checks, _) = column_info_to_ddl(&col, "store_order").expect("check column");
    assert_eq!(cols[0].name, "order");
    assert_eq!(checks[0].column, "order");
    assert_eq!(checks[0].expression, "\"order\" IN ($$asc$$, $$desc$$)");
}

#[test]
fn rendered_table_ddl_quotes_reserved_column_names() {
    let template_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = crate::template_engine::create_tera(&template_dir).unwrap();
    let project = crate::ProjectConfig::default();

    let mut ctx = test_context();
    ctx.columns.push(ColumnDef {
        name: "name".to_string(),
        pg_type: "TEXT".to_string(),
        nullable: false,
        default: None,
        is_primary_key: false,
        is_array: false,
    });
    let mut render_ctx = ctx.clone();
    super::accumulators::quote_ddl_identifiers(&mut render_ctx);
    let sql = render_template_with_project(&tera, "db/table.tera", &render_ctx, &project).unwrap();
    assert!(
        sql.contains("\n    \"name\" TEXT NOT NULL"),
        "rendered migration must quote reserved-word columns: {sql}"
    );
}

/// Issue #701: a MEDIA child table is one reference row per parent. Its RLS
/// migration dedups (newest per parent) then builds the UNIQUE index, inside
/// the NO FORCE window; a non-media child renders byte-identically to before.
#[test]
fn media_child_rls_dedups_then_indexes_and_non_media_is_untouched() {
    let parent = trust_parent_context();
    let plain = trust_junction_child();
    let mut media = trust_junction_child();
    media.media_reference = true;

    let plain_sql = render_ddl_template("db/rls.tera", &child_table_rls_context(&parent, &plain));
    let media_sql = render_ddl_template("db/rls.tera", &child_table_rls_context(&parent, &media));
    assert!(!plain_sql.contains("UNIQUE INDEX"), "{plain_sql}");

    let dedup = media_sql
        .find("DELETE FROM core.trust_settlor_ids AS older")
        .expect("dedup");
    let index = media_sql
        .find(
            "CREATE UNIQUE INDEX IF NOT EXISTS uq_trust_settlor_ids_trust_id ON \
             core.trust_settlor_ids(trust_id);",
        )
        .expect("unique index");
    let force = media_sql
        .find("\nALTER TABLE core.trust_settlor_ids FORCE ROW LEVEL SECURITY;")
        .expect("force");
    assert!(dedup < index && index < force, "{media_sql}");
    // Removing the media block reproduces the plain child migration exactly.
    let block_start = media_sql
        .find("\n\n-- Media overwrite-in-place")
        .expect("block");
    let block_end = index + media_sql[index..].find(';').expect("index terminator") + 1;
    let stripped = format!("{}{}", &media_sql[..block_start], &media_sql[block_end..]);
    assert_eq!(stripped, plain_sql);
}

/// Issue #701, the defensive GLOBAL-parent branch (no RLS migration exists
/// there, so the table migration owns the dedup + index).
#[test]
fn media_child_of_a_global_parent_indexes_in_the_table_migration() {
    let mut media = trust_junction_child();
    media.media_reference = true;
    let mut global = trust_parent_context();
    global.is_tenant_scoped = false;
    global.child_tables = vec![media.clone()];
    let sql = render_ddl_template("db/table.tera", &global);
    let dedup = sql
        .find("DELETE FROM core.trust_settlor_ids AS older")
        .expect("dedup");
    let index = sql
        .find("CREATE UNIQUE INDEX IF NOT EXISTS uq_trust_settlor_ids_trust_id")
        .expect("unique index");
    assert!(dedup < index, "{sql}");

    // A tenant-scoped parent's table migration carries neither (the RLS file
    // owns them), and a non-media child never does.
    let mut scoped = trust_parent_context();
    scoped.child_tables = vec![media];
    assert!(!render_ddl_template("db/table.tera", &scoped).contains("UNIQUE INDEX"));
    global.child_tables = vec![trust_junction_child()];
    assert!(!render_ddl_template("db/table.tera", &global).contains("UNIQUE INDEX"));
}

fn fk_to(schema: &str, table: &str) -> ForeignKeyDef {
    ForeignKeyDef {
        column: "target_id".into(),
        column_name: "target_id".into(),
        references_schema: schema.into(),
        references_table: table.into(),
        references_column: "id".into(),
        on_delete: "SET NULL".into(),
        is_codelist: false,
    }
}

/// Issue #322: an FK whose declared schema differs from the schema its
/// target table is emitted in is re-pointed, never silently dropped.
#[test]
fn fk_to_a_reclaimed_table_is_repointed_at_its_emitted_schema() {
    let universe = query::GeneratedTables::from_pairs([
        ("recruiting".to_string(), "position".to_string()),
        ("common".to_string(), "person".to_string()),
    ]);
    let mut fks = vec![fk_to("common", "position"), fk_to("common", "person")];
    query::retain_generated_fks(&mut fks, &universe);
    let targets: Vec<(&str, &str)> = fks
        .iter()
        .map(|f| (f.references_schema.as_str(), f.references_table.as_str()))
        .collect();
    assert_eq!(
        targets,
        vec![("recruiting", "position"), ("common", "person")]
    );
}

/// A table name emitted in two schemas is ambiguous: no remap is guessed,
/// and an FK naming neither is dropped (the pre-#322 rule).
#[test]
fn ambiguous_table_name_is_not_remapped() {
    let universe = query::GeneratedTables::from_pairs([
        ("a".to_string(), "item".to_string()),
        ("b".to_string(), "item".to_string()),
    ]);
    let mut fks = vec![
        fk_to("c", "item"),
        fk_to("b", "item"),
        fk_to("c", "missing"),
    ];
    query::retain_generated_fks(&mut fks, &universe);
    assert_eq!(fks.len(), 1);
    assert_eq!(fks[0].references_schema, "b");
}
