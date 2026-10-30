use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ExtensionPointsConfig {
    #[serde(flatten)]
    pub points: HashMap<String, ExtensionPointDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExtensionPointDef {
    pub name: String,
    pub description: String,
    pub cardinality: Cardinality,
    pub entities: Vec<String>,
    pub directions: Vec<Direction>,
    #[serde(default)]
    pub config: HashMap<String, ConfigFieldDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    Exclusive,
    Multiple,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Push,
    Pull,
    Bidirectional,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigFieldDef {
    #[serde(rename = "type")]
    pub field_type: ConfigFieldType,
    #[serde(default)]
    pub required: bool,
    pub label: String,
    #[serde(default)]
    pub options: Vec<String>,
    pub default: Option<toml::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFieldType {
    Text,
    Select,
    Toggle,
    Secret,
}

pub fn parse_extension_points(
    path: &Path,
) -> Result<ExtensionPointsConfig, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    parse_extension_points_str(&content)
}

pub fn parse_extension_points_str(
    content: &str,
) -> Result<ExtensionPointsConfig, Box<dyn std::error::Error>> {
    Ok(toml::from_str(content)?)
}

/// Returns extension point IDs that reference the given `domain.entity`.
pub fn points_for_entity(
    config: &ExtensionPointsConfig,
    domain: &str,
    entity: &str,
) -> Vec<String> {
    let target = format!("{}.{}", domain, entity);
    config
        .points
        .iter()
        .filter(|(_, def)| def.entities.contains(&target))
        .map(|(id, _)| id.clone())
        .collect()
}

// ── Integration manifests (issue #552) ──
//
// A manifest declares catalog entries for the `platform_integrations.catalog`
// table plus optional demo installations — the declarative replacement for
// consumers' hand-written bootstrap SQL. One generate-time seed migration
// (`0010_integration_catalog_seed.sql`) carries the whole set.

/// One manifest file's contents (`[integration.<id>]` tables + optional
/// `[[installation]]` demo installs).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct IntegrationManifestFile {
    #[serde(default)]
    pub integration: HashMap<String, IntegrationEntry>,
    #[serde(default, rename = "installation")]
    pub installations: Vec<DemoInstallation>,
}

/// A catalog entry. Mirrors the `platform_integrations.catalog` columns.
#[derive(Debug, Clone, Deserialize)]
pub struct IntegrationEntry {
    pub name: String,
    pub vendor: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub tier: ManifestTier,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub markets: Vec<String>,
    pub extension_point: String,
    #[serde(default)]
    pub worker_url: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Closed catalog tier set (the DDL CHECK constraint, surfaced at parse
/// time for a readable error).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestTier {
    Official,
    Partner,
    Community,
}

impl ManifestTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            ManifestTier::Official => "official",
            ManifestTier::Partner => "partner",
            ManifestTier::Community => "community",
        }
    }
}

/// An optional demo installation seeded alongside the catalog row —
/// replaces the hand-written bootstrap INSERT consumers used to carry.
#[derive(Debug, Clone, Deserialize)]
pub struct DemoInstallation {
    pub catalog: String,
    pub organization_id: String,
    #[serde(default = "default_demo_status")]
    pub status: String,
}

fn default_demo_status() -> String {
    "active".to_string()
}

/// Resolved, deterministic seed set: catalog entries sorted by id, demo
/// installations in declaration order, per-entry content hash computed.
#[derive(Debug, Clone, Default)]
pub struct IntegrationManifestSet {
    pub catalog: Vec<CatalogSeed>,
    pub installations: Vec<DemoInstallSeed>,
}

/// One resolved catalog row (worker_url keeps its raw `{app_name}`
/// template — substitution happens at generation time where the app name
/// is known).
#[derive(Debug, Clone)]
pub struct CatalogSeed {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub description: String,
    pub tier: String,
    pub categories: Vec<String>,
    pub markets: Vec<String>,
    pub extension_point: String,
    pub worker_url: Option<String>,
    pub enabled: bool,
    pub manifest_hash: String,
}

