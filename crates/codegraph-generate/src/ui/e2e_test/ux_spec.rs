use std::collections::BTreeMap;

use codegraph_config::DomainConfig;
use codegraph_config::ux::{Align, Display};
use codegraph_core::traits::GraphQuerier;

use crate::ProjectConfig;
use crate::error::Result;
use crate::ux::plan::{CollectionPlan, RowAction, build_ux_plan};
use crate::ux::sort::{
    apply_list_scope, collect_ux_plan_context, list_order_is_pinned, sort_plan_from_plan,
};

use super::context::{
    UxE2eActionsCtx, UxE2eAlignCheck, UxE2eChipCheck, UxE2eColumnCtx, UxE2eCopyCheck,
    UxE2eFirstColumnCtx, UxE2eFormatCheck, UxE2eSortCtx, UxE2eSortFlip, UxE2eSpecCtx,
};
use super::fixtures::test_value_for_field;
use super::page::UiField;

// ── ux-rules spec context (issue #302) ──────────────────────────────────

/// Build the ux-rules `.ux.test.ts` context for one entity.
///
/// Reuses [`collect_ux_plan_context`] + [`build_ux_plan`] — the EXACT
/// plan assembly the list page renders from (via `resolve_ux_context`),
/// so the spec can never drift from the markup it asserts. `None` when
/// the `ux_rules` plane is off or the entity is plan-less.
#[allow(clippy::too_many_arguments)]
pub(super) async fn build_ux_e2e_spec(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    project: &ProjectConfig,
    schema_title: &str,
    domain: &str,
    fields: &[UiField],
    create_fields: &[UiField],
    list_include: &[String],
    list_exclude: &[String],
    operations: &[String],
    initial_state: &str,
) -> Result<Option<UxE2eSpecCtx>> {
    let Some(rules) = project.ux.ux.as_ref() else {
        return Ok(None);
    };

    // Mirror the list-page scope exactly (dto list pins, #306 helpers).
    let list_fields = apply_list_scope(fields, list_include, list_exclude);
    let pinned = list_order_is_pinned(list_include, list_exclude);
    let plan_ctx =
        collect_ux_plan_context(db, config, schema_title, domain, list_fields, pinned).await?;
    let workflow_status_field = plan_ctx.workflow_status_field().map(str::to_string);
    let input = plan_ctx.plan_input(schema_title);
    let Some(plan) = build_ux_plan(Some(rules), &input)? else {
        return Ok(None);
    };

    let create_by_name: BTreeMap<&str, &UiField> =
        create_fields.iter().map(|f| (f.name.as_str(), f)).collect();
    let has_read = operations.iter().any(|op| op == "read");
    let has_update = operations.iter().any(|op| op == "update");
    let has_delete = operations.iter().any(|op| op == "delete");
    let has_table = matches!(plan.collection, CollectionPlan::Table);

    let mut columns = Vec::with_capacity(plan.column_order.len());
    let mut chip_checks = Vec::new();
    let mut format_checks = Vec::new();
    let mut align_checks = Vec::new();
    let mut copy_check = None;
    let mut first_column = None;

    for (index, name) in plan.column_order.iter().enumerate() {
        let Some(col) = plan.columns.get(name) else {
            continue;
        };
        let td_index = index + 1;
        let field = fields.iter().find(|f| &f.name == name);
        let create_field = create_by_name.get(name.as_str()).copied();
        let kebab_dimension = col.dimension.as_str().to_string();

        columns.push(UxE2eColumnCtx {
            key: name.clone(),
            dimension: kebab_dimension.clone(),
            display: display_kebab(col.display).to_string(),
            align: align_kebab(col.align).to_string(),
            sortable: col.sortable,
            truncate_tooltip: col.truncate_tooltip,
            td_index,
            is_audit: is_audit_stamp(name),
        });

        // Chip assertions: a chip column whose fixture label is known AND
        // whose value the fixture body actually controls. Columns omitted
        // from the create body (entity refs, VO nests) render their DB
        // default — asserting the fixture label against them is vacuous
        // (e.g. position_opening.approval_status_code). Booleans are
        // excluded — they render through the boolean Badge branch, which
        // carries no chip testid.
        if col.display == Display::Chip {
            let body_controlled = create_by_name
                .get(name.as_str())
                .map(|f| !f.is_entity_ref && f.nested_type_name.is_none())
                .unwrap_or(false);
            if body_controlled
                && let Some(text) =
                    chip_fixture_text(field, workflow_status_field.as_deref(), name, initial_state)
            {
                chip_checks.push(UxE2eChipCheck { text });
            }
        }

        // Clipboard/tooltip assertions: the FIRST copy-chip column with a
        // stable (non-random) fixture value.
        if copy_check.is_none()
            && col.display == Display::CopyChip
            && let Some(expr) = stable_fixture_expr(
                name,
                create_field,
                workflow_status_field.as_deref(),
                initial_state,
            )
        {
            copy_check = Some(UxE2eCopyCheck {
                key: name.clone(),
                td_index,
                expected_expr: expr,
                truncate_tooltip: col.truncate_tooltip,
            });
        }

        // Intl formatting assertions for money/quantity/time-point cells.
        // Audit stamps are excluded — they may be server-stamped, so their
        // cell values are not fixture-controlled.
        if !is_audit_stamp(name) {
            // Range fixtures are interval literals ('[a,b)'), array and
            // structured fixtures are JSONB — none is a valid Intl
            // formatter input, and the rendered cell transforms them.
            if let Some(fixture) = create_field
                .filter(|f| !f.is_range && !f.is_array && f.structured_sub_fields.is_empty())
                .and_then(stable_fixture_literal)
            {
                let kind = match col.dimension {
                    codegraph_config::ux::Dimension::Money => Some("money"),
                    codegraph_config::ux::Dimension::Quantity => Some("quantity"),
                    codegraph_config::ux::Dimension::TimePoint => Some("time-point"),
                    _ => None,
                };
                if let Some(kind) = kind {
                    format_checks.push(UxE2eFormatCheck {
                        kind: kind.to_string(),
                        td_index,
                        fixture_literal: fixture,
                    });
                }
            }
        }

        // Right-aligned columns: th (when sortable — the only th hook)
        // and td must carry `text-right tabular-nums`.
        if col.align == Align::Right {
            align_checks.push(UxE2eAlignCheck {
                key: name.clone(),
                td_index,
                sortable: col.sortable,
            });
        }

        // Readable-lead assertion: the FIRST column is the human-readable
        // Text field (plan row 17) — its raw cell shows the fixture text.
        // Money/time/copy renders transform the cell, and server-stamped
        // audit columns are not predictable, so they never qualify.
        if index == 0
            && col.dimension == codegraph_config::ux::Dimension::Text
            && col.display == Display::Raw
            && !is_audit_stamp(name)
        {
            // StructuredWrapper fixture literals are JSONB object literals
            // ('{ value: ... }', or '[{ ... }]' when array-typed) for the
            // create body — the rendered cell is the wrapper's stringified
            // form, so a toHaveText(object) is invalid Playwright. The
            // assertion simply doesn't apply. Value-object fields are
            // omitted from the generated fixture ("serde default"), so
            // their cells render the null placeholder — the first-column
            // literal would assert against text the fixture never sets.
            if let Some(literal) = create_field
                .filter(|f| {
                    !f.is_array
                        && f.structured_sub_fields.is_empty()
                        && f.nested_type_name.is_none()
                })
                .and_then(stable_fixture_literal)
            {
                first_column = Some(UxE2eFirstColumnCtx {
                    key: name.clone(),
                    expected_literal: literal,
                });
            }
        }
    }

    // Asc/desc flip: the first sortable table column with two distinct
    // stable fixture values (timelines keep their fixed DESC order).
    // Audit stamps are skipped — a server-stamped column is not a
    // controlled fixture value.
    let flip = if has_table {
        plan.column_order
            .iter()
            .enumerate()
            .find_map(|(index, name)| {
                if is_audit_stamp(name) {
                    return None;
                }
                let col = plan.columns.get(name)?;
                if !col.sortable {
                    return None;
                }
                let field = create_by_name.get(name.as_str())?;
                let alt = flip_alt_literal(field)?;
                Some(UxE2eSortFlip {
                    key: name.clone(),
                    td_index: index + 1,
                    alt_literal: alt,
                })
            })
    } else {
        None
    };

    let sort = if has_table {
        let sort_plan = sort_plan_from_plan(&plan);
        if sort_plan.is_empty() {
            None
        } else {
            let valid_fields_text = sort_plan.valid_fields_text();
            Some(UxE2eSortCtx {
                fields: sort_plan.fields,
                valid_fields_text,
                flip,
            })
        }
    } else {
        None
    };

    // Timeline params + a fixture-controlled order_by value (the spec
    // asserts its formatted date actually renders in a `<time>` cell).
    let (timeline, timeline_order_fixture) = match &plan.collection {
        CollectionPlan::Timeline {
            order_by,
            title_field,
            preview,
        } => {
            // A server-stamped order_by (audit) is not fixture-controlled.
            let fixture = (!is_audit_stamp(order_by))
                .then(|| create_by_name.get(order_by.as_str()))
                .and_then(|f| f.copied().and_then(stable_literal))
                .or_else(|| {
                    (workflow_status_field.as_deref() == Some(order_by.as_str()))
                        .then(|| format!("'{initial_state}'"))
                });
            (
                Some(super::page::UxTimelineCtx {
                    order_by: order_by.clone(),
                    title_field: title_field.clone(),
                    preview: preview.clone(),
                }),
                fixture,
            )
        }
        CollectionPlan::Table => (None, None),
    };

    // Actions are real UI affordances: filter to the entity's enabled
    // operations exactly like the list page does.
    let action_allowed = |action: RowAction| match action {
        RowAction::Open => has_read,
        RowAction::Edit => has_update,
        RowAction::Delete => has_delete,
    };
    let menu_live = plan
        .actions
        .menu
        .iter()
        .any(|spec| action_allowed(spec.action));
    let menu_deletes = plan.actions.menu.iter().any(|spec| {
        spec.action == RowAction::Delete
            && action_allowed(RowAction::Delete)
            && plan.actions.confirm.iter().any(|c| c == "delete")
    });
    let actions = menu_live.then_some(UxE2eActionsCtx {
        confirm_delete: menu_deletes,
    });

    Ok(Some(UxE2eSpecCtx {
        columns,
        has_table,
        timeline,
        timeline_order_fixture,
        actions,
        chip_checks,
        copy_check,
        format_checks,
        align_checks,
        sort,
        first_column,
        zebra: plan.visuals.zebra,
        locale: plan.format.locale.clone(),
        currency: plan.format.currency.clone(),
    }))
}

