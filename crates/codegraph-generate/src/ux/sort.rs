//! List-sort plane (issue #306).
//!
//! Derives one entity's `?sort=` allow-list from the SAME [`UxPlan`] the
//! list page renders — never a second inference path. The API handler
//! validates the params against the list (`Unknown sort field: …` 400 in
//! the include-path allow-list style), the query layer threads them, and
//! the repository emitter turns validated field names into quoted ORDER BY
//! columns with a deterministic `, id` tiebreaker.
//!
//! Layers:
//!
//! - [`sort_plan_from_plan`] — pure projection over a built plan
//!   (`ColumnPlan.sortable` in `column_order` sequence).
//! - [`collect_ux_plan_context`] — the graph/graph+config inputs
//!   [`build_ux_plan`] needs, assembled once and shared by the ui-page
//!   generator and the API-side resolvers.
//! - [`resolve_ux_sort_plan`] — flag-aware resolver: `project.ux` off ⇒
//!   an empty plan and every consumer stays byte-identical.

use std::collections::BTreeMap;

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{PolicyKind, PropertyNode, SoftDeleteMarker};

use crate::error::Result;
use crate::ui::page::UiField;
use crate::ux::plan::{build_ux_plan, UxPlan, UxPlanInput};
use crate::ProjectConfig;

/// The per-entity sort contract for list endpoints (issue #306).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UxSortPlan {
    /// Sortable field names — the `?sort=` keys (response property names,
    /// the same keys the list-page cells read) — in `column_order`
    /// sequence. Empty = the entity exposes no sorting.
    pub fields: Vec<String>,
}

impl UxSortPlan {
    /// True when the entity exposes no sortable column.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// The 400 message's allow-list text ("`a, b, c`").
    pub fn valid_fields_text(&self) -> String {
        self.fields.join(", ")
    }
}

/// Project the sortable-field allow-list out of a built plan (pure).
///
/// `column_order` sequence, `ColumnPlan.sortable` filter — the single
/// derivation every consumer shares.
pub fn sort_plan_from_plan(plan: &UxPlan) -> UxSortPlan {
    UxSortPlan {
        fields: plan
            .column_order
            .iter()
            .filter(|name| plan.columns.get(*name).is_some_and(|col| col.sortable))
            .cloned()
            .collect(),
    }
}

/// The dto list-field contract: `list_include` pins the exact set,
/// otherwise `list_exclude` removes. Shared by every sort-plane consumer
/// so the page's columns and the API's allow-list always agree.
pub fn apply_list_scope(
    fields: &[UiField],
    list_include: &[String],
    list_exclude: &[String],
) -> Vec<UiField> {
    if !list_include.is_empty() {
        fields
            .iter()
            .filter(|f| list_include.contains(&f.name))
            .cloned()
            .collect()
    } else {
        fields
            .iter()
            .filter(|f| !list_exclude.contains(&f.name))
            .cloned()
            .collect()
    }
}

/// True when the author pinned the list order via dto list pins.
pub fn list_order_is_pinned(list_include: &[String], list_exclude: &[String]) -> bool {
    !list_include.is_empty() || !list_exclude.is_empty()
}

/// Owned inputs for [`build_ux_plan`], collected from the graph once.
///
/// [`UxPlanInput`] borrows, so the owner must outlive the plan build —
/// callers keep this struct in scope (see [`UxPlanContext::plan_input`]).
pub struct UxPlanContext {
    /// The list-scoped UI fields (the caller applies the dto list scope).
    list_fields: Vec<UiField>,
    /// Graph properties, deduplicated by field name (`r#` stripped).
    props: Vec<PropertyNode>,
    workflow_status_field: Option<String>,
    workflow_terminal_states: Vec<String>,
    soft_delete_field: Option<String>,
    user_pinned_list_order: bool,
}

impl UxPlanContext {
    /// Borrow into the plan input [`build_ux_plan`] consumes.
    pub fn plan_input<'a>(&'a self, entity_title: &'a str) -> UxPlanInput<'a> {
        let mut prop_by_name: BTreeMap<&str, &PropertyNode> = BTreeMap::new();
        for prop in &self.props {
            let name = prop
                .rust_field_name
                .strip_prefix("r#")
                .unwrap_or(&prop.rust_field_name);
            prop_by_name.insert(name, prop);
        }
        UxPlanInput {
            entity_title,
            fields: &self.list_fields,
            prop_by_name,
            workflow_status_field: self.workflow_status_field.as_deref(),
            workflow_terminal_states: &self.workflow_terminal_states,
            has_soft_delete: self.soft_delete_field.is_some(),
            user_pinned_list_order: self.user_pinned_list_order,
        }
    }

    /// The soft-delete marker field, for row-visual contexts.
    pub fn soft_delete_field(&self) -> Option<&str> {
        self.soft_delete_field.as_deref()
    }

    /// The workflow status field, for row-visual contexts.
    pub fn workflow_status_field(&self) -> Option<&str> {
        self.workflow_status_field.as_deref()
    }
}