/// One resolved demo installation.
#[derive(Debug, Clone)]
pub struct DemoInstallSeed {
    pub catalog_id: String,
    pub organization_id: String,
    pub status: String,
}

/// Parse one manifest file's contents.
pub fn parse_integration_manifest_str(content: &str) -> Result<IntegrationManifestFile, String> {
    toml::from_str(content).map_err(|e| format!("invalid integration manifest TOML: {e}"))
}

/// Parse and merge the manifest files given by repeatable
/// `--integration-manifests` flags (in flag order).
pub fn parse_integration_manifests(paths: &[PathBuf]) -> Result<IntegrationManifestSet, String> {
    let mut files = Vec::new();
    for path in paths {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read integration manifest {}: {e}", path.display()))?;
        let file = parse_integration_manifest_str(&content)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        files.push(file);
    }
    merge_integration_manifests(&files)
}

/// Merge parsed manifest files into the deterministic seed set. Duplicate
/// catalog ids across files are an error (last-wins would silently drop a
/// declaration); duplicate demo (organization, catalog) pairs are an error
/// too — the SQL is idempotent but the declaration is confusing.
pub fn merge_integration_manifests(
    files: &[IntegrationManifestFile],
) -> Result<IntegrationManifestSet, String> {
    let mut seen_ids: HashMap<String, ()> = HashMap::new();
    let mut seen_installs: HashMap<(String, String), ()> = HashMap::new();
    let mut set = IntegrationManifestSet::default();

    for file in files {
        for (id, entry) in &file.integration {
            if seen_ids.insert(id.clone(), ()).is_some() {
                return Err(format!(
                    "duplicate integration catalog id '{id}' across manifest files"
                ));
            }
            set.catalog.push(CatalogSeed {
                id: id.clone(),
                name: entry.name.clone(),
                vendor: entry.vendor.clone(),
                version: entry.version.clone(),
                description: entry.description.clone(),
                tier: entry.tier.as_str().to_string(),
                categories: entry.categories.clone(),
                markets: entry.markets.clone(),
                extension_point: entry.extension_point.clone(),
                worker_url: entry.worker_url.clone(),
                enabled: entry.enabled,
                manifest_hash: manifest_hash(id, entry),
            });
        }
        for installation in &file.installations {
            let key = (
                installation.organization_id.clone(),
                installation.catalog.clone(),
            );
            if seen_installs.insert(key, ()).is_some() {
                return Err(format!(
                    "duplicate demo installation (organization_id, catalog) = ('{}', '{}') \
                     across manifest files",
                    installation.organization_id, installation.catalog
                ));
            }
            set.installations.push(DemoInstallSeed {
                catalog_id: installation.catalog.clone(),
                organization_id: installation.organization_id.clone(),
                status: installation.status.clone(),
            });
        }
    }

    set.catalog.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(set)
}

