use codegraph_config::DomainConfig;

#[test]
fn test_domain_auditable_defaults_true_for_entities() {
    let toml = r#"
[defaults]
auto_discover = true

[domains.payroll]
label = "Payroll"
schema_dir = "payroll/json"
postgres_schema = "payroll"
entities = ["PayRunType"]
"#;
    let config: DomainConfig = toml::from_str(toml).unwrap();
    let payroll = &config.domains["payroll"];
    assert!(payroll.auditable.unwrap_or(true));
}

#[test]
fn test_domain_auditable_explicit_false() {
    let toml = r#"
[defaults]
auto_discover = true

[domains.common]
label = "Common"
schema_dir = "common/json"
postgres_schema = "common"
auditable = false
"#;
    let config: DomainConfig = toml::from_str(toml).unwrap();
    let common = &config.domains["common"];
    assert_eq!(common.auditable, Some(false));
}

#[test]
fn test_domain_auditable_explicit_true() {
    let toml = r#"
[defaults]
auto_discover = true

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"
auditable = true
"#;
    let config: DomainConfig = toml::from_str(toml).unwrap();
    let recruiting = &config.domains["recruiting"];
    assert_eq!(recruiting.auditable, Some(true));
}
