//! `plugins.toml` configuration for the EmDash plugin generator family.
//!
//! The config declares, per `domains.toml` domain key, which EmDash plugin
//! package to emit, which entities get admin pages, and how the generic
//! Block Kit admin surface is shaped (columns, form fields, transition
//! actions, public routes, bespoke hooks). All keys are snake_case; entity
//! keys are the `domains.toml` entity names (with the `Type` suffix).

use std::collections::BTreeMap;

/// A plugin setting value from the flat `settings` map: bool | string | i64.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum SettingValue {
    Bool(bool),
    Str(String),
    Int(i64),
}

impl SettingValue {
    /// The JavaScript literal for this value (used as a settings default).
    pub fn js_literal(&self) -> String {
        match self {
            SettingValue::Bool(b) => b.to_string(),
            SettingValue::Str(s) => format!("\"{}\"", s.replace('"', "\\\"")),
            SettingValue::Int(i) => i.to_string(),
        }
    }

    /// Default control type for this value when `settings_types` is silent.
    pub fn inferred_type(&self) -> &'static str {
        match self {
            SettingValue::Bool(_) => "boolean",
            SettingValue::Int(_) => "number",
            SettingValue::Str(_) => "string",
        }
    }
}

/// Top-level `plugins.toml`: `[plugins.<domain_key>]` tables.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct EmdashPluginsConfig {
    #[serde(default)]
    pub plugins: std::collections::HashMap<String, EmdashPluginConfig>,
}

/// Load and parse a `plugins.toml` file. Errors are plain strings so the
/// CLI wrapper (cosmos-graph) can surface them without depending on
/// [`crate::error`].
pub fn load_plugins_config(path: &std::path::Path) -> Result<EmdashPluginsConfig, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read plugins config {}: {e}", path.display()))?;
    toml::from_str(&content)
        .map_err(|e| format!("failed to parse plugins config {}: {e}", path.display()))
}

/// Per-domain plugin configuration.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct EmdashPluginConfig {
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub settings: BTreeMap<String, SettingValue>,
    #[serde(default)]
    pub settings_help: BTreeMap<String, String>,
    #[serde(default)]
    pub settings_types: BTreeMap<String, String>,
    #[serde(default)]
    pub events_bridge: Option<EventsBridgeConfig>,
    #[serde(default)]
    pub entities: BTreeMap<String, EmdashEntityConfig>,
}

/// Optional backend domain-event poll bridge + subscriptions.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct EventsBridgeConfig {
    #[serde(default)]
    pub poll_task: bool,
    #[serde(default)]
    pub subscriptions: Vec<EventsSubscriptionConfig>,
}

/// One consumed topic: each subscription keeps consumption records in its own
/// storage collection (`ctx.storage.<storage>`).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct EventsSubscriptionConfig {
    pub topic: String,
    pub storage: String,
}

/// Per-entity plugin configuration (`[plugins.<domain>.entities.<Entity>]`).
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct EmdashEntityConfig {
    #[serde(default)]
    pub page: Option<EntityPageConfig>,
    #[serde(default)]
    pub title_field: Option<String>,
    #[serde(default)]
    pub columns: Option<Vec<EntityColumnConfig>>,
    #[serde(default)]
    pub sort: Option<EntitySortConfig>,
    #[serde(default)]
    pub actions: Option<Vec<EntityActionConfig>>,
    #[serde(default)]
    pub form_overrides: BTreeMap<String, FormFieldOverride>,
    #[serde(default)]
    pub hidden_fields: Vec<String>,
    #[serde(default)]
    pub public_list: Option<PublicListConfig>,
    #[serde(default)]
    pub public_detail: Option<PublicDetailConfig>,
    #[serde(default)]
    pub public_submit: Option<PublicSubmitConfig>,
    #[serde(default)]
    pub suppress_create: bool,
    #[serde(default)]
    pub bespoke: Vec<String>,
    #[serde(default)]
    pub custom_actions: Option<Vec<CustomActionConfig>>,
}

/// Admin page registration (emdash-plugin.jsonc `admin.pages` entry).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct EntityPageConfig {
    pub path: String,
    pub label: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
}

/// List column: `key` is the DTO field name, `label`/`format` optional.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct EntityColumnConfig {
    pub key: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
}

/// Client-side list sort.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct EntitySortConfig {
    pub key: String,
    #[serde(default)]
    pub dir: Option<String>,
}

/// A status-transition (or any field-set) action.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct EntityActionConfig {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub set: BTreeMap<String, SettingValue>,
    #[serde(default)]
    pub visible_when: Option<VisibleWhenConfig>,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub confirm: Option<String>,
}

/// Render filter: only show the action when `<field>` is one of `in`.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct VisibleWhenConfig {
    pub field: String,
    #[serde(default)]
    pub r#in: Vec<SettingValue>,
}

/// Per-field form tweak.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct FormFieldOverride {
    #[serde(default)]
    pub multiline: Option<bool>,
}

/// Public read-only list route with an in-handler equality filter.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PublicListConfig {
    pub route: String,
    #[serde(default)]
    pub filter: Option<PublicFilterConfig>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PublicFilterConfig {
    pub field: String,
    #[serde(default)]
    pub eq: Option<SettingValue>,
}

/// Public read-by-id route (`param` names the routeCtx.input key).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PublicDetailConfig {
    pub route: String,
    #[serde(default)]
    pub param: Option<String>,
}