/// Cross-validate the merged set against the parsed extension-points
/// config: every `extension_point` ref must resolve (the catalog FK and
/// the installations SQL both key on it) and every demo installation must
/// reference a declared catalog row with a well-formed org UUID and a
/// status the DDL CHECK accepts.
pub fn validate_integration_manifests(
    set: &IntegrationManifestSet,
    points: &ExtensionPointsConfig,
) -> Result<(), String> {
    let declared: Vec<&String> = points.points.keys().collect();
    for entry in &set.catalog {
        if !points.points.contains_key(&entry.extension_point) {
            return Err(format!(
                "integration catalog '{}' references unknown extension point '{}' \
                 (declared: {})",
                entry.id,
                entry.extension_point,
                declared
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    let catalog_ids: std::collections::HashSet<&String> =
        set.catalog.iter().map(|c| &c.id).collect();
    const VALID_STATUSES: [&str; 4] = ["pending", "active", "paused", "error"];
    for installation in &set.installations {
        if !catalog_ids.contains(&installation.catalog_id) {
            return Err(format!(
                "demo installation references unknown catalog '{}' (declared: {})",
                installation.catalog_id,
                catalog_ids
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !is_uuid_shape(&installation.organization_id) {
            return Err(format!(
                "demo installation for catalog '{}' carries a malformed \
                 organization_id '{}' (expected a UUID)",
                installation.catalog_id, installation.organization_id
            ));
        }
        if !VALID_STATUSES.contains(&installation.status.as_str()) {
            return Err(format!(
                "demo installation for catalog '{}' carries unknown status '{}' \
                 (valid: pending, active, paused, error)",
                installation.catalog_id, installation.status
            ));
        }
    }
    Ok(())
}

/// worker_url templating: `{app_name}` substitutes the project app name at
/// generation time; every other placeholder passes through verbatim.
pub fn render_worker_url(template: &str, app_name: &str) -> String {
    template.replace("{app_name}", app_name)
}

/// FNV-1a 64 over a canonical `0x1f`-joined field list, hex-encoded — the
/// `manifest_hash` column records which manifest revision produced a row.
fn manifest_hash(id: &str, entry: &IntegrationEntry) -> String {
    let canonical = [
        id,
        &entry.name,
        &entry.vendor,
        &entry.version,
        &entry.description,
        entry.tier.as_str(),
        &entry.categories.join(","),
        &entry.markets.join(","),
        &entry.extension_point,
        entry.worker_url.as_deref().unwrap_or(""),
        if entry.enabled { "true" } else { "false" },
    ]
    .join("\u{1f}");
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in canonical.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Cheap UUID shape check (8-4-4-4-12 hex) — Postgres enforces the real
/// type at migrate time; this catches typos at generation time.
fn is_uuid_shape(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    let widths = [8, 4, 4, 4, 12];
    parts.len() == 5
        && parts
            .iter()
            .zip(widths)
            .all(|(part, width)| part.len() == width && part.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[payroll-sync]
name = "Payroll Sync"
description = "Synchronise employee and pay data"
cardinality = "exclusive"
entities = ["common.worker", "compensation.pay_run"]
directions = ["push", "pull", "bidirectional"]

[payroll-sync.config]
pay_calendar = { type = "text", required = true, label = "Pay calendar name" }
sync_frequency = { type = "select", required = true, label = "Sync frequency", options = ["realtime", "hourly", "daily"] }

[government-filing]
name = "Government Filing"
description = "File statutory returns"
cardinality = "multiple"
entities = ["compensation.pay_run", "common.worker"]
directions = ["push"]

[government-filing.config]
employer_number = { type = "text", required = true, label = "Employer IRD number" }
"#;

    #[test]
    fn parses_extension_points() {
        let config = parse_extension_points_str(SAMPLE).unwrap();
        assert_eq!(config.points.len(), 2);

        let ps = &config.points["payroll-sync"];
        assert_eq!(ps.name, "Payroll Sync");
        assert_eq!(ps.cardinality, Cardinality::Exclusive);
        assert_eq!(ps.entities, vec!["common.worker", "compensation.pay_run"]);
        assert_eq!(ps.directions.len(), 3);
        assert_eq!(ps.config.len(), 2);

        let freq = &ps.config["sync_frequency"];
        assert_eq!(freq.field_type, ConfigFieldType::Select);
        assert!(freq.required);
        assert_eq!(freq.options, vec!["realtime", "hourly", "daily"]);
    }

    #[test]
    fn points_for_entity_finds_matches() {
        let config = parse_extension_points_str(SAMPLE).unwrap();
        let mut points = points_for_entity(&config, "compensation", "pay_run");
        points.sort();
        assert_eq!(points, vec!["government-filing", "payroll-sync"]);
    }

    #[test]
    fn points_for_entity_returns_empty_for_unmatched() {
        let config = parse_extension_points_str(SAMPLE).unwrap();
        let points = points_for_entity(&config, "recruiting", "candidate");
        assert!(points.is_empty());
    }

    // ── Integration manifests (issue #552) ──

    const MANIFEST_A: &str = r#"
[integration.xero-payroll]
name = "Xero Payroll"
vendor = "yestech"
version = "1.2.0"
description = "Push pay runs to Xero"
tier = "official"
categories = ["payroll"]
markets = ["nz"]
extension_point = "payroll-sync"
worker_url = "https://{app_name}-xero-payroll.workers.dev"

[[installation]]
catalog = "xero-payroll"
organization_id = "00000000-0000-0000-0000-000000000001"
"#;

    const MANIFEST_B: &str = r#"
[integration.ird-filing]
name = "IRD Filing"
vendor = "yestech"
version = "0.9.0"
tier = "partner"
categories = ["tax", "filing"]
markets = ["nz"]
extension_point = "government-filing"
enabled = false

[[installation]]
catalog = "ird-filing"
organization_id = "00000000-0000-0000-0000-000000000002"
status = "pending"
"#;

    #[test]
    fn parses_integration_manifest() {
        let file = parse_integration_manifest_str(MANIFEST_A).unwrap();
        assert_eq!(file.integration.len(), 1);
        let entry = &file.integration["xero-payroll"];
        assert_eq!(entry.name, "Xero Payroll");
        assert_eq!(entry.vendor, "yestech");
        assert_eq!(entry.version, "1.2.0");
        assert_eq!(entry.description, "Push pay runs to Xero");
        assert_eq!(entry.tier, ManifestTier::Official);
        assert_eq!(entry.categories, vec!["payroll"]);
        assert_eq!(entry.markets, vec!["nz"]);
        assert_eq!(entry.extension_point, "payroll-sync");
        assert_eq!(
            entry.worker_url.as_deref(),
            Some("https://{app_name}-xero-payroll.workers.dev")
        );
        // Defaults: enabled = true, demo install status = active.
        assert!(entry.enabled);
        assert_eq!(file.installations.len(), 1);
        assert_eq!(file.installations[0].catalog, "xero-payroll");
        assert_eq!(file.installations[0].status, "active");
    }

    #[test]
    fn merge_manifests_sorts_by_id_and_hashes_content() {
        let a = parse_integration_manifest_str(MANIFEST_A).unwrap();
        let b = parse_integration_manifest_str(MANIFEST_B).unwrap();
        let set = merge_integration_manifests(&[a, b]).unwrap();

        let ids: Vec<&str> = set.catalog.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["ird-filing", "xero-payroll"], "sorted by id");
        assert_eq!(set.installations.len(), 2);

        let ird = &set.catalog[0];
        assert_eq!(ird.tier, "partner");
        assert!(!ird.enabled, "enabled = false carries through");
        assert_eq!(ird.worker_url, None, "worker_url is optional");
        let xero = &set.catalog[1];
        assert_eq!(xero.tier, "official");

        // Deterministic + content-sensitive.
        let a2 = parse_integration_manifest_str(MANIFEST_A).unwrap();
        let b2 = parse_integration_manifest_str(MANIFEST_B).unwrap();
        let set2 = merge_integration_manifests(&[a2, b2]).unwrap();
        assert_eq!(xero.manifest_hash, set2.catalog[1].manifest_hash);
        assert_ne!(xero.manifest_hash, ird.manifest_hash);
        assert_eq!(xero.manifest_hash.len(), 16, "fnv1a-64 hex");
        let changed = MANIFEST_A.replace("1.2.0", "1.3.0");
        let set3 = merge_integration_manifests(&[
            parse_integration_manifest_str(&changed).unwrap(),
            parse_integration_manifest_str(MANIFEST_B).unwrap(),
        ])
        .unwrap();
        assert_ne!(
            xero.manifest_hash, set3.catalog[1].manifest_hash,
            "a manifest edit must change the row hash"
        );
    }

    #[test]
    fn merge_manifests_rejects_duplicate_ids() {
        let a = parse_integration_manifest_str(MANIFEST_A).unwrap();
        let a2 = parse_integration_manifest_str(MANIFEST_A).unwrap();
        let err = merge_integration_manifests(&[a, a2]).unwrap_err();
        assert!(
            err.contains("xero-payroll"),
            "duplicate error must name the catalog id: {err}"
        );
    }

    #[test]
    fn validate_manifests_rejects_unknown_extension_point() {
        let points = parse_extension_points_str(SAMPLE).unwrap();
        let manifest = r#"
[integration.broken]
name = "Broken"
vendor = "v"
version = "1.0.0"
tier = "community"
extension_point = "no-such-point"
"#;
        let set = merge_integration_manifests(&[parse_integration_manifest_str(manifest).unwrap()])
            .unwrap();
        let err = validate_integration_manifests(&set, &points).unwrap_err();
        assert!(
            err.contains("no-such-point") && err.contains("broken"),
            "error must name the manifest id and the unknown point: {err}"
        );
    }

    #[test]
    fn validate_manifests_accepts_known_extension_point() {
        let points = parse_extension_points_str(SAMPLE).unwrap();
        let set =
            merge_integration_manifests(&[parse_integration_manifest_str(MANIFEST_A).unwrap()])
                .unwrap();
        assert!(validate_integration_manifests(&set, &points).is_ok());
    }

    #[test]
    fn validate_manifests_rejects_bad_demo_installation() {
        let points = parse_extension_points_str(SAMPLE).unwrap();
        // Demo installation referencing an undeclared catalog.
        let bad_catalog = r#"
[[installation]]
catalog = "ghost"
organization_id = "00000000-0000-0000-0000-000000000003"
"#;
        let set =
            merge_integration_manifests(&[parse_integration_manifest_str(bad_catalog).unwrap()])
                .unwrap();
        let err = validate_integration_manifests(&set, &points).unwrap_err();
        assert!(
            err.contains("ghost"),
            "must name the unknown catalog: {err}"
        );

        // Malformed organization UUID.
        let bad_org = r#"
[[installation]]
catalog = "xero-payroll"
organization_id = "not-a-uuid"
"#;
        let mut with_entry = parse_integration_manifest_str(MANIFEST_A).unwrap();
        with_entry.installations.extend(
            parse_integration_manifest_str(bad_org)
                .unwrap()
                .installations,
        );
        let set = merge_integration_manifests(&[with_entry]).unwrap();
        let err = validate_integration_manifests(&set, &points).unwrap_err();
        assert!(
            err.contains("organization_id"),
            "must name the bad organization id: {err}"
        );

        // Unknown status.
        let bad_status = r#"
[[installation]]
catalog = "xero-payroll"
organization_id = "00000000-0000-0000-0000-000000000004"
status = "archived"
"#;
        let mut with_entry = parse_integration_manifest_str(MANIFEST_A).unwrap();
        with_entry.installations.extend(
            parse_integration_manifest_str(bad_status)
                .unwrap()
                .installations,
        );
        let set = merge_integration_manifests(&[with_entry]).unwrap();
        let err = validate_integration_manifests(&set, &points).unwrap_err();
        assert!(err.contains("archived"), "must name the bad status: {err}");
    }

    #[test]
    fn render_worker_url_substitutes_app_name() {
        assert_eq!(
            render_worker_url("https://{app_name}-xero-payroll.workers.dev", "crewbase"),
            "https://crewbase-xero-payroll.workers.dev"
        );
        // Unknown placeholders pass through verbatim.
        assert_eq!(
            render_worker_url("https://x.example/{org_id}/events", "app"),
            "https://x.example/{org_id}/events"
        );
        assert_eq!(
            render_worker_url("https://static.example", "app"),
            "https://static.example"
        );
    }
}