/// The kebab-case spelling of a resolved display (mirrors the list page).
fn display_kebab(display: Display) -> &'static str {
    match display {
        Display::Raw => "raw",
        Display::Chip => "chip",
        Display::CopyChip => "copy-chip",
        Display::Link => "link",
    }
}

/// The kebab-case spelling of a resolved alignment.
fn align_kebab(align: Align) -> &'static str {
    match align {
        Align::Left => "left",
        Align::Right => "right",
    }
}

/// Audit-stamp names trail `column_order` (`created_at`/`updated_at`/
/// `deleted_at`/`*_by`) — mirrors `ux::plan::is_audit_field`.
fn is_audit_stamp(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower == "created_at"
        || lower == "updated_at"
        || lower == "deleted_at"
        || lower.ends_with("_by")
}

/// The fixture value a column shows on the ux fixture row, as a JS
/// expression: `createdId` for the id column, the workflow initial state
/// for the status column, the create-field literal otherwise.
fn stable_fixture_expr(
    name: &str,
    create_field: Option<&UiField>,
    workflow_status_field: Option<&str>,
    initial_state: &str,
) -> Option<String> {
    if name == "id" {
        return Some("createdId".to_string());
    }
    if workflow_status_field == Some(name) && !initial_state.is_empty() {
        return Some(format!("'{initial_state}'"));
    }
    stable_literal(create_field?)
}

