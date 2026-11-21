//! The projection-read service-op arms (issue #768): a public abstract op
//! whose return slot is a projection boundary of the bound entity.
//!
//! The read model is constructed through the boundary's `from_row` — the same
//! constructor the repository projection finder uses, so the two read paths
//! compute the `derive` members identically. Two shapes share the arm: the
//! ITEM read (the route loaded the record) and the COLLECTION read (the
//! RLS-scoped `list_filtered` page).

use super::super::types::{ExecutionRenderFacts, ProjectionReadPlan, ServiceOpRoute};
use super::{audit_arity, domain_error};

/// The `projection_read` arm (issue #768). `plan.collection` selects the
/// collection-addressed shape; the record load for the item shape happens
/// OUTSIDE the arm (the route's prelude).
pub(super) fn projection_block(op: &ServiceOpRoute, plan: &ProjectionReadPlan) -> String {
    let facts = &op.render_facts;
    let mut block = String::from(PROJECTION_INTRO);
    // The `{% if collection %}` (`{% else %}`) tag newline: the intro's last
    // line is terminated, then the blank line the tag leaves behind.
    block.push_str("\n\n");
    if plan.collection {
        block.push_str(&collection_read(facts, plan));
    } else {
        block.push_str(&item_read(facts, plan));
    }
    // The `{% endif %}` tag newline (terminate + blank).
    block.push_str("\n\n");
    block.push_str(PROJECTION_ENVELOPE);
    block
}

/// The arm's shared preamble: the constructor contract and the `derive`
/// lowering note.
const PROJECTION_INTRO: &str = concat!(
    "    // Projection read (issue #768): the read model is constructed through\n",
    "    // the modeled boundary's `from_row` — the same constructor the\n",
    "    // repository projection finder uses, so both read paths compute the\n",
    "    // derived members identically. `derive` members were lowered at\n",
    "    // generation time (the bounded arithmetic subset); a field divisor\n",
    "    // follows Rust division semantics.",
);

/// The collection-addressed read: one RLS-scoped `list_filtered` page (the
/// v1 1000-row cap), mapped through `from_row`. The call's arity mirrors the
/// query plane exactly — the `include_deleted` argument for an auditable
/// entity, then the #306 `sort` argument when the entity's ux plan has
/// sortable columns.
fn collection_read(facts: &ExecutionRenderFacts, plan: &ProjectionReadPlan) -> String {
    let ux_sort = if facts.ux_sort_fields { ", None" } else { "" };
    format!(
        concat!(
            "    let (rows, _) = state.{domain}_{table}_queries\n",
            "        .list_filtered(0, 1000, &std::collections::HashMap::new(){audit}{sort}, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n",
            "        .await\n",
            "        .map_err(|e: {domain_error}| {{\n",
            "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to list {entity}\")\n",
            "                .with_correlation_id(correlation_id)\n",
            "        }})?;\n",
            "    let projection: Vec<{types}::dto::{module}::{projection}> =\n",
            "        rows.iter().map({types}::dto::{module}::{projection}::from_row).collect();",
        ),
        domain = facts.domain,
        table = facts.entity_table,
        audit = audit_arity(facts),
        sort = ux_sort,
        domain_error = domain_error(facts),
        entity = facts.entity_name,
        types = facts.domain_types_crate,
        module = plan.module,
        projection = plan.projection,
    )
}

/// The item-addressed read: the route already loaded the record, so the
/// projection is one `from_row` over it.
fn item_read(facts: &ExecutionRenderFacts, plan: &ProjectionReadPlan) -> String {
    format!(
        "    let projection = {types}::dto::{module}::{projection}::from_row(&response);",
        types = facts.domain_types_crate,
        module = plan.module,
        projection = plan.projection,
    )
}

/// The read envelope: the projection (or the projection page) in `data`.
const PROJECTION_ENVELOPE: &str = concat!(
    "    Ok(axum::Json(serde_json::json!({\n",
    "        \"data\": projection,\n",
    "        \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
    "    })).into_response())",
);

#[cfg(test)]
mod tests {
    use crate::ddd::service_ops::render::{render_execution_block, test_support};
    use crate::ddd::service_ops::types::{ExecutionKind, ProjectionReadPlan, ServiceOpRoute};

    /// The matrix fixture's `TaskSummary` projection, item-addressed.
    fn item_plan() -> ProjectionReadPlan {
        ProjectionReadPlan {
            module: "summary".to_string(),
            projection: "TaskSummary".to_string(),
            collection: false,
            derived: Vec::new(),
        }
    }

    /// The matrix fixture's collection-addressed `TaskSummary` page.
    fn collection_plan() -> ProjectionReadPlan {
        ProjectionReadPlan {
            collection: true,
            ..item_plan()
        }
    }

