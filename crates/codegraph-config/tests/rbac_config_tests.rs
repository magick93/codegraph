//! RBAC config tests (#169): `[rbac] roles_hierarchy` and per-entity
//! `permissions.min_roles` / `permissions.user_scope_column` in domains.toml.

use codegraph_config::config::parse_domain_config_str;

#[test]
fn parses_rbac_hierarchy_and_min_roles() {
    let config = parse_domain_config_str(
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[rbac]
roles_hierarchy = ["owner", "admin", "hr_admin", "payroll_admin", "manager", "member", "employee"]

[domains.compensation]
label = "Compensation"
schema_dir = "compensation"
postgres_schema = "compensation"
entities = ["PayRunType"]

[domains.compensation.entity_config.PayRunType]
operations = ["create", "read", "update", "delete"]

[domains.compensation.entity_config.PayRunType.permissions]
scope = "compensation.pay-run"
user_scope_column = "requested_by_user_id"

[domains.compensation.entity_config.PayRunType.permissions.min_roles]
create = "payroll_admin"
read = "employee"
update = "payroll_admin"
delete = "admin"
"#,
    )
    .expect("rbac domains.toml must parse");

    let rbac = config.rbac.expect("rbac section");
    let hierarchy = rbac.hierarchy_or_default();
    assert_eq!(hierarchy.len(), 7);
    assert_eq!(hierarchy[2], "hr_admin");

    let entry = config
        .domains
        .get("compensation")
        .unwrap()
        .entity_config
        .get("PayRunType")
        .unwrap();
    let min_roles = entry.permissions.min_roles.as_ref().expect("min_roles");
    assert_eq!(
        min_roles.get("create").map(String::as_str),
        Some("payroll_admin")
    );
    assert_eq!(min_roles.get("delete").map(String::as_str), Some("admin"));
    assert_eq!(
        entry.permissions.user_scope_column.as_deref(),
        Some("requested_by_user_id")
    );
}

#[test]
fn default_hierarchy_applies_when_rbac_absent() {
    let config = parse_domain_config_str(
        r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = []
"#,
    )
    .expect("plain domains.toml must parse");

    assert!(config.rbac.is_none());
    let hierarchy = codegraph_config::config::default_roles_hierarchy();
    assert_eq!(hierarchy, vec!["owner", "manager", "member", "employee"]);
}

#[test]
fn unknown_min_role_operation_is_a_hard_error() {
    let err = parse_domain_config_str(
        r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = []

[domains.common.entity_config.Code.permissions.min_roles]
upsert = "manager"
"#,
    )
    .expect_err("unknown operation must fail validation");
    let msg = err.to_string();
    assert!(
        msg.contains("unknown operation"),
        "error should name the unknown operation, got: {msg}"
    );
}

#[test]
fn min_role_outside_hierarchy_is_a_hard_error() {
    let err = parse_domain_config_str(
        r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = []

[domains.common.entity_config.Code.permissions.min_roles]
delete = "superuser"
"#,
    )
    .expect_err("role outside the hierarchy must fail validation");
    let msg = err.to_string();
    assert!(
        msg.contains("not in the [rbac] roles_hierarchy"),
        "error should name the hierarchy miss, got: {msg}"
    );
}

#[test]
fn duplicate_hierarchy_roles_are_a_hard_error() {
    let err = parse_domain_config_str(
        r#"
[rbac]
roles_hierarchy = ["owner", "owner", "member", "employee"]

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = []
"#,
    )
    .expect_err("duplicate hierarchy roles must fail validation");
    assert!(err.to_string().contains("more than once"));
}