/// Assemble the graph inputs [`build_ux_plan`] needs for one entity.
///
/// `list_fields` must already be list-scoped ([`apply_list_scope`]) — the
/// caller owns that policy. Mirrors the signals the list page renders
/// from: soft-delete marker (graph policy, else the auditable default),
/// workflow status/terminal states, and the domain's deduplicated
/// properties.
pub async fn collect_ux_plan_context(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    schema_title: &str,
    domain: &str,
    list_fields: Vec<UiField>,
    user_pinned_list_order: bool,
) -> Result<UxPlanContext> {
    // Soft-delete signal, mirroring `build_persistence_entity`: a
    // SoftDelete graph policy names its marker field; otherwise the
    // auditable audit band (the DEFAULT — `auditable` unset means true)
    // carries `deleted_at`, which inactive rows can check.
    let policies = db.get_policies_for_schema(schema_title).await?;
    let graph_marker: Option<String> = policies.iter().find_map(|p| match &p.kind {
        PolicyKind::SoftDelete(sd) => match &sd.marker {
            SoftDeleteMarker::Timestamp(name)
            | SoftDeleteMarker::Boolean(name)
            | SoftDeleteMarker::Status(name) => Some(name.clone()),
        },
        _ => None,
    });
    let auditable = config
        .domains
        .get(domain)
        .and_then(|d| d.auditable)
        .unwrap_or(true);
    let soft_delete_field = graph_marker.or_else(|| auditable.then(|| "deleted_at".to_string()));

    // Entity config keys on the rust type name (e.g. `Task`), not the
    // schema title (`TaskType`) — mirror the ui-page generator's lookup so
    // both sides see the same workflow signal.
    let schema = db
        .get_schema_in_domain(schema_title, domain)
        .await?
        .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.to_string()))?;
    let entity_cfg = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(&schema.rust_type_name));
    let workflow_status_field = entity_cfg
        .and_then(|ec| ec.workflow.as_ref())
        .map(|wf| wf.status_field.clone());
    let workflow_terminal_states = entity_cfg
        .and_then(|ec| ec.workflow.as_ref())
        .map(|wf| wf.terminal_states.clone())
        .unwrap_or_default();

    // Graph properties keyed by TS field name (best-effort: synthetic and
    // composite-expanded fields have no property and infer from the field
    // alone).
    let all_props = db.get_properties_in_domain(schema_title, domain).await?;
    let props: Vec<PropertyNode> = {
        let mut seen = std::collections::HashSet::new();
        all_props
            .into_iter()
            .filter(|p| seen.insert(p.rust_field_name.clone()))
            .collect()
    };

    Ok(UxPlanContext {
        list_fields,
        props,
        workflow_status_field,
        workflow_terminal_states,
        soft_delete_field,
        user_pinned_list_order,
    })
}