    fn projection_route(op_name: &str, plan: ProjectionReadPlan) -> ServiceOpRoute {
        test_support::route(
            "TaskService",
            op_name,
            test_support::matrix_facts(),
            ExecutionKind::ProjectionRead { plan },
            true,
        )
    }

    /// The `summarise` (item) arm EXTRACTED VERBATIM from the blessed
    /// `root_service_ops` snapshot.
    const MATRIX_PROJECTION_ITEM_BLOCK: &str = concat!(
        "    // Projection read (issue #768): the read model is constructed through\n",
        "    // the modeled boundary's `from_row` — the same constructor the\n",
        "    // repository projection finder uses, so both read paths compute the\n",
        "    // derived members identically. `derive` members were lowered at\n",
        "    // generation time (the bounded arithmetic subset); a field divisor\n",
        "    // follows Rust division semantics.\n",
        "\n",
        "    let projection = domain_types::dto::summary::TaskSummary::from_row(&response);\n",
        "\n",
        "    Ok(axum::Json(serde_json::json!({\n",
        "        \"data\": projection,\n",
        "        \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
        "    })).into_response())",
    );

    /// The `summariseAll` (collection) arm EXTRACTED VERBATIM from the blessed
    /// `root_service_ops` snapshot — including the #306 `sort` argument the
    /// fixture's `ux_sort_fields` turns on.
    const MATRIX_PROJECTION_COLLECTION_BLOCK: &str = concat!(
        "    // Projection read (issue #768): the read model is constructed through\n",
        "    // the modeled boundary's `from_row` — the same constructor the\n",
        "    // repository projection finder uses, so both read paths compute the\n",
        "    // derived members identically. `derive` members were lowered at\n",
        "    // generation time (the bounded arithmetic subset); a field divisor\n",
        "    // follows Rust division semantics.\n",
        "\n",
        "    let (rows, _) = state.common_task_queries\n",
        "        .list_filtered(0, 1000, &std::collections::HashMap::new(), None, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n",
        "        .await\n",
        "        .map_err(|e: CommonError| {\n",
        "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to list Task\")\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?;\n",
        "    let projection: Vec<domain_types::dto::summary::TaskSummary> =\n",
        "        rows.iter().map(domain_types::dto::summary::TaskSummary::from_row).collect();\n",
        "\n",
        "    Ok(axum::Json(serde_json::json!({\n",
        "        \"data\": projection,\n",
        "        \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
        "    })).into_response())",
    );

    /// The item read constructs the projection over the record the route
    /// loaded (the load itself is outside the arm).
    #[test]
    fn projection_item_renders_the_from_row_over_the_loaded_record() {
        let op = projection_route("summarise", item_plan());
        assert_eq!(render_execution_block(&op), MATRIX_PROJECTION_ITEM_BLOCK);
    }

    /// The collection read pages the RLS-scoped collection and maps every row
    /// through `from_row` — with the `sort` argument when the entity's ux
    /// plan has sortable columns (the query plane's arity).
    #[test]
    fn projection_collection_renders_the_scoped_list_and_the_sort_arity() {
        let mut op = projection_route("summariseAll", collection_plan());
        op.render_facts.ux_sort_fields = true;
        assert_eq!(
            render_execution_block(&op),
            MATRIX_PROJECTION_COLLECTION_BLOCK
        );
    }

    /// Without the ux sort plan the `list_filtered` call drops the `sort`
    /// argument (the arity the query plane emitted) — no trailing comma.
    #[test]
    fn projection_collection_omits_the_sort_argument_without_a_ux_plan() {
        let op = projection_route("summariseAll", collection_plan());
        let block = render_execution_block(&op);
        assert!(
            block.contains(
                ".list_filtered(0, 1000, &std::collections::HashMap::new(), api_key_info.api_key_id,"
            ),
            "no sort argument without a ux plan: {block}"
        );
    }

    /// An AUDITABLE entity's collection page carries `include_deleted` BEFORE
    /// the sort argument (the query plane's parameter order).
    #[test]
    fn projection_collection_orders_include_deleted_before_sort() {
        let mut op = projection_route("summariseAll", collection_plan());
        op.render_facts.auditable = true;
        op.render_facts.ux_sort_fields = true;
        let block = render_execution_block(&op);
        assert!(
            block.contains(
                ".list_filtered(0, 1000, &std::collections::HashMap::new(), false, None, api_key_info.api_key_id,"
            ),
            "auditable + ux sort renders `, false, None`: {block}"
        );
    }

    /// The item read never touches the query plane — the record load (and so
    /// the projection read's input) is the route's own prelude.
    #[test]
    fn projection_item_does_not_query_the_collection() {
        let op = projection_route("summarise", item_plan());
        let block = render_execution_block(&op);
        assert!(!block.contains("list_filtered"));
        assert!(!block.contains("state.common_task_queries"));
    }
}
