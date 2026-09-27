//! Domain dependency entries (issue #276): `[[domains.X.dependencies]]`
//! parse and validate; existing domains.toml files without the key parse
//! unchanged (byte-identity of the config surface).

use codegraph_config::config::parse_domain_config_str;

const BASE: &str = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
"#;

#[test]
fn dependency_entries_parse() {
    let toml = format!(
        "{BASE}\n[[domains.common.dependencies]]\ndomain = \"party\"\nsource = \"../party/out/graph.artifact.json\"\nversion = \"1.2.0\"\n"
    );
    let config = parse_domain_config_str(&toml).expect("config with a dependency must parse");
    let entry = &config.domains["common"];
    assert_eq!(entry.dependencies.len(), 1);
    let dep = &entry.dependencies[0];
    assert_eq!(dep.domain, "party");
    assert_eq!(dep.source, "../party/out/graph.artifact.json");
    assert_eq!(dep.version, "1.2.0");
}

#[test]
fn dependencies_default_to_empty() {
    let config = parse_domain_config_str(BASE).expect("plain config must parse");
    assert!(config.domains["common"].dependencies.is_empty());
}

#[test]
fn multiple_dependency_entries_parse() {
    let toml = format!(
        "{BASE}\n[[domains.common.dependencies]]\ndomain = \"party\"\nsource = \"../party/out/graph.artifact.json\"\nversion = \"1.2.0\"\n[[domains.common.dependencies]]\ndomain = \"geo\"\nsource = \"../geo/out/graph.artifact.json\"\nversion = \"0.3.1\"\n"
    );
    let config = parse_domain_config_str(&toml).expect("two dependencies must parse");
    let deps = &config.domains["common"].dependencies;
    assert_eq!(deps.len(), 2);
    assert_eq!(deps[1].domain, "geo");
    assert_eq!(deps[1].version, "0.3.1");
}

#[test]
fn malformed_dependency_missing_domain_is_a_parse_error() {
    let toml = format!(
        "{BASE}\n[[domains.common.dependencies]]\nsource = \"../party/out/graph.artifact.json\"\nversion = \"1.2.0\"\n"
    );
    let err = parse_domain_config_str(&toml).expect_err("missing domain must fail");
    assert!(
        err.to_string().to_lowercase().contains("domain"),
        "error must name the missing field: {err}"
    );
}

#[test]
fn malformed_dependency_missing_source_is_a_parse_error() {
    let toml = format!(
        "{BASE}\n[[domains.common.dependencies]]\ndomain = \"party\"\nversion = \"1.2.0\"\n"
    );
    let err = parse_domain_config_str(&toml).expect_err("missing source must fail");
    assert!(
        err.to_string().to_lowercase().contains("source"),
        "error must name the missing field: {err}"
    );
}

#[test]
fn malformed_dependency_missing_version_is_a_parse_error() {
    let toml = format!(
        "{BASE}\n[[domains.common.dependencies]]\ndomain = \"party\"\nsource = \"../party/out/graph.artifact.json\"\n"
    );
    let err = parse_domain_config_str(&toml).expect_err("missing version must fail");
    assert!(
        err.to_string().to_lowercase().contains("version"),
        "error must name the missing field: {err}"
    );
}

#[test]
fn dependency_with_empty_field_is_a_parse_error() {
    let toml = format!(
        "{BASE}\n[[domains.common.dependencies]]\ndomain = \"\"\nsource = \"../party/out/graph.artifact.json\"\nversion = \"1.2.0\"\n"
    );
    assert!(parse_domain_config_str(&toml).is_err());
}

#[test]
fn dependency_on_its_own_domain_is_a_parse_error() {
    let toml = format!(
        "{BASE}\n[[domains.common.dependencies]]\ndomain = \"common\"\nsource = \"../common/out/graph.artifact.json\"\nversion = \"1.0.0\"\n"
    );
    let err = parse_domain_config_str(&toml).expect_err("self-dependency must fail");
    assert!(
        err.to_string().contains("common"),
        "error must name the offending domain: {err}"
    );
}

#[test]
fn duplicate_dependency_domains_are_a_parse_error() {
    let toml = format!(
        "{BASE}\n[[domains.common.dependencies]]\ndomain = \"party\"\nsource = \"../party/out/graph.artifact.json\"\nversion = \"1.2.0\"\n[[domains.common.dependencies]]\ndomain = \"party\"\nsource = \"../party-copy/out/graph.artifact.json\"\nversion = \"1.2.0\"\n"
    );
    assert!(
        parse_domain_config_str(&toml).is_err(),
        "the same dependency domain must not be declared twice"
    );
}