/// Resolve one entity's sort plan from the graph + project config.
///
/// `project.ux` off (or a plan-less result) ⇒ an empty plan — every
/// consumer keeps its flag-off output byte-identical. Otherwise the plan
/// is built exactly as the ui-page generator builds it and the sortable
/// columns are projected out ([`sort_plan_from_plan`]) — one inference,
/// never a second.
pub async fn resolve_ux_sort_plan(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    project: &ProjectConfig,
    schema_title: &str,
    domain: &str,
) -> Result<UxSortPlan> {
    let Some(rules) = project.ux.ux.as_ref() else {
        return Ok(UxSortPlan::default());
    };

    let schema = db
        .get_schema_in_domain(schema_title, domain)
        .await?
        .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.to_string()))?;
    let entity_cfg = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(&schema.rust_type_name));
    let dto_config = entity_cfg.map(|ec| &ec.dto);
    let immutable_fields = dto_config
        .map(|d| d.immutable_fields.clone())
        .unwrap_or_default();
    let list_include = dto_config
        .map(|d| d.list_include.clone())
        .unwrap_or_default();
    let list_exclude = dto_config
        .map(|d| d.list_exclude.clone())
        .unwrap_or_default();

    let fields = crate::ui::common::collect_ui_fields(
        db,
        schema_title,
        &immutable_fields,
        Some(domain),
        config,
    )
    .await?;
    let list_fields = apply_list_scope(&fields, &list_include, &list_exclude);
    let pinned = list_order_is_pinned(&list_include, &list_exclude);

    let ctx =
        collect_ux_plan_context(db, config, schema_title, domain, list_fields, pinned).await?;
    let input = ctx.plan_input(schema_title);
    let Some(plan) = build_ux_plan(Some(rules), &input)? else {
        return Ok(UxSortPlan::default());
    };
    Ok(sort_plan_from_plan(&plan))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ux::plan::UxPlanInput;
    use codegraph_config::{builtin_ux_rules, parse_ux_rules_str};

    fn field(name: &str, pg_type: &str, ts_type: &str, input_type: &str) -> UiField {
        UiField {
            name: name.to_string(),
            label: String::new(),
            ts_type: ts_type.to_string(),
            input_type: input_type.to_string(),
            is_required: false,
            is_array: false,
            is_entity_ref: false,
            is_immutable: false,
            is_codelist: false,
            is_range: false,
            codelist_values: vec![],
            description: String::new(),
            pg_type: pg_type.to_string(),
            open_end: false,
            ref_api_path: None,
            structured_sub_fields: vec![],
            nested_type_name: None,
        }
    }

    fn codelist_field(name: &str) -> UiField {
        let mut f = field(name, "TEXT", "string", "select");
        f.is_codelist = true;
        f.codelist_values = vec!["draft".into(), "approved".into()];
        f
    }

    fn entity_ref_field(name: &str) -> UiField {
        let mut f = field(name, "UUID", "string", "text");
        f.is_entity_ref = true;
        f
    }

    fn build(fields: &[UiField], rules: Option<&codegraph_config::ux::UxRules>) -> UxPlan {
        let input = UxPlanInput {
            entity_title: "Task",
            fields,
            prop_by_name: BTreeMap::new(),
            workflow_status_field: None,
            workflow_terminal_states: &[],
            has_soft_delete: false,
            user_pinned_list_order: false,
        };
        build_ux_plan(rules, &input)
            .unwrap()
            .expect("rules present ⇒ plan")
    }

    fn pack() -> codegraph_config::ux::UxRules {
        builtin_ux_rules().unwrap().rules
    }

    fn ux_rules(toml: &str) -> codegraph_config::ux::UxRules {
        parse_ux_rules_str(toml).unwrap().rules
    }

    #[test]
    fn allow_list_follows_dimension_per_column_order() {
        let fields = vec![
            field("id", "UUID", "string", "text"),
            field("name", "TEXT", "string", "text"),
            field("headcount", "INTEGER", "number", "number"),
            field("total_amount", "NUMERIC(10,2)", "string", "number"),
            field("due_at", "TIMESTAMPTZ", "string", "datetime-local"),
            codelist_field("status"),
            entity_ref_field("assignee"),
            field("active", "BOOLEAN", "boolean", "checkbox"),
            field("created_at", "TIMESTAMPTZ", "string", "datetime-local"),
        ];
        let plan = build(&fields, Some(&pack()));
        let sort = sort_plan_from_plan(&plan);
        // Identifier/status/reference/flag excluded; Text/Quantity/Money/
        // TimePoint included, in column_order sequence (audit last).
        assert_eq!(
            sort.fields,
            vec![
                "name".to_string(),
                "headcount".to_string(),
                "total_amount".to_string(),
                "due_at".to_string(),
                "created_at".to_string(),
            ]
        );
        assert!(!sort.is_empty());
        assert!(sort.fields.iter().any(|f| f == "total_amount"));
        assert_eq!(
            sort.valid_fields_text(),
            "name, headcount, total_amount, due_at, created_at"
        );
    }

    #[test]
    fn rule_opt_out_removes_field_from_allow_list() {
        let fields = vec![field("name", "TEXT", "string", "text")];
        let rules = ux_rules("[[column]]\ndimension = \"text\"\nsortable = false\n");
        let sort = sort_plan_from_plan(&build(&fields, Some(&rules)));
        assert!(sort.is_empty(), "opt-out leaves no sortable fields");
    }

    #[test]
    fn rule_opt_in_adds_unsalvageable_dimension() {
        let fields = vec![
            codelist_field("status"),
            field("name", "TEXT", "string", "text"),
        ];
        let rules = ux_rules("[[column]]\ndimension = \"status-category\"\nsortable = true\n");
        let sort = sort_plan_from_plan(&build(&fields, Some(&rules)));
        assert_eq!(
            sort.fields,
            vec!["name".to_string(), "status".to_string()],
            "opt-in sorts a StatusCategory column too (column_order sequence)"
        );
    }

    #[test]
    fn no_sortable_columns_yields_empty_plan() {
        let fields = vec![
            field("id", "UUID", "string", "text"),
            codelist_field("status"),
        ];
        let sort = sort_plan_from_plan(&build(&fields, Some(&pack())));
        assert!(sort.is_empty());
        assert_eq!(sort.valid_fields_text(), "");
    }

    #[test]
    fn list_scope_helpers_match_dto_contract() {
        let fields = vec![
            field("id", "UUID", "string", "text"),
            field("name", "TEXT", "string", "text"),
            field("secret", "TEXT", "string", "text"),
        ];
        let names = |fs: &[UiField]| -> Vec<String> { fs.iter().map(|f| f.name.clone()).collect() };
        let include = vec!["name".to_string()];
        assert_eq!(
            names(&apply_list_scope(&fields, &include, &[])),
            vec!["name".to_string()],
            "include pins the exact set"
        );
        let exclude = vec!["secret".to_string()];
        assert_eq!(
            names(&apply_list_scope(&fields, &[], &exclude)),
            vec!["id".to_string(), "name".to_string()],
            "exclude removes"
        );
        assert!(!list_order_is_pinned(&[], &[]));
        assert!(list_order_is_pinned(&include, &[]));
        assert!(list_order_is_pinned(&[], &exclude));
    }
}