/// A create-field fixture literal the spec can predict exactly — random
/// placeholders (uuid templates) are excluded.
fn stable_literal(field: &UiField) -> Option<String> {
    let value = test_value_for_field(field);
    let stable = !value.is_empty() && !value.contains("${");
    stable.then_some(value)
}

/// Alias with the `Option` shape the column loop consumes.
fn stable_fixture_literal(field: &UiField) -> Option<String> {
    stable_literal(field)
}

/// The chip label a fixture row renders: the workflow initial state for
/// the status column, else the codelist's first value. Array codelists
/// render joined text, not a single chip — excluded.
fn chip_fixture_text(
    field: Option<&UiField>,
    workflow_status_field: Option<&str>,
    name: &str,
    initial_state: &str,
) -> Option<String> {
    if workflow_status_field == Some(name) && !initial_state.is_empty() {
        return Some(initial_state.to_string());
    }
    let field = field?;
    if !field.is_codelist || field.is_array {
        return None;
    }
    field.codelist_values.first().cloned()
}

/// A distinct alternative fixture literal for the asc/desc flip row.
fn flip_alt_literal(field: &UiField) -> Option<String> {
    if field.is_array || field.is_entity_ref || field.is_range {
        return None;
    }
    if !field.structured_sub_fields.is_empty() {
        return None;
    }
    // Value-object fields are omitted from the generated fixture
    // ("serde default") — every rendered cell is the null placeholder, so
    // no distinct alt literal can ever appear.
    if field.nested_type_name.is_some() {
        return None;
    }
    if field.is_codelist {
        if field.codelist_values.len() < 2 {
            return None;
        }
        return Some(format!("'{}'", field.codelist_values.last()?));
    }
    match field.input_type.as_str() {
        "number" => Some("7".to_string()),
        "date" => Some("'2024-06-01'".to_string()),
        "datetime-local" => Some("'2024-06-01T00:00:00Z'".to_string()),
        "text" => Some(format!("'Test {} B'", field.label)),
        _ => None,
    }
}
