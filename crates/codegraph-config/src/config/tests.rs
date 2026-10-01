use super::*;

/// Entity config keys may contain spaces (raw schema titles). The lookup
/// by concatenated rust_type_name ("ScreeningResult") must still find them.
#[test]
fn test_get_entity_config_matches_space_titled_keys() {
    let toml = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.compliance]
label = "Compliance"
schema_dir = "compliance"
postgres_schema = "compliance"
entities = ["Screening Result"]

[domains.compliance.entity_config."Screening Result"]
operations = ["create", "read", "list"]
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let entry = &config.domains["compliance"];

    // Direct (raw title) lookup
    let cfg = entry.get_entity_config("Screening Result").unwrap();
    assert_eq!(
        cfg.operations.as_deref(),
        Some(&["create".to_string(), "read".to_string(), "list".to_string()][..])
    );

    // Concatenated rust_type_name lookup (generate-time call pattern)
    let cfg = entry.get_entity_config("ScreeningResult").unwrap();
    assert_eq!(
        cfg.operations.as_deref(),
        Some(&["create".to_string(), "read".to_string(), "list".to_string()][..])
    );
}

#[test]
fn test_parse_minimal_config() {
    let toml = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
depends_on = []
entities = []

[domains.payroll]
label = "Payroll"
schema_dir = "payroll"
postgres_schema = "payroll"
depends_on = ["common"]
entities = ["PayRunType"]
"#;
    let config = parse_domain_config_str(toml).unwrap();
    assert_eq!(config.domains.len(), 2);

    let common = &config.domains["common"];
    assert_eq!(common.label, "Common");
    assert!(common.depends_on.is_empty());

    let payroll = &config.domains["payroll"];
    assert_eq!(payroll.depends_on, vec!["common"]);
    assert_eq!(payroll.entities, vec!["PayRunType"]);
}

