use codegraph_config::config::{parse_ui_domains_config_str, parse_ui_overrides_config_str};

#[test]
fn test_parse_empty_overrides() {
    let config = parse_ui_overrides_config_str("").unwrap();
    assert!(config.overrides.is_empty());
}

#[test]
fn test_parse_single_override() {
    let toml = r#"
[overrides."common/PersonProfile"]
detail = "@crewbase/ui/ProfileHeader"
list-cell = "@crewbase/ui/ProfileAvatar"
"#;
    let config = parse_ui_overrides_config_str(toml).unwrap();
    let entry = config.overrides.get("common/PersonProfile").unwrap();
    assert_eq!(entry.detail.as_deref(), Some("@crewbase/ui/ProfileHeader"));
    assert_eq!(
        entry.list_cell.as_deref(),
        Some("@crewbase/ui/ProfileAvatar")
    );
    assert!(entry.form.is_none());
    assert!(entry.inline.is_none());
}

#[test]
fn test_parse_multiple_overrides() {
    let toml = r#"
[overrides."common/PersonProfile"]
detail = "@crewbase/ui/ProfileHeader"
form = "@crewbase/ui/ProfileEditor"

[overrides."common/AddressType"]
form = "@crewbase/ui/AddressForm"
detail = "@crewbase/ui/AddressCard"

[overrides."recruiting/InterviewSchedule"]
detail = "@crewbase/ui/ScheduleCalendar"
"#;
    let config = parse_ui_overrides_config_str(toml).unwrap();
    assert_eq!(config.overrides.len(), 3);
}

#[test]
fn test_override_all_contexts() {
    let toml = r#"
[overrides."common/CurrencyAmount"]
detail = "@crewbase/ui/CurrencyDisplay"
list-cell = "@crewbase/ui/CurrencyBadge"
form = "@crewbase/ui/CurrencyField"
inline = "@crewbase/ui/CurrencyInline"
"#;
    let config = parse_ui_overrides_config_str(toml).unwrap();
    let entry = config.overrides.get("common/CurrencyAmount").unwrap();
    assert!(entry.detail.is_some());
    assert!(entry.list_cell.is_some());
    assert!(entry.form.is_some());
    assert!(entry.inline.is_some());
}

#[test]
fn test_parse_empty_ui_domains() {
    let config = parse_ui_domains_config_str("").unwrap();
    assert!(config.domains.is_empty());
}

#[test]
fn test_parse_wizard_enable_disable() {
    let toml = r#"
[recruiting.PositionOpening]
wizard = true

[common.AddressType]
wizard = false
"#;
    let config = parse_ui_domains_config_str(toml).unwrap();
    let po = config.get_entity("recruiting", "PositionOpening").unwrap();
    assert_eq!(po.wizard, Some(true));
    let addr = config.get_entity("common", "AddressType").unwrap();
    assert_eq!(addr.wizard, Some(false));
}

#[test]
fn test_parse_wizard_steps() {
    let toml = r#"
[recruiting.PositionOpening]
wizard = true

[recruiting.PositionOpening.wizard_config]
steps = ["basics", "profile", "competencies", "review"]
"#;
    let config = parse_ui_domains_config_str(toml).unwrap();
    let po = config.get_entity("recruiting", "PositionOpening").unwrap();
    assert_eq!(po.wizard, Some(true));
    let wc = po.wizard_config.as_ref().unwrap();
    assert_eq!(
        wc.steps,
        vec!["basics", "profile", "competencies", "review"]
    );
}

#[test]
fn test_missing_entity_returns_none() {
    let config = parse_ui_domains_config_str("").unwrap();
    assert!(config.get_entity("recruiting", "NonExistent").is_none());
}
