use std::collections::HashMap;

use serde::Deserialize;

use crate::error::DomainConfigError;

/// A declared namespace entry in `domains.toml` (issue #267).
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct NamespaceEntry {
    /// The bounded context this namespace belongs to. Optional and
    /// many-to-one: several namespaces may share a domain, and a namespace
    /// may have no domain at all (pure visibility scope).
    #[serde(default)]
    pub domain: Option<String>,
}

/// Collect `[namespaces.*]` declarations into `fqn → NamespaceEntry`.
///
/// TOML dotted keys create nested tables (`[namespaces.cdm.base.datetime]`
/// becomes `cdm → base → datetime`), while quoted keys stay flat
/// (`[namespaces."cdm.base.datetime"]`). This walk flattens both into the
/// dotted FQN form. A table holding the `domain` key is an entry; its
/// sub-tables are child namespaces. Unknown scalar keys are a parse error
/// (a typo like `domian` must not silently drop the assignment).
/// A namespace FQN must be dotted, non-empty, and free of empty segments
/// (so `""`, `"a."`, `".a"`, `"a..b"` are parse errors).
fn validate_fqn(fqn: &str) -> Result<(), DomainConfigError> {
    if fqn.is_empty() || fqn.split('.').any(|seg| seg.is_empty()) {
        return Err(DomainConfigError::Invalid(format!(
            "[namespaces] invalid namespace FQN {fqn:?}: dot-separated non-empty segments required"
        )));
    }
    Ok(())
}

/// Process one namespace declaration at `fqn`: read its optional `domain`
/// assignment, record it (when explicitly declared), and recurse into its
/// child namespace tables.
fn process_namespace_entry(
    table: &toml::Table,
    fqn: &str,
    out: &mut HashMap<String, NamespaceEntry>,
) -> Result<(), DomainConfigError> {
    let mut entry = NamespaceEntry::default();
    let mut has_domain_key = false;
    for (skey, svalue) in table {
        if skey == "domain" {
            has_domain_key = true;
            let Some(domain) = svalue.as_str() else {
                return Err(DomainConfigError::Invalid(format!(
                    "[namespaces.{fqn}].domain must be a string"
                )));
            };
            if domain.is_empty() {
                return Err(DomainConfigError::Invalid(format!(
                    "[namespaces.{fqn}].domain must not be empty"
                )));
            }
            entry.domain = Some(domain.to_string());
        } else if !svalue.is_table() {
            return Err(DomainConfigError::Invalid(format!(
                "[namespaces.{fqn}] has unknown key {skey:?} (expected `domain` or child namespace tables)"
            )));
        }
    }
    // Record an entry when the level is explicitly declared: either it
    // carries a `domain` key, or it is a LEAF declaration (no child
    // namespace tables — e.g. `[namespaces."billing.ledger"]` with no
    // keys). Intermediate levels of the nested spelling (`cdm` in
    // `[namespaces.cdm.base.datetime]`) are path segments only, not
    // declarations.
    let is_leaf = table.iter().all(|(_, v)| !v.is_table());
    if has_domain_key || is_leaf {
        out.insert(fqn.to_string(), entry);
    }
    // Recurse into child namespaces (dotted unquoted spelling).
    for (ckey, cvalue) in table {
        if ckey == "domain" {
            continue;
        }
        if let Some(child) = cvalue.as_table() {
            let child_fqn = format!("{fqn}.{ckey}");
            validate_fqn(&child_fqn)?;
            process_namespace_entry(child, &child_fqn, out)?;
        }
    }
    Ok(())
}

pub(super) fn deserialize_namespaces<'de, D>(
    d: D,
) -> Result<HashMap<String, NamespaceEntry>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = toml::Value::deserialize(d)?;
    let Some(table) = value.as_table() else {
        return Err(serde::de::Error::custom(
            "[namespaces] must be a table of namespace declarations",
        ));
    };
    let mut out = HashMap::new();
    for (key, value) in table {
        // `domain` is a reserved key at every level (the namespace→domain
        // assignment); it is never a namespace name.
        if key == "domain" {
            continue;
        }
        validate_fqn(key).map_err(serde::de::Error::custom)?;
        let Some(sub) = value.as_table() else {
            return Err(serde::de::Error::custom(format!(
                "[namespaces.{key}] must be a table (with an optional `domain` key)"
            )));
        };
        process_namespace_entry(sub, key, &mut out).map_err(serde::de::Error::custom)?;
    }
    Ok(out)
}