/// Public submission route. v1 handlers are ALWAYS bespoke: `handler` names
/// the `./bespoke` export invoked as `handler(routeCtx, ctx)`.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PublicSubmitConfig {
    pub route: String,
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub success_message: Option<String>,
    #[serde(default)]
    pub button_label: Option<String>,
    pub handler: String,
}

/// Action dispatched to a hand-written bespoke handler.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct CustomActionConfig {
    pub id: String,
    pub label: String,
    pub handler: String,
    #[serde(default)]
    pub style: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[plugins.events]
label = "Events"
description = "Events domain plugin"
icon = "calendar"
settings = { publicListing = true, waitlistAutoPromote = false, collectionSlug = "events" }
settings_help = { waitlistAutoPromote = "Off by default: promotion is manual (docs/user/rsvp)." }
settings_types = { publicListing = "boolean", rsvpCutoffHours = "number" }
events_bridge = { poll_task = true, subscriptions = [ { topic = "events.published", storage = "rsvp_sync_state" } ] }

[plugins.events.entities.RsvpType]
page = { path = "/registrations", label = "Registrations", icon = "calendar-check" }
title_field = "personName"
columns = [
  { key = "personName", label = "Person" },
  { key = "status" },
  { key = "plusOnes", label = "Plus ones", format = "number" },
]
sort = { key = "position", dir = "asc" }
actions = [
  { id = "confirm", label = "Confirm", set = { status = "Confirmed" }, visible_when = { field = "status", in = ["Pending"] } },
  { id = "cancel", label = "Cancel", set = { status = "Cancelled" }, visible_when = { field = "status", in = ["Pending", "Confirmed"] }, style = "danger", confirm = "Cancel this RSVP?" },
]
form_overrides = { accessibilityRequests = { multiline = true } }
hidden_fields = ["did", "collection"]
public_list = { route = "events", filter = { field = "isPublished", eq = true } }
public_detail = { route = "event", param = "id" }
public_submit = { route = "submitRsvp", fields = ["personName"], required = ["personName"], success_message = "Thanks!", button_label = "Send RSVP", handler = "submitRsvp" }
bespoke = ["promoteFirst"]
custom_actions = [ { id = "promote", label = "Promote first", handler = "promoteFirst", style = "primary" } ]
"#;

    #[test]
    fn parses_sample_plugins_toml() {
        let cfg: EmdashPluginsConfig = toml::from_str(SAMPLE).unwrap();
        let events = cfg.plugins.get("events").expect("events plugin");
        assert_eq!(events.label, "Events");
        assert_eq!(events.icon.as_deref(), Some("calendar"));
        assert_eq!(events.settings.len(), 3);
        assert!(matches!(
            events.settings["publicListing"],
            SettingValue::Bool(true)
        ));
        assert!(
            matches!(events.settings["collectionSlug"], SettingValue::Str(ref s) if s == "events")
        );
        assert_eq!(events.settings_types["rsvpCutoffHours"], "number");
        let bridge = events.events_bridge.as_ref().unwrap();
        assert!(bridge.poll_task);
        assert_eq!(bridge.subscriptions.len(), 1);
        assert_eq!(bridge.subscriptions[0].topic, "events.published");
        assert_eq!(bridge.subscriptions[0].storage, "rsvp_sync_state");

        let rsvp = events.entities.get("RsvpType").expect("RsvpType entity");
        let page = rsvp.page.as_ref().unwrap();
        assert_eq!(page.path, "/registrations");
        assert_eq!(page.label.as_deref(), Some("Registrations"));
        assert_eq!(rsvp.title_field.as_deref(), Some("personName"));
        let columns = rsvp.columns.as_ref().unwrap();
        assert_eq!(columns.len(), 3);
        assert_eq!(columns[2].format.as_deref(), Some("number"));
        let actions = rsvp.actions.as_ref().unwrap();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].set["status"].js_literal(), "\"Confirmed\"");
        let when = actions[1].visible_when.as_ref().unwrap();
        assert_eq!(when.field, "status");
        assert_eq!(when.r#in.len(), 2);
        assert_eq!(
            rsvp.form_overrides["accessibilityRequests"].multiline,
            Some(true)
        );
        assert_eq!(rsvp.hidden_fields, vec!["did", "collection"]);
        let plist = rsvp.public_list.as_ref().unwrap();
        assert_eq!(plist.route, "events");
        assert_eq!(plist.filter.as_ref().unwrap().field, "isPublished");
        assert!(matches!(
            plist.filter.as_ref().unwrap().eq,
            Some(SettingValue::Bool(true))
        ));
        let submit = rsvp.public_submit.as_ref().unwrap();
        assert_eq!(submit.handler, "submitRsvp");
        assert_eq!(rsvp.bespoke, vec!["promoteFirst"]);
        assert_eq!(
            rsvp.custom_actions.as_ref().unwrap()[0].handler,
            "promoteFirst"
        );
    }

    #[test]
    fn empty_config_parses() {
        let cfg: EmdashPluginsConfig = toml::from_str("").unwrap();
        assert!(cfg.plugins.is_empty());
    }

    #[test]
    fn setting_value_literals() {
        assert_eq!(SettingValue::Bool(false).js_literal(), "false");
        assert_eq!(SettingValue::Int(3).js_literal(), "3");
        assert_eq!(SettingValue::Str("a\"b".into()).js_literal(), "\"a\\\"b\"");
        assert_eq!(SettingValue::Bool(true).inferred_type(), "boolean");
    }
}