#[test]
fn test_parse_entity_config() {
    let toml = r#"
[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"
entities = ["CandidateType"]

[domains.recruiting.entity_config.CandidateType]
operations = ["create", "read", "update", "list"]
role = "root"

[domains.recruiting.entity_config.CandidateType.dto]
immutable_fields = ["ssn"]
list_exclude = ["detailed_notes"]
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let recruiting = &config.domains["recruiting"];
    let candidate = &recruiting.entity_config["CandidateType"];
    assert_eq!(candidate.role.as_deref(), Some("root"));
    assert_eq!(candidate.dto.immutable_fields, vec!["ssn"]);
    assert_eq!(candidate.dto.list_exclude, vec!["detailed_notes"]);
    // api_key_scope defaults to None (module name fallback at codegen time).
    assert!(candidate.api_key_scope.is_none());
}

#[test]
fn test_parse_api_key_scope_override() {
    let toml = r#"
[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"
entities = ["CandidateType"]

[domains.recruiting.entity_config.CandidateType]
api_key_scope = "candidates"

[domains.recruiting.entity_config.CandidateType.permissions]
scope = "recruiting:candidate"
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let candidate = &config.domains["recruiting"].entity_config["CandidateType"];
    assert_eq!(candidate.api_key_scope.as_deref(), Some("candidates"));
    assert_eq!(
        candidate.permissions.scope.as_deref(),
        Some("recruiting:candidate")
    );
}

#[test]
fn test_parse_workflow_config() {
    let toml = r#"
[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"
entities = ["PositionOpeningType"]

[domains.recruiting.entity_config.PositionOpeningType]
role = "root"
path_segment = "position-openings"

[domains.recruiting.entity_config.PositionOpeningType.workflow]
status_field = "document_status_code"
approval_status_field = "approval_status_code"
states = ["draft", "active", "closed", "cancelled"]
initial_state = "draft"
terminal_states = ["closed", "cancelled"]
generate_action_endpoints = true
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let recruiting = &config.domains["recruiting"];
    let po = &recruiting.entity_config["PositionOpeningType"];
    let wf = po.workflow.as_ref().expect("should have workflow config");
    assert_eq!(wf.status_field, "document_status_code");
    assert_eq!(
        wf.approval_status_field.as_deref(),
        Some("approval_status_code")
    );
    assert_eq!(wf.states.len(), 4);
    assert_eq!(wf.initial_state, "draft");
    assert_eq!(wf.terminal_states, vec!["closed", "cancelled"]);
    assert!(wf.generate_action_endpoints);
}

#[test]
fn test_parse_workflow_transitions() {
    let toml = r#"
[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"
entities = ["PositionOpeningType"]

[domains.recruiting.entity_config.PositionOpeningType]
role = "root"

[domains.recruiting.entity_config.PositionOpeningType.workflow]
status_field = "document_status_code"
approval_status_field = "approval_status_code"
states = ["draft", "active", "closed", "cancelled"]
initial_state = "draft"
terminal_states = ["closed", "cancelled"]
generate_action_endpoints = true
status_codelist = "RecruitingDocumentStatusCodeList"
approval_status_codelist = "ApprovalStatusCodeList"

[domains.recruiting.entity_config.PositionOpeningType.workflow.transitions]
draft = ["active", "cancelled"]
active = ["closed", "cancelled"]

[domains.recruiting.entity_config.PositionOpeningType.workflow.dual_status_guards]
active = "Approved"
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let po = &config.domains["recruiting"].entity_config["PositionOpeningType"];
    let wf = po.workflow.as_ref().unwrap();
    assert_eq!(wf.transitions.len(), 2);
    assert_eq!(wf.transitions["draft"], vec!["active", "cancelled"]);
    assert_eq!(wf.transitions["active"], vec!["closed", "cancelled"]);
    assert_eq!(
        wf.status_codelist.as_deref(),
        Some("RecruitingDocumentStatusCodeList")
    );
    assert_eq!(
        wf.approval_status_codelist.as_deref(),
        Some("ApprovalStatusCodeList")
    );
    assert_eq!(wf.dual_status_guards["active"], "Approved");
}

#[test]
fn test_parse_workflow_data_guards_and_timers() {
    let toml = r#"
[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"
entities = ["CandidateType"]

[domains.recruiting.entity_config.CandidateType]
role = "root"
workflow_file = "workflows/recruiting/candidate.toml"
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let candidate = &config.domains["recruiting"].entity_config["CandidateType"];
    assert_eq!(
        candidate.workflow_file.as_deref(),
        Some("workflows/recruiting/candidate.toml")
    );
}

#[test]
fn test_parse_entity_without_workflow() {
    let toml = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["PersonType"]

[domains.common.entity_config.PersonType]
role = "root"
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let person = &config.domains["common"].entity_config["PersonType"];
    assert!(person.workflow.is_none());
}

#[test]
fn parse_new_override_format() {
    let toml = r#"
[defaults]
auto_discover = true

[domains.benefits]
label = "Benefits"
schema_dir = "benefits"
postgres_schema = "benefits"
depends_on = ["common"]
force_entities = ["CensusType"]
force_value_objects = ["CopayType", "CoinsuranceType"]
exclude = ["hros", "hyper4"]

[domains.benefits.entity_config.CensusType]
role = "root"
path_segment = "census"
tag = "Benefits"
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let benefits = &config.domains["benefits"];
    assert_eq!(benefits.force_entities, vec!["CensusType"]);
    assert_eq!(
        benefits.force_value_objects,
        vec!["CopayType", "CoinsuranceType"]
    );
    assert_eq!(benefits.exclude, vec!["hros", "hyper4"]);
    assert!(config.defaults.auto_discover);
}

#[test]
fn backward_compat_old_format() {
    let toml = r#"
[defaults]
auto_discover = false

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["PersonType"]
exclude_entities = ["NameType"]
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let common = &config.domains["common"];
    assert_eq!(common.entities, vec!["PersonType"]);
    assert_eq!(common.exclude_entities, vec!["NameType"]);
}

#[test]
fn test_parse_defaults() {
    let toml = r#"
[domains.wellness]
label = "Wellness"
schema_dir = "wellness"
postgres_schema = "wellness"
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let wellness = &config.domains["wellness"];
    assert!(wellness.depends_on.is_empty());
    assert!(wellness.entities.is_empty());
}

#[test]
fn test_parse_search_config() {
    let toml = r#"
[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"
entities = ["CandidateType"]

[domains.recruiting.entity_config.CandidateType]
role = "root"

[domains.recruiting.entity_config.CandidateType.search]
fts_columns = ["executive_summary", "objective"]
fts_language = "english"
embedding_columns = ["executive_summary"]
embedding_dimensions = 1536

[domains.recruiting.entity_config.CandidateType.search.fts_weights]
executive_summary = "A"
objective = "B"
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let candidate = &config.domains["recruiting"].entity_config["CandidateType"];
    let search = &candidate.search;
    assert_eq!(
        search.fts_columns.as_deref(),
        Some(&["executive_summary".to_string(), "objective".to_string()][..])
    );
    assert_eq!(search.fts_weights["executive_summary"], "A");
    assert_eq!(search.fts_weights["objective"], "B");
    assert_eq!(search.fts_language, "english");
    assert_eq!(search.embedding_columns, vec!["executive_summary"]);
    assert_eq!(search.embedding_dimensions, 1536);
}

#[test]
fn test_search_config_defaults() {
    let toml = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["PersonType"]

[domains.common.entity_config.PersonType]
role = "root"
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let person = &config.domains["common"].entity_config["PersonType"];
    // No search config → defaults
    assert!(person.search.fts_columns.is_none());
    assert!(person.search.fts_weights.is_empty());
    assert_eq!(person.search.fts_language, "english");
    assert!(person.search.embedding_columns.is_empty());
    assert_eq!(person.search.embedding_dimensions, 1536);
    assert_eq!(person.search.fts_rest_mode, "query_param");
}

#[test]
fn parse_max_bulk_size_defaults() {
    let toml_str = r#"
[defaults]
max_bulk_size = 200

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"
"#;
    let config = parse_domain_config_str(toml_str).unwrap();
    assert_eq!(config.defaults.max_bulk_size, 200);
}

#[test]
fn parse_max_bulk_size_entity_override() {
    let toml_str = r#"
[defaults]
max_bulk_size = 100

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"

[domains.recruiting.entity_config.CandidateType]
max_bulk_size = 500
"#;
    let config = parse_domain_config_str(toml_str).unwrap();
    let entity_cfg = config.domains["recruiting"]
        .entity_config
        .get("CandidateType")
        .unwrap();
    assert_eq!(entity_cfg.max_bulk_size, Some(500));
}

#[test]
fn parse_max_bulk_size_absent_uses_default() {
    let toml_str = r#"
[defaults]

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"
"#;
    let config = parse_domain_config_str(toml_str).unwrap();
    assert_eq!(config.defaults.max_bulk_size, 100);
}

#[test]
fn test_search_config_fts_disabled() {
    let toml = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["PersonType"]

[domains.common.entity_config.PersonType]
role = "root"

[domains.common.entity_config.PersonType.search]
fts_columns = []
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let person = &config.domains["common"].entity_config["PersonType"];
    // Explicit empty = FTS disabled
    assert_eq!(person.search.fts_columns.as_deref(), Some(&[][..]));
}

#[test]
fn test_hierarchy_field_parsing() {
    let toml_str = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"

[domains.common.entity_config.OrganizationType]
role = "root"
path_segment = "organizations"
tag = "Organizations"
hierarchy_field = "parent_organization_id"
"#;
    let config = parse_domain_config_str(toml_str).unwrap();
    let entity = config.domains["common"]
        .entity_config
        .get("OrganizationType")
        .unwrap();
    assert_eq!(
        entity.hierarchy_field.as_deref(),
        Some("parent_organization_id")
    );
}

#[test]
fn parse_types_import_prefix_default() {
    let toml_str = r#"
[defaults]

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"
"#;
    let config = parse_domain_config_str(toml_str).unwrap();
    assert_eq!(
        config.defaults.types_import_prefix,
        "codegraph_type_contracts"
    );
}

#[test]
fn parse_types_import_prefix_custom() {
    let toml_str = r#"
[defaults]
types_import_prefix = "crate::structured"

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"
"#;
    let config = parse_domain_config_str(toml_str).unwrap();
    assert_eq!(config.defaults.types_import_prefix, "crate::structured");
}

#[test]
fn test_hierarchy_field_absent() {
    let toml_str = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"

[domains.common.entity_config.SomeType]
role = "root"
path_segment = "some"
tag = "Some"
"#;
    let config = parse_domain_config_str(toml_str).unwrap();
    let entity = config.domains["common"]
        .entity_config
        .get("SomeType")
        .unwrap();
    assert!(entity.hierarchy_field.is_none());
}

#[test]
fn parse_auditable_defaults() {
    let toml_str = r#"
[defaults]

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"
"#;
    let config = parse_domain_config_str(toml_str).unwrap();
    let recruiting = &config.domains["recruiting"];
    assert!(recruiting.auditable.is_none());
}

#[test]
fn parse_auditable_explicit() {
    let toml_str = r#"
[defaults]

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"
auditable = false
"#;
    let config = parse_domain_config_str(toml_str).unwrap();
    let recruiting = &config.domains["recruiting"];
    assert_eq!(recruiting.auditable, Some(false));
}

#[test]
fn parse_tier_default() {
    let toml_str = r#"
[defaults]

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"
"#;
    let config = parse_domain_config_str(toml_str).unwrap();
    assert_eq!(config.domains["recruiting"].tier, "extended");
}

#[test]
fn parse_tier_explicit() {
    let toml_str = r#"
[defaults]

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"
tier = "core"
"#;
    let config = parse_domain_config_str(toml_str).unwrap();
    assert_eq!(config.domains["recruiting"].tier, "core");
}

#[test]
fn parse_types_import_prefix_domain_override() {
    let toml_str = r#"
[defaults]
types_import_prefix = "codegraph_type_contracts"

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"

[domains.recruiting.entity_config.CandidateType]
role = "root"
"#;
    let config = parse_domain_config_str(toml_str).unwrap();
    assert_eq!(
        config.defaults.types_import_prefix,
        "codegraph_type_contracts"
    );
}

#[test]
fn parse_types_import_prefix_entity_override() {
    let toml_str = r#"
[defaults]
types_import_prefix = "crate::structured"

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"

[domains.recruiting.entity_config.CandidateType]
role = "root"
"#;
    let config = parse_domain_config_str(toml_str).unwrap();
    assert_eq!(config.defaults.types_import_prefix, "crate::structured");
}

#[test]
fn parse_allow_include_present() {
    let toml = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType"]

[domains.hr.entity_config.WorkerType]
role = "root"
allow_include = ["person", "deployment", "deployment.position"]
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let hr = &config.domains["hr"];
    let worker = &hr.entity_config["WorkerType"];
    assert_eq!(
        worker.allow_include.as_deref(),
        Some(
            &[
                "person".to_string(),
                "deployment".to_string(),
                "deployment.position".to_string()
            ][..]
        )
    );
}

#[test]
fn parse_allow_include_absent() {
    let toml = r#"
[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType"]

[domains.hr.entity_config.WorkerType]
role = "root"
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let worker = &config.domains["hr"].entity_config["WorkerType"];
    assert!(worker.allow_include.is_none());
}

#[test]
fn parse_allow_include_empty() {
    let toml = r#"
[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType"]

[domains.hr.entity_config.WorkerType]
role = "root"
allow_include = []
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let worker = &config.domains["hr"].entity_config["WorkerType"];
    assert_eq!(worker.allow_include, Some(vec![]));
}

#[test]
fn parse_allow_include_non_ascii() {
    let toml = r#"
[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType"]

[domains.hr.entity_config.WorkerType]
role = "root"
allow_include = ["person"]
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let worker = &config.domains["hr"].entity_config["WorkerType"];
    assert_eq!(
        worker.allow_include.as_deref(),
        Some(&["person".to_string()][..])
    );
}

#[test]
fn parse_custom_routes_flag() {
    let toml = r#"
[domains.platform]
label = "Platform"
schema_dir = "platform"
postgres_schema = "platform"
depends_on = []
auditable = false
entities = []
auto_discover = false
custom_routes = true
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let platform = &config.domains["platform"];
    assert!(platform.custom_routes);

    // Absent key defaults to false.
    let toml = r#"
[domains.crm]
label = "CRM"
schema_dir = "crm"
postgres_schema = "crm"
entities = ["PersonRecordType"]
"#;
    let config = parse_domain_config_str(toml).unwrap();
    assert!(!config.domains["crm"].custom_routes);
}

#[test]
fn parse_worker_topology_keys_all_present() {
    let toml = r#"
[domains.payroll]
label = "Payroll"
schema_dir = "payroll"
postgres_schema = "payroll"
depends_on = ["common", "timecard"]
worker_name = "hr-payroll-worker"
custom_domain = "api.example.com/payroll/*"
service_bindings = ["common", "timecard"]
hyperdrive_binding = "PAYROLL_DB"
cron_triggers = ["0 0 * * *", "*/15 * * * *"]
remote_include_mode = "http"
webhooks = true
queue_name = "payroll-webhook-jobs"
queue_binding = "PAYROLL_WEBHOOKS"
queue_max_retries = 7
queue_max_concurrency = 10
observability = true
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let payroll = &config.domains["payroll"];
    assert_eq!(payroll.worker_name.as_deref(), Some("hr-payroll-worker"));
    assert_eq!(
        payroll.custom_domain.as_deref(),
        Some("api.example.com/payroll/*")
    );
    assert_eq!(
        payroll.service_bindings.as_deref(),
        Some(&["common".to_string(), "timecard".to_string()][..])
    );
    assert_eq!(payroll.hyperdrive_binding.as_deref(), Some("PAYROLL_DB"));
    assert_eq!(
        payroll.cron_triggers.as_deref(),
        Some(&["0 0 * * *".to_string(), "*/15 * * * *".to_string()][..])
    );
    assert_eq!(payroll.remote_include_mode.as_deref(), Some("http"));
    assert_eq!(payroll.webhooks, Some(true));
    assert_eq!(payroll.queue_name.as_deref(), Some("payroll-webhook-jobs"));
    assert_eq!(payroll.queue_binding.as_deref(), Some("PAYROLL_WEBHOOKS"));
    assert_eq!(payroll.queue_max_retries, Some(7));
    assert_eq!(payroll.queue_max_concurrency, Some(10));
    assert_eq!(payroll.observability, Some(true));

    // Accessors return explicit values when set.
    assert_eq!(
        payroll.worker_name_or("hr-app-payroll"),
        "hr-payroll-worker"
    );
    assert_eq!(payroll.hyperdrive_binding_or("HYPERDRIVE"), "PAYROLL_DB");
    assert_eq!(payroll.remote_include_mode_or("sql"), "http");
    assert_eq!(
        payroll.service_bindings_or_depends(),
        vec!["common", "timecard"]
    );
    assert!(payroll.webhooks_or(false));
    assert_eq!(
        payroll.queue_binding_or("WEBHOOK_QUEUE"),
        "PAYROLL_WEBHOOKS"
    );
    assert_eq!(
        payroll.queue_name_or("payroll-webhooks"),
        "payroll-webhook-jobs"
    );
    assert_eq!(payroll.queue_max_retries_or(5), 7);

    // Accessor returns the explicit value when set.
    assert!(payroll.observability_or(false));
}

#[test]
fn parse_worker_topology_keys_absent_use_serde_defaults() {
    let toml = r#"
[domains.payroll]
label = "Payroll"
schema_dir = "payroll"
postgres_schema = "payroll"
depends_on = ["common"]
"#;
    let config = parse_domain_config_str(toml).unwrap();
    let payroll = &config.domains["payroll"];
    assert!(payroll.worker_name.is_none());
    assert!(payroll.custom_domain.is_none());
    assert!(payroll.service_bindings.is_none());
    assert!(payroll.hyperdrive_binding.is_none());
    assert!(payroll.cron_triggers.is_none());
    assert!(payroll.remote_include_mode.is_none());
    assert!(payroll.webhooks.is_none());
    assert!(payroll.queue_name.is_none());
    assert!(payroll.queue_binding.is_none());
    assert!(payroll.queue_max_retries.is_none());
    assert!(payroll.queue_max_concurrency.is_none());
    assert!(payroll.observability.is_none());

    // Accessors fall back to defaults.
    assert_eq!(payroll.worker_name_or("hr-app-payroll"), "hr-app-payroll");
    assert_eq!(payroll.hyperdrive_binding_or("HYPERDRIVE"), "HYPERDRIVE");
    assert_eq!(payroll.remote_include_mode_or("sql"), "sql");
    assert_eq!(payroll.service_bindings_or_depends(), vec!["common"]);
    assert!(!payroll.webhooks_or(false));
    assert_eq!(payroll.queue_binding_or("WEBHOOK_QUEUE"), "WEBHOOK_QUEUE");
    assert_eq!(
        payroll.queue_name_or("hr-app-payroll-webhooks"),
        "hr-app-payroll-webhooks"
    );
    assert_eq!(payroll.queue_max_retries_or(5), 5);

    // Observability defaults off when unset.
    assert!(!payroll.observability_or(false));
}

// ── Namespace declarations (issue #267) ────────────────────────────

#[test]
fn parse_namespaces_quoted_dotted_keys() {
    let toml = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"

[namespaces."cdm.base.datetime"]
domain = "products"

[namespaces."billing.ledger"]
"#;
    let config = parse_domain_config_str(toml).unwrap();
    assert_eq!(config.namespaces.len(), 2);
    assert_eq!(
        config.namespaces["cdm.base.datetime"].domain.as_deref(),
        Some("products")
    );
    // No `domain` key → unassigned namespace (pure visibility scope).
    assert_eq!(config.namespaces["billing.ledger"].domain, None);
}

#[test]
fn parse_namespaces_nested_unquoted_keys_flatten_to_dotted_fqn() {
    let toml = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"

[namespaces.cdm.base.datetime]
domain = "products"
"#;
    let config = parse_domain_config_str(toml).unwrap();
    // The nested spelling flattens to the SAME fqn as the quoted form.
    assert_eq!(
        config.namespaces["cdm.base.datetime"].domain.as_deref(),
        Some("products")
    );
    // Intermediate levels are NOT entries (no domain key).
    assert!(!config.namespaces.contains_key("cdm"));
    assert!(!config.namespaces.contains_key("cdm.base"));
}

#[test]
fn parse_namespaces_mixed_spellings_and_intermediate_assignment() {
    let toml = r#"
[domains.products]
label = "Products"
schema_dir = "products"
postgres_schema = "products"

[namespaces.cdm]
domain = "products"

[namespaces.cdm.base]
domain = "products"

[namespaces."cdm.base.money"]
"#;
    let config = parse_domain_config_str(toml).unwrap();
    // A level can be BOTH an entry and a parent.
    assert_eq!(config.namespaces["cdm"].domain.as_deref(), Some("products"));
    assert_eq!(
        config.namespaces["cdm.base"].domain.as_deref(),
        Some("products")
    );
    assert_eq!(config.namespaces["cdm.base.money"].domain, None);
}

#[test]
fn parse_namespaces_many_to_one_assignment() {
    let toml = r#"
[domains.products]
label = "Products"
schema_dir = "products"
postgres_schema = "products"

[namespaces."a.one"]
domain = "products"
[namespaces."a.two"]
domain = "products"
"#;
    let config = parse_domain_config_str(toml).unwrap();
    assert_eq!(config.namespaces["a.one"].domain, Some("products".into()));
    assert_eq!(config.namespaces["a.two"].domain, Some("products".into()));
}

#[test]
fn parse_namespaces_absent_is_empty_map() {
    let toml = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
"#;
    let config = parse_domain_config_str(toml).unwrap();
    assert!(config.namespaces.is_empty());
}

#[test]
fn parse_namespaces_invalid_fqn_is_parse_error() {
    for bad in ["a..b", ".a", "a."] {
        let toml = format!("[namespaces.\"{bad}\"]\n");
        let err = parse_domain_config_str(&toml).unwrap_err();
        assert!(
            err.to_string().contains("FQN"),
            "{bad:?} should be rejected: {err}"
        );
    }
}

#[test]
fn parse_namespaces_unknown_scalar_key_is_parse_error() {
    let toml = r#"
[namespaces."cdm.base"]
domian = "products"
"#;
    let err = parse_domain_config_str(toml).unwrap_err();
    assert!(err.to_string().contains("unknown key"), "{err}");
}

#[test]
fn parse_namespaces_non_string_domain_is_parse_error() {
    let toml = r#"
[namespaces."cdm.base"]
domain = 3
"#;
    let err = parse_domain_config_str(toml).unwrap_err();
    assert!(err.to_string().contains("must be a string"), "{err}");
}

#[test]
fn parse_namespaces_scalar_value_is_parse_error() {
    let toml = "namespaces = 3\n";
    let err = parse_domain_config_str(toml).unwrap_err();
    assert!(err.to_string().contains("must be a table"), "{err}");
}
